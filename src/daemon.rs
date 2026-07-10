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
//!   - `dl daemon start`  foreground daemon (logs to stderr, ignores idle timeout)
//!   - `dl daemon stop`    sends `shutdown` over the socket
//!   - default invocation auto-attaches; spawns if no socket / dead PID
//!   - `DL_NO_DAEMON=1` opts out (in-process, the pre-daemon path; used by tests)
//!   - `DL_DAEMON_IDLE_SECS=N` overrides the 30 min default

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};

use crate::ast::Program;
use crate::engine::{DiagRow, Engine};
use crate::rpc::{self, Request, Response, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND};
use crate::watchgate::WatchGate;
use crate::{config, db};

const DEFAULT_IDLE_SECS: u64 = 30 * 60;
const IDLE_TICK_SECS: u64 = 30;
/// Default effect-drain cadence when `DL_POLL_SECS` is unset. A program with
/// `@async`/`@stream` effects needs SOME poll to drain its queue off-tick; making
/// this a default (rather than opt-in) is what stops effects silently sitting at
/// `state='queued'` under a bare `dl daemon start`. The poll loop no-ops cheaply when
/// the loaded program has no effects, so a non-effect daemon pays ~nothing.
const DEFAULT_POLL_SECS: u64 = 2;

/// Lock a daemon mutex, recovering the guard if a prior holder panicked. One
/// connection thread panicking mid-critical-section should not brick every other
/// thread on `.unwrap()`; the guarded state tolerates it — a `Program` swap is an
/// atomic replace and `Engine` mutations sit behind SQLite transactions that roll
/// back on unwind. Centralizing the lock here also keeps `daemon.rs` under the
/// unwrap budget (the poison policy lives in one place, not 48 call sites).
#[inline]
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
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

/// macOS caps `sockaddr_un.sun_path` at 104 bytes; Linux at 108. A deep project
/// root's `<root>/.dl/daemon.sock` overruns it, so `UnixListener::bind` fails and
/// the daemon silently dies (every invocation then falls back to in-process
/// after the attach timeout). Below this length we keep the natural root-local
/// path (discoverable, self-cleaning); at or above it, `socket_path` relocates
/// only the socket to a short hashed path. Leave slack for the trailing NUL.
const SUN_MAX: usize = 100;

/// `<home>/daemon.sock`, unless that path is too long for `sun_path` — then a
/// short, deterministic path under a short base dir keyed by the root's hash.
/// Both bind and every connect derive it from the same `root`, so they always
/// agree. Only the socket moves; the pid/log/cache files stay root-local.
pub fn socket_path(root: Option<&Path>) -> PathBuf {
    let natural = home_dir(root).join("daemon.sock");
    if natural.as_os_str().len() < SUN_MAX {
        natural
    } else {
        short_socket_path(root)
    }
}

/// A short, collision-free socket path for a root whose natural socket path is
/// too long for `sun_path`. Keyed by the blake3 hash of the canonicalized root
/// (lexical fallback if canonicalize fails — bind and connect both see the same
/// pre-canonicalize string, so they still agree). `<tmp>/dl-sock/<16hex>.sock`
/// is ~30 bytes, well under the cap.
fn short_socket_path(root: Option<&Path>) -> PathBuf {
    let key = match root {
        Some(r) => {
            let canon = r.canonicalize().unwrap_or_else(|_| r.to_path_buf());
            let hash = blake3::hash(canon.as_os_str().as_encoded_bytes());
            hash.to_hex()[..16].to_string()
        }
        // Rootless singleton: hash its (short) home so it stays deterministic.
        None => {
            let hash = blake3::hash(daemon_home().as_os_str().as_encoded_bytes());
            hash.to_hex()[..16].to_string()
        }
    };
    short_sock_dir().join(format!("{key}.sock"))
}

/// Short base directory for relocated sockets. `TMPDIR`-derived so it stays on a
/// short path; created 0700 on first use.
fn short_sock_dir() -> PathBuf {
    let base = std::env::temp_dir().join("dl-sock");
    if let Err(e) = std::fs::create_dir_all(&base) {
        // Best-effort; bind will surface the real error if the dir is unusable.
        tracing::debug!("short_sock_dir create {}: {e}", base.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700));
    }
    base
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
    /// Identity of the binary this daemon is RUNNING, captured at startup
    /// (`env!("CARGO_PKG_VERSION")` + the exe's mtime). A client that rebuilt
    /// or reinstalled `dl` computes a different id from the on-disk exe and
    /// respawns instead of attaching to a stale daemon. See `build_id`.
    pub build_id: String,
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
    /// Whether the last FULL tick left the program quiescent — no non-timer rel
    /// moved, no `@next` carry staged, no non-stream effect in-flight (the
    /// `TickReport::is_settled` predicate). The poll loop keeps ticking + draining
    /// toward this; `await_quiescent` blocks a client until it flips true. An
    /// incremental (source-change) tick clears it. Starts false: not-yet-known
    /// until the first full tick runs.
    pub settled: AtomicBool,
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
        *lock(&self.last_activity) = Instant::now();
    }

    fn tick_full(&self, quiet: bool) -> Result<()> {
        let tick_next = self.tick_count.load(Ordering::Relaxed) + 1;
        crate::activity::begin_tick(tick_next, &self.program_display);
        let prog = lock(&self.prog);
        let mut eng = lock(&self.eng);
        let report = eng.tick_report(&prog, quiet)?;
        drop(eng);
        drop(prog);
        crate::activity::end_tick();
        // A full tick knows everything that moved, so it is the authoritative
        // settle observation (`await_quiescent` reads this).
        self.settled.store(report.is_settled(), Ordering::Relaxed);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        // Full tick: changed paths unknown, subscribers re-publish everything.
        *lock(&self.last_changed_paths) = Vec::new();
        self.broadcast_diag_changed();
        Ok(())
    }

    fn tick_paths(&self, paths: &[PathBuf], quiet: bool) -> Result<()> {
        let tick_next = self.tick_count.load(Ordering::Relaxed) + 1;
        crate::activity::begin_tick(tick_next, &self.program_display);
        crate::activity::set(
            crate::activity::Phase::Reconcile,
            format!("{} changed path(s)", paths.len()),
        );
        let prog = lock(&self.prog);
        let mut eng = lock(&self.eng);
        eng.tick_paths(&prog, paths, quiet)?;
        drop(eng);
        drop(prog);
        crate::activity::end_tick();
        // A source change happened; the poll loop's next full tick + effect drain
        // decides whether we are quiescent again, so clear the flag now.
        self.settled.store(false, Ordering::Relaxed);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        // Incremental tick: only these paths' rows could have moved; subscribers
        // can re-publish just these files.
        *lock(&self.last_changed_paths) = paths.to_vec();
        self.broadcast_diag_changed();
        Ok(())
    }

    /// Re-parse the program files, swap the parsed `Program`, re-tick. Called
    /// by the watcher when `no_respawn` is set and a program file changed.
    /// A parse or type error keeps the last good program.
    fn reload_program(&self) -> Result<()> {
        let all = lock(&self.program_files).clone();
        // Parse only the files that currently exist: a deleted watched path must
        // not wedge the tick loop (prepare_paths would fail to read it). The
        // watched SET is left intact — an atomic save (delete+create) or a
        // re-created file reloads on its next event. If every file is (this
        // instant) missing, keep the last-good program rather than parsing an
        // empty set (which has no root path).
        let files: Vec<PathBuf> = all.iter().filter(|f| f.exists()).cloned().collect();
        if files.is_empty() {
            if !all.is_empty() {
                eprintln!("[daemon] all {} watched program file(s) missing; keeping last-good program", all.len());
            }
            return Ok(());
        }
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
            let mut p = lock(&self.prog);
            *p = new_prog;
        }
        if let Err(e) = lock(&self.eng).save_program_meta(&all) {
            eprintln!("[daemon] save_program_meta: {e}");
        }
        self.tick_full(false)?;
        Ok(())
    }

    /// Re-discover `.dl` files under `<root>/.dl/`, re-merge the program
    /// if the file set changed, re-tick. Called by the watcher when a new
    /// `.dl` file appears in discovery mode (k8s-style add-a-file), and also
    /// as the first probe on a discovered-program content edit (see the
    /// `touches_program` branch in `watcher_loop`). Returns `Ok(true)` when
    /// the file set changed and the program was re-merged, `Ok(false)` when
    /// the set is unchanged (a content edit to an already-discovered file, or
    /// not in discovery mode at all) — the caller tells those two apart.
    fn reload_discovery(&self) -> Result<bool> {
        if !self.discovery_mode {
            return Ok(false);
        }
        let files = crate::resolve_programs(&[], &self.root)?;
        let mut canon: Vec<PathBuf> = files
            .iter()
            .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
            .collect();
        canon.sort();
        {
            let pf = lock(&self.program_files);
            if canon == *pf {
                return Ok(false);
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
            return Ok(false);
        }
        crate::render_type_diags_eprintln(&type_diags);
        {
            let mut pf = lock(&self.program_files);
            *pf = canon;
        }
        {
            let mut p = lock(&self.prog);
            *p = new_prog;
        }
        {
            let pf = lock(&self.program_files).clone();
            if let Err(e) = lock(&self.eng).save_program_meta(&pf) {
                eprintln!("[daemon] save_program_meta: {e}");
            }
        }
        let n = lock(&self.program_files).len();
        eprintln!("[daemon] discovery reload: {n} file(s)");
        self.tick_full(false)?;
        Ok(true)
    }

    /// One poll cycle (the clock source for `@async`): advance the tick (which
    /// re-emits steady `@async` requests, idempotent), then drain every
    /// outstanding `pending_effect` through a `ShellEffectExec` built from the
    /// program's `effect_cmd(kind, template)` rows. On any drained response,
    /// re-tick so the new facts propagate and notify subscribers. Returns the
    /// number of effects drained. Kinds with no `effect_cmd` template stay queued.
    fn poll_tick(&self) -> Result<usize> {
        // Advance the clock + re-stage requests. tick_full takes its own locks,
        // so hold none here.
        self.tick_full(true)?;
        // Drain network/mutating sinks (repo pulls + checkout sweeps) on the
        // daemon's cadence. `tick_full` is now a pure read; this is the only
        // place the daemon fires those rows + registers newly-pulled repos.
        // (A bare `?` query path goes through `query` RPC and never calls this.)
        let sinks_drained = {
            let prog = lock(&self.prog);
            let mut eng = lock(&self.eng);
            crate::activity::set(crate::activity::Phase::Effects, "external sinks");
            eng.drain_external_sinks(&prog).unwrap_or_else(|e| {
                eprintln!("[daemon] drain_external_sinks: {e}");
                0
            })
        };
        let arity = {
            let prog = lock(&self.prog);
            crate::engine::async_effect_arity(&prog)
        };
        if arity.is_empty() { return Ok(sinks_drained); }
        let (templates, cwd) = {
            // Typed `sh` decls supply the base registry; the dynamic
            // `effect_cmd(kind, template)` relation overlays them (a data-driven
            // fallback for templates not declared at parse time).
            let mut m = {
                let prog = lock(&self.prog);
                crate::engine::shell_templates(&prog)
            };
            let eng = lock(&self.eng);
            if let Ok(rows) = eng.query_sql("SELECT kind, template FROM rel_effect_cmd", &[]) {
                for row in rows {
                    if let (Some(k), Some(t)) = (row.first().and_then(|v| v.as_str()),
                                                 row.get(1).and_then(|v| v.as_str())) {
                        m.insert(k.to_string(), t.to_string());
                    }
                }
            }
            (m, eng.root())
        };
        let exec = crate::engine::ShellEffectExec { templates, n_out: arity, cwd };
        let n = {
            let prog = lock(&self.prog);
            let mut eng = lock(&self.eng);
            // One-shot @async requests, then long-lived @stream subscriptions: a
            // stream yields N head rows per drain and stays 'running'.
            crate::activity::set(crate::activity::Phase::Effects, "drain");
            let a = eng.drain_effects(&prog, &exec)?;
            let s = eng.drain_streams(&prog, &exec)?;
            a + s
        };
        let n = n + sinks_drained;
        self.touch();
        if n > 0 {
            // Responses landed in their rel; re-tick so downstream derived rules
            // see them, then republish.
            self.tick_full(true)?;
            self.broadcast_diag_changed();
        }
        Ok(n)
    }

    fn broadcast_diag_changed(&self) {
        // Snapshot paths under the lock so write_frame (which can take time on a
        // slow consumer) doesn't hold it.
        let paths: Vec<String> = lock(&self.last_changed_paths).iter()
            .map(|p| p.to_string_lossy().into_owned()).collect();
        let note = json!({"jsonrpc": "2.0", "method": "diag_changed", "params": {
            "tick": self.tick_count.load(Ordering::Relaxed),
            "paths": paths,
        }});
        let body = match serde_json::to_string(&note) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut subs = lock(&self.subscribers);
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
        let prog = lock(&self.prog);
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
            let eng = lock(&self.eng);
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
        let mut subs = lock(&self.subscribers);
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
        crate::resolve_programs(&[], &eng_root).unwrap_or_default()
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
    if !repos.is_empty() { eprintln!("[config] {} repo(s) registered", repos.len()); }
    eng.set_repos(repos.clone());
    crate::activity::set(crate::activity::Phase::ColdTick, display.as_str());
    // Ensure `.dl/` exists before the cold tick so the perf log (default path
    // `<root>/.dl/perf.jsonl`) is writable from tick 1 — perflog never creates
    // `.dl/` itself (it must not side-effect a fresh dir), so the daemon does.
    let _ = std::fs::create_dir_all(home_dir(home.as_deref()));
    eng.tick(&prog, false)?;
    crate::activity::end_tick();
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
        build_id: build_id(),
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
        settled: AtomicBool::new(false),
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
    if let Some(dir) = sock.parent() { std::fs::create_dir_all(dir)?; }
    let listener = UnixListener::bind(&sock)?;
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    write_pid_file(home.as_deref(), programs.first().map(|s| s.as_str()))?;
    eprintln!("[daemon] listening on {} (pid {}, idle {}s)",
        sock.display(), std::process::id(), idle_secs);
    if let Some(p) = crate::perflog::path() {
        eprintln!("[daemon] perf log {} (tail -f | jq .total_ms, .phase, .ms)", p.display());
    }

    let d = daemon.clone();
    std::thread::Builder::new().name("dl-watch".into())
        .spawn(move || watcher_loop(d))?;

    if !foreground {
        let d = daemon.clone();
        std::thread::Builder::new().name("dl-idle".into())
            .spawn(move || idle_loop(d, idle_secs))?;
    }

    // @async clock: poll-drive the effect drain when DL_POLL_SECS is set.
    if let Some(secs) = poll_interval_secs() {
        let d = daemon.clone();
        std::thread::Builder::new().name("dl-poll".into())
            .spawn(move || poll_loop(d, secs))?;
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
    // The receive-side gate: mirrors the scan corpus's include decision so the
    // watcher only ticks for files the engine could scan. Built over the self
    // root + every config repo; grows via `add_root` when a tick pulls a repo.
    let mut gate = WatchGate::new(std::slice::from_ref(&d.root));
    // Count of active watch registrations (FSEvents streams on macOS; the root
    // watch above already succeeded, so start at 1). This is the number the
    // watcher itself installs — on macOS one per subtree, so it IS the watch
    // count; on Linux notify expands each Recursive registration to one inotify
    // watch per subdirectory internally, so the real kernel count is higher (see
    // /proc/<pid>/fdinfo). Logged whenever it changes so a pull is visible.
    let mut watch_count: usize = 1;
    // Watch every folder in view (config repos), so a rootless serving daemon
    // reacts to source edits across the whole view, not just its own root.
    for rc in load_repos_eager() {
        if rc.root.exists() && rc.root != d.root
            && watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
            watch_count += 1;
            gate.add_root(&rc.root);
            eprintln!("[daemon] watching repo {} ({})", rc.slug, rc.root.display());
        }
    }
    let cfg_path = config::SprfConfig::config_path()
        .and_then(|p| p.canonicalize().ok().or(Some(p)));
    if let Some(cp) = &cfg_path {
        if let Some(dir) = cp.parent() {
            if dir.exists() && watcher.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                watch_count += 1;
                eprintln!("[daemon] watching config {}", cp.display());
            }
        }
    }
    // Narrow-watch the `.git` dir of the self root and every config repo, so a
    // HEAD move (commit, checkout, pull) is observed even when the program only
    // scans WORK — but WITHOUT the `.git/objects` churn a `git status`/checkout
    // produces. The gate keeps only `HEAD`/`packed-refs`/`refs/` under a git dir
    // and routes them to `on_git_event` (rev-cursor diff + broadcast, no re-tick).
    watch_count += watch_git_narrow(&mut watcher, &mut gate, &d.root);
    for rc in load_repos_eager() {
        if rc.root.exists() { watch_count += watch_git_narrow(&mut watcher, &mut gate, &rc.root); }
    }

    eprintln!("[daemon] watcher ready ({}) — {watch_count} watch(es)", d.root.display());
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
        // Quiet-period debounce: coalesce a burst (an editor's multi-file save,
        // a `git checkout`'s rewrite stream) into one tick. Drain until QUIET ms
        // of silence, capped at MAX_WINDOW so sustained churn (a running build)
        // still ticks within a bounded latency. A `notify::Error` or a rescan
        // signal in the stream forces a full-corpus recovery tick (Phase 4).
        const QUIET: Duration = Duration::from_millis(120);
        const MAX_WINDOW: Duration = Duration::from_millis(600);
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut rescan = false;
        match first {
            Ok(ev) => {
                if ev.need_rescan() { rescan = true; } else { paths.extend(ev.paths); }
            }
            Err(e) => {
                eprintln!("[daemon] watch error, forcing full tick: {e}");
                rescan = true;
            }
        }
        let window_start = Instant::now();
        loop {
            match rx.recv_timeout(QUIET) {
                Ok(Ok(ev)) => {
                    if ev.need_rescan() { rescan = true; } else { paths.extend(ev.paths); }
                    if window_start.elapsed() > MAX_WINDOW { break; }
                }
                Ok(Err(e)) => {
                    eprintln!("[daemon] watch error, forcing full tick: {e}");
                    rescan = true;
                    if window_start.elapsed() > MAX_WINDOW { break; }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,      // burst settled
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return, // watcher gone
            }
        }
        // Dropped/overflowed events: recover with a whole-corpus tick, loudly.
        if rescan {
            let tick_num = d.tick_count.load(Ordering::Relaxed);
            match d.tick_full(true) {
                Ok(()) => eprintln!("[daemon] tick #{tick_num} (rescan recovery) ok"),
                Err(e) => eprintln!("[daemon] tick #{tick_num} (rescan recovery) error: {e}"),
            }
            d.touch();
            continue;
        }
        // Gate the batch to files the engine could scan (mirrors the scan
        // corpus: gitignore-honored, `.git` pruned to the narrow ref paths, the
        // daemon's own `.dl/cache.db*`/`daemon.*` bookkeeping dropped). Pure
        // noise (all-gitignored, all-`.git/objects`) yields an empty set: no
        // tick, and (Phase 5) no `touch`, so the daemon can idle out.
        let touches_git = gate.touches_git(&paths);
        let paths: Vec<PathBuf> = gate.filter(paths);
        if paths.is_empty() && !touches_git {
            continue;  // noise only: no activity, no tick, idle timer untouched
        }
        d.touch();  // real activity: reset the idle timer
        let touches_cfg = cfg_path.as_ref().is_some_and(|c|
            paths.iter().any(|p| p.canonicalize().ok().as_deref() == Some(c) || p == c));
        let mut paths = paths;
        let touches_program = d.program_in_paths(&paths);
        tracing::debug!(n_paths = paths.len(), program = touches_program,
            cfg = touches_cfg, git = touches_git, "watcher event");
        // A `.dl` program edit either respawns (cold path: re-parse from
        // scratch via the spawn-if-missing dance) or hot-reloads (tray path
        // and discovery-mode path: re-parse in place, swap the Mutex<Program>,
        // re-tick). A discovery daemon has no startup positional args to
        // respawn FROM — exiting would strand it until some other `dl`
        // client happens to run — so it always hot-reloads a content edit to
        // an already-discovered file (e.g. the VS Code panel appending a mark
        // fact to `.dl/marks.dl`).
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
            if d.discovery_mode {
                // The file set can't have changed (touches_program only fires
                // for paths already in program_files), so this is a content
                // edit: reload_discovery's early-return confirms the set is
                // unchanged, then reload_program re-parses the edited text.
                match d.reload_discovery() {
                    Ok(false) => {
                        eprintln!("[daemon] program edit ({}) — reloading (discovery)", names.join(", "));
                        match d.reload_program() {
                            Ok(()) => {}
                            Err(e) => eprintln!("[daemon] reload failed, keeping old: {e}"),
                        }
                    }
                    Ok(true) => {} // file set changed; reload_discovery already re-ticked
                    Err(e) => eprintln!("[daemon] discovery reload: {e}"),
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
                    Ok(_) => {}
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
            // Pure metadata churn (`.git` write with no ref advance and no
            // worktree diff) is a no-op — don't log it, or the git-watch stream
            // floods daemon.log. Only announce a real advance/diff.
            if n > 0 || !changed.is_empty() {
                eprintln!("[daemon] git change — {n} ref(s) advanced, {} worktree file(s)", changed.len());
            }
            if changed.is_empty() { continue; }
            paths = changed;
        }
        let tick_label;
        let result = if touches_cfg {
            tick_label = "config change";
            let mut eng = lock(&d.eng);
            eng.set_repos(load_repos_eager());
            drop(eng);
            // Re-persist the repo set so `_repo` tracks a config edit.
            if let Err(e) = lock(&d.eng).save_repos_meta() {
                eprintln!("[daemon] save_repos_meta: {e}");
            }
            d.tick_full(true)
        } else if paths.is_empty() {
            tick_label = "empty event";
            d.tick_full(true)
        } else {
            tick_label = "source change";
            d.tick_paths(&paths, true)
        };
        let n_paths = paths.len();
        let tick_num = d.tick_count.load(Ordering::Relaxed);
        let tick_ok = result.is_ok();
        match result {
            Ok(()) => eprintln!("[daemon] tick #{tick_num} ({tick_label}, {n_paths} paths) ok"),
            Err(e) => eprintln!("[daemon] tick #{tick_num} ({tick_label}, {n_paths} paths) error: {e}"),
        }
        // A tick may have pulled new repos (a `repo`-sink drained). Watch any
        // root not yet watched — source tree, gate entry, and narrow git dir —
        // so the next edit or commit there is reactive.
        if tick_ok {
            let before = watch_count;
            for rc in lock(&d.eng).snapshot_repos() {
                if rc.root.exists() && watched.insert(rc.root.clone())
                    && watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
                    watch_count += 1;
                    gate.add_root(&rc.root);
                    watch_count += watch_git_narrow(&mut watcher, &mut gate, &rc.root);
                    eprintln!("[daemon] watching (pulled) {} ({})", rc.slug, rc.root.display());
                }
            }
            if watch_count != before {
                eprintln!("[daemon] watch count now {watch_count} (+{})", watch_count - before);
            }
        }
    }
}

/// Narrow-watch a repo's `.git` dir: the git dir itself NonRecursive (delivers
/// `HEAD`/`packed-refs`, not the `objects/` churn) and `refs/` Recursive, and
/// register it with the gate so those ref paths route to `on_git_event` while
/// everything else under `.git` is dropped. Resolves the real git dir via
/// `rev-parse --git-dir` so worktrees/submodules (`.git` file pointer) land on
/// the actual dir. A free fn (not a closure) so its `&mut` borrows do not
/// persist across the watch loop's own use of `watcher`/`gate`. Returns the
/// number of watch registrations it successfully installed (for the log count).
fn watch_git_narrow(watcher: &mut notify::RecommendedWatcher, gate: &mut WatchGate, root: &Path) -> usize {
    use notify::{RecursiveMode, Watcher};
    let out = match std::process::Command::new("git").arg("-C").arg(root)
        .args(["rev-parse", "--git-dir"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return 0,
    };
    let gd = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let gdp = if Path::new(&gd).is_absolute() { PathBuf::from(&gd) } else { root.join(&gd) };
    if !gdp.exists() { return 0; }
    gate.add_git_dir(&gdp);
    let mut added = 0;
    for (path, recursive) in gate.git_watch_targets(&gdp) {
        if !path.exists() { continue; }
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        if watcher.watch(&path, mode).is_ok() { added += 1; }
    }
    added
}

/// Identity of the currently-running `dl` binary: the compiled crate version
/// plus the on-disk exe's mtime (seconds). The version alone can't tell two
/// same-version rebuilds apart; the mtime does. Computed once at daemon startup
/// and echoed by `ping`; a client recomputes it from the on-disk exe and
/// respawns on mismatch. Falls back to the bare version if the exe path or its
/// mtime is unreadable (best-effort; a missing mtime just disables the rebuild
/// check, never forces a spurious respawn since both sides see the same fallback).
fn build_id() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mtime = std::env::current_exe().ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    match mtime {
        Some(secs) => format!("{version}:{secs}"),
        None => version.to_string(),
    }
}

impl Daemon {
    fn program_in_paths(&self, paths: &[PathBuf]) -> bool {
        let pf = lock(&self.program_files);
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
        let last = *lock(&d.last_activity);
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

// ---------- poll thread (the @async clock source) ----------

/// The effect-drain cadence. `DL_POLL_SECS` overrides it: `N>0` = every N
/// seconds; `0` = OFF (the explicit escape hatch, for a program that manages its
/// own drain or wants none). UNSET = drain by default at `DEFAULT_POLL_SECS` —
/// so `@async`/`@stream` effects fire under a bare `dl daemon start` without the
/// caller having to know an env var exists. The poll loop gates on the program
/// actually having effects, so this default costs a non-effect daemon nothing.
/// A DSL `every(secs)` clock source (per-program cadence) is the follow-up.
fn poll_interval_secs() -> Option<u64> {
    match std::env::var("DL_POLL_SECS") {
        Ok(s) => s.parse::<u64>().ok().filter(|&n| n > 0), // "0"/junk => off
        Err(_) => Some(DEFAULT_POLL_SECS),                 // unset => default on
    }
}

fn poll_loop(d: Arc<Daemon>, secs: u64) {
    eprintln!("[daemon] poll loop every {secs}s (@async drain)");
    loop {
        std::thread::sleep(Duration::from_secs(secs));
        if d.shutdown_requested.load(Ordering::Relaxed) { return; }
        // Cheap gate: only tick+drain when the loaded program has effect rules.
        // Keeps the default-on poll from re-ticking a plain (effect-free) daemon
        // every interval — the reason polling used to be strictly opt-in.
        let has_effects = {
            let prog = lock(&d.prog);
            !crate::engine::async_effect_arity(&prog).is_empty()
        };
        if !has_effects { continue; }
        match d.poll_tick() {
            Ok(n) if n > 0 => eprintln!("[daemon] poll: drained {n} effect(s)"),
            Ok(_) => {}
            Err(e) => eprintln!("[daemon] poll error: {e}"),
        }
    }
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
            // `dl daemon stop` returns promptly. A graceful wake (self-pipe, non-
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
        "ping" => {
            let act = crate::activity::snapshot();
            Response::ok(req.id, json!({
                "ok": true,
                "build_id": d.build_id,
                "root": d.root.to_string_lossy(),
                "tick_count": d.tick_count.load(Ordering::Relaxed),
                "settled": d.settled.load(Ordering::Relaxed),
                "program": d.program_display,
                "program_files": lock(&d.program_files).iter()
                    .map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
                "activity": {
                    "phase": act.phase.as_str(),
                    "detail": act.detail,
                    "program": act.program,
                    "tick": act.tick,
                    "elapsed_ms": act.elapsed_ms,
                },
            }))
        }
        // Block until the program is quiescent (the poll loop keeps ticking +
        // draining toward it) or `timeout_ms` elapses. The daemon owns the effect
        // runtime, so this is the daemon-side twin of `dl --settle`: a client can
        // wait for every cascade (effects, demand hops, repo pulls) to run at
        // least once. Polls the flag from this connection's own thread; the poll
        // loop / watcher threads set it. Default timeout 30s.
        "await_quiescent" => {
            let timeout_ms = req.params.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(30_000);
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            loop {
                if d.settled.load(Ordering::Relaxed) {
                    return Response::ok(req.id, json!({
                        "settled": true,
                        "tick_count": d.tick_count.load(Ordering::Relaxed),
                    }));
                }
                if Instant::now() >= deadline {
                    return Response::ok(req.id, json!({
                        "settled": false,
                        "tick_count": d.tick_count.load(Ordering::Relaxed),
                    }));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        "query" => {
            let prog = lock(&d.prog);
            let eng = lock(&d.eng);
            let _ = eng.log_query("daemon", "query", "", "[]");
            let _ = crate::rels::refresh_query_log(&eng);
            match eng.run_queries_capture(&prog) {
                Ok(results) => Response::ok(req.id, json!({"results": results.iter().map(|r| json!({
                    "rel": r.rel, "columns": r.columns, "rows": r.rows,
                })).collect::<Vec<_>>()})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "diag" => {
            let only = req.params.get("path").and_then(|p| p.as_str());
            let eng = lock(&d.eng);
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
            let eng = lock(&d.eng);
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
            let eng = lock(&d.eng);
            match eng.hover(file, text) {
                Ok(md) => Response::ok(req.id, json!({"markdown": md})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "subscribe" => {
            let events = req.params.get("events").cloned().unwrap_or(Value::Array(vec![]));
            if let Some(s) = subscriber_stream {
                lock(&d.subscribers).push(s);
                Response::ok(req.id, json!({"events": events, "ok": true}))
            } else {
                Response::err(req.id, INVALID_PARAMS, "subscribe requires a kept-open socket")
            }
        }
        "schema" => {
            let eng = lock(&d.eng);
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
        // Dump a relation's live rows by name: column names from the engine's rel
        // metadata + the stringified rows (the `dl daemon rows REL` shortcut). Answers
        // "what did this demand sink resolve to?" without hand-writing a `?` query.
        "query_rel" => {
            let rel = match req.params.get("rel").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing rel"),
            };
            let eng = lock(&d.eng);
            let Some(meta) = eng.rels.get(rel) else {
                return Response::err(req.id, INVALID_PARAMS,
                    format!("unknown relation {rel:?}"));
            };
            let cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
            let rows = eng.rel_rows(rel, cols.len());
            Response::ok(req.id, json!({"columns": cols, "rows": rows}))
        }
        // `dl what <anchor>` / `dl summary <path>` against the daemon's warm
        // engine: resolve the anchor and read the persisted rel tables. No
        // family forcing here — the daemon reads whatever its served program
        // already populated (the in-process CLI path forces cold families).
        "what" => {
            let anchor = match req.params.get("anchor").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing anchor"),
            };
            let limit = req.params.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
            let offset = req.params.get("offset").and_then(|v| v.as_u64()).map(|n| n as usize);
            let eng = lock(&d.eng);
            let out = crate::anchor::what(&eng, anchor, limit, offset);
            Response::ok(req.id, json!({
                "columns": out.columns, "rows": out.rows,
                "total": out.total, "notes": out.notes,
            }))
        }
        "summary" => {
            let path = match req.params.get("path").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing path"),
            };
            let eng = lock(&d.eng);
            let out = crate::anchor::summary(&eng, path);
            Response::ok(req.id, json!({
                "columns": out.columns, "rows": out.rows,
                "total": out.total, "notes": out.notes,
            }))
        }
        "query_sql" => {
            let sql_raw = match req.params.get("sql").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing sql"),
            };
            let params: Vec<Value> = req.params.get("params")
                .and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let eng = lock(&d.eng);
            let params_json = serde_json::to_string(&params).unwrap_or_else(|_| "[]".into());
            let _ = eng.log_query("daemon", "query_sql", sql_raw, &params_json);
            let _ = crate::rels::refresh_query_log(&eng);
            match eng.query_sql(sql_raw, &params) {
                Ok(rows) => Response::ok(req.id, json!({"rows": rows})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        // One rpc-port request pumped through the daemon's warm engine: inject
        // into the @in(rpc) rel, tick, drain the @out(rpc) rel. The `dl --mcp`
        // adapter calls this per inbound frame, so the daemon stays the single
        // engine/lock owner (no second process fighting for the db).
        "mcp_request" => {
            let p = &req.params;
            let (Some(in_rel), Some(out_rel), Some(rid), Some(method)) = (
                p.get("in_rel").and_then(|v| v.as_str()),
                p.get("out_rel").and_then(|v| v.as_str()),
                p.get("id").and_then(|v| v.as_str()),
                p.get("method").and_then(|v| v.as_str()),
            ) else {
                return Response::err(req.id, INVALID_PARAMS,
                    "mcp_request needs in_rel, out_rel, id, method");
            };
            let args = p.get("params").and_then(|v| v.as_str()).unwrap_or("null");
            let prog = lock(&d.prog);
            // Validate against the DAEMON's loaded program, not the adapter's
            // file: the two can drift, and injecting into a rel that is not an
            // @in(rpc) port here would corrupt an ordinary relation.
            for (rel, dir) in [(in_rel, crate::ast::PortDir::In), (out_rel, crate::ast::PortDir::Out)] {
                match crate::mcp::port_decl(&prog, rel) {
                    Some(port) if port.dir == dir && port.class == "rpc" => {}
                    _ => return Response::err(req.id, INVALID_PARAMS, format!(
                        "rel {rel} is not an @{}(rpc) port in the daemon's loaded program",
                        if dir == crate::ast::PortDir::In { "in" } else { "out" })),
                }
            }
            let mut eng = lock(&d.eng);
            let run = (|| -> anyhow::Result<Vec<(String, String)>> {
                eng.inject_rpc(in_rel, rid, method, args)?;
                eng.tick(&prog, true)?;
                eng.drain_rpc(out_rel, in_rel)
            })();
            d.tick_count.fetch_add(1, Ordering::Relaxed);
            match run {
                Ok(rows) => Response::ok(req.id, json!({"rows": rows})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        // Retire unanswered request ids from an @in(rpc) rel (the adapter's
        // -32601 path). Answered ids were already retired by the drain.
        "mcp_retire" => {
            let Some(in_rel) = req.params.get("in_rel").and_then(|v| v.as_str()) else {
                return Response::err(req.id, INVALID_PARAMS, "mcp_retire needs in_rel");
            };
            let ids: Vec<String> = req.params.get("ids").and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            {
                let prog = lock(&d.prog);
                match crate::mcp::port_decl(&prog, in_rel) {
                    Some(port) if port.dir == crate::ast::PortDir::In && port.class == "rpc" => {}
                    _ => return Response::err(req.id, INVALID_PARAMS, format!(
                        "rel {in_rel} is not an @in(rpc) port in the daemon's loaded program")),
                }
            }
            let mut eng = lock(&d.eng);
            match eng.retire_rpc(in_rel, &ids) {
                Ok(()) => Response::ok(req.id, json!({"ok": true})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        // One harness-hook event fed through the daemon's warm engine: append a
        // `hook_event` row, tick, done. `dl --hook` calls this before reading the
        // emit rels, so the daemon stays the single engine/lock owner. Unlike
        // `mcp_request` there is no port to drift-validate: `hook_event` is a
        // built-in rel (always declared by the priming tick), engine-owned, so
        // the only check is that the payload carries the fields.
        "hook_event" => {
            let p = &req.params;
            let (Some(kind), Some(session), Some(json)) = (
                p.get("kind").and_then(|v| v.as_str()),
                p.get("session").and_then(|v| v.as_str()),
                p.get("json").and_then(|v| v.as_str()),
            ) else {
                return Response::err(req.id, INVALID_PARAMS, "hook_event needs kind, session, json");
            };
            let seq = p.get("seq").and_then(|v| v.as_i64()).unwrap_or_else(|| {
                SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64).unwrap_or(0)
            });
            let prog = lock(&d.prog);
            let mut eng = lock(&d.eng);
            let run = (|| -> anyhow::Result<()> {
                eng.insert_hook_event(kind, session, seq, json)?;
                eng.tick(&prog, true)
            })();
            d.tick_count.fetch_add(1, Ordering::Relaxed);
            match run {
                Ok(()) => Response::ok(req.id, json!({"ok": true})),
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
            let eng = lock(&d.eng);
            let q = |sql: &str| eng.query_sql(sql, &[]).unwrap_or_default();
            Response::ok(req.id, json!({
                "root": d.root.to_string_lossy(),
                "program": d.program_display,
                "tick_count": d.tick_count.load(Ordering::Relaxed),
                "subscribers": lock(&d.subscribers).len(),
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
                        let mut pf = lock(&d.program_files);
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
                            if let Err(e) = lock(&d.eng)
                                .save_program_meta(&lock(&d.program_files).clone()) {
                                eprintln!("[daemon] save_program_meta: {e}");
                            }
                            let files: Vec<String> = lock(&d.program_files).iter()
                                .map(|f| f.to_string_lossy().into_owned()).collect();
                            Response::ok(req.id, json!({
                                "loaded": canon.to_string_lossy(),
                                "already_loaded": already,
                                "program_files": files,
                            }))
                        }
                        Err(e) => {
                            // Roll back the just-added file. reload_program
                            // re-parses ALL program_files, so leaving a bad file
                            // in the set would fail every future reload/tick —
                            // the daemon would wedge. Removing it (only if this
                            // load added it) keeps the daemon on its last-good
                            // program; the load fails cleanly.
                            if !already {
                                lock(&d.program_files).retain(|f| f != &canon);
                            }
                            Response::err(req.id, INTERNAL_ERROR, format!("reload: {e}"))
                        }
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
        let base = lock(&d.prog);
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
/// If already up, returns immediately. Otherwise spawns `dl daemon start` detached
/// (rooted at X via `DL_DAEMON_ROOT`+cwd, foreground=false so idle timeout
/// applies) and poll-connects until
/// the daemon responds to `ping` or the connect-time budget is exhausted.
pub fn ensure_daemon(root: &Path, program: Option<&str>) -> Result<()> {
    if is_running(Some(root)) {
        // Ping to confirm it's our daemon and not a stale socket.
        let mut s = connect(Some(root))?;
        let req = Request::new(0, "ping", json!({}));
        if let Ok(r) = rpc_call(&mut s, &req) {
            if r.error.is_none() {
                // Attach only if the daemon runs THIS binary. A rebuilt or
                // reinstalled `dl` reports a different build_id than the daemon
                // captured at startup; replace the stale daemon so the new code
                // actually takes effect (otherwise the old daemon owns the
                // socket forever and the fresh binary never runs).
                let running = r.result.as_ref()
                    .and_then(|v| v.get("build_id"))
                    .and_then(|v| v.as_str());
                match running {
                    Some(id) if id == build_id() => return Ok(()),
                    Some(_) => {
                        eprintln!("[daemon] running binary changed — restarting daemon");
                        let _ = stop(Some(root));
                    }
                    // Pre-handshake daemon (no build_id field): leave it be.
                    None => return Ok(()),
                }
            }
        }
    }
    // Stale, mismatched, or missing. Spawn detached.
    spawn_detached(root, program)?;
    wait_ready(root, program)
}

/// Stop the daemon for `root` (if running) and respawn it detached with the
/// CURRENT binary, rediscovering `<root>/.dl/*.dl`. The `dl daemon restart` backend:
/// the one-liner after `cargo install`, since a long-running daemon keeps its
/// old in-memory image until it is replaced. Prints what it did.
pub fn restart(root: &Path) -> Result<()> {
    let was_running = is_running(Some(root));
    if was_running { let _ = stop(Some(root)); }
    spawn_detached(root, None)?;
    // Do NOT fail on a slow first tick: a large repo's cold extraction can exceed
    // the connect budget. Poll briefly for a fast confirmation on small repos; a
    // timeout just means the daemon is still warming up in the background (it was
    // spawned detached), which is not an error — restart is fire-and-forget.
    let ready = wait_ready(root, None).is_ok();
    eprintln!("[daemon] {} at {} (build {}){}",
        if was_running { "restarted" } else { "started" },
        root.display(), build_id(),
        if ready { "" } else { " — starting (first tick still in progress)" });
    Ok(())
}

fn spawn_detached(root: &Path, program: Option<&str>) -> Result<()> {
    let exe = std::env::current_exe()
        .context("locate current exe for daemon spawn")?;
    let log = root.join(".dl").join("daemon.log");
    if let Some(p) = log.parent() { std::fs::create_dir_all(p)?; }
    // Backstop against unbounded growth: start a respawn's log fresh once it has
    // grown past the cap (the reactive-tick path is quiet, so steady growth is
    // just the one-line `[daemon] tick #N` heartbeat, but a long-lived storm or
    // a chatty program can still accumulate). Truncate on respawn, else append.
    const LOG_CAP_BYTES: u64 = 8 * 1024 * 1024;
    let oversized = std::fs::metadata(&log).map(|m| m.len() > LOG_CAP_BYTES).unwrap_or(false);
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(!oversized)
        .write(oversized)
        .truncate(oversized)
        .open(&log)?;
    let stderr = log_file.try_clone()?;
    let mut cmd = std::process::Command::new(exe);
    // No `--root` flag exists: the spawned per-repo daemon learns its root from
    // the internal `DL_DAEMON_ROOT` env (the `daemon` subcommand reads it) + cwd.
    // A terminal `dl daemon start` with neither is the rootless singleton at the
    // XDG home.
    cmd.args(["daemon", "start"]).env("DL_DAEMON_ROOT", root).current_dir(root);
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
            .unwrap_or(CONNECT_BACKOFF_MS.last().copied().unwrap_or(500));
        backoff_idx += 1;
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
}

/// Send `shutdown` to the daemon at this home (`Some(root)` → per-root,
/// `None` → the singleton XDG serving daemon). Ok if it acknowledged or was not
/// running. Used by `dl daemon stop` / `dl daemon stop`.
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

/// Block until the daemon at this home reports the program quiescent (every
/// cascade — effects, demand hops, repo pulls — has run at least once) or
/// `timeout_ms` elapses. Returns `(settled, tick_count)`. The daemon-side twin
/// of `dl --settle`: `--settle` owns the effect runtime in-process; this waits
/// on the already-running daemon that owns it. `root=None` targets the rootless
/// serving daemon.
pub fn await_quiescent(root: Option<&Path>, timeout_ms: u64) -> Result<(bool, u64)> {
    let mut s = connect(root)?;
    // The socket read timeout must outlast the daemon's blocking wait, else the
    // client gives up before the answer arrives.
    let _ = s.set_read_timeout(Some(Duration::from_millis(timeout_ms + 5_000)));
    let req = Request::new(0, "await_quiescent", json!({"timeout_ms": timeout_ms}));
    let resp = rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error {
        bail!("await_quiescent failed: {}", e.message);
    }
    let r = resp.result.unwrap_or(json!({}));
    Ok((r.get("settled").and_then(|v| v.as_bool()).unwrap_or(false),
        r.get("tick_count").and_then(|v| v.as_u64()).unwrap_or(0)))
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

/// Fetch relation `rel`'s current rows from the running daemon: returns the
/// column names and the stringified rows (the `dl daemon rows REL` backend).
/// `root=None` targets the global rootless serving daemon.
pub fn query_rel(root: Option<&Path>, rel: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut s = connect(root)?;
    let req = Request::new(0, "query_rel", json!({"rel": rel}));
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

/// One `dl what` / `dl summary` answer from the daemon: header, rows, pre-paging
/// total, advisory notes. Mirrors `anchor::QueryOut` across the wire.
pub struct QueryAnswer {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total: usize,
    pub notes: Vec<String>,
}

/// Decode a `{columns, rows, total, notes}` RPC result into a [`QueryAnswer`].
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

/// `dl what <anchor>` against the daemon (the `what` RPC). Backs the daemon-first
/// path in `cli::query`.
pub fn what(root: Option<&Path>, anchor: &str, limit: Option<usize>, offset: Option<usize>)
    -> Result<QueryAnswer>
{
    let mut s = connect(root)?;
    let mut params = json!({"anchor": anchor});
    if let Some(l) = limit { params["limit"] = json!(l); }
    if let Some(o) = offset { params["offset"] = json!(o); }
    let req = Request::new(0, "what", params);
    let resp = rpc_call(&mut s, &req)?;
    if let Some(err) = resp.error { anyhow::bail!("{}", err.message); }
    Ok(decode_query_answer(&resp.result.unwrap_or_default()))
}

/// `dl summary <path>` against the daemon (the `summary` RPC).
pub fn summary(root: Option<&Path>, path: &str) -> Result<QueryAnswer> {
    let mut s = connect(root)?;
    let req = Request::new(0, "summary", json!({"path": path}));
    let resp = rpc_call(&mut s, &req)?;
    if let Some(err) = resp.error { anyhow::bail!("{}", err.message); }
    Ok(decode_query_answer(&resp.result.unwrap_or_default()))
}

/// Ping the running daemon and return its status JSON (build_id, root,
/// tick_count, settled, program). `Ok(None)` when no daemon answers on this
/// root's socket. The `dl daemon status` backend.
pub fn status(root: Option<&Path>) -> Result<Option<Value>> {
    let mut s = match connect(root) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let req = Request::new(0, "ping", json!({}));
    match rpc_call(&mut s, &req) {
        Ok(r) if r.error.is_none() => Ok(r.result),
        _ => Ok(None),
    }
}

// ---------- small helpers shared with lib.rs ----------

// Silent: called on every git event (`on_git_event` rebuilds the repo set to
// diff refs), so an announcement here floods daemon.log. The cold-serve path
// logs the count once; a config error still surfaces.
pub(crate) fn load_repos_eager() -> Vec<config::RepoConfig> {
    match config::SprfConfig::load_default() {
        Ok(cfg) if !cfg.repos.is_empty() => cfg.repos,
        Ok(_) => Vec::new(),
        Err(e) => { eprintln!("[config] ignored: {e}"); Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_id_is_stable_and_version_prefixed() {
        let a = build_id();
        let b = build_id();
        assert_eq!(a, b, "build_id must be stable within one process");
        assert!(a.starts_with(env!("CARGO_PKG_VERSION")),
            "build_id should carry the crate version: {a}");
    }
}
