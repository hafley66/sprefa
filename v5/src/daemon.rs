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

/// The rootless daemon's home: `$XDG_STATE_HOME/sprefa` (or `~/.local/state/
/// sprefa`). One singleton serving daemon lives here, decoupled from any
/// project root — the "folders in view" model. Created on demand.
pub fn daemon_home() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("sprefa")
}

/// The control-file directory for a daemon. `Some(root)` is the per-root
/// auto-attach daemon at `<root>/.dl` (spawn-if-missing for one-shots, keyed by
/// the repo being queried). `None` is the singleton rootless serving daemon at
/// the XDG home. Keeping the two namespaces distinct lets them coexist: a
/// `dl <prog>` one-shot spawns its own per-root helper without colliding with
/// the long-lived serving daemon.
fn home_dir(root: Option<&Path>) -> PathBuf {
    match root {
        Some(r) => r.join(".dl"),
        None => daemon_home(),
    }
}

/// `<home>/daemon.sock`.
pub fn socket_path(root: Option<&Path>) -> PathBuf {
    home_dir(root).join("daemon.sock")
}

/// `<home>/daemon.pid`.
pub fn pid_path(root: Option<&Path>) -> PathBuf {
    home_dir(root).join("daemon.pid")
}

fn write_pid_file(root: Option<&Path>, program: Option<&str>) -> Result<()> {
    let dir = home_dir(root);
    std::fs::create_dir_all(&dir)?;
    let pid = std::process::id();
    let start = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
    let prog = program.unwrap_or("");
    std::fs::write(pid_path(root), format!("{pid}\n{start}\n{prog}\n"))?;
    Ok(())
}

#[allow(dead_code)]
fn read_pid_file(root: Option<&Path>) -> Option<(u32, u64, String)> {
    let txt = std::fs::read_to_string(pid_path(root)).ok()?;
    let mut lines = txt.lines();
    let pid: u32 = lines.next()?.parse().ok()?;
    let start: u64 = lines.next()?.parse().ok()?;
    let prog = lines.next().unwrap_or("").to_string();
    Some((pid, start, prog))
}

fn remove_pid_file(root: Option<&Path>) {
    let _ = std::fs::remove_file(pid_path(root));
}

// ---------- Daemon state ----------

pub struct Daemon {
    /// Engine/content/watch base. For a rootless serving daemon this is the XDG
    /// home (a benign "self" that scans nothing); the view comes from config.
    pub root: PathBuf,
    /// The `--root` the daemon was launched with, for CONTROL files only
    /// (socket/pid). `None` = the singleton rootless daemon at the XDG home.
    pub home: Option<PathBuf>,
    pub program_display: String,
    /// Canonicalized absolute paths the daemon parsed. Edit-exact detection
    /// compares against this list, not "any .dl file under .dl/" (the v1
    /// heuristic that fired on unrelated .dl edits and on residual FSEvents
    /// from the spawning shell's recent file setup). Wrapped in a Mutex so
    /// discovery-mode hot-reload can add/remove files after startup.
    pub program_files: Mutex<Vec<PathBuf>>,
    /// True when the daemon started without an explicit program file (discovery
    /// mode: picks up every `<root>/.dl/*.dl` at startup). When true, new or
    /// removed `.dl` files in `.dl/` trigger a re-discovery and re-merge.
    pub discovery_mode: bool,
    /// When true, program-file edits hot-reload instead of triggering respawn.
    /// Tray mode sets this: the user wants the menu bar item to stay alive
    /// across edits. The watcher re-parses the program files in place, swaps
    /// the parsed `Program`, and re-ticks. A parse failure logs and keeps the
    /// last good program.
    pub no_respawn: bool,
    pub prog: Mutex<Program>,
    pub eng: Mutex<Engine>,
    pub last_activity: Mutex<Instant>,
    pub tick_count: AtomicU64,
    pub shutdown_requested: AtomicBool,
    /// The paths touched by the most recent tick (absolute). Empty after a
    /// full tick (paths unknown) — subscribers treat empty as "re-publish
    /// everything". Targeted publish on incremental ticks is the v1.1
    /// refinement: only files whose `diag` rows could have moved need a
    /// re-publish, not the whole tree.
    pub last_changed_paths: Mutex<Vec<PathBuf>>,
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
        let prog = self.prog.lock().unwrap();
        let mut eng = self.eng.lock().unwrap();
        eng.tick(&prog, quiet)?;
        drop(eng);
        drop(prog);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        // Full tick: changed paths unknown, subscribers re-publish everything.
        *self.last_changed_paths.lock().unwrap() = Vec::new();
        self.broadcast_diag_changed();
        Ok(())
    }

    fn tick_paths(&self, paths: &[PathBuf], quiet: bool) -> Result<()> {
        let prog = self.prog.lock().unwrap();
        let mut eng = self.eng.lock().unwrap();
        eng.tick_paths(&prog, paths, quiet)?;
        drop(eng);
        drop(prog);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        // Incremental tick: only these paths' rows could have moved; subscribers
        // can re-publish just these files.
        *self.last_changed_paths.lock().unwrap() = paths.to_vec();
        self.broadcast_diag_changed();
        Ok(())
    }

    /// Re-parse the program files, swap the parsed `Program`, re-tick. Called
    /// by the watcher when `no_respawn` is set and a program file changed.
    /// A parse or type error keeps the last good program.
    fn reload_program(&self) -> Result<()> {
        let files = self.program_files.lock().unwrap().clone();
        let (new_prog, type_diags, _display) = crate::prepare_paths(&files)?;
        let n_err = type_diags
            .iter()
            .filter(|d| d.severity == crate::ast::Severity::Error)
            .count();
        if n_err > 0 {
            bail!("{n_err} type error(s) in reloaded program; keeping old");
        }
        crate::render_type_diags_eprintln(&type_diags);
        {
            let mut p = self.prog.lock().unwrap();
            *p = new_prog;
        }
        if let Err(e) = self.eng.lock().unwrap().save_program_meta(&files) {
            eprintln!("[daemon] save_program_meta: {e}");
        }
        self.tick_full(false)?;
        Ok(())
    }

    /// Re-discover `.dl` files under `<root>/.dl/`, re-merge the program
    /// if the file set changed, re-tick. Called by the watcher when a new
    /// `.dl` file appears in discovery mode (k8s-style add-a-file). If the
    /// set is unchanged, returns immediately.
    fn reload_discovery(&self) -> Result<()> {
        if !self.discovery_mode {
            return Ok(());
        }
        let files = crate::resolve_programs(None, &self.root)?;
        let mut canon: Vec<PathBuf> = files
            .iter()
            .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
            .collect();
        canon.sort();
        {
            let pf = self.program_files.lock().unwrap();
            if canon == *pf {
                return Ok(());
            }
        }
        let (new_prog, type_diags, _display) = crate::prepare_paths(&files)?;
        let n_err = type_diags
            .iter()
            .filter(|d| d.severity == crate::ast::Severity::Error)
            .count();
        if n_err > 0 {
            crate::render_type_diags_eprintln(&type_diags);
            eprintln!("[daemon] discovery reload: {n_err} type error(s); keeping old");
            return Ok(());
        }
        crate::render_type_diags_eprintln(&type_diags);
        {
            let mut pf = self.program_files.lock().unwrap();
            *pf = canon;
        }
        {
            let mut p = self.prog.lock().unwrap();
            *p = new_prog;
        }
        {
            let pf = self.program_files.lock().unwrap().clone();
            if let Err(e) = self.eng.lock().unwrap().save_program_meta(&pf) {
                eprintln!("[daemon] save_program_meta: {e}");
            }
        }
        let n = self.program_files.lock().unwrap().len();
        eprintln!("[daemon] discovery reload: {n} file(s)");
        self.tick_full(false)?;
        Ok(())
    }

    fn broadcast_diag_changed(&self) {
        // Snapshot paths under the lock so write_frame (which can take time on a
        // slow consumer) doesn't hold it.
        let paths: Vec<String> = self.last_changed_paths.lock().unwrap().iter()
            .map(|p| p.to_string_lossy().into_owned()).collect();
        let note = json!({"jsonrpc": "2.0", "method": "diag_changed", "params": {
            "tick": self.tick_count.load(Ordering::Relaxed),
            "paths": paths,
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

    /// The git refs to watch for advance: always `HEAD`, plus every non-WORK rev
    /// literal the loaded program scans (a `scan(... rev: "v1.2")` pins a tag, so
    /// a retag should fire). Deduped, HEAD first.
    fn watched_ref_names(&self) -> Vec<String> {
        let mut names = vec!["HEAD".to_string()];
        let prog = self.prog.lock().unwrap();
        for item in &prog.items {
            if let crate::ast::Item::Rule(r) = item {
                for b in &r.body {
                    if let crate::ast::BodyItem::Scan { rev: crate::ast::Term::Str(s), .. } = b {
                        if s.as_str() != "WORK" && !names.contains(s) {
                            names.push(s.clone());
                        }
                    }
                }
            }
        }
        names
    }

    /// React to a `.git` change: observe each watched ref's current oid against
    /// the persisted cursor, and on advance diff old→new against the `_file`
    /// index and broadcast a `rev_advanced` notification. Returns the number of
    /// refs that advanced AND the union of worktree files the diff says changed
    /// (absolute paths), so the watcher can re-analyze them. Driving the tick
    /// from the diff is deterministic — FSEvents does not reliably co-deliver a
    /// checkout's rewritten files with the `.git` event that accompanied them.
    fn on_git_event(&self) -> (usize, Vec<PathBuf>) {
        let self_slug = self.root.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned());
        let mut repos: Vec<(String, PathBuf)> = vec![(self_slug, self.root.clone())];
        for rc in load_repos_eager() {
            if rc.root.exists() && !repos.iter().any(|(s, _)| s == &rc.slug) {
                repos.push((rc.slug, rc.root));
            }
        }
        let names = self.watched_ref_names();
        let mut advances: Vec<(String, String, String, String, Vec<String>)> = Vec::new();
        let mut changed: Vec<PathBuf> = Vec::new();
        {
            let eng = self.eng.lock().unwrap();
            for (slug, root) in &repos {
                for name in &names {
                    match eng.observe_ref(slug, root, name) {
                        Ok(Some((old, new))) => {
                            let files = eng
                                .files_changed_between(slug, root, old.as_deref().unwrap_or(""), &new)
                                .unwrap_or_default();
                            // Collect absolute worktree paths for the watcher's
                            // follow-up tick (deduped across refs/repos).
                            for f in &files {
                                let abs = root.join(f);
                                if abs.exists() && !changed.contains(&abs) {
                                    changed.push(abs);
                                }
                            }
                            advances.push((slug.clone(), name.clone(),
                                old.unwrap_or_default(), new, files));
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!("[daemon] observe_ref {slug}/{name}: {e}"),
                    }
                }
            }
            // Project the updated `_ref`/`_rev_log` into the query rels so a
            // `query` RPC sees the advance without a re-tick.
            if !advances.is_empty() {
                if let Err(e) = eng.refresh_daemon_rels() {
                    eprintln!("[daemon] refresh_daemon_rels: {e}"); 
                }
            }
        }
        self.touch();
        if !advances.is_empty() {
            self.broadcast_rev_advanced(&advances);
        }
        (advances.len(), changed)
    }

    /// Push one `rev_advanced` notification per advanced ref to every subscriber.
    /// Mirrors `broadcast_diag_changed`: snapshot, write outside the rel lock,
    /// reap broken subscribers.
    fn broadcast_rev_advanced(&self, advances: &[(String, String, String, String, Vec<String>)]) {
        let mut subs = self.subscribers.lock().unwrap();
        let mut broken: Vec<usize> = Vec::new();
        for (repo, name, old, new, files) in advances {
            let note = json!({"jsonrpc": "2.0", "method": "rev_advanced", "params": {
                "repo": repo, "ref": name, "old": old, "new": new, "paths": files,
            }});
            let body = match serde_json::to_string(&note) { Ok(s) => s, Err(_) => continue };
            for (i, s) in subs.iter().enumerate() {
                let mut guard = match s.lock() {
                    Ok(g) => g,
                    Err(_) => { if !broken.contains(&i) { broken.push(i); } continue; }
                };
                if rpc::write_frame(&mut *guard, &body).is_err() && !broken.contains(&i) {
                    broken.push(i);
                }
            }
        }
        broken.sort_unstable();
        for i in broken.into_iter().rev() { subs.swap_remove(i); }
    }
}

// ---------- daemon entry ----------

/// Run the daemon in the foreground. Binds the socket, parses the program,
/// does the cold tick, then drives the notify watcher + accept loop until
/// shutdown. Ignores idle timeout when `foreground` is true (caller wants it
/// alive for debugging); the spawn-if-missing path passes `foreground=false`.
/// When `tray` is true, the main thread runs the menu bar event loop and the
/// accept loop is moved to a worker thread (mac needs the main thread for
/// CFRunLoop / NSApplication). Tray mode also disables program-edit respawn:
/// the daemon stays alive until explicit `--stop` or tray Quit. Hot reload of
/// an edited program file is the v1.2 polish item.
pub fn run_daemon(
    programs: &[String],
    db_path: Option<&str>,
    root: Option<PathBuf>,
    foreground: bool,
    tray: bool,
) -> Result<()> {
    // `home` selects the control-file location (Some(root) → per-root,
    // None → singleton XDG). `eng_root` is the Engine/content/watch base: the
    // given root, or the XDG home as a benign self when launched rootless (the
    // view then comes entirely from config repos).
    let home = root.clone();
    let eng_root = root.clone().unwrap_or_else(daemon_home);
    // Explicit positional files load as the program set (merged in order, with
    // `use` includes spliced); no positionals falls back to `<root>/.dl/*.dl`
    // discovery.
    let files = if programs.is_empty() {
        // Rootless serving daemon: tolerate an empty `.dl/` (no discovery
        // files) so it starts empty and grows via the `load` RPC. A non-empty
        // discovery or an explicit program file set still works as before.
        crate::resolve_programs(None, &eng_root).unwrap_or_default()
    } else {
        programs.iter().map(PathBuf::from).collect()
    };
    let (prog, type_diags, display) = if files.is_empty() {
        (crate::ast::Program { items: vec![] }, vec![], "<serving>".to_string())
    } else {
        crate::prepare_paths(&files)?
    };
    crate::render_type_diags_eprintln(&type_diags);
    let n_err = type_diags.iter().filter(|d| d.severity == crate::ast::Severity::Error).count();
    if n_err > 0 { bail!("{n_err} type error(s) in program; daemon not started"); }

    let conn = db::open(db_path)?;
    let mut eng = Engine::new(conn, eng_root.clone());
    // Rootless serving: self.root is the XDG state dir, a placeholder. Self-form
    // scans / gen writes in loaded scripts then target each rule's own repo.
    if root.is_none() { eng.set_root_implicit(true); }
    let repos = load_repos_eager();
    eng.set_repos(repos.clone());
    eng.tick(&prog, false)?;
    let canon_files: Vec<PathBuf> = files
        .iter()
        .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
        .collect();
    // Persist the repo set + loaded program into the db so a restart can diff
    // them, and seed the rev cursor for every watched ref (HEAD + program revs).
    if let Err(e) = eng.save_repos_meta() { eprintln!("[daemon] save_repos_meta: {e}"); }
    if let Err(e) = eng.save_program_meta(&canon_files) { eprintln!("[daemon] save_program_meta: {e}"); }
    eprintln!("[daemon] cold tick done ({} type diag(s), program {})", type_diags.len(), display);

    let idle_secs = idle_timeout_secs();
    let daemon = Arc::new(Daemon {
        root: eng_root.clone(),
        home: home.clone(),
        program_display: display,
        program_files: Mutex::new(canon_files),
        discovery_mode: programs.is_empty(),
        // Hot-reload (not respawn) when the daemon is tray-driven OR a rootless
        // serving daemon (no explicit program, grown via the `load` RPC). A
        // respawn would re-resolve from empty discovery and lose every loaded
        // script; the serving daemon has no startup program to respawn from.
        no_respawn: tray || (programs.is_empty() && root.is_none()),
        prog: Mutex::new(prog),
        eng: Mutex::new(eng),
        last_activity: Mutex::new(Instant::now()),
        tick_count: AtomicU64::new(1),
        shutdown_requested: AtomicBool::new(false),
        last_changed_paths: Mutex::new(Vec::new()),
        subscribers: Mutex::new(Vec::new()),
    });

    // Bind socket (reap stale first).
    let sock = socket_path(home.as_deref());
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
    write_pid_file(home.as_deref(), programs.first().map(|s| s.as_str()))?;
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

    // Accept loop off-main so the main thread can drive the tray event loop
    // (mac requires this; on other platforms it's harmless).
    let d = daemon.clone();
    std::thread::Builder::new()
        .name("dl-accept".into())
        .spawn(move || accept_loop(d, listener))?;

    if tray {
        crate::tray::run_tray(daemon.clone())?;
    } else {
        // No tray: park on the shutdown flag. The RPC shutdown handler, idle
        // timeout, and watcher's program-edit exit all call process::exit(0)
        // directly, so this loop is mostly a placeholder; it wakes if some
        // future path sets the flag without exiting.
        while !daemon.shutdown_requested.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    shutdown_cleanup(&daemon);
    Ok(())
}

fn accept_loop(daemon: Arc<Daemon>, listener: UnixListener) {
    let mut next_id: u64 = 0;
    for stream in listener.incoming() {
        if daemon.shutdown_requested.load(Ordering::Relaxed) {
            break;
        }
        match stream {
            Ok(stream) => {
                next_id += 1;
                let d = daemon.clone();
                if std::thread::Builder::new()
                    .name(format!("dl-conn-{next_id}"))
                    .spawn(move || handle_connection(d, stream))
                    .is_err()
                {
                    eprintln!("[daemon] thread spawn failed for connection {next_id}");
                }
            }
            Err(e) => eprintln!("[daemon] accept error: {e}"),
        }
    }
}

pub(crate) fn shutdown_cleanup(d: &Daemon) {
    let sock = socket_path(d.home.as_deref());
    let _ = std::fs::remove_file(&sock);
    remove_pid_file(d.home.as_deref());
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
    // Watch every folder in view (config repos), so a rootless serving daemon
    // reacts to source edits across the whole view, not just its own root.
    for rc in load_repos_eager() {
        if rc.root.exists() && rc.root != d.root
            && watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
            eprintln!("[daemon] watching repo {} ({})", rc.slug, rc.root.display());
        }
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
    // Always watch the `.git` dir of the self root and every config repo, so a
    // HEAD move (commit, checkout, pull) is observed even when the program only
    // scans WORK. `git_dirs` = canonical git-dir paths; a watcher event under any
    // of them routes to `on_git_event` (rev-cursor diff + broadcast, no re-tick).
    let mut git_dirs: Vec<PathBuf> = Vec::new();
    let mut watch_git = |root: &Path| {
        if let Ok(out) = std::process::Command::new("git").arg("-C").arg(root)
            .args(["rev-parse", "--git-dir"]).output() {
            if out.status.success() {
                let gd = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let gdp = if Path::new(&gd).is_absolute() { PathBuf::from(&gd) } else { root.join(&gd) };
                if gdp.exists() && watcher.watch(&gdp, RecursiveMode::Recursive).is_ok() {
                    if let Some(c) = gdp.canonicalize().ok() {
                        if !git_dirs.contains(&c) { git_dirs.push(c); }
                    }
                }
            }
        }
    };
    watch_git(&d.root);
    for rc in load_repos_eager() {
        if rc.root.exists() { watch_git(&rc.root); }
    }

    eprintln!("[daemon] watcher ready ({})", d.root.display());
    let watcher_start = std::time::Instant::now();
    const STARTUP_GRACE: Duration = Duration::from_secs(1);
    // Roots already watched (self + config + pulled). A successful tick may pull
    // new repos into the engine's registered set; the loop diff below adds a
    // notify watch for each new root so edits in a dynamically-reached repo
    // react, not just the statically-configured ones.
    let mut watched: std::collections::HashSet<PathBuf> = std::collections::HashSet::from_iter([
        d.root.clone(),
    ].into_iter().chain(load_repos_eager().into_iter().filter(|r| r.root.exists()).map(|r| r.root)));
    while let Ok(first) = rx.recv() {
        // Startup grace: drain residual FSEvents from the spawning shell's
        // recent file activity (mkdir + write land in FSEvents after the
        // watch registers, as CREATE or MODIFY). Anything that happened
        // before the watcher was ready should not fire a tick or an exit.
        if watcher_start.elapsed() < STARTUP_GRACE {
            while rx.try_recv().is_ok() {}
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Ok(ev) = first {
            paths.extend(ev.paths);
        }
        std::thread::sleep(Duration::from_millis(150));
        while let Ok(ev) = rx.try_recv() {
            if let Ok(ev) = ev {
                paths.extend(ev.paths);
            }
        }
        d.touch();  // any watcher event resets idle, even if no tick results
        let touches_cfg = cfg_path.as_ref().is_some_and(|c|
            paths.iter().any(|p| p.canonicalize().ok().as_deref() == Some(c) || p == c));
        let touches_git = !git_dirs.is_empty()
            && paths.iter().any(|p| git_dirs.iter().any(|g| p.starts_with(g)));
        let touches_program = d.program_in_paths(&paths);
        tracing::debug!(n_paths = paths.len(), program = touches_program,
            cfg = touches_cfg, git = touches_git, "watcher event");
        // A `.dl` program edit either respawns (cold path: re-parse from
        // scratch via the spawn-if-missing dance) or hot-reloads (tray path:
        // re-parse in place, swap the Mutex<Program>, re-tick).
        if touches_program {
            let names: Vec<String> = paths.iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
                .collect();
            if d.no_respawn {
                eprintln!("[daemon] program edit ({}) — reloading", names.join(", "));
                match d.reload_program() {
                    Ok(()) => {}
                    Err(e) => eprintln!("[daemon] reload failed, keeping old: {e}"),
                }
                continue;
            }
            eprintln!("[daemon] program file changed ({}); exiting for respawn", names.join(", "));
            d.shutdown_requested.store(true, Ordering::Relaxed);
            std::process::exit(0);
        }
        // Discovery mode: a new or removed .dl file under .dl/ triggers
        // re-discovery and re-merge (k8s-style add-a-file).
        if d.discovery_mode {
            let has_dl = paths.iter().any(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("dl")
                    && p.strip_prefix(&d.root)
                        .map(|r| r.starts_with(".dl"))
                        .unwrap_or(false)
            });
            if has_dl {
                eprintln!("[daemon] .dl discovery change — re-merging program");
                match d.reload_discovery() {
                    Ok(()) => {}
                    Err(e) => eprintln!("[daemon] discovery reload: {e}"),
                }
                continue;
            }
        }
        // A `.git` move (commit/checkout/pull/reset): broadcast the rev advance,
        // then re-analyze the worktree files the git diff says changed. The tick
        // is driven from the deterministic `files_changed_between` diff, not the
        // notify batch — FSEvents does not reliably co-deliver a checkout's
        // rewritten files with the `.git` event that accompanied them. Pure
        // metadata churn (no ref advance → empty diff) continues without a tick.
        if touches_git {
            let (n, changed) = d.on_git_event();
            eprintln!("[daemon] git change — {n} ref(s) advanced, {} worktree file(s)", changed.len());
            if changed.is_empty() { continue; }
            paths = changed;
        }
        let tick_label;
        let result = if touches_cfg {
            tick_label = "config change";
            let mut eng = d.eng.lock().unwrap();
            eng.set_repos(load_repos_eager());
            drop(eng);
            // Re-persist the repo set so `_repo` tracks a config edit.
            if let Err(e) = d.eng.lock().unwrap().save_repos_meta() {
                eprintln!("[daemon] save_repos_meta: {e}");
            }
            d.tick_full(false)
        } else if paths.is_empty() {
            tick_label = "empty event";
            d.tick_full(false)
        } else {
            tick_label = "source change";
            d.tick_paths(&paths, false)
        };
        let n_paths = paths.len();
        let tick_num = d.tick_count.load(Ordering::Relaxed);
        let tick_ok = result.is_ok();
        match result {
            Ok(()) => eprintln!("[daemon] tick #{tick_num} ({tick_label}, {n_paths} paths) ok"),
            Err(e) => eprintln!("[daemon] tick #{tick_num} ({tick_label}, {n_paths} paths) error: {e}"),
        }
        // A tick may have pulled new repos (a `repo`-sink drained). Watch any
        // root not yet watched so the next edit there is reactive. Git-dir
        // watching for pulled repos is deferred (commit reactivity follow-up).
        if tick_ok {
            for rc in d.eng.lock().unwrap().snapshot_repos() {
                if rc.root.exists() && watched.insert(rc.root.clone()) {
                    if watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
                        eprintln!("[daemon] watching (pulled) {} ({})", rc.slug, rc.root.display());
                    }
                }
            }
        }
    }
}

impl Daemon {
    fn program_in_paths(&self, paths: &[PathBuf]) -> bool {
        let pf = self.program_files.lock().unwrap();
        for p in paths {
            let canon = p.canonicalize().unwrap_or_else(|_| p.clone());
            if pf.iter().any(|f| f == &canon) {
                return true;
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
            "program_files": d.program_files.lock().unwrap().iter()
                .map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
        })),
        "query" => {
            let prog = d.prog.lock().unwrap();
            let eng = d.eng.lock().unwrap();
            match eng.run_queries_capture(&prog) {
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
        "schema" => {
            let eng = d.eng.lock().unwrap();
            let builtin = crate::engine::builtin_rel_names();
            let mut relations: Vec<Value> = Vec::new();
            for (name, meta) in eng.rels.iter() {
                let cols: Vec<Value> = meta.cols.iter().map(|c| json!({
                    "name": c.name, "ty": format!("{:?}", c.ty),
                })).collect();
                let mut rel = json!({"name": name, "columns": cols});
                if builtin.contains(name) {
                    rel["builtin"] = Value::Bool(true);
                }
                relations.push(rel);
            }
            // SQLite source tables that back the engine but aren't declared rels
            // (so they're absent from `eng.rels`); still queryable, so list them.
            // module_import/module_edge_rev/crate_edge are declared rels above —
            // do NOT re-list them here or they'd appear twice.
            let extra: &[(&str, &[(&str, &str)])] = &[
                ("_file", &[("repo", "text"), ("path", "text"), ("rev", "text"), ("hash", "text"), ("mtime", "int"), ("size", "int")]),
                ("_files", &[("id", "int"), ("content_hash", "text"), ("path", "text"), ("size", "int")]),
                ("_where_bytes", &[("id", "int"), ("repo", "text"), ("path", "text"), ("rev", "text"), ("byte", "int"), ("line", "int"), ("col", "int")]),
                ("_program", &[("path", "text"), ("hash", "text"), ("mtime", "int"), ("loaded_at", "int")]),
                ("_repo", &[("slug", "text"), ("root", "text"), ("url", "text"), ("registered_at", "int")]),
                ("_ref", &[("repo", "text"), ("name", "text"), ("oid", "text"), ("observed_at", "int")]),
                ("_rev_log", &[("id", "int"), ("repo", "text"), ("name", "text"), ("old", "text"), ("new", "text"), ("at", "int")]),
            ];
            for (name, cols) in extra {
                let cols_json: Vec<Value> = cols.iter().map(|(n, t)| json!({"name": n, "ty": t})).collect();
                relations.push(json!({"name": name, "columns": cols_json, "builtin": true}));
            }
            Response::ok(req.id, json!({"relations": relations}))
        }
        "query_sql" => {
            let sql_raw = match req.params.get("sql").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing sql"),
            };
            let params: Vec<Value> = req.params.get("params")
                .and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let eng = d.eng.lock().unwrap();
            match eng.query_sql(sql_raw, &params) {
                Ok(rows) => Response::ok(req.id, json!({"rows": rows})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "eval" => {
            let text = match req.params.get("text").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing text"),
            };
            match run_eval(d, text) {
                Ok(v) => Response::ok(req.id, v),
                Err((code, msg)) => Response::err(req.id, code, msg),
            }
        }
        "status" => {
            let eng = d.eng.lock().unwrap();
            let q = |sql: &str| eng.query_sql(sql, &[]).unwrap_or_default();
            Response::ok(req.id, json!({
                "root": d.root.to_string_lossy(),
                "program": d.program_display,
                "tick_count": d.tick_count.load(Ordering::Relaxed),
                "subscribers": d.subscribers.lock().unwrap().len(),
                "programs": q("SELECT path, hash, mtime, loaded_at FROM _program ORDER BY path"),
                "repos": q("SELECT slug, root, url, registered_at FROM _repo ORDER BY slug"),
                "refs": q("SELECT repo, name, oid, observed_at FROM _ref ORDER BY repo, name"),
                "advances": q("SELECT repo, name, old, new, at FROM _rev_log ORDER BY id DESC LIMIT 50"),
            }))
        }
        "shutdown" => Response::ok(req.id, json!({"ok": true})),
        "load" => {
            // Load a script into the running daemon. mode="once" evals it
            // ephemerally (throwaway engine, run_eval); mode="watched" joins it
            // to the loaded program (push program_files + reload_program) so its
            // rules run on every tick and hot-reload on edit.
            let path = match req.params.get("path").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => return Response::err(req.id, INVALID_PARAMS, "missing path"),
            };
            let mode = req.params.get("mode").and_then(|v| v.as_str()).unwrap_or("watched");
            match mode {
                "once" => {
                    let text = match std::fs::read_to_string(&path) {
                        Ok(t) => t,
                        Err(e) => return Response::err(req.id, INVALID_PARAMS, format!("read {path}: {e}")),
                    };
                    match run_eval(d, &text) {
                        Ok(v) => Response::ok(req.id, v),
                        Err((code, msg)) => Response::err(req.id, code, msg),
                    }
                }
                "watched" => {
                    let canon = match std::fs::canonicalize(&path) {
                        Ok(c) => c,
                        Err(e) => return Response::err(req.id, INVALID_PARAMS, format!("canonicalize {path}: {e}")),
                    };
                    let already = {
                        let mut pf = d.program_files.lock().unwrap();
                        let dup = pf.iter().any(|f| f == &canon);
                        if !dup { pf.push(canon.clone()); }
                        dup
                    };
                    // reload_program re-parses ALL program_files, swaps the
                    // Program, re-ticks. A parse/type error keeps the old
                    // program (returns Err) — the push remains but is inert
                    // until it parses clean.
                    match d.reload_program() {
                        Ok(()) => {
                            if let Err(e) = d.eng.lock().unwrap()
                                .save_program_meta(&d.program_files.lock().unwrap().clone()) {
                                eprintln!("[daemon] save_program_meta: {e}");
                            }
                            let files: Vec<String> = d.program_files.lock().unwrap().iter()
                                .map(|f| f.to_string_lossy().into_owned()).collect();
                            Response::ok(req.id, json!({
                                "loaded": canon.to_string_lossy(),
                                "already_loaded": already,
                                "program_files": files,
                            }))
                        }
                        Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("reload: {e}")),
                    }
                }
                other => Response::err(req.id, INVALID_PARAMS,
                    format!("mode must be watched|once, got {other}")),
            }
        }
        other => Response::err(req.id, METHOD_NOT_FOUND, format!("unknown method: {other}")),
    }
}

/// Evaluate a scratch `.dl` snippet without touching the live engine or db.
///
/// The snippet is parsed, spliced onto the loaded program (so it can reference
/// the program's relations), type-checked, then ticked on a THROWAWAY in-memory
/// engine that shares the daemon's root + repos. Only the snippet's own `?`
/// queries are captured. Nothing persists: scratch relations never appear in
/// `schema`, never hit the warm db. The cost is a cold tick per eval (the
/// merged program's source rules re-run on the fresh db).
fn run_eval(d: &Daemon, text: &str) -> Result<Value, (i64, String)> {
    let toks = crate::lex::lex(text).map_err(|e| (INVALID_PARAMS, format!("lex: {e}")))?;
    let snippet = crate::parse::parse(toks).map_err(|e| (INVALID_PARAMS, format!("parse: {e}")))?;
    // Capture only the snippet's queries, evaluated against the ticked tables.
    let snippet_queries: Vec<crate::ast::Item> = snippet
        .items
        .iter()
        .filter(|i| matches!(i, crate::ast::Item::Query(_)))
        .cloned()
        .collect();

    // Splice onto the loaded program so the snippet sees its relations.
    let mut merged = {
        let base = d.prog.lock().unwrap();
        Program {
            items: base.items.iter().cloned().chain(snippet.items).collect(),
        }
    };
    let diags = crate::typecheck::check_and_normalize(&mut merged, "<scratch>");
    let diag_json = |x: &crate::ast::TypeDiag| {
        json!({"severity": x.severity.as_str(), "code": x.code, "message": x.msg})
    };
    let type_errs: Vec<Value> = diags
        .iter()
        .filter(|x| x.severity == crate::ast::Severity::Error)
        .map(diag_json)
        .collect();
    if !type_errs.is_empty() {
        return Ok(json!({"ok": false, "results": [], "diagnostics": type_errs}));
    }

    let conn = db::open(None).map_err(|e| (INTERNAL_ERROR, format!("db: {e}")))?;
    let mut eng = Engine::new(conn, d.root.clone());
    eng.set_repos(load_repos_eager());
    eng.tick(&merged, true)
        .map_err(|e| (INTERNAL_ERROR, format!("tick: {e}")))?;

    let qprog = Program { items: snippet_queries };
    let results = eng
        .run_queries_capture(&qprog)
        .map_err(|e| (INTERNAL_ERROR, format!("query: {e}")))?;
    let rel_diags = eng.diags(None).unwrap_or_default();
    let all_diags: Vec<Value> = diags
        .iter()
        .map(diag_json)
        .chain(rel_diags.iter().map(diag_to_json))
        .collect();
    Ok(json!({
        "ok": true,
        "results": results.iter().map(|r| json!({
            "rel": r.rel, "columns": r.columns, "rows": r.rows,
        })).collect::<Vec<_>>(),
        "diagnostics": all_diags,
    }))
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

/// True iff a daemon is listening at this home (`Some(root)` → per-root,
/// `None` → the singleton XDG serving daemon).
pub fn is_running(root: Option<&Path>) -> bool {
    UnixStream::connect(socket_path(root)).is_ok()
}

/// Connect (must be already running). Returns a framed stream.
pub fn connect(root: Option<&Path>) -> Result<UnixStream> {
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
    if is_running(Some(root)) {
        // Ping to confirm it's our daemon and not a stale socket.
        let mut s = connect(Some(root))?;
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
        if let Ok(mut s) = UnixStream::connect(socket_path(Some(root))) {
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

/// Send `shutdown` to the daemon at this home (`Some(root)` → per-root,
/// `None` → the singleton XDG serving daemon). Ok if it acknowledged or was not
/// running. Used by `dl --stop` / `dl --stop --root X`.
pub fn stop(root: Option<&Path>) -> Result<()> {
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

/// Load a script into the daemon at this home. mode="watched" joins the program
/// (persistent, reactive, hot-reloaded on edit); mode="once" evals it
/// ephemerally on a throwaway engine and returns the query results.
/// `root=None` targets the global rootless serving daemon.
pub fn load(root: Option<&Path>, path: &str, mode: &str) -> Result<Response> {
    let mut s = connect(root)?;
    let req = Request::new(0, "load", json!({"path": path, "mode": mode}));
    rpc_call(&mut s, &req)
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
