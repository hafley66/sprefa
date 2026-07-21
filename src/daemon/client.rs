//! Client half: connection, lifecycle (start/stop/restart/supervise), and
//! the typed query wrappers CLI/LSP call (relocated from `daemon.rs`;
//! decomposition plan step 6).
use super::*;

const CONNECT_BACKOFF_MS: &[u64] = &[10, 20, 40, 80, 160, 320, 500];
const CONNECT_TOTAL_TIMEOUT_SECS: u64 = 5;

// ---------- client side ----------

/// True iff the daemon is enabled for this process (env opt-out check).
pub fn enabled() -> bool {
    std::env::var("DL_NO_DAEMON").ok().as_deref() != Some("1")
}

/// True iff the daemon should manage this root. A workspace opts INTO daemon
/// management by having a `.dl/` directory.
pub fn enabled_for(root: &Path) -> bool {
    enabled() && root.join(".dl").is_dir()
}

/// True iff the singleton daemon is listening.
pub fn is_running() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

/// Connect to the singleton (must be already running). Returns an HTTP-over-UDS
/// client (`crate::daemon_client`); the JSON-RPC envelopes are unchanged.
pub fn connect() -> Result<crate::daemon_client::DaemonClient> {
    crate::daemon_client::DaemonClient::connect_to(&socket_path())
}

/// Inject the `root` envelope key into a params object (no-op for `None`).
fn with_root(mut params: Value, root: Option<&Path>) -> Value {
    if let Some(r) = root {
        params["root"] = json!(r.to_string_lossy());
    }
    params
}

/// Send one request, wait for the response — but never block forever on a
/// wedged daemon. The wait runs inside `DaemonClient::call`, which heartbeats
/// every 10s with the daemon's current phase (from the same on-disk `why.jsonl`
/// trail `dl daemon why` reads) and gives up with exit 75 after
/// `DL_MAX_WALL_SECS` total (default 300; `0` disables the deadline).
pub fn rpc_call(client: &mut crate::daemon_client::DaemonClient, req: &Request) -> Result<Response> {
    client.call(req)
}

/// Ensure the singleton daemon is running (spawn detached if not). Attaches only
/// if the running daemon runs THIS binary (build_id match); otherwise replaces
/// the stale daemon. `root`/`program` are accepted for call-site compatibility;
/// the root registers lazily on its first RPC (attach IS registration).
pub fn ensure_daemon(_root: &Path, _program: Option<&str>) -> Result<()> {
    ensure_singleton()
}

/// Client autostart gate (failure-modes class 16, user ruling 2026-07-18):
/// implicit attach paths (`dl file.dl`, `dl --check`, mcp, lsp) never spawn a
/// server — they attach if one runs, otherwise fall back in-process. Only
/// `DL_AUTOSTART=1` (test harnesses) re-enables implicit spawning. Explicit
/// verbs (`dl daemon start`/`load`/`load-once`, `dl watch`) call
/// `start_singleton`, which always may spawn.
pub fn autostart_allowed() -> bool {
    matches!(std::env::var("DL_AUTOSTART").ok().as_deref(), Some("1") | Some("true"))
}

/// Attach-or-spawn for implicit call sites; spawning gated by `autostart_allowed`.
pub fn ensure_singleton() -> Result<()> {
    ensure_singleton_inner(false)
}

/// Attach-or-spawn for EXPLICIT user commands (`dl daemon start`, `dl watch`).
pub fn start_singleton() -> Result<()> {
    ensure_singleton_inner(true)
}

fn ensure_singleton_inner(explicit: bool) -> Result<()> {
    let pid = std::process::id();
    let ppid = std::os::unix::process::parent_id();
    tracing::debug!(pid, ppid, explicit,
        autostart_env = ?std::env::var("DL_AUTOSTART").ok(),
        "[daemon] ensure_singleton: enter");
    let running = is_running();
    tracing::debug!(pid, running, "[daemon] ensure_singleton: liveness probe");
    if running {
        let mut s = connect()?;
        let req = Request::new(0, "ping", json!({}));
        match rpc_call(&mut s, &req) {
            Ok(r) if r.error.is_none() => {
                let running_id = r.result.as_ref()
                    .and_then(|v| v.get("build_id"))
                    .and_then(|v| v.as_str());
                tracing::debug!(pid,
                    running_build = ?running_id, self_build = build_id(),
                    "[daemon] ensure_singleton: ping ok");
                match running_id {
                    Some(id) if id == build_id() => return Ok(()),
                    Some(stale) => {
                        tracing::warn!(pid, running_build = %stale, self_build = build_id(),
                            "[daemon] running binary changed — restarting daemon");
                        let _ = stop();
                    }
                    None => return Ok(()),
                }
            }
            Ok(r) => tracing::warn!(pid, error = ?r.error,
                "[daemon] ensure_singleton: ping returned rpc error"),
            Err(e) => tracing::warn!(pid, error = %e,
                "[daemon] ensure_singleton: ping failed on live socket"),
        }
    }
    if !explicit && !autostart_allowed() {
        // The class-16 stop order made structural: a kill stays a kill. No
        // implicit client resurrects the daemon behind the user's back.
        tracing::warn!(pid, ppid,
            "[daemon] no live singleton and autostart is disabled — refusing to spawn \
             (start one: `dl daemon start`; tests: DL_AUTOSTART=1)");
        anyhow::bail!(
            "no daemon running and autostart is disabled — start one with `dl daemon start`");
    }
    // The respawn-storm event: this client found no live daemon and is about
    // to spawn one. A one-shot `dl --check` autostarting a daemon, killed
    // externally, then re-autostarting on the NEXT invocation — repeatedly —
    // is exactly the incident this warning is for; it lands in the CALLING
    // process's own dl.log/error.log (via `crate::trace::init`), not the new
    // daemon's, since the daemon doesn't exist yet to log it.
    tracing::warn!(pid, ppid, explicit, "[daemon] no live singleton found — spawning one");
    let spawn_started = std::time::Instant::now();
    spawn_detached()?;
    tracing::debug!(pid, ms = spawn_started.elapsed().as_millis() as u64,
        "[daemon] ensure_singleton: spawned detached, waiting ready");
    let ready = wait_ready();
    tracing::debug!(pid, ok = ready.is_ok(),
        ms = spawn_started.elapsed().as_millis() as u64,
        "[daemon] ensure_singleton: wait_ready done");
    ready
}

/// Stop the singleton and respawn it detached with the CURRENT binary. The
/// `dl daemon restart` backend for the un-supervised (fallback) path — plan
/// section 3.4: no service manager installed, CI, `cargo test`.
pub fn restart() -> Result<()> {
    let was_running = is_running();
    if was_running { let _ = stop(); }
    spawn_detached()?;
    let ready = wait_ready().is_ok();
    eprintln!("[daemon] {} (build {}){}",
        if was_running { "restarted" } else { "started" },
        build_id(),
        if ready { "" } else { " — starting (first tick still in progress)" }); // @eprintln-ok: human-facing status report for dl daemon restart
    Ok(())
}

/// Attach-or-spawn via the OS service manager, for `dl daemon start` on the
/// real (non-sandboxed) default home once `dl daemon install` has registered
/// it. Mirrors `ensure_singleton_inner`'s ping-then-spawn shape, but the
/// "spawn" step is `crate::supervise::start`/`restart` (launchctl kickstart /
/// systemctl start) instead of `spawn_detached` — plan section 3.5.2: "become
/// thin wrappers with raw launchctl/systemctl... fallbacks", this is that
/// wrapper for the CLI's explicit-start path.
pub fn start_singleton_supervised() -> Result<()> {
    if is_running() {
        let mut s = connect()?;
        let req = Request::new(0, "ping", json!({}));
        if let Ok(r) = rpc_call(&mut s, &req) {
            if r.error.is_none() {
                let running_id = r.result.as_ref()
                    .and_then(|v| v.get("build_id"))
                    .and_then(|v| v.as_str());
                match running_id {
                    Some(id) if id == build_id() => return Ok(()),
                    Some(stale) => {
                        tracing::warn!(running_build = %stale, self_build = build_id(),
                            "[daemon] supervised singleton binary changed — restarting via the service manager");
                        crate::supervise::restart()?;
                        return wait_ready();
                    }
                    None => return Ok(()),
                }
            }
        }
    }
    crate::supervise::start()?;
    wait_ready()
}

/// `dl daemon restart` under supervision: same UX contract as `restart()`
/// (identical status line) with `crate::supervise::restart` (`kickstart -k`)
/// as the respawn mechanism instead of `stop()` + `spawn_detached()`.
pub fn restart_supervised() -> Result<()> {
    let was_running = is_running();
    crate::supervise::restart()?;
    let ready = wait_ready().is_ok();
    eprintln!("[daemon] {} (build {}){}",
        if was_running { "restarted" } else { "started" },
        build_id(),
        if ready { "" } else { " — starting (first tick still in progress)" }); // @eprintln-ok: human-facing status report for dl daemon restart
    Ok(())
}

/// Spawn the singleton daemon detached (background, idle timeout on).
pub fn spawn_detached() -> Result<()> {
    let exe = std::env::current_exe()
        .context("locate current exe for daemon spawn")?;
    let home = daemon_home();
    std::fs::create_dir_all(&home)?;
    // Same path `daemon::logcap::sweep` checks on every idle tick once the
    // child is running; this spawn-time check just means a fresh child never
    // starts by appending onto an already-oversized file from a prior run.
    let log = daemon_log_path(&home);
    let oversized = std::fs::metadata(&log)
        .map(|m| m.len() > crate::daemon::logcap::EXTERNAL_LOG_CAP_BYTES)
        .unwrap_or(false);
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(!oversized)
        .write(oversized)
        .truncate(oversized)
        .open(&log)?;
    let stderr = log_file.try_clone()?;
    let mut cmd = std::process::Command::new(exe);
    // The detached child runs the singleton in the background with the idle timer
    // on (`serve` = run_daemon(foreground=false)).
    cmd.args(["daemon", "serve"]);
    // The CLI dispatch path initializes the global rayon pool before the daemon
    // module runs, so seed DL_RAYON_THREADS with the daemon's budget so the child
    // process's pool is sized correctly. DL_DAEMON_THREADS wins if present.
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let n = daemon_thread_count(
        cores,
        std::env::var("DL_DAEMON_THREADS").ok().as_deref(),
    );
    cmd.env("DL_RAYON_THREADS", n.to_string());
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(stderr));
    cmd.spawn().context("spawn daemon")?;
    Ok(())
}

pub(crate) fn wait_ready() -> Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(CONNECT_TOTAL_TIMEOUT_SECS);
    let mut backoff_idx = 0;
    loop {
        if start.elapsed() > timeout {
            bail!("daemon did not become ready in {}s", CONNECT_TOTAL_TIMEOUT_SECS);
        }
        if let Ok(mut s) = connect() {
            let req = Request::new(0, "ping", json!({}));
            if let Ok(resp) = rpc_call(&mut s, &req) {
                if resp.error.is_none() { return Ok(()); }
            }
        }
        let delay_ms = CONNECT_BACKOFF_MS.get(backoff_idx)
            .copied()
            .unwrap_or(CONNECT_BACKOFF_MS.last().copied().unwrap_or(500));
        backoff_idx += 1;
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

/// Send `shutdown` to the singleton. Ok if it acknowledged or was not running.
pub fn stop() -> Result<()> {
    if !is_running() {
        let _ = std::fs::remove_file(socket_path());
        let _ = std::fs::remove_file(pid_path());
        return Ok(());
    }
    let mut s = connect()?;
    let req = Request::new(1, "shutdown", json!({}));
    let resp = rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error {
        bail!("daemon shutdown refused: {}", e.message);
    }
    for _ in 0..50 {
        if !is_running() { return Ok(()); }
        std::thread::sleep(Duration::from_millis(20));
    }
    bail!("daemon did not close socket after shutdown")
}

/// Deregister one root from the running singleton (`dl daemon drop`). `purge`
/// deletes its db dir.
pub fn drop_root(root: &Path, purge: bool) -> Result<()> {
    if !is_running() {
        // Nothing serving; scrub the persisted record + optionally the db.
        let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let key = key_of(&canon);
        let mut records = read_roots_json();
        records.retain(|r| r.key != key);
        write_roots_json(&records);
        if purge { let _ = std::fs::remove_dir_all(root_db_dir(&key)); }
        return Ok(());
    }
    let mut s = connect()?;
    let params = with_root(json!({"purge": purge}), Some(root));
    let req = Request::new(1, "drop_root", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error { bail!("drop_root: {}", e.message); }
    Ok(())
}

/// Block until the given root reports quiescent, or `timeout_ms` elapses. The
/// client's watched wait (10s heartbeats, `DL_MAX_WALL_SECS` cap) covers the
/// long server-side hold.
pub fn await_quiescent(root: Option<&Path>, timeout_ms: u64) -> Result<(bool, u64)> {
    let mut s = connect()?;
    let params = with_root(json!({"timeout_ms": timeout_ms}), root);
    let req = Request::new(0, "await_quiescent", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error {
        bail!("await_quiescent failed: {}", e.message);
    }
    let r = resp.result.unwrap_or(json!({}));
    Ok((r.get("settled").and_then(|v| v.as_bool()).unwrap_or(false),
        r.get("tick_count").and_then(|v| v.as_u64()).unwrap_or(0)))
}

/// Load a script into the given root. mode="watched" joins the program;
/// mode="once" evals it ephemerally.
pub fn load(root: Option<&Path>, path: &str, mode: &str) -> Result<Response> {
    let mut s = connect()?;
    let params = with_root(json!({"path": path, "mode": mode}), root);
    let req = Request::new(0, "load", params);
    rpc_call(&mut s, &req)
}

/// Fetch relation `rel`'s current rows from the given root.
pub fn query_rel(root: Option<&Path>, rel: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut s = connect()?;
    let params = with_root(json!({"rel": rel}), root);
    let req = Request::new(0, "query_rel", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(err) = resp.error {
        anyhow::bail!("{}", err.message);
    }
    let result = resp.result.unwrap_or_default();
    let cols: Vec<String> = result.get("columns").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let rows: Vec<Vec<String>> = result.get("rows").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|r| r.as_array().map(|cells| {
            cells.iter().map(|c| match c {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            }).collect()
        })).collect())
        .unwrap_or_default();
    Ok((cols, rows))
}

/// One `dl what` / `dl summary` answer from the daemon.
pub struct QueryAnswer {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total: usize,
    pub notes: Vec<String>,
}

fn decode_query_answer(result: &Value) -> QueryAnswer {
    let strs = |v: Option<&Value>| -> Vec<String> {
        v.and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    let rows: Vec<Vec<String>> = result.get("rows").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|r| r.as_array().map(|cells| {
            cells.iter().map(|c| match c {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            }).collect()
        })).collect())
        .unwrap_or_default();
    QueryAnswer {
        columns: strs(result.get("columns")),
        rows,
        total: result.get("total").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
        notes: strs(result.get("notes")),
    }
}

/// `dl what <anchor>` against the daemon.
pub fn what(root: Option<&Path>, anchor: &str, limit: Option<usize>, offset: Option<usize>)
    -> Result<QueryAnswer>
{
    let mut s = connect()?;
    let mut params = with_root(json!({"anchor": anchor}), root);
    if let Some(l) = limit { params["limit"] = json!(l); }
    if let Some(o) = offset { params["offset"] = json!(o); }
    let req = Request::new(0, "what", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(err) = resp.error { anyhow::bail!("{}", err.message); }
    Ok(decode_query_answer(&resp.result.unwrap_or_default()))
}

/// `dl summary <path>` against the daemon.
pub fn summary(root: Option<&Path>, path: &str) -> Result<QueryAnswer> {
    let mut s = connect()?;
    let params = with_root(json!({"path": path}), root);
    let req = Request::new(0, "summary", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(err) = resp.error { anyhow::bail!("{}", err.message); }
    Ok(decode_query_answer(&resp.result.unwrap_or_default()))
}

/// `dl q <verb> <target>` against the daemon.
pub fn q(root: Option<&Path>, verb: &str, target: &str, limit: Option<usize>, offset: Option<usize>)
    -> Result<QueryAnswer>
{
    let mut s = connect()?;
    let mut params = with_root(json!({"verb": verb, "target": target}), root);
    if let Some(l) = limit { params["limit"] = json!(l); }
    if let Some(o) = offset { params["offset"] = json!(o); }
    let req = Request::new(0, "q", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(err) = resp.error { anyhow::bail!("{}", err.message); }
    Ok(decode_query_answer(&resp.result.unwrap_or_default()))
}

/// Ping the running daemon for the given root and return its status JSON.
/// `Ok(None)` when no daemon answers. The `dl daemon status` backend when a root
/// is given; pass `None` for the process summary.
pub fn status(root: Option<&Path>) -> Result<Option<Value>> {
    let mut s = match connect() {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let params = with_root(json!({}), root);
    let req = Request::new(0, "ping", params);
    match rpc_call(&mut s, &req) {
        Ok(r) if r.error.is_none() => Ok(r.result),
        _ => Ok(None),
    }
}

/// List the running daemon's job table (newest first). The `dl daemon jobs`
/// backend; process-wide, so no root envelope.
pub fn jobs() -> Result<Vec<Value>> {
    let mut s = connect()?;
    let req = Request::new(0, "jobs", json!({}));
    let resp = rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error { bail!("jobs: {}", e.message); }
    Ok(resp.result
        .and_then(|v| v.get("jobs").and_then(|j| j.as_array()).cloned())
        .unwrap_or_default())
}

/// Register a root with the running singleton (spawning it first if needed), and
/// wait for the cold tick. Returns the served root's tick count.
pub fn add_root(root: &Path) -> Result<()> {
    ensure_singleton()?;
    let mut s = connect()?;
    let params = with_root(json!({}), Some(root));
    let req = Request::new(0, "add_root", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error { bail!("add_root: {}", e.message); }
    Ok(())
}
