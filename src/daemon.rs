//! Singleton daemon + registered roots.
//!
//! ONE daemon process lives at a constant home (`$XDG_STATE_HOME/sprefa`, or
//! `~/.local/state/sprefa`) and serves EVERY `.dl`-owning root. cwd only picks
//! WHICH root a query addresses; it is not the daemon's identity anymore. There
//! is one Unix socket. Each RPC carries a `root` key in its params; the daemon
//! routes it to that root's warm `Engine` (its own SQLite db under
//! `<home>/roots/<key>/db.sqlite`). A root with no key (`root` absent) addresses
//! the config-view engine (the org/folders-in-view model — it scans nothing and
//! draws its facts from the configured repos).
//!
//! This replaces the old "one daemon per workspace root" scheme, where every
//! `.dl` folder minted its own daemon + socket + db under `<root>/.dl/`. That
//! per-root spawn-if-missing is what leaked processes (one per test sandbox) and
//! once bound a real repo's socket to a throwaway program. A singleton with a
//! registry deletes the mechanism.
//!
//! Control files at `<home>/`:
//!   - `daemon.sock`  the ONE Unix domain socket (mode 0600)
//!   - `daemon.pid`   text file: `pid\nstart_secs\n`
//!   - `daemon.log`   one log, lines prefixed `[<root basename>]`
//!   - `roots.json`   registered-root persistence (replayed on boot)
//!   - `db.sqlite`    the config-view engine db
//!   - `roots/<key>/db.sqlite`   one db per registered root
//!
//! Lifecycle:
//!   - `dl daemon start`        detaches a background daemon by default
//!   - `dl daemon start --foreground`   runs it in this process (debug path)
//!   - `dl daemon stop`          global shutdown (every root)
//!   - `dl daemon drop <root>`   deregister one root (`--purge` deletes its db)
//!   - default invocation auto-attaches (spawns the singleton if none) and names
//!     its root in the RPC; an unregistered `.dl` root auto-registers on attach
//!   - `DL_NO_DAEMON=1` opts out (in-process, the pre-daemon path; used by tests)
//!   - `DL_DAEMON_IDLE_SECS=N` overrides the 30 min default
//!   - `DL_DAEMON_MEM_MB=N` RSS ceiling (default 4096); the serve loop exits
//!     with code 137 if the process grows past it. `0` disables the guard.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard};
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
/// no served root has effects, so a non-effect daemon pays ~nothing.
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

/// Read-lock a poison-tolerant `RwLock` (same policy as `lock`). Used only for
/// the read-path shape snapshot (`ServedRoot::read_view`), which a reader clones
/// under a short read lock and a tick refreshes under a short write lock.
#[inline]
fn rlock<T>(m: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    m.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Best-effort label for whichever RPC most recently acquired a served
/// root's engine mutex — read by `lock_eng`'s waiter to name the "behind
/// <current op>" half of a `[wait]` verdict. Approximate by construction: it
/// records the last op to ACQUIRE the lock, not the op currently holding it
/// at the moment a new waiter starts blocking (there is no cheap way to know
/// that without wrapping every RPC body in a guard-drop callback), so a
/// long-idle op between ticks can leave a stale label. Good enough for "which
/// kind of request tends to hold the lock a while," the diagnostic this
/// exists for.
static CURRENT_OP: OnceLock<Mutex<String>> = OnceLock::new();

fn current_op_cell() -> &'static Mutex<String> {
    CURRENT_OP.get_or_init(|| Mutex::new("none".to_string()))
}

/// Lock a served root's engine mutex, timing the wait. A wait at or past
/// `verdict::LOCK_WAIT_WARN_MS` logs `[wait] <rpc> waited <ms>ms behind
/// <current op>` (stderr + perf.jsonl); every acquisition then relabels
/// `CURRENT_OP` to `method` for the next waiter. Replaces the bare
/// `lock(&sr.eng)` call at every RPC dispatch site that touches the engine.
fn lock_eng<'a>(sr: &'a ServedRoot, method: &str) -> MutexGuard<'a, Engine> {
    let behind = lock(current_op_cell()).clone();
    let start = Instant::now();
    let guard = lock(&sr.eng);
    let waited_ms = start.elapsed().as_millis() as u64;
    if waited_ms >= crate::verdict::LOCK_WAIT_WARN_MS {
        crate::verdict::verdict(
            "lock-wait",
            &format!("[wait] {method} waited {waited_ms}ms behind {behind}"),
            &[("rpc", method), ("waited_ms", &waited_ms.to_string()), ("behind", &behind)],
        );
    }
    *lock(current_op_cell()) = method.to_string();
    guard
}
const CONNECT_BACKOFF_MS: &[u64] = &[10, 20, 40, 80, 160, 320, 500];
const CONNECT_TOTAL_TIMEOUT_SECS: u64 = 5;

// ---------- path helpers ----------

/// The daemon's home: `$XDG_STATE_HOME/sprefa` (or `~/.local/state/sprefa`). ONE
/// singleton serving daemon lives here, decoupled from any project root — the
/// "folders in view" model. Tests set `XDG_STATE_HOME` to a sandbox, which makes
/// a stray test daemon structurally unable to bind a developer's socket. Created
/// on demand.
pub fn daemon_home() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("sprefa")
}

/// macOS caps `sockaddr_un.sun_path` at 104 bytes; Linux at 108. A deep
/// `$XDG_STATE_HOME` (a sandbox under a long temp path) can overrun it, so
/// `socket_path` relocates the socket to a short hashed path when the natural
/// one is too long. Both bind and every connect derive it from the same home, so
/// they always agree. Leave slack for the trailing NUL.
const SUN_MAX: usize = 100;

/// `<home>/daemon.sock`, unless that path is too long for `sun_path` — then a
/// short, deterministic path keyed by the home's hash. THE singleton socket.
pub fn socket_path() -> PathBuf {
    socket_path_for(&daemon_home())
}

/// The socket for a given home dir (env-independent; the deep-home test drives
/// this directly). `<home>/daemon.sock`, relocated to a short hashed path when
/// the natural one overruns `sun_path`.
pub fn socket_path_for(home: &Path) -> PathBuf {
    let natural = home.join("daemon.sock");
    if natural.as_os_str().len() < SUN_MAX {
        natural
    } else {
        let hash = blake3::hash(home.as_os_str().as_encoded_bytes());
        short_sock_dir().join(format!("{}.sock", &hash.to_hex()[..16]))
    }
}

/// Short base directory for a relocated socket. `TMPDIR`-derived so it stays on a
/// short path; created 0700 on first use.
fn short_sock_dir() -> PathBuf {
    let base = std::env::temp_dir().join("dl-sock");
    if let Err(e) = std::fs::create_dir_all(&base) {
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
pub fn pid_path() -> PathBuf {
    daemon_home().join("daemon.pid")
}

/// `<home>/roots.json` — the registered-root persistence file.
fn roots_json_path() -> PathBuf {
    daemon_home().join("roots.json")
}

/// `<home>/roots/<key>` — one dir per registered root; holds `db.sqlite`.
fn root_db_dir(key: &str) -> PathBuf {
    daemon_home().join("roots").join(key)
}

/// The registry key for a root: blake3-16hex of its canonical path. Symlinked
/// aliases collapse to one entry.
fn key_of(canon: &Path) -> String {
    blake3::hash(canon.as_os_str().as_encoded_bytes()).to_hex()[..16].to_string()
}

fn write_pid_file() -> Result<()> {
    let dir = daemon_home();
    std::fs::create_dir_all(&dir)?;
    let pid = std::process::id();
    let start = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs();
    std::fs::write(pid_path(), format!("{pid}\n{start}\n"))?;
    Ok(())
}

#[allow(dead_code)]
fn read_pid_file() -> Option<(u32, u64)> {
    let txt = std::fs::read_to_string(pid_path()).ok()?;
    let mut lines = txt.lines();
    let pid: u32 = lines.next()?.parse().ok()?;
    let start: u64 = lines.next()?.parse().ok()?;
    Some((pid, start))
}

fn remove_pid_file() {
    let _ = std::fs::remove_file(pid_path());
}

// ---------- shared process handles ----------

/// Handles cloned into every `ServedRoot` so its per-root tick methods can reach
/// the process-wide subscriber list / shutdown flag / build identity without a
/// back-pointer to the whole `Daemon`.
#[derive(Clone)]
struct Shared {
    build_id: Arc<str>,
    shutdown_requested: Arc<AtomicBool>,
    subscribers: Arc<Mutex<Vec<Arc<Mutex<UnixStream>>>>>,
}

// ---------- one served root ----------

/// One registered `.dl`-owning root served by the singleton, or the config-view
/// engine (`key == None`). Holds that root's warm `Engine` + parsed `Program` +
/// per-root watch filter. Byte-identical to the old per-root daemon's behavior;
/// only WHERE the db lives (a constant home position) changed.
pub struct ServedRoot {
    /// Engine/content/watch base. For the config view this is the XDG home (a
    /// benign "self" that scans nothing); the view comes from config repos.
    pub root: PathBuf,
    /// Registry key (blake3-16hex of the canonical root). `None` = config view.
    pub key: Option<String>,
    pub program_display: String,
    shared: Shared,
    /// Canonicalized absolute program-file paths the engine parsed.
    pub program_files: Mutex<Vec<PathBuf>>,
    /// True when this root loaded via `<root>/.dl/*.dl` discovery (vs an explicit
    /// program set): new/removed `.dl` files re-merge.
    pub discovery_mode: bool,
    pub prog: Mutex<Program>,
    pub eng: Mutex<Engine>,
    pub last_activity: Mutex<Instant>,
    pub tick_count: AtomicU64,
    /// Whether the last FULL tick left the program quiescent (the poll loop drives
    /// toward this; `await_quiescent` blocks on it).
    pub settled: AtomicBool,
    /// The paths touched by the most recent tick (absolute). Empty after a full
    /// tick.
    pub last_changed_paths: Mutex<Vec<PathBuf>>,
    /// Set by `drop_root`; the watcher thread observes it and exits, dropping its
    /// `Arc<ServedRoot>` so the engine closes.
    pub stopped: Arc<AtomicBool>,
    /// `tick_count`'s value as of the end of the last FULL tick (`tick_full`).
    /// Only a full tick runs `rebuild_async` (queues fresh `@async`/`@stream`
    /// requests over the converged derived state); the watcher's incremental
    /// `tick_paths` never does. So `tick_count != last_full_tick_count` means
    /// a path-tick landed source motion since we last gave `rebuild_async` a
    /// chance to see it — `poll_idle`'s cheap "source changed" half. See
    /// `poll_idle` for the full gate (CPU-hog fix Part 1).
    last_full_tick_count: AtomicU64,
    /// Part 3 (poll error backoff): consecutive `poll_tick` errors for this
    /// root. Reset to 0 on the first successful poll after a failure.
    poll_fail_streak: AtomicU32,
    /// Part 3: poll cycles left to SKIP (decremented, not re-attempted) before
    /// the next `poll_tick` call. Set by `poll_backoff_cycles(poll_fail_streak)`
    /// after an error.
    poll_skip: AtomicU32,
    /// The served root's on-disk db file (the writer engine's db). The lock-free
    /// read path (`crate::daemon_read`) opens READ-ONLY connections on it. `None`
    /// only for a hypothetical in-memory served root (none exist today), which
    /// sends every read to the engine-lock fallback.
    db_path: Option<PathBuf>,
    /// Shape snapshot for the lock-free read path, refreshed whenever the
    /// program (re)loads (`refresh_read_view`, called from `tick_full`). A read
    /// RPC clones this `Arc` under a short read lock and answers `query` /
    /// `query_rel` / `query_sql` / `schema` from committed SQLite state WITHOUT
    /// taking `lock_eng`, so read latency is independent of tick duration.
    read_view: RwLock<Arc<crate::daemon_read::ReadView>>,
}

impl ServedRoot {
    fn touch(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    /// Clone the current read-path shape snapshot — a cheap `Arc` clone under a
    /// short read lock that never contends with a tick.
    fn read_view(&self) -> Arc<crate::daemon_read::ReadView> {
        rlock(&self.read_view).clone()
    }

    /// Rebuild the read-path snapshot from the given engine + program. Called at
    /// the end of a full tick — the only path that can change rel shapes or the
    /// `?` query set — while the tick still holds `eng`+`prog`, so it is a
    /// straight clone under a short write lock.
    fn refresh_read_view(&self, eng: &Engine, prog: &Program) {
        let view = crate::daemon_read::ReadView::snapshot(&eng.rels, prog, self.db_path.clone());
        *self.read_view.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(view);
    }

    fn root_label(&self) -> String {
        self.root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned())
    }

    fn tick_full(&self, quiet: bool) -> Result<()> {
        let tick_next = self.tick_count.load(Ordering::Relaxed) + 1;
        crate::activity::begin_tick(tick_next, &self.program_display);
        let prog = lock(&self.prog);
        let mut eng = lock(&self.eng);
        let report = eng.tick_report(&prog, quiet)?;
        // A full tick is the only path that can change rel shapes or the `?`
        // query set (program reloads all end here) — refresh the read-path
        // snapshot while still holding eng+prog.
        self.refresh_read_view(&eng, &prog);
        drop(eng);
        drop(prog);
        crate::activity::end_tick();
        self.settled.store(report.is_settled(), Ordering::Relaxed);
        let n = self.tick_count.fetch_add(1, Ordering::Relaxed) + 1;
        // This WAS a full tick, so it just ran `rebuild_async` over the
        // converged state — resync `last_full_tick_count` (`poll_idle`'s
        // "source changed since the last full tick" half goes false again).
        self.last_full_tick_count.store(n, Ordering::Relaxed);
        self.touch();
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
        self.settled.store(false, Ordering::Relaxed);
        self.tick_count.fetch_add(1, Ordering::Relaxed);
        self.touch();
        *lock(&self.last_changed_paths) = paths.to_vec();
        self.broadcast_diag_changed();
        Ok(())
    }

    /// Re-parse the program files, swap the parsed `Program`, re-tick. A parse or
    /// type error keeps the last good program.
    fn reload_program(&self) -> Result<()> {
        let all = lock(&self.program_files).clone();
        let files: Vec<PathBuf> = all.iter().filter(|f| f.exists()).cloned().collect();
        if files.is_empty() {
            if !all.is_empty() {
                eprintln!("[{}] all {} watched program file(s) missing; keeping last-good program",
                    self.root_label(), all.len());
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
            eprintln!("[{}] save_program_meta: {e}", self.root_label());
        }
        self.tick_full(false)?;
        Ok(())
    }

    /// Re-discover `.dl` files under `<root>/.dl/`, re-merge the program if the
    /// file set changed, re-tick. `Ok(true)` = set changed and re-merged;
    /// `Ok(false)` = set unchanged (content edit, or not in discovery mode).
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
            eprintln!("[{}] discovery reload: {n_err} type error(s); keeping old", self.root_label());
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
                eprintln!("[{}] save_program_meta: {e}", self.root_label());
            }
        }
        let n = lock(&self.program_files).len();
        eprintln!("[{}] discovery reload: {n} file(s)", self.root_label());
        self.tick_full(false)?;
        Ok(true)
    }

    /// Cheap idle probe for the poll cycle (CPU-hog fix, Part 1). `true` means
    /// this root's poll cycle has nothing to do: skip the whole thing (no
    /// `tick_full`, no drain, no corpus walk) rather than paying `tick_full`'s
    /// full source reconcile every `DEFAULT_POLL_SECS` regardless of state.
    ///
    /// Two probes, both O(1)/indexed — never a corpus walk:
    ///   (a) `pending_effect` COUNT (queued|running, any kind incl. `@stream`
    ///       subscriptions, which sit 'running' forever and need a continuing
    ///       drain) — non-zero means there is already drainable work.
    ///   (b) `tick_count != last_full_tick_count` — a path-tick (the watcher,
    ///       on a file change) landed source motion that no full tick has
    ///       run `rebuild_async` over yet, so a new `@async` request may be
    ///       owed (e.g. `watch-ext.dl`'s `ext_built`, gated on `ext_src`'s
    ///       content hash, not on wall-clock time).
    ///
    /// One case neither probe catches: an `@async`/`@stream` rule gated on
    /// `every`/`clock` fires purely off a wall-clock boundary crossing, with
    /// no associated file change the watcher would ever see — `rebuild_async`
    /// (the only place that evaluates the cadence and queues a fresh request)
    /// runs ONLY inside a full tick, so such a program genuinely needs the
    /// periodic full tick unconditionally, same as before this fix (see
    /// `gc_done_effects`'s doc comment on "a cadence-bucketed poll queues a
    /// fresh row every `clock` bucket forever" — a real, intentional pattern).
    /// `every_rels_used`/`clock_rels_used` scan the whole program (not just
    /// async rule bodies) — a derived rule elsewhere reading `every`/`clock`
    /// also opts a root out of the idle skip, which is conservative-correct,
    /// not a regression (such a root already relied on the always-full-tick
    /// poll before this fix).
    fn poll_idle(&self) -> Result<bool> {
        let cadence_driven = {
            let prog = lock(&self.prog);
            crate::engine::every_rels_used(&prog) || crate::engine::clock_rels_used(&prog)
        };
        if cadence_driven { return Ok(false); }
        // `self.settled` is the LAST full tick's `TickReport::is_settled()` —
        // quiescence can only be CONFIRMED by a tick that sees nothing move
        // (a tick that just landed a response is itself reported unsettled,
        // by design: is_settled() requires changed_rels to be timer-only).
        // So a not-yet-settled root owes one more full tick regardless of the
        // two cheap probes below — skipping it would freeze `settled` at
        // `false` forever the moment the queue empties, which is exactly the
        // state `dl daemon await-settle` blocks on.
        if !self.settled.load(Ordering::Relaxed) { return Ok(false); }
        let pending = lock(&self.eng).pending_effect_count()?;
        if pending > 0 { return Ok(false); }
        let dirty = self.tick_count.load(Ordering::Relaxed)
            != self.last_full_tick_count.load(Ordering::Relaxed);
        Ok(!dirty)
    }

    /// One poll cycle (the clock source for `@async`): advance the tick, then
    /// drain outstanding effects + external sinks. Returns the number drained.
    /// Skips entirely (see `poll_idle`) when there is nothing to integrate.
    fn poll_tick(&self) -> Result<usize> {
        if self.poll_idle()? { return Ok(0); }
        self.tick_full(true)?;
        let sinks_drained = {
            let prog = lock(&self.prog);
            let mut eng = lock(&self.eng);
            crate::activity::set(crate::activity::Phase::Effects, "external sinks");
            eng.drain_external_sinks(&prog).unwrap_or_else(|e| {
                eprintln!("[{}] drain_external_sinks: {e}", self.root_label());
                0
            })
        };
        let arity = {
            let prog = lock(&self.prog);
            crate::engine::async_effect_arity(&prog)
        };
        if arity.is_empty() { return Ok(sinks_drained); }
        let (templates, cwd) = {
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
            crate::activity::set(crate::activity::Phase::Effects, "drain");
            let a = eng.drain_effects(&prog, &exec)?;
            let s = eng.drain_streams(&prog, &exec)?;
            a + s
        };
        let n = n + sinks_drained;
        self.touch();
        if n > 0 {
            self.tick_full(true)?;
            self.broadcast_diag_changed();
        }
        Ok(n)
    }

    fn has_effects(&self) -> bool {
        let prog = lock(&self.prog);
        !crate::engine::async_effect_arity(&prog).is_empty()
    }

    fn broadcast_diag_changed(&self) {
        let paths: Vec<String> = lock(&self.last_changed_paths).iter()
            .map(|p| p.to_string_lossy().into_owned()).collect();
        let note = json!({"jsonrpc": "2.0", "method": "diag_changed", "params": {
            "root": self.root.to_string_lossy(),
            "tick": self.tick_count.load(Ordering::Relaxed),
            "paths": paths,
        }});
        let body = match serde_json::to_string(&note) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut subs = lock(&self.shared.subscribers);
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
        for i in broken.into_iter().rev() { subs.swap_remove(i); }
    }

    /// The git refs to watch for advance: always `HEAD`, plus every non-WORK rev
    /// literal the loaded program scans.
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

    /// React to a `.git` change: diff each watched ref old→new against `_file` and
    /// broadcast `rev_advanced`. Returns (refs advanced, worktree files changed).
    fn on_git_event(&self) -> (usize, Vec<PathBuf>) {
        let mut repos: Vec<(String, PathBuf)> = vec![(self.root_label(), self.root.clone())];
        // This engine's corpus (hermetic served root => just its own root; the
        // config view => the config repos), not every ambient config repo.
        for rc in lock(&self.eng).snapshot_repos() {
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
                        Err(e) => eprintln!("[{}] observe_ref {slug}/{name}: {e}", self.root_label()),
                    }
                }
            }
            if !advances.is_empty() {
                if let Err(e) = eng.refresh_daemon_rels() {
                    eprintln!("[{}] refresh_daemon_rels: {e}", self.root_label());
                }
            }
        }
        self.touch();
        if !advances.is_empty() {
            self.broadcast_rev_advanced(&advances);
        }
        (advances.len(), changed)
    }

    fn broadcast_rev_advanced(&self, advances: &[(String, String, String, String, Vec<String>)]) {
        let mut subs = lock(&self.shared.subscribers);
        let mut broken: Vec<usize> = Vec::new();
        for (repo, name, old, new, files) in advances {
            let note = json!({"jsonrpc": "2.0", "method": "rev_advanced", "params": {
                "root": self.root.to_string_lossy(),
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

    /// Open a served root: open its db, load its program (`<root>/.dl/*.dl`
    /// discovery, or an explicit program set), cold-tick, and build the struct.
    /// `key == None` is the config view: the engine roots at the XDG home, scans
    /// nothing, and draws facts from the config repos.
    fn open(
        root: Option<PathBuf>,
        key: Option<String>,
        programs: &[String],
        db_path: Option<&str>,
        shared: Shared,
    ) -> Result<Arc<ServedRoot>> {
        let is_config = key.is_none();
        let eng_root = root.clone().unwrap_or_else(daemon_home);
        let files = if programs.is_empty() {
            crate::resolve_programs(&[], &eng_root).unwrap_or_default()
        } else {
            programs.iter().map(PathBuf::from).collect()
        };
        let (prog, type_diags, display) = if files.is_empty() {
            (Program { items: vec![] }, vec![], "<serving>".to_string())
        } else {
            crate::prepare_paths(&files)?
        };
        crate::render_type_diags_eprintln(&type_diags);
        let n_err = type_diags.iter().filter(|d| d.severity == crate::ast::Severity::Error).count();
        if n_err > 0 { bail!("{n_err} type error(s) in program; root not served"); }

        // Ensure the db's parent dir exists before opening it (the per-root db
        // lives under <home>/roots/<key>/, which won't exist on first register).
        if let Some(k) = &key { let _ = std::fs::create_dir_all(root_db_dir(k)); }
        let conn = db::open(db_path)?;
        let mut eng = Engine::new(conn, eng_root.clone());
        if is_config { eng.set_root_implicit(true); }
        eng.set_repos(served_repos(is_config));
        crate::activity::set(crate::activity::Phase::ColdTick, display.as_str());
        eng.tick(&prog, false)?;
        crate::activity::end_tick();
        let canon_files: Vec<PathBuf> = files
            .iter()
            .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
            .collect();
        if let Err(e) = eng.save_repos_meta() { eprintln!("[daemon] save_repos_meta: {e}"); }
        if let Err(e) = eng.save_program_meta(&canon_files) { eprintln!("[daemon] save_program_meta: {e}"); }

        let label = eng_root.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| eng_root.to_string_lossy().into_owned());
        eprintln!("[{label}] served ({} type diag(s), program {display})", type_diags.len());

        // Initial read-path snapshot from the cold-tick engine + program.
        let db_path_buf = db_path.map(PathBuf::from);
        let read_view = crate::daemon_read::ReadView::snapshot(&eng.rels, &prog, db_path_buf.clone());

        Ok(Arc::new(ServedRoot {
            root: eng_root,
            key,
            program_display: display,
            shared,
            program_files: Mutex::new(canon_files),
            discovery_mode: programs.is_empty(),
            prog: Mutex::new(prog),
            eng: Mutex::new(eng),
            last_activity: Mutex::new(Instant::now()),
            tick_count: AtomicU64::new(1),
            settled: AtomicBool::new(false),
            last_changed_paths: Mutex::new(Vec::new()),
            stopped: Arc::new(AtomicBool::new(false)),
            // The cold tick just above (`eng.tick(&prog, false)`) IS a full
            // tick — it already ran `rebuild_async` once — so start in sync
            // with `tick_count` (both 1), not dirty.
            last_full_tick_count: AtomicU64::new(1),
            poll_fail_streak: AtomicU32::new(0),
            poll_skip: AtomicU32::new(0),
            db_path: db_path_buf,
            read_view: RwLock::new(Arc::new(read_view)),
        }))
    }
}

// ---------- registered-root persistence ----------

/// One line in `roots.json`.
#[derive(Clone)]
struct RootRecord {
    root: PathBuf,
    key: String,
    added_at: u64,
}

fn read_roots_json() -> Vec<RootRecord> {
    let txt = match std::fs::read_to_string(roots_json_path()) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let v: Value = match serde_json::from_str(&txt) { Ok(v) => v, Err(_) => return Vec::new() };
    v.as_array().map(|arr| arr.iter().filter_map(|r| {
        let root = r.get("root").and_then(|x| x.as_str())?;
        let key = r.get("key").and_then(|x| x.as_str())?;
        let added_at = r.get("added_at").and_then(|x| x.as_u64()).unwrap_or(0);
        Some(RootRecord { root: PathBuf::from(root), key: key.to_string(), added_at })
    }).collect()).unwrap_or_default()
}

fn write_roots_json(records: &[RootRecord]) {
    let arr: Vec<Value> = records.iter().map(|r| json!({
        "root": r.root.to_string_lossy(), "key": r.key, "added_at": r.added_at,
    })).collect();
    let _ = std::fs::create_dir_all(daemon_home());
    if let Ok(s) = serde_json::to_string_pretty(&Value::Array(arr)) {
        let _ = std::fs::write(roots_json_path(), s);
    }
}

// ---------- the singleton daemon ----------

/// The one process. Owns the socket, the config-view engine, and the registry of
/// per-root engines.
pub struct Daemon {
    /// XDG state home (control files, roots.json, per-root dbs).
    pub home: PathBuf,
    launch_exe_stamp: Option<ExeStamp>,
    pub build_id: Arc<str>,
    pub shutdown_requested: Arc<AtomicBool>,
    pub subscribers: Arc<Mutex<Vec<Arc<Mutex<UnixStream>>>>>,
    /// The `root`-absent config view (org/folders model).
    pub config: Arc<ServedRoot>,
    /// key -> served root. The registry.
    pub roots: Mutex<HashMap<String, Arc<ServedRoot>>>,
}

impl Daemon {
    fn shared(&self) -> Shared {
        Shared {
            build_id: self.build_id.clone(),
            shutdown_requested: self.shutdown_requested.clone(),
            subscribers: self.subscribers.clone(),
        }
    }

    /// Count of registered (non-config) served roots, poison-tolerant. Read by the
    /// HTTP `/health` probe, which must answer without taking any engine mutex —
    /// only this roots-map lock. Mirrors `daemon_summary`'s `root_count`.
    pub(crate) fn served_root_count(&self) -> usize {
        lock(&self.roots).len()
    }

    /// Every served root (config view + registered), for the idle/poll loops and
    /// status.
    fn all_roots(&self) -> Vec<Arc<ServedRoot>> {
        let mut v = vec![self.config.clone()];
        v.extend(lock(&self.roots).values().cloned());
        v
    }

    /// Route a `root` param to its served engine. `None` -> config view. A miss on
    /// a path that owns `.dl/` auto-registers (attach IS registration, cold-ticks
    /// inside the daemon; the caller blocks on the reply). A miss on a non-`.dl`
    /// path is a loud error naming `add_root`.
    fn resolve(self: &Arc<Self>, root: Option<&str>) -> Result<Arc<ServedRoot>, String> {
        let raw = match root {
            None | Some("") => return Ok(self.config.clone()),
            Some(r) => r,
        };
        let canon = Path::new(raw).canonicalize().unwrap_or_else(|_| PathBuf::from(raw));
        let key = key_of(&canon);
        if let Some(sr) = lock(&self.roots).get(&key) {
            return Ok(sr.clone());
        }
        if canon.join(".dl").is_dir() {
            self.add_root(&canon).map_err(|e| e.to_string())
        } else {
            Err(format!(
                "unknown root {} (owns no .dl/; nothing to serve). \
                 Register a .dl root with `dl daemon` from inside it.",
                canon.display()
            ))
        }
    }

    /// Register a root (idempotent). Canonicalizes, refuses a root nested inside —
    /// or containing — an already-registered root (the SCIP explosion guard's
    /// tone), opens its engine (db under `<home>/roots/<key>/`), spawns its
    /// watcher, and persists to `roots.json`. Returns the served root.
    fn add_root(self: &Arc<Self>, root: &Path) -> Result<Arc<ServedRoot>> {
        let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let key = key_of(&canon);
        // Idempotent hit.
        if let Some(sr) = lock(&self.roots).get(&key) {
            return Ok(sr.clone());
        }
        // Nested-registration guard (mirror the SCIP explosion-guard tone: loud,
        // names both paths). A root inside another registered root — or one that
        // contains one — would double-serve overlapping trees.
        {
            let roots = lock(&self.roots);
            for existing in roots.values() {
                if canon.starts_with(&existing.root) {
                    bail!(
                        "refusing to register {}: it is nested inside already-registered root {}. \
                         One daemon serves each root once; register the outer root and query the \
                         inner path against it, or `dl daemon drop {}` first.",
                        canon.display(), existing.root.display(), existing.root.display()
                    );
                }
                if existing.root.starts_with(&canon) {
                    bail!(
                        "refusing to register {}: already-registered root {} lives inside it. \
                         Registering the parent would double-serve the child tree; \
                         `dl daemon drop {}` first if you want the parent served instead.",
                        canon.display(), existing.root.display(), existing.root.display()
                    );
                }
            }
        }
        let db = root_db_dir(&key).join("db.sqlite");
        let db_str = db.to_string_lossy().into_owned();
        let sr = ServedRoot::open(Some(canon.clone()), Some(key.clone()), &[], Some(&db_str), self.shared())?;
        lock(&self.roots).insert(key.clone(), sr.clone());
        // Persist.
        let added_at = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs()).unwrap_or(0);
        {
            let mut records = read_roots_json();
            if !records.iter().any(|r| r.key == key) {
                records.push(RootRecord { root: canon.clone(), key: key.clone(), added_at });
                write_roots_json(&records);
            }
        }
        // Spawn its watcher.
        let sr_watch = sr.clone();
        let launch_exe_stamp = self.launch_exe_stamp;
        let _ = std::thread::Builder::new().name(format!("dl-watch-{key}"))
            .spawn(move || watcher_loop(sr_watch, launch_exe_stamp));
        eprintln!("[daemon] registered root {} (key {key})", canon.display());
        Ok(sr)
    }

    /// Deregister a root: stop its watcher, drop the engine, keep the db (re-add
    /// warms from it). `purge` deletes `<home>/roots/<key>/`.
    fn drop_root(self: &Arc<Self>, root: &Path, purge: bool) -> Result<()> {
        let canon = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let key = key_of(&canon);
        let sr = lock(&self.roots).remove(&key);
        if let Some(sr) = &sr {
            sr.stopped.store(true, Ordering::Relaxed);
        }
        {
            let mut records = read_roots_json();
            records.retain(|r| r.key != key);
            write_roots_json(&records);
        }
        if purge {
            let _ = std::fs::remove_dir_all(root_db_dir(&key));
        }
        if sr.is_none() {
            eprintln!("[daemon] drop_root {}: not registered", canon.display());
        } else {
            eprintln!("[daemon] deregistered root {} (key {key}){}",
                canon.display(), if purge { ", db purged" } else { "" });
        }
        Ok(())
    }
}

// ---------- background scheduling budget ----------

/// Positive nice value for the daemon process on unix. Higher values mean lower
/// CPU priority, keeping the daemon from competing with the user's foreground.
const DAEMON_NICE: libc::c_int = 10;

/// macOS QoS class for utility/background work. Values come from `<sys/qos.h>`;
/// UTILITY is `0x11`.
#[cfg(target_os = "macos")]
const QOS_CLASS_UTILITY: u32 = 0x11;

/// Compute the rayon thread-count budget for the daemon. Never claim more than a
/// quarter of the machine's cores, but keep at least 2 threads so small laptops
/// don't collapse to serial operation. `DL_DAEMON_THREADS` overrides the
/// heuristic.
fn daemon_thread_count(cores: usize, env: Option<&str>) -> usize {
    env.and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| std::cmp::max(2, cores / 4))
}

/// Apply the daemon's background scheduling budget once, at the top of the serve
/// path. macOS threads inherit the QoS class of the spawning thread, so setting
/// this on the main thread before spawning any workers keeps the whole daemon in
/// the utility tier where the platform supports it. On all unix systems the
/// process also gets a positive nice value as a portable fallback. The rayon
/// global pool is bounded here; if it was already initialized earlier in this
/// process (e.g. by the CLI dispatch path) the call is a no-op and we report the
/// current pool size.
fn apply_daemon_budget() -> (&'static str, i32, usize) {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn pthread_set_qos_class_self_np(class: u32, priority: libc::c_int) -> libc::c_int;
        }
        // SAFETY: libc call with a constant, valid QoS class; ignore errors.
        unsafe { pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0) };
    }

    #[cfg(unix)]
    {
        // SAFETY: setpriority on this process is always valid; ignore EPERM.
        unsafe { libc::setpriority(libc::PRIO_PROCESS, 0 as libc::id_t, DAEMON_NICE) };
    }

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let desired = daemon_thread_count(
        cores,
        std::env::var("DL_DAEMON_THREADS").ok().as_deref(),
    );

    // Bound the global rayon pool. This may already have been configured by the
    // CLI entry path; build_global returns an error in that case rather than
    // panicking, so the daemon stays safe to run both foreground and detached.
    let _ = rayon::ThreadPoolBuilder::new().num_threads(desired).build_global();
    let threads = rayon::current_num_threads();

    let qos_label = if cfg!(target_os = "macos") { "utility" } else { "none" };
    let priority = if cfg!(unix) { DAEMON_NICE as i32 } else { 0 };
    (qos_label, priority, threads)
}

// ---------- daemon entry ----------

/// Run the singleton daemon in this process. Binds the one socket at the XDG
/// home, builds the config-view engine, replays `roots.json` (re-registers every
/// persisted root, warm from its db), optionally registers `initial_root`, then
/// drives the accept + idle + poll loops until shutdown. `foreground=true`
/// disables the idle timeout (debug path); `false` = the detached background
/// daemon that self-reaps when every root goes idle.
pub fn run_daemon(
    programs: &[String],
    db_path: Option<&str>,
    initial_root: Option<PathBuf>,
    foreground: bool,
    tray: bool,
) -> Result<()> {
    let (qos_label, priority, threads) = apply_daemon_budget();
    eprintln!("[daemon] background budget: qos={qos_label} nice={priority} threads={threads}");

    let home = daemon_home();
    let _ = std::fs::create_dir_all(&home);
    let launch_exe_stamp = current_exe_stamp();

    let build_id: Arc<str> = Arc::from(build_id().as_str());
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let subscribers: Arc<Mutex<Vec<Arc<Mutex<UnixStream>>>>> = Arc::new(Mutex::new(Vec::new()));
    let shared = Shared { build_id: build_id.clone(), shutdown_requested: shutdown_requested.clone(), subscribers: subscribers.clone() };

    let repos = load_repos_eager();
    if !repos.is_empty() { eprintln!("[config] {} repo(s) registered", repos.len()); }

    // The config-view engine (root:None). An explicit --db points it at that file;
    // otherwise the home db.
    let config_db = db_path.map(|s| s.to_string())
        .unwrap_or_else(|| home.join("db.sqlite").to_string_lossy().into_owned());
    let config = ServedRoot::open(None, None, &[], Some(&config_db), shared.clone())
        .context("open config-view engine")?;

    let daemon = Arc::new(Daemon {
        home: home.clone(),
        launch_exe_stamp,
        build_id: build_id.clone(),
        shutdown_requested: shutdown_requested.clone(),
        subscribers: subscribers.clone(),
        config,
        roots: Mutex::new(HashMap::new()),
    });

    // Bind the socket (reap a stale one first) BEFORE registering roots, so a
    // second daemon fails fast rather than cold-ticking every root then losing
    // the bind race.
    let sock = socket_path();
    if sock.exists() {
        if UnixStream::connect(&sock).is_err() {
            let _ = std::fs::remove_file(&sock);
        } else {
            bail!("daemon already running on socket {}", sock.display());
        }
    }
    if let Some(dir) = sock.parent() { std::fs::create_dir_all(dir)?; }
    let listener = UnixListener::bind(&sock)?;
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    write_pid_file()?;
    let idle_secs = idle_timeout_secs();
    eprintln!("[daemon] listening on {} (pid {}, idle {}s){}",
        sock.display(), std::process::id(), idle_secs,
        if foreground { " [foreground]" } else { "" });
    if let Some(p) = crate::perflog::path() {
        eprintln!("[daemon] perf log {} (tail -f | jq .total_ms, .phase, .ms)", p.display());
    }
    crate::verdict::emit_run_header(
        "daemon",
        initial_root.as_ref().map(|r| r.to_string_lossy().into_owned()).unwrap_or_else(|| home.to_string_lossy().into_owned()).as_str(),
        &config_db,
        "singleton daemon home db",
        "self (this process IS the daemon)",
        env!("CARGO_PKG_VERSION"),
        &build_id,
    );

    // Config-view watcher (watches the config repos).
    {
        let c = daemon.config.clone();
        let stamp = launch_exe_stamp;
        std::thread::Builder::new().name("dl-watch-config".into())
            .spawn(move || watcher_loop(c, stamp))?;
    }

    // Replay roots.json: re-register every persisted root (warm from its db).
    // Part 4 (stale-root eviction): a record whose directory no longer
    // exists AT ALL (not just a missing `.dl/`) is dropped from the
    // replayed set and roots.json is rewritten without it — vs. the old
    // behavior of silently re-skipping the same dead entry every boot
    // forever. Its db under `roots/<key>/` is left on disk untouched.
    let mut kept: Vec<RootRecord> = Vec::new();
    let mut evicted_any = false;
    for rec in read_roots_json() {
        if !rec.root.exists() {
            eprintln!("[daemon] root {} no longer exists; evicting from roots.json (key {})",
                rec.root.display(), rec.key);
            evicted_any = true;
            continue;
        }
        kept.push(rec.clone());
        if rec.root.join(".dl").is_dir() {
            match daemon.add_root(&rec.root) {
                Ok(_) => {}
                Err(e) => eprintln!("[daemon] replay {}: {e}", rec.root.display()),
            }
        } else {
            eprintln!("[daemon] replay skip {} (no .dl/)", rec.root.display());
        }
    }
    if evicted_any { write_roots_json(&kept); }

    // Register the initial root (a `dl daemon start --foreground` from inside a
    // repo, or an explicit program set).
    if let Some(r) = &initial_root {
        if r.join(".dl").is_dir() {
            let canon = r.canonicalize().unwrap_or_else(|_| r.clone());
            let key = key_of(&canon);
            if !lock(&daemon.roots).contains_key(&key) {
                let db = root_db_dir(&key).join("db.sqlite");
                let db_str = db.to_string_lossy().into_owned();
                match ServedRoot::open(Some(canon.clone()), Some(key.clone()), programs, Some(&db_str), daemon.shared()) {
                    Ok(sr) => {
                        lock(&daemon.roots).insert(key.clone(), sr.clone());
                        let added_at = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                        let mut records = read_roots_json();
                        if !records.iter().any(|x| x.key == key) {
                            records.push(RootRecord { root: canon.clone(), key: key.clone(), added_at });
                            write_roots_json(&records);
                        }
                        let sr_watch = sr.clone();
                        let stamp = launch_exe_stamp;
                        std::thread::Builder::new().name(format!("dl-watch-{key}"))
                            .spawn(move || watcher_loop(sr_watch, stamp))?;
                        eprintln!("[daemon] registered initial root {}", canon.display());
                    }
                    Err(e) => eprintln!("[daemon] initial root {}: {e}", canon.display()),
                }
            }
        }
    }

    if !foreground {
        let d = daemon.clone();
        std::thread::Builder::new().name("dl-idle".into())
            .spawn(move || idle_loop(d, idle_secs))?;
    }
    if let Some(secs) = poll_interval_secs() {
        let d = daemon.clone();
        std::thread::Builder::new().name("dl-poll".into())
            .spawn(move || poll_loop(d, secs))?;
    }

    let d = daemon.clone();
    std::thread::Builder::new()
        .name("dl-accept".into())
        .spawn(move || accept_loop(d, listener))?;

    // Standard HTTP/JSON transport alongside the UDS socket. Binds 127.0.0.1:0
    // and publishes `<home>/http.json` so clients can find the port. A bind
    // failure is logged but non-fatal: the UDS transport stays authoritative.
    if let Err(e) = crate::daemon_http::serve(daemon.clone(), &build_id) {
        eprintln!("[daemon] http transport disabled: {e}");
    }

    if tray {
        crate::tray::run_tray(daemon.clone())?;
    } else {
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

pub(crate) fn shutdown_cleanup(_d: &Daemon) {
    let sock = socket_path();
    let _ = std::fs::remove_file(&sock);
    let _ = std::fs::remove_file(crate::daemon_http::http_json_path());
    remove_pid_file();
    eprintln!("[daemon] shut down cleanly");
}

// ---------- watcher thread (one per served root) ----------

fn watcher_loop(d: Arc<ServedRoot>, launch_exe_stamp: Option<ExeStamp>) {
    use notify::{RecursiveMode, Watcher};
    let is_config = d.key.is_none();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = match notify::recommended_watcher(move |res| { let _ = tx.send(res); }) {
        Ok(w) => w,
        Err(e) => { eprintln!("[{}] watcher init failed: {e}", d.root_label()); return; }
    };
    // The config view roots at the XDG home (which holds the per-root dbs); do NOT
    // watch it recursively. A registered root watches its own tree.
    let mut gate = if is_config {
        WatchGate::new(&[])
    } else {
        if let Err(e) = watcher.watch(&d.root, RecursiveMode::Recursive) {
            eprintln!("[{}] watch root failed: {e}", d.root_label());
            return;
        }
        WatchGate::new(std::slice::from_ref(&d.root))
    };
    let mut watch_count: usize = if is_config { 0 } else { 1 };
    // Watch every folder in THIS ENGINE'S corpus (its `snapshot_repos()`) so
    // corpus edits react. A hermetic served root's snapshot is empty (only its
    // own `--root`, watched above), so one save wakes only that root; the config
    // view's snapshot is the config repos. Following the engine, not
    // `load_repos_eager()`, is what makes the watcher hermetic in lockstep.
    for rc in lock(&d.eng).snapshot_repos() {
        if rc.root.exists() && rc.root != d.root
            && watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
            watch_count += 1;
            gate.add_root(&rc.root);
        }
    }
    let cfg_path = config::SprfConfig::config_path()
        .and_then(|p| p.canonicalize().ok().or(Some(p)));
    if is_config {
        if let Some(cp) = &cfg_path {
            if let Some(dir) = cp.parent() {
                if dir.exists() && watcher.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                    watch_count += 1;
                }
            }
        }
        // Watch home/.dl for load'd programs (non-recursive; not the dbs).
        let dl = d.root.join(".dl");
        if dl.is_dir() { let _ = watcher.watch(&dl, RecursiveMode::NonRecursive); }
    }
    // Narrow-watch the `.git` dir of the root + this engine's corpus repos.
    if !is_config { watch_count += watch_git_narrow(&mut watcher, &mut gate, &d.root); }
    for rc in lock(&d.eng).snapshot_repos() {
        if rc.root.exists() { watch_count += watch_git_narrow(&mut watcher, &mut gate, &rc.root); }
    }

    eprintln!("[{}] watcher ready — {watch_count} watch(es)", d.root_label());
    let watcher_start = std::time::Instant::now();
    const STARTUP_GRACE: Duration = Duration::from_secs(1);
    let mut watched: std::collections::HashSet<PathBuf> = std::collections::HashSet::from_iter([
        d.root.clone(),
    ].into_iter().chain(lock(&d.eng).snapshot_repos().into_iter().filter(|r| r.root.exists()).map(|r| r.root)));
    loop {
        // Observe the drop flag between events so `drop_root` can retire us.
        let first = match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(ev) => ev,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if d.stopped.load(Ordering::Relaxed) { return; }
                enforce_mem_limit(&d.root_label());
                enforce_fresh_binary(launch_exe_stamp);
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if d.stopped.load(Ordering::Relaxed) { return; }
        if watcher_start.elapsed() < STARTUP_GRACE {
            while rx.try_recv().is_ok() {}
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        const QUIET: Duration = Duration::from_millis(120);
        const MAX_WINDOW: Duration = Duration::from_millis(600);
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut rescan = false;
        match first {
            Ok(ev) => {
                if ev.need_rescan() { rescan = true; } else { paths.extend(ev.paths); }
            }
            Err(e) => {
                eprintln!("[{}] watch error, forcing full tick: {e}", d.root_label());
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
                    eprintln!("[{}] watch error, forcing full tick: {e}", d.root_label());
                    rescan = true;
                    if window_start.elapsed() > MAX_WINDOW { break; }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        if rescan {
            let tick_num = d.tick_count.load(Ordering::Relaxed);
            match d.tick_full(true) {
                Ok(()) => eprintln!("[{}] tick #{tick_num} (rescan recovery) ok", d.root_label()),
                Err(e) => eprintln!("[{}] tick #{tick_num} (rescan recovery) error: {e}", d.root_label()),
            }
            d.touch();
            enforce_mem_limit(&d.root_label());
            enforce_fresh_binary(launch_exe_stamp);
            continue;
        }
        let touches_git = gate.touches_git(&paths);
        let paths: Vec<PathBuf> = gate.filter(paths);
        if paths.is_empty() && !touches_git {
            continue;
        }
        d.touch();
        let touches_cfg = cfg_path.as_ref().is_some_and(|c|
            paths.iter().any(|p| p.canonicalize().ok().as_deref() == Some(c) || p == c));
        let mut paths = paths;
        let touches_program = d.program_in_paths(&paths);
        if touches_program {
            let names: Vec<String> = paths.iter()
                .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
                .collect();
            // Singleton daemon: program edits ALWAYS hot-reload (never exit for
            // respawn — one exit would kill every served root).
            if d.discovery_mode {
                match d.reload_discovery() {
                    Ok(false) => {
                        eprintln!("[{}] program edit ({}) — reloading (discovery)", d.root_label(), names.join(", "));
                        if let Err(e) = d.reload_program() {
                            eprintln!("[{}] reload failed, keeping old: {e}", d.root_label());
                        }
                    }
                    Ok(true) => {}
                    Err(e) => eprintln!("[{}] discovery reload: {e}", d.root_label()),
                }
            } else {
                eprintln!("[{}] program edit ({}) — reloading", d.root_label(), names.join(", "));
                if let Err(e) = d.reload_program() {
                    eprintln!("[{}] reload failed, keeping old: {e}", d.root_label());
                }
            }
            continue;
        }
        if d.discovery_mode {
            let has_dl = paths.iter().any(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("dl")
                    && p.strip_prefix(&d.root)
                        .map(|r| r.starts_with(".dl"))
                        .unwrap_or(false)
            });
            if has_dl {
                eprintln!("[{}] .dl discovery change — re-merging program", d.root_label());
                if let Err(e) = d.reload_discovery() {
                    eprintln!("[{}] discovery reload: {e}", d.root_label());
                }
                continue;
            }
        }
        if touches_git {
            let (n, changed) = d.on_git_event();
            if n > 0 || !changed.is_empty() {
                eprintln!("[{}] git change — {n} ref(s) advanced, {} worktree file(s)", d.root_label(), changed.len());
            }
            if changed.is_empty() { continue; }
            paths = changed;
        }
        let tick_label;
        let result = if touches_cfg {
            tick_label = "config change";
            let mut eng = lock(&d.eng);
            eng.set_repos(served_repos(d.key.is_none()));
            drop(eng);
            if let Err(e) = lock(&d.eng).save_repos_meta() {
                eprintln!("[{}] save_repos_meta: {e}", d.root_label());
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
            Ok(()) => eprintln!("[{}] tick #{tick_num} ({tick_label}, {n_paths} paths) ok", d.root_label()),
            Err(e) => eprintln!("[{}] tick #{tick_num} ({tick_label}, {n_paths} paths) error: {e}", d.root_label()),
        }
        // A tick is where the image grows (extract, closure, spine writes), so
        // check the ceiling here too, not only on the idle heartbeat.
        enforce_mem_limit(&d.root_label());
        enforce_fresh_binary(launch_exe_stamp);
        if tick_ok {
            let before = watch_count;
            for rc in lock(&d.eng).snapshot_repos() {
                if rc.root.exists() && watched.insert(rc.root.clone())
                    && watcher.watch(&rc.root, RecursiveMode::Recursive).is_ok() {
                    watch_count += 1;
                    gate.add_root(&rc.root);
                    watch_count += watch_git_narrow(&mut watcher, &mut gate, &rc.root);
                    eprintln!("[{}] watching (pulled) {} ({})", d.root_label(), rc.slug, rc.root.display());
                }
            }
            if watch_count != before {
                eprintln!("[{}] watch count now {watch_count} (+{})", d.root_label(), watch_count - before);
            }
        }
    }
}

/// Narrow-watch a repo's `.git` dir. Returns the number of watch registrations
/// installed.
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

/// Identity of the currently-running `dl` binary: crate version + the exe's
/// mtime. A client that rebuilt/reinstalled computes a different id and respawns.
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

// ---------- idle thread (all roots) ----------

/// Exit when EVERY served root has been idle past the threshold (keep engines
/// warm while any is active). Per-root eviction is a follow-up; this arc keeps
/// them all warm until the whole process idles out.
fn idle_loop(d: Arc<Daemon>, idle_secs: u64) {
    loop {
        std::thread::sleep(Duration::from_secs(IDLE_TICK_SECS));
        let roots = d.all_roots();
        let all_idle = roots.iter().all(|sr| {
            lock(&sr.last_activity).elapsed() > Duration::from_secs(idle_secs)
        });
        if all_idle {
            eprintln!("[daemon] all roots idle {}min, exiting", idle_secs / 60);
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

/// Hard RSS ceiling for the daemon process, in MB. Over it, the serve loop
/// exits (a runaway extract loop once grew the served image past a gigabyte and
/// pinned the machine). `DL_DAEMON_MEM_MB=0` disables the guard. The default is
/// generous: a multi-repo daemon legitimately holds a few hundred MB, so the
/// ceiling catches a genuine leak/storm, not steady-state serving.
const DEFAULT_MEM_LIMIT_MB: u64 = 4096;
/// Exit code the memory guard uses, distinct from a clean shutdown so a
/// supervisor can tell a self-kill from an orderly stop.
const MEM_GUARD_EXIT_CODE: i32 = 137;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExeStamp {
    len: u64,
    mtime: SystemTime,
}

/// Return the current executable's stat identity. A missing stat is unknown,
/// not evidence that the daemon's binary was replaced.
fn current_exe_stamp() -> Option<ExeStamp> {
    if std::env::var_os("DL_EXE_STAMP").is_some() { return None; }
    let path = std::env::current_exe().ok()?;
    let metadata = std::fs::metadata(path).ok()?;
    Some(ExeStamp { len: metadata.len(), mtime: metadata.modified().ok()? })
}

fn should_exit_for_binary_change(launch: Option<ExeStamp>, current: Option<ExeStamp>) -> bool {
    matches!((launch, current), (Some(before), Some(after)) if before != after)
}

fn enforce_fresh_binary(launch_exe_stamp: Option<ExeStamp>) {
    if launch_exe_stamp.is_none() { return; }
    if should_exit_for_binary_change(launch_exe_stamp, current_exe_stamp()) {
        eprintln!("[daemon] binary replaced on disk, exiting so the next call runs the new version");
        std::process::exit(0);
    }
}

fn mem_limit_mb() -> Option<u64> {
    match std::env::var("DL_DAEMON_MEM_MB") {
        Ok(s) => s.parse::<u64>().ok().filter(|&n| n > 0),
        Err(_) => Some(DEFAULT_MEM_LIMIT_MB),
    }
}

/// Current resident set size of this process in MB, via `ps` (portable across
/// macOS and Linux, no extra dependency; `ps` reports RSS in KB). `None` when
/// the read fails, so the guard simply skips that check rather than guessing.
fn self_rss_mb() -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p"])
        .arg(std::process::id().to_string())
        .output()
        .ok()?;
    if !out.status.success() { return None; }
    String::from_utf8_lossy(&out.stdout).trim().parse::<u64>().ok().map(|kb| kb / 1024)
}

/// Exit the process if RSS has crossed the ceiling. Called on the serve loop's
/// 1s heartbeat and after each tick, so both an idle leak and a busy-tick storm
/// trip it. Logs the numbers before exiting so the cause is in the daemon log.
fn enforce_mem_limit(label: &str) {
    let Some(limit) = mem_limit_mb() else { return };
    let Some(rss) = self_rss_mb() else { return };
    if rss > limit {
        eprintln!("[{label}] RSS {rss}MB exceeded limit {limit}MB — exiting (memory guard; \
                   set DL_DAEMON_MEM_MB to adjust, 0 to disable)");
        std::process::exit(MEM_GUARD_EXIT_CODE);
    }
}

// ---------- poll thread (the @async clock source, all roots) ----------

fn poll_interval_secs() -> Option<u64> {
    match std::env::var("DL_POLL_SECS") {
        Ok(s) => s.parse::<u64>().ok().filter(|&n| n > 0),
        Err(_) => Some(DEFAULT_POLL_SECS),
    }
}

/// Part 3 (poll error backoff): after `streak` consecutive `poll_tick`
/// errors, how many subsequent poll cycles to SKIP (not even attempt)
/// before trying again — `min(2^streak, cap)`, `cap` chosen so a wedged root
/// is still probed at least once a minute regardless of `poll_secs`.
/// `streak == 0` (healthy, or the tick right after recovery) skips nothing.
/// Pure function: no engine/socket access, so it is unit-testable without a
/// live daemon.
fn poll_backoff_cycles(streak: u32, poll_secs: u64) -> u32 {
    if streak == 0 { return 0; }
    let cap = (60u64 / poll_secs.max(1)).max(1) as u32;
    let cycles = 1u32.checked_shl(streak.min(31)).unwrap_or(u32::MAX);
    cycles.min(cap)
}

fn poll_loop(d: Arc<Daemon>, secs: u64) {
    eprintln!("[daemon] poll loop every {secs}s (@async drain)");
    loop {
        std::thread::sleep(Duration::from_secs(secs));
        if d.shutdown_requested.load(Ordering::Relaxed) { return; }
        for sr in d.all_roots() {
            // Part 4 (stale-root eviction): a registered root whose directory
            // vanished out from under the daemon (a temp job dir cleaned up
            // after `dl` auto-registered it on attach, etc.) is deregistered
            // here instead of being served — and error-looped — forever. The
            // config view (`key == None`) has no directory of its own to
            // vanish and is never a candidate.
            if sr.key.is_some() && !sr.root.exists() {
                eprintln!("[daemon] root {} no longer exists; deregistering", sr.root.display());
                let _ = d.drop_root(&sr.root, false);
                continue;
            }
            if !sr.has_effects() { continue; }
            // Part 3: a root backing off from a prior poll error skips this
            // cycle without even attempting `poll_tick`.
            let skip = sr.poll_skip.load(Ordering::Relaxed);
            if skip > 0 {
                sr.poll_skip.store(skip - 1, Ordering::Relaxed);
                continue;
            }
            match sr.poll_tick() {
                Ok(n) => {
                    if n > 0 { eprintln!("[{}] poll: drained {n} effect(s)", sr.root_label()); }
                    if sr.poll_fail_streak.swap(0, Ordering::Relaxed) > 0 {
                        eprintln!("[{}] poll recovered", sr.root_label());
                    }
                }
                Err(e) => {
                    let streak = sr.poll_fail_streak.fetch_add(1, Ordering::Relaxed) + 1;
                    let skip = poll_backoff_cycles(streak, secs);
                    sr.poll_skip.store(skip, Ordering::Relaxed);
                    eprintln!("[{}] poll error (backing off {skip} cycle(s)): {e}", sr.root_label());
                }
            }
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
            None => continue,
        };
        let is_shutdown = req.method == "shutdown";
        let is_subscribe = req.method == "subscribe";
        let sub = if is_subscribe {
            stream.try_clone().ok().map(|s| Arc::new(Mutex::new(s)))
        } else {
            None
        };
        let resp = handle_request(&d, &req, sub);
        let out = serde_json::to_string(&resp.to_json()).unwrap_or_else(|_| "{}".into());
        if rpc::write_frame(&mut stream, &out).is_err() { return; }
        if is_shutdown {
            d.shutdown_requested.store(true, Ordering::Relaxed);
            shutdown_cleanup(&d);
            std::process::exit(0);
        }
    }
}

pub(crate) fn parse_request(v: Value) -> Option<Request> {
    let id = v.get("id")?.as_u64()?;
    let method = v.get("method")?.as_str()?.to_string();
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    Some(Request { id, method, params })
}

/// The `root` envelope key: an absolute path in the request's params. Absent =
/// the config view.
fn req_root(req: &Request) -> Option<String> {
    req.params.get("root").and_then(|v| v.as_str()).map(String::from)
}

pub(crate) fn handle_request(d: &Arc<Daemon>, req: &Request, subscriber_stream: Option<Arc<Mutex<UnixStream>>>) -> Response {
    // ----- process-level methods (no root routing) -----
    match req.method.as_str() {
        "shutdown" => return Response::ok(req.id, json!({"ok": true})),
        "subscribe" => {
            let events = req.params.get("events").cloned().unwrap_or(Value::Array(vec![]));
            return if let Some(s) = subscriber_stream {
                lock(&d.subscribers).push(s);
                Response::ok(req.id, json!({"events": events, "ok": true}))
            } else {
                Response::err(req.id, INVALID_PARAMS, "subscribe requires a kept-open socket")
            };
        }
        "add_root" => {
            let Some(path) = req_root(req) else {
                return Response::err(req.id, INVALID_PARAMS, "add_root needs root");
            };
            return match d.add_root(Path::new(&path)) {
                Ok(sr) => Response::ok(req.id, json!({
                    "root": sr.root.to_string_lossy(),
                    "key": sr.key,
                    "tick_count": sr.tick_count.load(Ordering::Relaxed),
                })),
                Err(e) => Response::err(req.id, INVALID_PARAMS, format!("{e}")),
            };
        }
        "drop_root" => {
            let Some(path) = req_root(req) else {
                return Response::err(req.id, INVALID_PARAMS, "drop_root needs root");
            };
            let purge = req.params.get("purge").and_then(|v| v.as_bool()).unwrap_or(false);
            return match d.drop_root(Path::new(&path), purge) {
                Ok(()) => Response::ok(req.id, json!({"ok": true})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            };
        }
        // `ping`/`status` WITHOUT a root return the process summary + the roots list.
        "ping" | "status" if req_root(req).is_none() => {
            return daemon_summary(d, req);
        }
        _ => {}
    }

    // ----- root-scoped methods -----
    let sr = match d.resolve(req_root(req).as_deref()) {
        Ok(sr) => sr,
        Err(e) => return Response::err(req.id, INVALID_PARAMS, e),
    };
    let resp = dispatch_root(&sr, d, req);
    sr.touch();
    resp
}

/// Process-level summary for a rootless `ping`/`status`: build identity + every
/// served root with its tick count.
fn daemon_summary(d: &Arc<Daemon>, req: &Request) -> Response {
    let act = crate::activity::snapshot();
    let roots: Vec<Value> = lock(&d.roots).values().map(|sr| json!({
        "root": sr.root.to_string_lossy(),
        "key": sr.key,
        "tick_count": sr.tick_count.load(Ordering::Relaxed),
        "program": sr.program_display,
        "settled": sr.settled.load(Ordering::Relaxed),
    })).collect();
    Response::ok(req.id, json!({
        "ok": true,
        "build_id": &*d.build_id,
        "home": d.home.to_string_lossy(),
        "config_tick_count": d.config.tick_count.load(Ordering::Relaxed),
        "root_count": roots.len(),
        "roots": roots,
        "activity": {
            "phase": act.phase.as_str(),
            "detail": act.detail,
            "program": act.program,
            "tick": act.tick,
            "elapsed_ms": act.elapsed_ms,
        },
    }))
}

/// Dispatch a root-scoped method against the resolved served root.
fn dispatch_root(sr: &Arc<ServedRoot>, _d: &Arc<Daemon>, req: &Request) -> Response {
    match req.method.as_str() {
        "ping" => {
            let act = crate::activity::snapshot();
            Response::ok(req.id, json!({
                "ok": true,
                "build_id": &*sr.shared.build_id,
                "root": sr.root.to_string_lossy(),
                "key": sr.key,
                "tick_count": sr.tick_count.load(Ordering::Relaxed),
                "settled": sr.settled.load(Ordering::Relaxed),
                "program": sr.program_display,
                "program_files": lock(&sr.program_files).iter()
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
        "await_quiescent" => {
            let timeout_ms = req.params.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(30_000);
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            loop {
                if sr.settled.load(Ordering::Relaxed) {
                    return Response::ok(req.id, json!({
                        "settled": true,
                        "tick_count": sr.tick_count.load(Ordering::Relaxed),
                    }));
                }
                if Instant::now() >= deadline {
                    return Response::ok(req.id, json!({
                        "settled": false,
                        "tick_count": sr.tick_count.load(Ordering::Relaxed),
                    }));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        "query" => {
            // Fast path: answer from committed SQLite state off a read-only
            // connection, no `lock_eng`. `None` = aggregate query / no on-disk
            // db, which needs the engine (falls through below).
            match crate::daemon_read::query(&sr.read_view()) {
                Some(Ok(v)) => Response::ok(req.id, v),
                Some(Err((code, msg))) => Response::err(req.id, code, msg),
                None => {
                    let prog = lock(&sr.prog);
                    let eng = lock_eng(sr, &req.method);
                    let _ = eng.log_query("daemon", "query", "", "[]");
                    let _ = crate::rels::refresh_query_log(&eng);
                    match eng.run_queries_capture(&prog) {
                        Ok(results) => Response::ok(req.id, json!({"results": results.iter().map(|r| json!({
                            "rel": r.rel, "columns": r.columns, "rows": r.rows,
                        })).collect::<Vec<_>>()})),
                        Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
                    }
                }
            }
        }
        "diag" => {
            let only = req.params.get("path").and_then(|p| p.as_str());
            let eng = lock_eng(sr, &req.method);
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
            let eng = lock_eng(sr, &req.method);
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
            let eng = lock_eng(sr, &req.method);
            match eng.hover(file, text) {
                Ok(md) => Response::ok(req.id, json!({"markdown": md})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "schema" => {
            // Shapes come from the read-path snapshot (refreshed each program
            // load); no `lock_eng`.
            Response::ok(req.id, crate::daemon_read::schema(&sr.read_view()))
        }
        "query_rel" => {
            let rel = match req.params.get("rel").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing rel"),
            };
            match crate::daemon_read::query_rel(&sr.read_view(), rel) {
                Some(Ok(v)) => Response::ok(req.id, v),
                Some(Err((code, msg))) => Response::err(req.id, code, msg),
                None => {
                    let eng = lock_eng(sr, &req.method);
                    let Some(meta) = eng.rels.get(rel) else {
                        return Response::err(req.id, INVALID_PARAMS,
                            format!("unknown relation {rel:?}"));
                    };
                    let cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
                    let rows = eng.rel_rows(rel, cols.len());
                    Response::ok(req.id, json!({"columns": cols, "rows": rows}))
                }
            }
        }
        "what" => {
            let anchor = match req.params.get("anchor").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing anchor"),
            };
            let limit = req.params.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
            let offset = req.params.get("offset").and_then(|v| v.as_u64()).map(|n| n as usize);
            let eng = lock_eng(sr, &req.method);
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
            let eng = lock_eng(sr, &req.method);
            let out = crate::anchor::summary(&eng, path);
            Response::ok(req.id, json!({
                "columns": out.columns, "rows": out.rows,
                "total": out.total, "notes": out.notes,
            }))
        }
        "q" => {
            let verb = match req.params.get("verb").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing verb"),
            };
            let arg = match req.params.get("target").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing target"),
            };
            let limit = req.params.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
            let offset = req.params.get("offset").and_then(|v| v.as_u64()).map(|n| n as usize);
            match run_q_eval(sr, verb, arg, limit, offset) {
                Ok(v) => Response::ok(req.id, v),
                Err((code, msg)) => Response::err(req.id, code, msg),
            }
        }
        "query_sql" => {
            let sql_raw = match req.params.get("sql").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing sql"),
            };
            let params: Vec<Value> = req.params.get("params")
                .and_then(|v| v.as_array()).cloned().unwrap_or_default();
            match crate::daemon_read::query_sql(&sr.read_view(), sql_raw, &params) {
                Some(Ok(v)) => Response::ok(req.id, v),
                Some(Err((code, msg))) => Response::err(req.id, code, msg),
                None => {
                    let eng = lock_eng(sr, &req.method);
                    let params_json = serde_json::to_string(&params).unwrap_or_else(|_| "[]".into());
                    let _ = eng.log_query("daemon", "query_sql", sql_raw, &params_json);
                    let _ = crate::rels::refresh_query_log(&eng);
                    match eng.query_sql(sql_raw, &params) {
                        Ok(rows) => Response::ok(req.id, json!({"rows": rows})),
                        Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
                    }
                }
            }
        }
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
            let prog = lock(&sr.prog);
            for (rel, dir) in [(in_rel, crate::ast::PortDir::In), (out_rel, crate::ast::PortDir::Out)] {
                match crate::mcp::port_decl(&prog, rel) {
                    Some(port) if port.dir == dir && port.class == "rpc" => {}
                    _ => return Response::err(req.id, INVALID_PARAMS, format!(
                        "rel {rel} is not an @{}(rpc) port in the daemon's loaded program",
                        if dir == crate::ast::PortDir::In { "in" } else { "out" })),
                }
            }
            let mut eng = lock_eng(sr, &req.method);
            let run = (|| -> anyhow::Result<Vec<(String, String)>> {
                eng.inject_rpc(in_rel, rid, method, args)?;
                eng.tick(&prog, true)?;
                eng.drain_rpc(out_rel, in_rel)
            })();
            sr.tick_count.fetch_add(1, Ordering::Relaxed);
            match run {
                Ok(rows) => Response::ok(req.id, json!({"rows": rows})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "mcp_retire" => {
            let Some(in_rel) = req.params.get("in_rel").and_then(|v| v.as_str()) else {
                return Response::err(req.id, INVALID_PARAMS, "mcp_retire needs in_rel");
            };
            let ids: Vec<String> = req.params.get("ids").and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            {
                let prog = lock(&sr.prog);
                match crate::mcp::port_decl(&prog, in_rel) {
                    Some(port) if port.dir == crate::ast::PortDir::In && port.class == "rpc" => {}
                    _ => return Response::err(req.id, INVALID_PARAMS, format!(
                        "rel {in_rel} is not an @in(rpc) port in the daemon's loaded program")),
                }
            }
            let mut eng = lock_eng(sr, &req.method);
            match eng.retire_rpc(in_rel, &ids) {
                Ok(()) => Response::ok(req.id, json!({"ok": true})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
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
            let prog = lock(&sr.prog);
            let mut eng = lock_eng(sr, &req.method);
            let run = (|| -> anyhow::Result<()> {
                eng.insert_hook_event(kind, session, seq, json)?;
                eng.tick(&prog, true)
            })();
            sr.tick_count.fetch_add(1, Ordering::Relaxed);
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
            match run_eval(sr, text) {
                Ok(v) => Response::ok(req.id, v),
                Err((code, msg)) => Response::err(req.id, code, msg),
            }
        }
        "load" => {
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
                    match run_eval(sr, &text) {
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
                        let mut pf = lock(&sr.program_files);
                        let dup = pf.iter().any(|f| f == &canon);
                        if !dup { pf.push(canon.clone()); }
                        dup
                    };
                    match sr.reload_program() {
                        Ok(()) => {
                            if let Err(e) = lock_eng(sr, &req.method)
                                .save_program_meta(&lock(&sr.program_files).clone()) {
                                eprintln!("[{}] save_program_meta: {e}", sr.root_label());
                            }
                            let files: Vec<String> = lock(&sr.program_files).iter()
                                .map(|f| f.to_string_lossy().into_owned()).collect();
                            Response::ok(req.id, json!({
                                "loaded": canon.to_string_lossy(),
                                "already_loaded": already,
                                "program_files": files,
                            }))
                        }
                        Err(e) => {
                            if !already {
                                lock(&sr.program_files).retain(|f| f != &canon);
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
fn run_eval(sr: &Arc<ServedRoot>, text: &str) -> Result<Value, (i64, String)> {
    let toks = crate::lex::lex(text).map_err(|e| (INVALID_PARAMS, format!("lex: {e}")))?;
    let snippet = crate::parse::parse(toks).map_err(|e| (INVALID_PARAMS, format!("parse: {e}")))?;
    let snippet_queries: Vec<crate::ast::Item> = snippet
        .items
        .iter()
        .filter(|i| matches!(i, crate::ast::Item::Query(_)))
        .cloned()
        .collect();

    let mut merged = {
        let base = lock(&sr.prog);
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
    let mut eng = Engine::new(conn, sr.root.clone());
    eng.set_repos(served_repos(sr.key.is_none()));
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

/// Evaluate a `dl q <verb>` against a SCRATCH engine (never the served one):
/// build the embedded verb program (with the `target` fact injected), merge it
/// onto the base program so it inherits the served scan corpus, tick a fresh
/// in-memory engine, capture the verb's `?` query, and shape it into the
/// `{columns, rows, total, notes}` envelope with the `resolve_name` note. Mirrors
/// `run_eval`; the daemon-side of the `dl q` runner.
fn run_q_eval(
    sr: &Arc<ServedRoot>,
    verb: &str,
    arg: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Value, (i64, String)> {
    let Some(spec) = crate::verbs::find(verb) else {
        return Err((INVALID_PARAMS,
            format!("unknown verb {verb:?}; available verbs: {}", crate::verbs::verb_list())));
    };
    let snippet = crate::verbs::verb_program(spec, arg)
        .map_err(|e| (INVALID_PARAMS, format!("verb program: {e}")))?;
    let snippet_queries: Vec<crate::ast::Item> = snippet
        .items
        .iter()
        .filter(|i| matches!(i, crate::ast::Item::Query(_)))
        .cloned()
        .collect();
    let mut merged = {
        let base = lock(&sr.prog);
        Program { items: base.items.iter().cloned().chain(snippet.items).collect() }
    };
    let diags = crate::typecheck::check_and_normalize(&mut merged, "<verb>");
    if diags.iter().any(|d| d.severity == crate::ast::Severity::Error) {
        let msgs: Vec<String> = diags.iter()
            .filter(|d| d.severity == crate::ast::Severity::Error)
            .map(|d| d.msg.clone()).collect();
        return Err((INTERNAL_ERROR, format!("verb typecheck: {}", msgs.join("; "))));
    }
    let conn = db::open(None).map_err(|e| (INTERNAL_ERROR, format!("db: {e}")))?;
    let mut eng = Engine::new(conn, sr.root.clone());
    eng.set_repos(served_repos(sr.key.is_none()));
    eng.tick(&merged, true).map_err(|e| (INTERNAL_ERROR, format!("tick: {e}")))?;
    let qprog = Program { items: snippet_queries };
    let results = eng.run_queries_capture(&qprog)
        .map_err(|e| (INTERNAL_ERROR, format!("query: {e}")))?;
    let (columns, rows) = crate::verbs::shape(results);
    let total = rows.len();
    let rows = crate::verbs::page(rows, limit, offset);
    let notes = vec![crate::verbs::resolve_note(&eng, arg)];
    Ok(json!({"columns": columns, "rows": rows, "total": total, "notes": notes}))
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
/// management by having a `.dl/` directory.
pub fn enabled_for(root: &Path) -> bool {
    enabled() && root.join(".dl").is_dir()
}

/// True iff the singleton daemon is listening.
pub fn is_running() -> bool {
    UnixStream::connect(socket_path()).is_ok()
}

/// Connect to the singleton (must be already running). Returns a framed stream.
pub fn connect() -> Result<UnixStream> {
    UnixStream::connect(socket_path())
        .with_context(|| format!("connect daemon socket {}", socket_path().display()))
}

/// Inject the `root` envelope key into a params object (no-op for `None`).
fn with_root(mut params: Value, root: Option<&Path>) -> Value {
    if let Some(r) = root {
        params["root"] = json!(r.to_string_lossy());
    }
    params
}

/// Send one request, read one response.
pub fn rpc_call(stream: &mut UnixStream, req: &Request) -> Result<Response> {
    let body = serde_json::to_string(&req.to_json())?;
    rpc::write_frame(stream, &body)?;
    let resp_body = rpc::read_frame(stream)?
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection without responding"))?;
    let v: Value = serde_json::from_str(&resp_body)?;
    let r = Response::from_value(v)?;
    Ok(r)
}

/// Ensure the singleton daemon is running (spawn detached if not). Attaches only
/// if the running daemon runs THIS binary (build_id match); otherwise replaces
/// the stale daemon. `root`/`program` are accepted for call-site compatibility;
/// the root registers lazily on its first RPC (attach IS registration).
pub fn ensure_daemon(_root: &Path, _program: Option<&str>) -> Result<()> {
    ensure_singleton()
}

/// Spawn-if-missing for the singleton.
pub fn ensure_singleton() -> Result<()> {
    if is_running() {
        let mut s = connect()?;
        let req = Request::new(0, "ping", json!({}));
        if let Ok(r) = rpc_call(&mut s, &req) {
            if r.error.is_none() {
                let running = r.result.as_ref()
                    .and_then(|v| v.get("build_id"))
                    .and_then(|v| v.as_str());
                match running {
                    Some(id) if id == build_id() => return Ok(()),
                    Some(_) => {
                        eprintln!("[daemon] running binary changed — restarting daemon");
                        let _ = stop();
                    }
                    None => return Ok(()),
                }
            }
        }
    }
    spawn_detached()?;
    wait_ready()
}

/// Stop the singleton and respawn it detached with the CURRENT binary. The
/// `dl daemon restart` backend.
pub fn restart() -> Result<()> {
    let was_running = is_running();
    if was_running { let _ = stop(); }
    spawn_detached()?;
    let ready = wait_ready().is_ok();
    eprintln!("[daemon] {} (build {}){}",
        if was_running { "restarted" } else { "started" },
        build_id(),
        if ready { "" } else { " — starting (first tick still in progress)" });
    Ok(())
}

/// Spawn the singleton daemon detached (background, idle timeout on).
pub fn spawn_detached() -> Result<()> {
    let exe = std::env::current_exe()
        .context("locate current exe for daemon spawn")?;
    let home = daemon_home();
    std::fs::create_dir_all(&home)?;
    let log = home.join("daemon.log");
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

fn wait_ready() -> Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(CONNECT_TOTAL_TIMEOUT_SECS);
    let mut backoff_idx = 0;
    loop {
        if start.elapsed() > timeout {
            bail!("daemon did not become ready in {}s", CONNECT_TOTAL_TIMEOUT_SECS);
        }
        if let Ok(mut s) = UnixStream::connect(socket_path()) {
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
        remove_pid_file();
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

/// Block until the given root reports quiescent, or `timeout_ms` elapses.
pub fn await_quiescent(root: Option<&Path>, timeout_ms: u64) -> Result<(bool, u64)> {
    let mut s = connect()?;
    let _ = s.set_read_timeout(Some(Duration::from_millis(timeout_ms + 5_000)));
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

// ---------- small helpers shared with lib.rs ----------

pub(crate) fn load_repos_eager() -> Vec<config::RepoConfig> {
    match config::SprfConfig::load_default() {
        Ok(cfg) if !cfg.repos.is_empty() => cfg.repos,
        Ok(_) => Vec::new(),
        Err(e) => { eprintln!("[config] ignored: {e}"); Vec::new() }
    }
}

/// Whether daemon-SERVED engines ingest the ambient config repos. Off by
/// default (hermetic): a per-root served engine's corpus is its own repo plus
/// whatever its program declares, so one file save wakes only that root instead
/// of ticking every registered engine. `DL_AMBIENT_REPOS=1` on the daemon
/// process restores the old cross-root behavior. (Ad-hoc CLI runs never route
/// through here; they load config repos directly and are unaffected. Accepted
/// truthy spellings: `1` / `true` / `yes`. This is the daemon's counterpart to
/// the other `DL_*` process knobs like `DL_NO_FETCH`.)
pub(crate) fn ambient_repos_enabled() -> bool {
    matches!(
        std::env::var("DL_AMBIENT_REPOS").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Config repos to seed a daemon-served engine with. `is_config` = the
/// config-view engine (root:None), which EXISTS to serve the config repos and
/// always gets them. A per-root served engine (Rule B: hermetic daemon) gets
/// them only under `DL_AMBIENT_REPOS`; otherwise it stays hermetic (empty).
/// `Engine::set_repos` still canonical-dedupes whatever it receives, so a
/// config listing one directory under two slugs collapses to one repo.
pub(crate) fn served_repos(is_config: bool) -> Vec<config::RepoConfig> {
    if is_config || ambient_repos_enabled() {
        load_repos_eager()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The memory guard reads a real RSS for the running test process (a few MB
    /// at least) and the default ceiling is well above it, so a steady-state
    /// daemon never trips. Guards the `ps`-parse and the unit conversion.
    #[test]
    fn self_rss_is_readable_and_under_default_ceiling() {
        let rss = self_rss_mb().expect("ps must report this process's RSS");
        assert!(rss > 0, "RSS should be a positive MB figure, got {rss}");
        assert!(rss < DEFAULT_MEM_LIMIT_MB,
            "test process RSS {rss}MB should sit under the {DEFAULT_MEM_LIMIT_MB}MB ceiling");
    }

    /// `DL_DAEMON_MEM_MB=0` disables the guard; unset yields the default.
    #[test]
    fn mem_limit_env_zero_disables_else_default() {
        // Unset -> default (the env is process-global; assert the parse shape,
        // not a specific set, to stay hermetic against the ambient environment).
        assert_eq!("0".parse::<u64>().ok().filter(|&n| n > 0), None);
        assert_eq!("2048".parse::<u64>().ok().filter(|&n| n > 0), Some(2048));
        assert_eq!(DEFAULT_MEM_LIMIT_MB, 4096);
    }

    #[test]
    fn fresh_binary_compare_exits_only_for_two_confirmed_stamps() {
        let before = ExeStamp { len: 10, mtime: SystemTime::UNIX_EPOCH };
        let after = ExeStamp { len: 11, mtime: SystemTime::UNIX_EPOCH };
        assert!(should_exit_for_binary_change(Some(before), Some(after)));
        assert!(!should_exit_for_binary_change(Some(before), Some(before)));
        assert!(!should_exit_for_binary_change(Some(before), None));
        assert!(!should_exit_for_binary_change(None, Some(after)));
    }

    #[test]
    fn build_id_is_stable_and_version_prefixed() {
        let a = build_id();
        let b = build_id();
        assert_eq!(a, b, "build_id must be stable within one process");
        assert!(a.starts_with(env!("CARGO_PKG_VERSION")),
            "build_id should carry the crate version: {a}");
    }

    #[test]
    fn key_of_is_stable_and_short() {
        let p = std::path::Path::new("/tmp/some/root");
        let a = key_of(p);
        assert_eq!(a, key_of(p), "key must be deterministic");
        assert_eq!(a.len(), 16, "key is 16 hex chars");
    }

    /// Part 3: a healthy root (streak 0, incl. right after recovery) never
    /// skips a poll cycle.
    #[test]
    fn poll_backoff_zero_streak_skips_nothing() {
        assert_eq!(poll_backoff_cycles(0, 2), 0);
        assert_eq!(poll_backoff_cycles(0, 1), 0);
    }

    /// Part 3: 2, 4, 8, ... cycles per consecutive failure at the default
    /// 2s cadence, capped once the equivalent wall time would exceed ~60s.
    #[test]
    fn poll_backoff_doubles_then_caps_near_60s() {
        let secs = 2u64;
        let cap = 30; // 60s / 2s
        assert_eq!(poll_backoff_cycles(1, secs), 2);
        assert_eq!(poll_backoff_cycles(2, secs), 4);
        assert_eq!(poll_backoff_cycles(3, secs), 8);
        assert_eq!(poll_backoff_cycles(4, secs), 16);
        assert_eq!(poll_backoff_cycles(5, secs), cap, "2^5=32 already exceeds the 30-cycle cap");
        assert_eq!(poll_backoff_cycles(10, secs), cap, "stays capped, never grows unbounded");
        assert_eq!(poll_backoff_cycles(31, secs), cap, "a huge streak must not overflow/panic");
    }

    /// Part 3: the cap tracks `poll_secs` — a slower poll cadence caps at
    /// fewer cycles so the wall-clock ceiling (~60s) stays roughly constant.
    #[test]
    fn poll_backoff_cap_scales_with_poll_secs() {
        assert_eq!(poll_backoff_cycles(10, 10), 6, "60s / 10s = 6 cycles");
        assert_eq!(poll_backoff_cycles(10, 60), 1, "60s / 60s = 1 cycle, never zero");
        assert_eq!(poll_backoff_cycles(10, 0), 60, "poll_secs=0 must not divide by zero");
    }

    #[test]
    fn daemon_thread_count_caps_at_quarter_cores_with_floor_of_two() {
        assert_eq!(daemon_thread_count(1, None), 2);
        assert_eq!(daemon_thread_count(2, None), 2);
        assert_eq!(daemon_thread_count(4, None), 2);
        assert_eq!(daemon_thread_count(7, None), 2);
        assert_eq!(daemon_thread_count(8, None), 2);
        assert_eq!(daemon_thread_count(16, None), 4);
        assert_eq!(daemon_thread_count(32, None), 8);
        assert_eq!(daemon_thread_count(100, None), 25);
    }

    #[test]
    fn daemon_thread_count_env_override_wins_when_positive() {
        assert_eq!(daemon_thread_count(16, Some("1")), 1);
        assert_eq!(daemon_thread_count(16, Some("6")), 6);
        assert_eq!(daemon_thread_count(16, Some("99")), 99);
        // Zero / empty / non-numeric fall back to the cores heuristic.
        assert_eq!(daemon_thread_count(16, Some("0")), 4);
        assert_eq!(daemon_thread_count(16, Some("")), 4);
        assert_eq!(daemon_thread_count(16, Some("bogus")), 4);
    }
}
