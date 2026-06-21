//! Long-lived daemon mode + spawn-if-missing client.
//!
//! Phase 1 (no tray): one daemon per workspace root holds a warm `Engine` +
//! SQLite db + notify watcher. CLI/check/LSP clients attach over a Unix domain
//! socket and reuse the warm tables instead of cold-ticking every invocation.
//! The gradle shape, in one binary.
//!
//! Discovery files at `<root>/.dl/`:
//!   - `daemon.sock`  Unix domain socket (mode 0600)
//!   - `daemon.pid`   text file: `pid\nstart_secs\nprogram_path\n`
//!
//! Lifecycle:
//!   - `dl --daemon`  foreground daemon (logs to stderr, ignores idle timeout)
//!   - `dl --stop`    sends `shutdown` over the socket
//!   - default invocation auto-attaches; spawns if no socket / dead PID
//!   - `DL_NO_DAEMON=1` opts out (in-process, the pre-daemon path; used by tests)
//!   - `DL_DAEMON_IDLE_SECS=N` overrides the 30 min default

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::ast::Program;
use crate::engine::{DiagRow, Engine};
use crate::rpc::{self, Request, Response, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND};
use crate::{config, db};

const DEFAULT_IDLE_SECS: u64 = 30 * 60;
const IDLE_TICK_SECS: u64 = 30;
const CONNECT_BACKOFF_MS: &[u64] = &[10, 20, 40, 80, 160, 320, 500];
const CONNECT_TOTAL_TIMEOUT_SECS: u64 = 5;

// ---------- path helpers ----------

/// `<root>/.dl/daemon.sock`.
pub fn socket_path(root: &Path) -> PathBuf {
    root.join(".dl").join("daemon.sock")
}

/// `<root>/.dl/daemon.pid`.
pub fn pid_path(root: &Path) -> PathBuf {
    root.join(".dl").join("daemon.pid")
}

fn write_pid_file(root: &Path, program: Option<&str>) -> Result<()> {
    let dir = root.join(".dl");
    std::fs::create_dir_all(&dir)?;
    let pid = std::process::id();
    let start = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
    let prog = program.unwrap_or("");
    std::fs::write(pid_path(root), format!("{pid}\n{start}\n{prog}\n"))?;
    Ok(())
}

#[allow(dead_code)]
fn read_pid_file(root: &Path) -> Option<(u32, u64, String)> {
    let txt = std::fs::read_to_string(pid_path(root)).ok()?;
    let mut lines = txt.lines();
    let pid: u32 = lines.next()?.parse().ok()?;
    let start: u64 = lines.next()?.parse().ok()?;
    let prog = lines.next().unwrap_or("").to_string();
    Some((pid, start, prog))
}

fn remove_pid_file(root: &Path) {
    let _ = std::fs::remove_file(pid_path(root));
}

// ---------- Daemon state ----------

pub struct Daemon {
    pub root: PathBuf,
    pub program_display: String,
    pub prog: Program,
    pub eng: Mutex<Engine>,
    pub last_activity: Mutex<Instant>,
    pub tick_count: AtomicU64,
    pub shutdown_requested: AtomicBool,
    /// Subscribers for pushed notifications. Each subscriber is a kept-open
    /// socket; the watcher writes one `diag_changed` notification per tick.
    /// Broken subscribers are reaped on the next broadcast.
    pub subscribers: Mutex<Vec<Arc<Mutex<UnixStream>>>>,
}

impl Daemon {
    fn touch(&self) {
        *self.last_activity.lock().unwrap() = Instant::now();
    }

    fn tick_full(&self, quiet: bool) -> Result<()> {
        let mut eng = self.eng.lock().unwrap();
        eng.tick(&self.prog, quiet)?;
        drop(eng);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        self.broadcast_diag_changed();
        Ok(())
    }

    fn tick_paths(&self, paths: &[PathBuf], quiet: bool) -> Result<()> {
        let mut eng = self.eng.lock().unwrap();
        eng.tick_paths(&self.prog, paths, quiet)?;
        drop(eng);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        self.broadcast_diag_changed();
        Ok(())
    }

    fn broadcast_diag_changed(&self) {
        let note = json!({"jsonrpc": "2.0", "method": "diag_changed", "params": {
            "tick": self.tick_count.load(Ordering::Relaxed)
        }});
        let body = match serde_json::to_string(&note) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut subs = self.subscribers.lock().unwrap();
        let mut broken: Vec<usize> = Vec::new();
        for (i, s) in subs.iter().enumerate() {
            let mut guard = match s.lock() {
                Ok(g) => g,
                Err(_) => { broken.push(i); continue; }
            };
            if rpc::write_frame(&mut *guard, &body).is_err() {
                broken.push(i);
            }
        }
        // Reap broken subscribers (descending so indexes stay valid).
        for i in broken.into_iter().rev() { subs.swap_remove(i); }
    }
}

// ---------- daemon entry ----------

/// Run the daemon in the foreground. Binds the socket, parses the program,
/// does the cold tick, then drives the notify watcher + accept loop until
/// shutdown. Ignores idle timeout when `foreground` is true (caller wants it
/// alive for debugging); the spawn-if-missing path passes `foreground=false`.
pub fn run_daemon(
    program: Option<&str>,
    db_path: Option<&str>,
    root: PathBuf,
    foreground: bool,
) -> Result<()> {
    let files = crate::resolve_programs(program, &root)?;
    let (prog, type_diags, display) = crate::prepare_paths(&files)?;
    crate::render_type_diags_eprintln(&type_diags);
    let n_err = type_diags.iter().filter(|d| d.severity == crate::ast::Severity::Error).count();
    if n_err > 0 { bail!("{n_err} type error(s) in program; daemon not started"); }

    let conn = db::open(db_path)?;
    let mut eng = Engine::new(conn, root.clone());
    eng.set_repos(load_repos_eager());
    eng.tick(&prog, false)?;
    eprintln!("[daemon] cold tick done ({} type diag(s), program {})", type_diags.len(), display);

    let idle_secs = idle_timeout_secs();
    let daemon = Arc::new(Daemon {
        root: root.clone(),
        program_display: display,
        prog,
        eng: Mutex::new(eng),
        last_activity: Mutex::new(Instant::now()),
        tick_count: AtomicU64::new(1),
        shutdown_requested: AtomicBool::new(false),
        subscribers: Mutex::new(Vec::new()),
    });

    // Bind socket (reap stale first).
    let sock = socket_path(&root);
    if sock.exists() {
        // Stale socket file from a killed -9 daemon. The PID file may also be
        // stale; try connect first to confirm liveness, then unlink if dead.
        if UnixStream::connect(&sock).is_err() {
            let _ = std::fs::remove_file(&sock);
        } else {
            bail!("daemon already running on socket {}", sock.display());
        }
    }
    std::fs::create_dir_all(sock.parent().unwrap())?;
    let listener = UnixListener::bind(&sock)?;
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    write_pid_file(&root, program)?;
    eprintln!("[daemon] listening on {} (pid {}, idle {}s)",
        sock.display(), std::process::id(), idle_secs);

    let d = daemon.clone();
    std::thread::Builder::new().name("dl-watch".into())
        .spawn(move || watcher_loop(d))?;

    if !foreground {
        let d = daemon.clone();
        std::thread::Builder::new().name("dl-idle".into())
            .spawn(move || idle_loop(d, idle_secs))?;
    }

    // Accept loop. Main thread.
    let mut next_id: u64 = 0;
    for stream in listener.incoming() {
        if daemon.shutdown_requested.load(Ordering::Relaxed) { break; }
        match stream {
            Ok(stream) => {
                next_id += 1;
                let d = daemon.clone();
                std::thread::Builder::new().name(format!("dl-conn-{next_id}"))
                    .spawn(move || handle_connection(d, stream))?;
            }
            Err(e) => eprintln!("[daemon] accept error: {e}"),
        }
    }
    shutdown_cleanup(&daemon);
    Ok(())
}

fn shutdown_cleanup(d: &Daemon) {
    let sock = socket_path(&d.root);
    let _ = std::fs::remove_file(&sock);
    remove_pid_file(&d.root);
    eprintln!("[daemon] shut down cleanly");
}

// ---------- watcher thread ----------

fn watcher_loop(d: Arc<Daemon>) {
    use notify::{RecursiveMode, Watcher};
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| { let _ = tx.send(res); }) {
        Ok(w) => w,
        Err(e) => { eprintln!("[daemon] watcher init failed: {e}"); return; }
    };
    if let Err(e) = watcher.watch(&d.root, RecursiveMode::Recursive) {
        eprintln!("[daemon] watch root failed: {e}");
        return;
    }
    let cfg_path = config::SprfConfig::config_path()
        .and_then(|p| p.canonicalize().ok().or(Some(p)));
    if let Some(cp) = &cfg_path {
        if let Some(dir) = cp.parent() {
            if dir.exists() && watcher.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                eprintln!("[daemon] watching config {}", cp.display());
            }
        }
    }
    let scans_git = d.prog.items.iter().any(|i| matches!(i, crate::ast::Item::Rule(r)
        if r.body.iter().any(|b| matches!(b, crate::ast::BodyItem::Scan { rev: crate::ast::Term::Str(s), .. } if s.as_str() != "WORK"))));
    let mut git_dir: Option<PathBuf> = None;
    if scans_git {
        if let Ok(out) = std::process::Command::new("git").arg("-C").arg(&d.root)
            .args(["rev-parse", "--git-dir"]).output() {
            if out.status.success() {
                let gd = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let gdp = if Path::new(&gd).is_absolute() { PathBuf::from(&gd) } else { d.root.join(&gd) };
                if gdp.exists() && watcher.watch(&gdp, RecursiveMode::Recursive).is_ok() {
                    git_dir = gdp.canonicalize().ok();
                }
            }
        }
    }

    eprintln!("[daemon] watcher ready ({})", d.root.display());
    while let Ok(first) = rx.recv() {
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Ok(ev) = first { paths.extend(ev.paths); }
        std::thread::sleep(Duration::from_millis(150));
        while let Ok(ev) = rx.try_recv() {
            if let Ok(ev) = ev { paths.extend(ev.paths); }
        }
        d.touch();  // any watcher event resets idle, even if no tick results
        let touches_cfg = cfg_path.as_ref().is_some_and(|c|
            paths.iter().any(|p| p.canonicalize().ok().as_deref() == Some(c) || p == c));
        let touches_git = git_dir.as_ref().is_some_and(|g| paths.iter().any(|p| p.starts_with(g)));
        // A `.dl` program edit can't be hot-reloaded in v1; bail and let the
        // next invocation respawn with the new program.
        let touches_program = d.program_in_paths(&paths);
        if touches_program {
            eprintln!("[daemon] program file changed; exiting for respawn");
            d.shutdown_requested.store(true, Ordering::Relaxed);
            // Closing the listener from another thread is awkward; just exit.
            std::process::exit(0);
        }
        let result = if touches_cfg {
            let mut eng = d.eng.lock().unwrap();
            eng.set_repos(load_repos_eager());
            drop(eng);
            d.tick_full(false)
        } else if touches_git || paths.is_empty() {
            d.tick_full(false)
        } else {
            d.tick_paths(&paths, false)
        };
        if let Err(e) = result {
            eprintln!("[daemon] tick error: {e}");
        }
    }
}

impl Daemon {
    fn program_in_paths(&self, paths: &[PathBuf]) -> bool {
        // Cheapest heuristic: any path whose extension is `.dl` under the root's
        // `.dl/` dir, or matching the explicit program path. v1 trades precision
        // (we can't distinguish the program file from other .dl files in
        // discovery mode) for simplicity; a false-positive exit just respawns.
        for p in paths {
            if p.extension().and_then(|e| e.to_str()) == Some("dl") {
                if let Ok(rel) = p.strip_prefix(&self.root) {
                    if rel.starts_with(".dl") { return true; }
                }
                if self.program_display.contains(&*p.to_string_lossy()) { return true; }
            }
        }
        false
    }
}

// ---------- idle thread ----------

fn idle_loop(d: Arc<Daemon>, idle_secs: u64) {
    loop {
        std::thread::sleep(Duration::from_secs(IDLE_TICK_SECS));
        let last = *d.last_activity.lock().unwrap();
        if last.elapsed() > Duration::from_secs(idle_secs) {
            eprintln!("[daemon] idle {}min, exiting", idle_secs / 60);
            shutdown_cleanup(&d);
            std::process::exit(0);
        }
    }
}

fn idle_timeout_secs() -> u64 {
    std::env::var("DL_DAEMON_IDLE_SECS").ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_IDLE_SECS)
}

// ---------- per-connection handler ----------

fn handle_connection(d: Arc<Daemon>, mut stream: UnixStream) {
    loop {
        let body = match rpc::read_frame(&mut stream) {
            Ok(Some(b)) => b,
            Ok(None) => return,
            Err(e) => { eprintln!("[daemon] read error: {e}"); return; }
        };
        let v: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                let r = Response::err(0, rpc::PARSE_ERROR, format!("parse: {e}"));
                let _ = rpc::write_frame(&mut stream, &serde_json::to_string(&r.to_json()).unwrap());
                continue;
            }
        };
        let req = match parse_request(v) {
            Some(r) => r,
            None => {
                // Probably a notification (no id). v1 ignores those inbound.
                continue;
            }
        };
        let is_shutdown = req.method == "shutdown";
        let is_subscribe = req.method == "subscribe";
        let resp = handle_request(&d, &req, if is_subscribe { Some(stream.try_clone().ok().map(|s| Arc::new(Mutex::new(s)))) } else { None }.flatten());
        let out = serde_json::to_string(&resp.to_json()).unwrap_or_else(|_| "{}".into());
        if rpc::write_frame(&mut stream, &out).is_err() { return; }
        if is_shutdown {
            d.shutdown_requested.store(true, Ordering::Relaxed);
            // The accept loop is blocked in `listener.incoming().next()`;
            // setting the flag alone does not wake it. Cleanup + exit here so
            // `dl --stop` returns promptly. A graceful wake (self-pipe, non-
            // blocking accept) is the v1.1 polish item.
            shutdown_cleanup(&d);
            std::process::exit(0);
        }
        d.touch();
    }
}

fn parse_request(v: Value) -> Option<Request> {
    let id = v.get("id")?.as_u64()?;
    let method = v.get("method")?.as_str()?.to_string();
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    Some(Request { id, method, params })
}

fn handle_request(d: &Daemon, req: &Request, subscriber_stream: Option<Arc<Mutex<UnixStream>>>) -> Response {
    match req.method.as_str() {
        "ping" => Response::ok(req.id, json!({
            "ok": true,
            "root": d.root.to_string_lossy(),
            "tick_count": d.tick_count.load(Ordering::Relaxed),
            "program": d.program_display,
        })),
        "query" => {
            let eng = d.eng.lock().unwrap();
            match eng.run_queries_capture(&d.prog) {
                Ok(results) => Response::ok(req.id, json!({"results": results.iter().map(|r| json!({
                    "rel": r.rel, "columns": r.columns, "rows": r.rows,
                })).collect::<Vec<_>>()})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "diag" => {
            let only = req.params.get("path").and_then(|p| p.as_str());
            let eng = d.eng.lock().unwrap();
            match eng.diags(only) {
                Ok(rows) => Response::ok(req.id, json!({"rows": rows.iter().map(diag_to_json).collect::<Vec<_>>()})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "definition" => {
            let file = match req.params.get("file").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing file"),
            };
            let text = match req.params.get("text").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing text"),
            };
            let eng = d.eng.lock().unwrap();
            match eng.definition_targets(file, text) {
                Ok(targets) => Response::ok(req.id, json!({"targets": targets.iter()
                    .map(|(f, l)| json!([f, l])).collect::<Vec<_>>()})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "hover" => {
            let file = req.params.get("file").and_then(|v| v.as_str()).unwrap_or("");
            let text = match req.params.get("text").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing text"),
            };
            let eng = d.eng.lock().unwrap();
            match eng.hover(file, text) {
                Ok(md) => Response::ok(req.id, json!({"markdown": md})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "subscribe" => {
            let events = req.params.get("events").cloned().unwrap_or(Value::Array(vec![]));
            if let Some(s) = subscriber_stream {
                d.subscribers.lock().unwrap().push(s);
                Response::ok(req.id, json!({"events": events, "ok": true}))
            } else {
                Response::err(req.id, INVALID_PARAMS, "subscribe requires a kept-open socket")
            }
        }
        "shutdown" => Response::ok(req.id, json!({"ok": true})),
        other => Response::err(req.id, METHOD_NOT_FOUND, format!("unknown method: {other}")),
    }
}

fn diag_to_json(d: &DiagRow) -> Value {
    json!({
        "path": d.path, "line": d.line, "col": d.col,
        "endLine": d.end_line, "endCol": d.end_col,
        "severity": d.severity, "code": d.code, "message": d.msg,
        "hint": d.hint,
    })
}

// ---------- client side ----------

/// True iff the daemon is enabled for this process (env opt-out check).
pub fn enabled() -> bool {
    std::env::var("DL_NO_DAEMON").ok().as_deref() != Some("1")
}

/// True iff the daemon should manage this root. A workspace opts INTO daemon
/// management by having a `.dl/` directory (the same gate discovery mode
/// uses). Without `.dl/` the daemon has no socket home and we stay in-process,
/// so a one-off `dl p.dl` in a tempdir never spawns a side process.
pub fn enabled_for(root: &Path) -> bool {
    enabled() && root.join(".dl").is_dir()
}

/// True iff a daemon is currently listening on `<root>/.dl/daemon.sock`.
pub fn is_running(root: &Path) -> bool {
    UnixStream::connect(socket_path(root)).is_ok()
}

/// Connect (must be already running). Returns a framed stream.
pub fn connect(root: &Path) -> Result<UnixStream> {
    UnixStream::connect(socket_path(root))
        .with_context(|| format!("connect daemon socket {}", socket_path(root).display()))
}

/// Send one request, read one response. For one-shot RPCs (ping, query, diag,
/// shutdown). Subscribe uses a long-lived connection instead.
pub fn rpc_call(stream: &mut UnixStream, req: &Request) -> Result<Response> {
    let body = serde_json::to_string(&req.to_json())?;
    rpc::write_frame(stream, &body)?;
    let resp_body = rpc::read_frame(stream)?
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection without responding"))?;
    let v: Value = serde_json::from_str(&resp_body)?;
    let r = Response::from_value(v)?;
    Ok(r)
}

/// Spawn-if-missing: ensure a daemon is running on `<root>/.dl/daemon.sock`.
/// If already up, returns immediately. Otherwise spawns `dl --daemon --root X`
/// detached (foreground=false so idle timeout applies) and poll-connects until
/// the daemon responds to `ping` or the connect-time budget is exhausted.
pub fn ensure_daemon(root: &Path, program: Option<&str>) -> Result<()> {
    if is_running(root) {
        // Ping to confirm it's our daemon and not a stale socket.
        let mut s = connect(root)?;
        let req = Request::new(0, "ping", json!({}));
        match rpc_call(&mut s, &req) {
            Ok(r) if r.error.is_none() => return Ok(()),
            _ => {}  // fall through to respawn
        }
    }
    // Stale or missing. Spawn detached.
    spawn_detached(root, program)?;
    wait_ready(root, program)
}

fn spawn_detached(root: &Path, program: Option<&str>) -> Result<()> {
    let exe = std::env::current_exe()
        .context("locate current exe for daemon spawn")?;
    let log = root.join(".dl").join("daemon.log");
    if let Some(p) = log.parent() { std::fs::create_dir_all(p)?; }
    let log_file = std::fs::OpenOptions::new()
        .create(true).append(true).open(&log)?;
    let stderr = log_file.try_clone()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("--daemon").arg("--root").arg(root);
    if let Some(p) = program { cmd.arg(p); }
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(stderr));
    // v1 detach: stdio redirected; child keeps running after parent exits
    // (orphaned, reparented to launchd/init). A real setsid + new session
    // can come with the windows-subsystem / packaging phase.
    cmd.spawn().context("spawn daemon")?;
    Ok(())
}

fn wait_ready(root: &Path, _program: Option<&str>) -> Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(CONNECT_TOTAL_TIMEOUT_SECS);
    let mut backoff_idx = 0;
    loop {
        if start.elapsed() > timeout {
            bail!("daemon did not become ready in {}s", CONNECT_TOTAL_TIMEOUT_SECS);
        }
        if let Ok(mut s) = UnixStream::connect(socket_path(root)) {
            let req = Request::new(0, "ping", json!({}));
            if let Ok(resp) = rpc_call(&mut s, &req) {
                if resp.error.is_none() { return Ok(()); }
            }
        }
        let delay_ms = CONNECT_BACKOFF_MS.get(backoff_idx)
            .copied()
            .unwrap_or(*CONNECT_BACKOFF_MS.last().unwrap());
        backoff_idx += 1;
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

/// Send `shutdown` to the daemon on `<root>/.dl/daemon.sock`. Returns Ok if the
/// daemon acknowledged or was not running. Used by `dl --stop`.
pub fn stop(root: &Path) -> Result<()> {
    if !is_running(root) {
        // Stale files; clean up.
        let _ = std::fs::remove_file(socket_path(root));
        remove_pid_file(root);
        return Ok(());
    }
    let mut s = connect(root)?;
    let req = Request::new(1, "shutdown", json!({}));
    let resp = rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error {
        bail!("daemon shutdown refused: {}", e.message);
    }
    // Give the daemon a moment to clean up.
    for _ in 0..50 {
        if !is_running(root) { return Ok(()); }
        std::thread::sleep(Duration::from_millis(20));
    }
    bail!("daemon did not close socket after shutdown")
}

// ---------- small helpers shared with lib.rs ----------

fn load_repos_eager() -> Vec<config::RepoConfig> {
    match config::SprfConfig::load_default() {
        Ok(cfg) if !cfg.repos.is_empty() => {
            eprintln!("[config] {} repo(s) registered", cfg.repos.len());
            cfg.repos
        }
        Ok(_) => Vec::new(),
        Err(e) => { eprintln!("[config] ignored: {e}"); Vec::new() }
    }
}
