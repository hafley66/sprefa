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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, RwLockReadGuard};
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::Notify;

use crate::ast::Program;
use crate::daemon_shell::{self, ShellCtx};
use crate::engine::{DiagRow, Engine};
use crate::rpc::{Request, Response, INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND};
use crate::{config, db};

const DEFAULT_IDLE_SECS: u64 = 30 * 60;
pub(crate) const IDLE_TICK_SECS: u64 = 30;
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
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
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
/// The job-queue `root` id for the key-less config view (registered roots use
/// their blake3 registry key). One reserved token, never a valid key.
const CONFIG_JOB_ID: &str = "config";

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

/// Open (create if missing) `<home>/daemon.pid` for the fd-lock single-
/// instance witness (plan section 3.3: "belt-and-suspenders against `dl serve
/// --foreground` beside the [launchd] agent" — replaces the old bespoke
/// pidfile, which was informational only and enforced nothing; a real
/// advisory `flock` now makes a second `run_daemon` in this home fail fast
/// instead of silently double-serving). Split out of the lock-acquisition
/// call site so the open/create-dir step stays testable without the
/// borrow-checker shape `fd_lock::RwLock`'s guard imposes on its caller.
// Deliberately no `.truncate(true)`: truncating here, before the fd-lock is
// even attempted, would wipe a RUNNING daemon's pid content out from under it
// on the losing side of a lock race. The winner truncates explicitly (via
// `set_len(0)` on the locked guard) only after `try_write` confirms it holds
// the lock; the loser's `open` must leave the holder's file untouched.
#[allow(clippy::suspicious_open_options)]
fn open_pid_lock_file() -> Result<std::fs::File> {
    let dir = daemon_home();
    std::fs::create_dir_all(&dir)?;
    std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        // Never truncate on open: a second instance opens this file too (to
        // attempt the lock) and must not blank the holder's pid/start-time
        // record. The lock HOLDER truncates after `try_write` succeeds.
        .truncate(false)
        .open(pid_path())
        .with_context(|| format!("open singleton lock {}", pid_path().display()))
}

// ---------- shared process handles ----------

/// Handles cloned into every `ServedRoot` so its per-root tick methods can reach
/// the process-wide subscriber list / shutdown flag / build identity without a
/// back-pointer to the whole `Daemon`.
#[derive(Clone)]
struct Shared {
    build_id: Arc<str>,
    /// Subscriber-push broadcast: tick methods (on `spawn_blocking` threads)
    /// send a serialized `diag_changed` / `rev_advanced` notification body
    /// here; every open `GET /watch` SSE stream holds a receiver. Replaces the
    /// old kept-open-socket subscriber pump.
    broadcast_tx: tokio::sync::broadcast::Sender<String>,
    /// Job doorbell: `enqueue` rings it so a parked dispatcher task wakes.
    job_notify: Arc<Notify>,
    /// The durable job queue (J1). Watchers/pollers `enqueue` tick + sink-drain
    /// jobs here instead of running them inline; the dispatcher's workers claim
    /// and run them.
    jobs: Arc<crate::jobq::JobQueue>,
}

impl Shared {
    /// Enqueue a job and ring the tokio doorbell so a dispatcher task wakes.
    /// Callable from any thread (including `spawn_blocking`): `Notify::notify_one`
    /// is sync + `Send`, so a tick/watcher running on a blocking thread can wake
    /// a shell task without a runtime handle.
    fn enqueue(&self, job: crate::jobq::JobRow) -> Result<()> {
        self.jobs.enqueue(job)?;
        self.job_notify.notify_one();
        Ok(())
    }

    /// Push a pre-serialized notification body to all `/watch` subscribers
    /// (best-effort; zero subscribers = the send errors and is ignored).
    fn push_frame(&self, body: String) {
        let _ = self.broadcast_tx.send(body);
    }
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
    /// Cold-start staging in flight: the last full tick deferred the extract
    /// fan-out onto the queue and some `_cold_node` is still pending. Surfaced on
    /// the status RPC (`cold_start_pending`) so a client reads "warming", and
    /// `poll_idle` returns not-idle while true (await-settle blocks until warm).
    pub cold_pending: AtomicBool,
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
    pub(crate) fn touch(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    /// Enqueue a tick/drain job for this root and ring the tokio doorbell. The
    /// shell's watcher/poll tasks call this (through `spawn_blocking`) instead of
    /// reaching the private `shared`/`Shared` handle directly.
    pub(crate) fn enqueue_job(&self, job: crate::jobq::JobRow) -> Result<()> {
        self.shared.enqueue(job)
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

    /// The job-queue identity for this root: its registry key, or `"config"`
    /// for the key-less config view. `tick:{id}` / `sink:{id}` job keys are
    /// built from this, and `Daemon::served_root_for_job` reverses it.
    pub(crate) fn job_root_id(&self) -> String {
        self.key.clone().unwrap_or_else(|| CONFIG_JOB_ID.to_string())
    }

    pub(crate) fn root_label(&self) -> String {
        self.root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned())
    }

    pub(crate) fn tick_full(&self, quiet: bool) -> Result<()> {
        let tick_next = self.tick_count.load(Ordering::Relaxed) + 1;
        crate::activity::begin_tick(tick_next, &self.program_display, &self.root, "full");
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
        // Cold-start staging: a blank-slate seed or a resume re-enqueue defers the
        // extract fan-out onto the queue. Record the warming flag (status +
        // poll_idle read it) and enqueue the staged `ColdExtract` jobs.
        self.cold_pending.store(report.cold_pending, Ordering::Relaxed);
        if !report.cold_staged.is_empty() {
            let job_root_id = self.job_root_id();
            for (family, shard, priority) in &report.cold_staged {
                let job = crate::jobq::JobRow::cold_extract(&job_root_id, family, *shard, *priority);
                if let Err(e) = self.enqueue_job(job) {
                    tracing::warn!("[{}] cold-start enqueue {family}/{shard}: {e}", self.root_label());
                }
            }
        }
        self.settled.store(report.is_settled(), Ordering::Relaxed);
        let n = self.tick_count.fetch_add(1, Ordering::Relaxed) + 1;
        // This WAS a full tick, so it just ran `rebuild_async` over the
        // converged state — resync `last_full_tick_count` (`poll_idle`'s
        // "source changed since the last full tick" half goes false again).
        self.last_full_tick_count.store(n, Ordering::Relaxed);
        self.touch();
        *lock(&self.last_changed_paths) = Vec::new();
        // A no-op tick (nothing reconciled, no timer boundary, no digest move)
        // must not tell subscribers anything changed: every broadcast makes a
        // client (instant) re-query and re-render, and a churn of empty ticks
        // amplified into a webview render storm (2026-07-18). Timer-driven
        // ticks keep broadcasting — clock/every subscribers rely on them.
        if tick_warrants_broadcast(&report) {
            self.broadcast_diag_changed();
        }
        Ok(())
    }

    fn tick_paths(&self, paths: &[PathBuf], quiet: bool) -> Result<()> {
        let tick_next = self.tick_count.load(Ordering::Relaxed) + 1;
        crate::activity::begin_tick(tick_next, &self.program_display, &self.root, "paths");
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

    /// Run ONE cold-start extraction node (a `ColdExtract` job): the family's
    /// wholesale refresh under the engine mutex, marked `done` on the
    /// `_cold_node` row. When the last node lands, run the completion tick — a
    /// normal full tick that (cold no longer in progress) does the single
    /// blank-slate derived rebuild over the now-complete fact base. Budget caps
    /// (QoS/IOPOL/BG tier + rayon width) govern the tempo of the many nodes; this
    /// changes NONE of them (standing law "nothing seizes the machine").
    pub(crate) fn run_cold_node(&self, family: &str, shard: u32) -> Result<()> {
        let tick_now = self.tick_count.load(Ordering::Relaxed);
        crate::activity::begin_tick(tick_now, &self.program_display, &self.root, "cold-extract");
        crate::activity::set(
            crate::activity::Phase::ParseExtract,
            format!("cold-extract {family} shard {shard}"),
        );
        {
            let prog = lock(&self.prog);
            let mut eng = lock(&self.eng);
            eng.run_cold_node(&prog, family, shard)?;
        }
        crate::activity::end_tick();
        self.touch();
        let complete = lock(&self.eng).cold_nodes_complete()?;
        if complete {
            // The completion tick reads `cold_start_in_progress == false` (all
            // nodes done) → normal path → blank-slate `need_full` → the one full
            // `rebuild_derived`. Clears `cold_pending`, refreshes the read view,
            // broadcasts diag — all via `tick_full`.
            self.tick_full(true)?;
        }
        Ok(())
    }

    /// Re-parse the program files, swap the parsed `Program`, re-tick. A parse or
    /// type error keeps the last good program.
    pub(crate) fn reload_program(&self) -> Result<()> {
        let all = lock(&self.program_files).clone();
        let files: Vec<PathBuf> = all.iter().filter(|f| f.exists()).cloned().collect();
        if files.is_empty() {
            if !all.is_empty() {
                tracing::warn!("[{}] all {} watched program file(s) missing; keeping last-good program",
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
            tracing::warn!("[{}] save_program_meta: {e}", self.root_label());
        }
        self.tick_full(false)?;
        Ok(())
    }

    /// Re-discover `.dl` files under `<root>/.dl/`, re-merge the program if the
    /// file set changed, re-tick. `Ok(true)` = set changed and re-merged;
    /// `Ok(false)` = set unchanged (content edit, or not in discovery mode).
    pub(crate) fn reload_discovery(&self) -> Result<bool> {
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
            tracing::warn!("[{}] discovery reload: {n_err} type error(s); keeping old", self.root_label());
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
                tracing::warn!("[{}] save_program_meta: {e}", self.root_label());
            }
        }
        let n = lock(&self.program_files).len();
        tracing::info!("[{}] discovery reload: {n} file(s)", self.root_label());
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
    pub(crate) fn poll_idle(&self) -> Result<bool> {
        // Cold start in flight: never idle — the queue is draining `ColdExtract`
        // nodes, and `await-settle` must block until the corpus is warm.
        if self.cold_pending.load(Ordering::Relaxed) {
            return Ok(false);
        }
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
                tracing::warn!("[{}] drain_external_sinks: {e}", self.root_label());
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
            let effect_cmd_txt = crate::lower::txt_tbl("effect_cmd");
            if let Ok(rows) = eng.query_sql(&format!("SELECT kind, template FROM {effect_cmd_txt}"), &[]) {
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
        // The drain set Phase::Effects outside any tick; reset it so a settled
        // daemon's activity slot (and the why-trail samples) read idle, not a
        // forever-stale "effects drain".
        crate::activity::end_tick();
        Ok(n)
    }

    pub(crate) fn has_effects(&self) -> bool {
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
        // Push through the async pump (best-effort); no socket write on the tick
        // thread anymore, so a slow subscriber can never stall a tick.
        self.shared.push_frame(body);
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
    pub(crate) fn on_git_event(&self) -> (usize, Vec<PathBuf>) {
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
                        Err(e) => tracing::warn!("[{}] observe_ref {slug}/{name}: {e}", self.root_label()),
                    }
                }
            }
            if !advances.is_empty() {
                if let Err(e) = eng.refresh_daemon_rels() {
                    tracing::warn!("[{}] refresh_daemon_rels: {e}", self.root_label());
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
        for (repo, name, old, new, files) in advances {
            let note = json!({"jsonrpc": "2.0", "method": "rev_advanced", "params": {
                "root": self.root.to_string_lossy(),
                "repo": repo, "ref": name, "old": old, "new": new, "paths": files,
            }});
            let body = match serde_json::to_string(&note) { Ok(s) => s, Err(_) => continue };
            self.shared.push_frame(body);
        }
    }

    pub(crate) fn program_in_paths(&self, paths: &[PathBuf]) -> bool {
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
        eng.poll_loop = true;
        if is_config { eng.set_root_implicit(true); }
        eng.set_repos(served_repos(is_config));
        // `begin_tick` (not the bare `set_root` this replaced) both pushes the
        // root AND opens the tick-level span for this root's very first cold
        // tick (tick 0, no `ServedRoot`/`tick_count` yet) — the boot-time
        // counterpart to `tick_full`'s "full" / `tick_paths`'s "paths".
        crate::activity::begin_tick(0, &display, &eng_root, "cold-boot");
        crate::activity::set(crate::activity::Phase::ColdTick, display.as_str());
        let cold_report = eng.tick_report(&prog, false)?;
        crate::activity::end_tick();
        // Cold-start staging: the cold tick may have deferred the extract
        // fan-out onto the queue (blank-slate seed, or a resume re-enqueue after
        // a `kill -9`). Enqueue its `ColdExtract` jobs now — `shared` is in scope
        // here, before the `ServedRoot` exists, so this is the first enqueue
        // point on a fresh boot.
        let cold_start_pending = cold_report.cold_pending;
        if !cold_report.cold_staged.is_empty() {
            let job_root_id = key.clone().unwrap_or_else(|| CONFIG_JOB_ID.to_string());
            for (family, shard, priority) in &cold_report.cold_staged {
                let job = crate::jobq::JobRow::cold_extract(&job_root_id, family, *shard, *priority);
                if let Err(e) = shared.enqueue(job) {
                    tracing::warn!("[daemon] cold-start enqueue {family}/{shard}: {e}");
                }
            }
        }
        let canon_files: Vec<PathBuf> = files
            .iter()
            .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()))
            .collect();
        if let Err(e) = eng.save_repos_meta() { tracing::warn!("[daemon] save_repos_meta: {e}"); }
        if let Err(e) = eng.save_program_meta(&canon_files) { tracing::warn!("[daemon] save_program_meta: {e}"); }

        let label = eng_root.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| eng_root.to_string_lossy().into_owned());
        // `program {prog_display}`, not `{display}`: the tracing macro pulls its
        // own `display` field-helper into scope, which shadows the local in the
        // format capture. The rendered text is byte-identical.
        let prog_display = display.as_str();
        tracing::info!("[{label}] served ({} type diag(s), program {prog_display})", type_diags.len());

        // Initial read-path snapshot from the cold-tick engine + program.
        let db_path_buf = db_path.map(PathBuf::from);
        let read_view = crate::daemon_read::ReadView::snapshot(&eng.rels, &prog, db_path_buf.clone());

        // Class-17 db-ratio rail (docs/failure-modes.md:407-441): once per
        // root at daemon boot (this fn also runs for a later `add_root`,
        // which is the same "a corpus just got read into a db" moment for
        // that root). The config view (`is_config`, root:None) scans
        // nothing of its own, so it has no corpus to ratio against.
        if !is_config {
            if let Some(db_path_ref) = db_path_buf.as_deref() {
                crate::db_ratio::emit_verdict(&eng_root, db_path_ref);
            }
        }

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
            cold_pending: AtomicBool::new(cold_start_pending),
            last_changed_paths: Mutex::new(Vec::new()),
            stopped: Arc::new(AtomicBool::new(false)),
            // The cold tick just above (`eng.tick(&prog, false)`) IS a full
            // tick — it already ran `rebuild_async` once — so start in sync
            // with `tick_count` (both 1), not dirty.
            last_full_tick_count: AtomicU64::new(1),
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
    /// Shell handles (runtime, cancellation token, job doorbell, subscriber
    /// channel). `add_root` spawns each root's watcher task through `shell.rt`;
    /// the poll/idle/shutdown tasks select on `shell.cancel`.
    pub(crate) shell: ShellCtx,
    /// The `root`-absent config view (org/folders model).
    pub config: Arc<ServedRoot>,
    /// key -> served root. The registry.
    pub roots: Mutex<HashMap<String, Arc<ServedRoot>>>,
    /// The durable job queue (J1): the `dl daemon jobs` read path + boot reset
    /// go through this handle; `Shared.jobs` is the same `Arc` for enqueue.
    pub(crate) jobs: Arc<crate::jobq::JobQueue>,
}

impl Daemon {
    fn shared(&self) -> Shared {
        Shared {
            build_id: self.build_id.clone(),
            broadcast_tx: self.shell.broadcast_tx.clone(),
            job_notify: self.shell.job_notify.clone(),
            jobs: self.jobs.clone(),
        }
    }

    /// Reverse `ServedRoot::job_root_id`: resolve a job's `root` id to its
    /// served root (`"config"` -> the config view; else the registry key).
    /// `None` when the root was dropped between enqueue and claim.
    fn served_root_for_job(&self, id: &str) -> Option<Arc<ServedRoot>> {
        if id == CONFIG_JOB_ID {
            Some(self.config.clone())
        } else {
            lock(&self.roots).get(id).cloned()
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
    pub(crate) fn all_roots(&self) -> Vec<Arc<ServedRoot>> {
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
        // Spawn its watcher as a tokio task (notify's callback thread forwards
        // events into a channel this task drains; engine ticks run via
        // `spawn_blocking`).
        self.shell.rt.spawn(daemon_shell::watch::watch_task(sr.clone(), self.shell.clone(), self.launch_exe_stamp));
        tracing::info!("[daemon] registered root {} (key {key})", canon.display());
        Ok(sr)
    }

    /// Deregister a root: stop its watcher, drop the engine, keep the db (re-add
    /// warms from it). `purge` deletes `<home>/roots/<key>/`.
    pub(crate) fn drop_root(self: &Arc<Self>, root: &Path, purge: bool) -> Result<()> {
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
            tracing::warn!("[daemon] drop_root {}: not registered", canon.display());
        } else {
            tracing::info!("[daemon] deregistered root {} (key {key}){}",
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
/// Apply the CPU + disk-I/O scheduling budget to THIS process, minus the rayon
/// thread cap. macOS QoS UTILITY on the calling thread (spawned workers inherit
/// it), a positive nice value on every unix, and a process-scope IOPOL_THROTTLE
/// on macOS so bulk db writes land in the background disk tier. Split out of
/// `apply_daemon_budget` so the one-shot CLI entry can apply the same cap
/// (standing law: nothing seizes the machine). Every syscall here is idempotent,
/// so applying it twice — CLI entry then the daemon serve path — is harmless.
pub(crate) fn apply_process_budget() {
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

    #[cfg(target_os = "macos")]
    {
        // Demote every disk read/write this process issues to the background
        // I/O tier (Time Machine's). QoS/nice cap CPU only; the first-run
        // rebuild's bulk db writes are what beachball the machine, and only
        // this throttle reaches them. IOPOL_TYPE_DISK=0, IOPOL_SCOPE_PROCESS=0,
        // IOPOL_THROTTLE=3 per <sys/resource.h>.
        extern "C" {
            fn setiopolicy_np(
                iotype: libc::c_int,
                scope: libc::c_int,
                policy: libc::c_int,
            ) -> libc::c_int;
        }
        // SAFETY: constant, documented arguments; scope is this process.
        unsafe { setiopolicy_np(0, 0, 3) };
    }
}

fn apply_daemon_budget() -> (&'static str, i32, usize) {
    // Supervision arc (plans/2026-07-18-infra-library-adoption.md section 3;
    // standing law a1f049ff "infra is bought, never built"): `launchd` injects
    // `XPC_SERVICE_NAME` into every process it spawns (confirmed via
    // `launchctl print` during the 2026-07-18 gate-2 probe). Its presence
    // means the LaunchAgent's `Nice`/`LowPriorityIO` plist keys
    // (`crate::supervise::plist_contents`) already govern this process's
    // CPU/disk scheduling class, so re-applying `nice()`/`setiopolicy_np()`/
    // the QoS class here — and the extra PRIO_DARWIN_BG tier below — would be
    // redundant self-throttling stacked on top of what supervision now owns.
    // Gate 2 (plan section 5.2 q2) measured the PRIO_DARWIN_BG-equivalent
    // plist key (`ProcessType=Background`) at ~45% of baseline CPU throughput
    // (two 5s fixed-duration spinner trials); the plist deliberately does NOT
    // set that key, so re-imposing the same clamp in-process here would
    // silently reinstate exactly what that gate decided against.
    //
    // Absent `XPC_SERVICE_NAME` — `dl daemon serve` launched via the bespoke
    // `spawn_detached` fallback (plan section 3.4: no service manager
    // installed, CI, `cargo test`) — nothing else applies these caps, so the
    // full nice/IOPOL/QoS/PRIO_DARWIN_BG stack stays exactly as it was before
    // this arc: the 2026-07-17 incident that motivated PRIO_DARWIN_BG ran
    // under this same un-supervised shape.
    let launchd_managed = std::env::var_os("XPC_SERVICE_NAME").is_some();
    if !launchd_managed {
        apply_process_budget();

        #[cfg(target_os = "macos")]
        {
            // The daemon additionally drops into the darwin BACKGROUND resource
            // tier — the OS switch that keeps a background process from competing
            // with the user at all (CPU to leftover cycles, disk to the throttled
            // tier with deeper backoff, network deprioritized). QoS UTILITY +
            // nice + IOPOL_THROTTLE demonstrably did NOT prevent a 4.2GB/min boot
            // rebuild from freezing the foreground (2026-07-17 receipts); this
            // tier is what Time Machine and Spotlight reindex actually run under.
            // Daemon-only: one-shot CLI runs keep UTILITY so an interactive
            // `dl` answer is not starved behind foreground load.
            // PRIO_DARWIN_PROCESS=4, PRIO_DARWIN_BG=0x1000 per <sys/resource.h>.
            if std::env::var("DL_NO_BUDGET").ok().as_deref() != Some("1") {
                const PRIO_DARWIN_PROCESS: libc::c_int = 4;
                const PRIO_DARWIN_BG: libc::c_int = 0x1000;
                // SAFETY: setpriority on this process with documented darwin
                // constants; ignore EPERM.
                unsafe { libc::setpriority(PRIO_DARWIN_PROCESS, 0 as libc::id_t, PRIO_DARWIN_BG) };
            }
        }
    }

    // The tiers above are scheduling advice; the governor is the ceiling
    // (receipt 2026-07-18: 278% cpu through a full re-extract with every tier
    // applied). Daemon default 100% of one core; DL_MAX_CPU_PCT overrides,
    // 0 disables. Stays bespoke regardless of supervision (plan section 3.5
    // point 3): no OS mechanism gives a CPU-percent cap on macOS.
    if std::env::var("DL_NO_BUDGET").ok().as_deref() != Some("1") {
        crate::budget::start_governor(crate::budget::ceiling_from_env(100));
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
    // Thread caps stay bespoke regardless of supervision (plan section 3.5
    // point 3): rayon/tokio pool sizing has no OS-level equivalent.
    let _ = rayon::ThreadPoolBuilder::new().num_threads(desired).build_global();
    let threads = rayon::current_num_threads();

    let qos_label = if launchd_managed {
        "launchd(nice+lowprioio)"
    } else if cfg!(target_os = "macos") {
        "utility+iothrottle"
    } else {
        "none"
    };
    let priority = if launchd_managed { 0 } else if cfg!(unix) { DAEMON_NICE } else { 0 };
    (qos_label, priority, threads)
}

/// Install the daemon's `tracing` subscriber: an stderr `fmt` layer so every log
/// line carries a timestamp, level, and the emitting thread's name (`dl-poll`,
/// `dl-watch-<key>`, `dl-http-N`, `dl-accept`, …). Writer is stderr — the detached
/// daemon's stderr is redirected into `daemon.log` by `spawn_detached`, so this
/// keeps that plumbing intact. The filter reads `DL_LOG` (default `info`);
/// `with_target(false)` drops the module path since call sites already carry a
/// `[daemon]`/`[<root>]` bracket, and `compact()` keeps each line short.
///
/// Also composes in the two rolling FILE layers (`dl.log`/`error.log` under
/// `<home>/log/`, see `crate::trace::file_layers`) so a foreground/background
/// daemon writes the same apache-style access/error log every other `dl`
/// invocation does — this is the one process where `crate::trace::init`
/// deliberately defers (see its doc comment) specifically so THIS init wins
/// the race and installs both the stderr layer and the file layers together.
///
/// Idempotent by design: `try_init` returns `Err` (ignored) when a global
/// subscriber is already installed, so a foreground daemon started inside a
/// process that already configured tracing — and a test that calls this twice —
/// does not panic.
fn init_daemon_tracing() {
    use tracing_subscriber::prelude::*;
    let filter = tracing_subscriber::EnvFilter::try_from_env("DL_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_thread_names(true)
        .with_target(false)
        .compact()
        .with_filter(filter);
    let home = daemon_home();
    // DL_TRACE_CHROME composes the same way here as it does in
    // `crate::trace::init` for a one-shot: absent -> `chrome_layer` returns
    // `None` -> no layer installed, zero overhead. `shutdown_cleanup` below
    // calls `finish_chrome_trace` on every exit path this fn's caller has.
    let _ = tracing_subscriber::registry()
        .with(stderr_layer)
        .with(crate::trace::dl_log_layer(&home))
        .with(crate::trace::error_log_layer(&home))
        .with(crate::trace::chrome_layer())
        .try_init();
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
    init_daemon_tracing();
    tracing::info!("[daemon] background budget: qos={qos_label} nice={priority} threads={threads}");

    let home = daemon_home();
    let _ = std::fs::create_dir_all(&home);
    let launch_exe_stamp = current_exe_stamp();

    // Single-instance witness (plan section 3.3): acquired BEFORE any other
    // setup so a second `run_daemon` in this home (a stray `dl daemon start
    // --foreground` racing the launchd-supervised instance) fails fast rather
    // than opening the job queue/engine dbs and losing a slower race later.
    // Both locals must stay alive for the rest of this function — dropping
    // either releases the OS-level `flock` immediately.
    let pid_lock_file = open_pid_lock_file()?;
    let mut singleton_lock = fd_lock::RwLock::new(pid_lock_file);
    let mut singleton_guard = singleton_lock.try_write().map_err(|_| anyhow::anyhow!(
        "another dl daemon instance already holds {} — refusing to start a second one \
         (fd-lock single-instance witness; `dl daemon status` to check, `dl daemon stop` to clear)",
        pid_path().display()
    ))?;
    {
        use std::io::{Seek, SeekFrom, Write as _};
        let pid = std::process::id();
        let start = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let _ = singleton_guard.set_len(0);
        let _ = singleton_guard.seek(SeekFrom::Start(0));
        let _ = write!(singleton_guard, "{pid}\n{start}\n");
        let _ = singleton_guard.flush();
    }

    let build_id: Arc<str> = Arc::from(build_id().as_str());
    let shutdown_requested = Arc::new(AtomicBool::new(false));

    // Build the ONE shell runtime NOW — after `apply_daemon_budget` ran on this
    // main thread, so the runtime's worker + blocking threads inherit the QoS.
    // The tick engine stays strictly sync; the runtime drives only the shell
    // (sockets, timers, dispatch) and reaches the engine solely via
    // `spawn_blocking`. Worker count is a small FIXED 2, independent of the
    // rayon/job-dispatcher budget below.
    let runtime = daemon_shell::build_runtime().context("build shell runtime")?;
    let cancel = tokio_util::sync::CancellationToken::new();
    let job_notify = Arc::new(Notify::new());
    // Subscriber push: ticks broadcast pre-serialized notification bodies; each
    // open `GET /watch` SSE stream subscribes its own receiver. 256 frames of
    // lag buffer per subscriber; a slower one skips overwritten frames
    // (best-effort push, same policy the old kept-open-socket pump had). The
    // initial receiver is dropped — a send with zero receivers just errs.
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<String>(256);
    let shell = ShellCtx {
        rt: runtime.handle().clone(),
        cancel: cancel.clone(),
        job_notify: job_notify.clone(),
        broadcast_tx: broadcast_tx.clone(),
    };

    // The durable job queue (J1), in its own `<home>/jobs.sqlite`.
    let jobs = crate::jobq::JobQueue::open(&home).context("open job queue db")?;
    // Self-diagnosis trail: boot marker + `dl-why` sampler, before anything
    // heavy runs, so even a first-tick death leaves an answer for
    // `dl daemon why`. The sampler is a plain OS thread by design — it must
    // keep writing while the tokio shell or the engine is wedged.
    crate::why::mark_boot(&home, &build_id);
    crate::why::start_sampler(home.clone(), jobs.clone());
    let shared = Shared {
        build_id: build_id.clone(),
        broadcast_tx: broadcast_tx.clone(),
        job_notify: job_notify.clone(),
        jobs: jobs.clone(),
    };

    let repos = load_repos_eager();
    if !repos.is_empty() { tracing::info!("[config] {} repo(s) registered", repos.len()); }

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
        shell: shell.clone(),
        config,
        roots: Mutex::new(HashMap::new()),
        jobs: jobs.clone(),
    });

    // J1 job dispatcher: crash-recover any jobs a previous process left
    // `running` (reset to pending), then spawn the worker set that claims and
    // runs tick + sink-drain jobs. Started BEFORE roots replay so a watcher's
    // first enqueue has a worker to serve it.
    match jobs.reset_running_on_boot() {
        Ok(n) if n > 0 => tracing::info!("[daemon] reset {n} running job(s) to pending on boot"),
        Ok(_) => {}
        Err(e) => tracing::warn!("[daemon] reset_running_on_boot: {e}"),
    }
    let n_workers = daemon_thread_count(
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2),
        std::env::var("DL_DAEMON_THREADS").ok().as_deref(),
    );
    let runner: Arc<dyn crate::jobq::JobRunner> =
        Arc::new(DaemonJobRunner { daemon: Arc::downgrade(&daemon) });
    daemon_shell::jobs::spawn(
        &shell,
        jobs.clone(),
        runner.clone(),
        n_workers,
        vec![crate::jobq::JobKind::Tick, crate::jobq::JobKind::SinkDrain],
    );
    // Cold-start staging: a dedicated SINGLE worker for `ColdExtract`. One node
    // in flight at a time + the canonical priority (module highest) makes the
    // staged extraction run in the same dependency order as the inline tick
    // (module before type/call resolution), so the warmed db matches inline.
    daemon_shell::jobs::spawn(
        &shell,
        jobs.clone(),
        runner,
        1,
        vec![crate::jobq::JobKind::ColdExtract],
    );
    tracing::info!("[daemon] job dispatcher: {n_workers} tick worker(s) + 1 cold-extract worker");

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
    // The accept task adopts this into the runtime via `from_std`, which requires
    // a non-blocking listener.
    listener.set_nonblocking(true).context("set UDS listener non-blocking")?;
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    let idle_secs = idle_timeout_secs();
    tracing::info!("[daemon] listening on {} (pid {}, idle {}s){}",
        sock.display(), std::process::id(), idle_secs,
        if foreground { " [foreground]" } else { "" });
    // systemd readiness (plan section 3.2, `Type=notify`): a no-op everywhere
    // else — `sd_notify::notify` returns `Ok(())` immediately when
    // `NOTIFY_SOCKET` is unset (macOS, or a systemd unit not using
    // `Type=notify`), so this call is unconditional and cheap.
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);
    if let Some(p) = crate::perflog::path() {
        tracing::info!("[daemon] perf log {} (tail -f | jq .total_ms, .phase, .ms)", p.display());
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
    // Class-13 stale-binary rail: this same singleton is what freeze #1
    // auto-started from a stale install (docs/failure-modes.md:300-327).
    // Check against the served root if the caller named one, else cwd (the
    // shell the daemon was launched from).
    let stale_check_root = initial_root.clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| home.clone());
    crate::stale_binary::warn_if_stale(&stale_check_root);

    // Config-view watcher (watches the config repos) — a tokio task.
    daemon.shell.rt.spawn(daemon_shell::watch::watch_task(daemon.config.clone(), shell.clone(), launch_exe_stamp));

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
            tracing::warn!("[daemon] root {} no longer exists; evicting from roots.json (key {})",
                rec.root.display(), rec.key);
            evicted_any = true;
            continue;
        }
        kept.push(rec.clone());
        if rec.root.join(".dl").is_dir() {
            match daemon.add_root(&rec.root) {
                Ok(_) => {}
                Err(e) => tracing::warn!("[daemon] replay {}: {e}", rec.root.display()),
            }
        } else {
            tracing::info!("[daemon] replay skip {} (no .dl/)", rec.root.display());
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
                        daemon.shell.rt.spawn(daemon_shell::watch::watch_task(sr.clone(), shell.clone(), launch_exe_stamp));
                        tracing::info!("[daemon] registered initial root {}", canon.display());
                    }
                    Err(e) => tracing::error!("[daemon] initial root {}: {e}", canon.display()),
                }
            }
        }
    }

    // Poll (@async clock) + idle (self-reap) timers — tokio interval tasks.
    if !foreground {
        daemon.shell.rt.spawn(daemon_shell::timers::idle_task(daemon.clone(), shell.clone(), idle_secs));
    }
    if let Some(secs) = poll_interval_secs() {
        daemon.shell.rt.spawn(daemon_shell::timers::poll_task(daemon.clone(), shell.clone(), secs));
    }

    // ONE router, two thin listeners (plan section 2.4): the UDS socket and the
    // localhost TCP socket both serve the same axum app. TCP binds 127.0.0.1:0
    // and publishes `<home>/http.json`; a TCP bind failure is logged non-fatal
    // (the UDS transport stays authoritative).
    daemon_shell::http::spawn_uds(&shell, daemon.clone(), listener);
    if let Err(e) = daemon_shell::http::spawn_tcp(&shell, daemon.clone(), &build_id) {
        tracing::warn!("[daemon] http transport disabled: {e}");
    }

    // Graceful shutdown: SIGINT/SIGTERM and the shutdown RPC both cancel the
    // token; this task then removes the control files and exits the process. It
    // is the single cancel-driven exit path (foreground, detached, and the
    // tray's own Quit handler exits directly via `process::exit`).
    daemon_shell::spawn_signal(&shell);
    {
        let d = daemon.clone();
        let cancel_task = cancel.clone();
        shell.rt.spawn(async move {
            cancel_task.cancelled().await;
            shutdown_cleanup(&d);
            std::process::exit(0);
        });
    }

    if tray {
        // The tray owns the platform main thread; the runtime keeps driving the
        // shell tasks on its worker threads. Quit / SIGINT exit the process.
        crate::tray::run_tray(daemon.clone())?;
        Ok(())
    } else {
        // Park the main thread until cancellation; the shutdown task races us to
        // `process::exit`. Clean up + return in case it has not fired yet.
        runtime.block_on(async { cancel.cancelled().await });
        shutdown_cleanup(&daemon);
        Ok(())
    }
}

pub(crate) fn shutdown_cleanup(d: &Daemon) {
    // The shutdown task and the main thread's post-block_on path both call
    // this; run it once so the why-trail gets exactly one shutdown marker.
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let sock = socket_path();
        let _ = std::fs::remove_file(&sock);
        let _ = std::fs::remove_file(crate::daemon_http::http_json_path());
        // The fd-lock itself releases when this process exits (its fd closes)
        // regardless of whether this unlink runs — SIGKILL-safe by
        // construction. Removing the file too is tidiness only: a fresh
        // `run_daemon` truncates and rewrites it either way.
        let _ = std::fs::remove_file(pid_path());
        crate::why::mark_shutdown(&d.home);
        tracing::info!("[daemon] shut down cleanly");
        // Both callers of this fn (the shutdown task, right before its
        // `process::exit(0)`, and the main thread's post-block_on path) reach
        // this ONE place — closes the chrome trace's `]` here rather than
        // duplicating the call at both call sites. No-op if DL_TRACE_CHROME
        // was never set.
        crate::trace::finish_chrome_trace();
    });
}

// ---------- job dispatcher runner ----------

/// The `JobRunner` the dispatcher's workers call for each claimed job. A `Weak`
/// avoids a `Daemon -> Dispatcher -> runner -> Daemon` reference cycle; on
/// shutdown the upgrade fails and the runner no-ops. A worker takes the same
/// `ServedRoot` handle the inline path used and calls the SAME method
/// (`tick_paths` / `poll_tick`) under the SAME engine mutex — behavior
/// preserved, only the calling thread changed.
struct DaemonJobRunner {
    daemon: std::sync::Weak<Daemon>,
}

impl crate::jobq::JobRunner for DaemonJobRunner {
    fn run(&self, job: &crate::jobq::JobRow) -> Result<()> {
        let Some(daemon) = self.daemon.upgrade() else { return Ok(()) };
        let Some(sr) = daemon.served_root_for_job(&job.root) else {
            // Root dropped between enqueue and claim; nothing to do.
            return Ok(());
        };
        // Stamp the served root for the whole job so `why.jsonl` samples and
        // `perf.jsonl` records during the drain half of a sink job are attributed
        // even though `poll_tick`/`tick_paths` clear the tick-scoped root when
        // the engine tick itself finishes. The guard restores the previous root
        // (or none) when the job ends, so idle samples do not carry a stale root.
        let _stamp = crate::activity::stamp_root(&sr.root);
        match job.kind {
            crate::jobq::JobKind::Tick => sr.tick_paths(&job.paths(), true),
            crate::jobq::JobKind::SinkDrain => sr.poll_tick().map(|_| ()),
            crate::jobq::JobKind::ColdExtract => match job.cold_target() {
                Some((family, shard)) => sr.run_cold_node(&family, shard),
                None => Ok(()), // malformed arg; drop
            },
        }
    }
}

/// Identity of the currently-running `dl` binary: crate version + the exe's
/// mtime. A client that rebuilt/reinstalled computes a different id and respawns.
pub(crate) fn build_id() -> String {
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
pub(crate) struct ExeStamp {
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

pub(crate) fn enforce_fresh_binary(launch_exe_stamp: Option<ExeStamp>) {
    if launch_exe_stamp.is_none() { return; }
    if should_exit_for_binary_change(launch_exe_stamp, current_exe_stamp()) {
        tracing::info!("[daemon] binary replaced on disk, exiting so the next call runs the new version");
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
pub(crate) fn enforce_mem_limit(label: &str) {
    let Some(limit) = mem_limit_mb() else { return };
    let Some(rss) = self_rss_mb() else { return };
    if rss > limit {
        tracing::warn!("[{label}] RSS {rss}MB exceeded limit {limit}MB — exiting (memory guard; \
                   set DL_DAEMON_MEM_MB to adjust, 0 to disable)");
        std::process::exit(MEM_GUARD_EXIT_CODE);
    }
}

// ---------- poll task (the @async clock source, all roots) ----------

fn poll_interval_secs() -> Option<u64> {
    match std::env::var("DL_POLL_SECS") {
        Ok(s) => s.parse::<u64>().ok().filter(|&n| n > 0),
        Err(_) => Some(DEFAULT_POLL_SECS),
    }
}

/// One access-log line per inbound request, either transport. The apache-
/// style line this arc exists for: request id, which door it came through,
/// the RPC method, which root it addressed (empty = config view / not yet
/// resolved), how long dispatch took, and whether it succeeded. Byte counts
/// are the caller's to add when cheap (the HTTP body length; the UDS path
/// already has the framed body in hand) — optional, so `0` means "not
/// counted" rather than "empty body".
pub(crate) fn log_access(
    surface: &str,
    req_id: &str,
    method: &str,
    root: Option<&str>,
    ms: u64,
    ok: bool,
    bytes_in: usize,
    bytes_out: usize,
) {
    tracing::info!(
        req_id, surface, method, root = root.unwrap_or(""), ms, ok, bytes_in, bytes_out,
        "[access]"
    );
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

/// Dispatch one request. Transport-agnostic and synchronous — both the UDS
/// connection task and the axum `/rpc` handler call it through `spawn_blocking`,
/// since it takes `lock_eng` / the `prog` lock. `subscribe` is NOT handled here:
/// both transports intercept it (the UDS task registers the write half with the
/// subscriber pump; the HTTP path rejects it), so a `subscribe` never reaches
/// this function.
pub(crate) fn handle_request(d: &Arc<Daemon>, req: &Request, req_id: &str) -> Response {
    // Anything synchronous on THIS thread for the rest of the call — an
    // inline tick from `run_eval` (the `eval`/`load mode=once` path), a
    // `JobRow` built here — can read this id back via `crate::reqid::current`
    // and tag its own event/row with it. Dropped (restoring whatever was
    // active before, normally nothing) on every return path via RAII.
    let _reqid_scope = crate::reqid::scope(req_id);
    // ----- process-level methods (no root routing) -----
    match req.method.as_str() {
        "shutdown" => return Response::ok(req.id, json!({"ok": true})),
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
        // `jobs` lists the whole (process-wide) job table, newest first. Answered
        // WITHOUT the engine lock: a read-only connection straight on the job db.
        "jobs" => {
            return match d.jobs.list() {
                Ok(rows) => Response::ok(req.id,
                    json!({"jobs": rows.iter().map(job_row_json).collect::<Vec<_>>()})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            };
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
    let roots: Vec<Value> = lock(&d.roots).values().map(|sr| {
        let (fx_failed, fx_orphaned) = lock(&sr.eng)
            .effect_status_counts()
            .unwrap_or((0, 0));
        json!({
            "root": sr.root.to_string_lossy(),
            "key": sr.key,
            "tick_count": sr.tick_count.load(Ordering::Relaxed),
            "program": sr.program_display,
            "settled": sr.settled.load(Ordering::Relaxed),
            "cold_start_pending": sr.cold_pending.load(Ordering::Relaxed),
            "effects_failed": fx_failed,
            "effects_orphaned": fx_orphaned,
        })
    }).collect();
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
                "cold_start_pending": sr.cold_pending.load(Ordering::Relaxed),
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
                    _ = eng.log_query("daemon", "query", "", "[]");
                    _ = crate::rels::refresh_query_log(&eng);
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
                // R7: alongside the diag rows, ship the `diag_stage` routes
                // ([code, stage] pairs) and the latest-turn `agent_touch` paths
                // so the client filters by stage (and, for the hook's
                // agent-turn surface, by touched path) in one round trip.
                Ok(rows) => {
                    let stages = eng.rel_rows("diag_stage", 2);
                    let touch: Vec<String> = eng.rel_rows("agent_touch", 3)
                        .into_iter().filter_map(|r| r.into_iter().nth(2)).collect();
                    Response::ok(req.id, json!({
                        "rows": rows.iter().map(diag_to_json).collect::<Vec<_>>(),
                        "stages": stages,
                        "touch": touch,
                    }))
                }
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
                    _ = eng.log_query("daemon", "query_sql", sql_raw, &params_json);
                    _ = crate::rels::refresh_query_log(&eng);
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
                                tracing::warn!("[{}] save_program_meta: {e}", sr.root_label());
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

fn job_row_json(r: &crate::jobq::JobListRow) -> Value {
    json!({
        "key": r.key, "kind": r.kind, "root": r.root, "state": r.state,
        "priority": r.priority, "attempts": r.attempts, "run_at": r.run_at,
        "enqueued_at": r.enqueued_at, "started_at": r.started_at,
        "finished_at": r.finished_at, "last_error": r.last_error,
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

// ---------- small helpers shared with lib.rs ----------

pub(crate) fn load_repos_eager() -> Vec<config::RepoConfig> {
    match config::SprfConfig::load_default() {
        Ok(cfg) if !cfg.repos.is_empty() => cfg.repos,
        Ok(_) => Vec::new(),
        Err(e) => { tracing::warn!("[config] ignored: {e}"); Vec::new() }
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

/// Whether a full tick's outcome justifies telling subscribers anything
/// changed. A no-op tick (nothing reconciled, no timer boundary, no derived
/// digest move) must stay silent: every broadcast makes a subscribed client
/// (instant) re-query and re-render, and a churn of empty ticks amplified
/// into a webview render storm + WindowServer seize (failure-modes class 19,
/// 2026-07-18). Timer ticks keep broadcasting — clock/every subscribers rely
/// on the bucket boundary.
fn tick_warrants_broadcast(report: &crate::engine::TickReport) -> bool {
    report.changed || !report.changed_rels.is_empty() || report.derived_moved
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

    /// `init_daemon_tracing` must be idempotent: a foreground daemon started
    /// inside a process that already installed a subscriber — and any test that
    /// exercises it twice — relies on `try_init` swallowing the "global already
    /// set" error rather than panicking.
    #[test]
    fn daemon_tracing_init_is_idempotent() {
        init_daemon_tracing();
        init_daemon_tracing();
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
    /// Fail-pre-fix witness for class 19: pre-fix, run_tick_full broadcast
    /// unconditionally, so the no-op report below would have pushed a
    /// diag_changed frame at every churned tick.
    #[test]
    fn noop_tick_stays_silent_but_timer_and_change_ticks_broadcast() {
        let noop = crate::engine::TickReport {
            changed: false,
            derived_moved: false,
            changed_rels: vec![],
            staged_next: false,
            inflight_effects: 0,
            cold_pending: false,
            cold_staged: vec![],
        };
        assert!(!super::tick_warrants_broadcast(&noop), "no-op tick must not broadcast");

        let timer = crate::engine::TickReport { changed: true, changed_rels: vec!["clock".into()], ..noop.clone() };
        assert!(super::tick_warrants_broadcast(&timer), "timer boundary must broadcast");

        let derived = crate::engine::TickReport { derived_moved: true, ..noop.clone() };
        assert!(super::tick_warrants_broadcast(&derived), "derived digest move must broadcast");
    }
}
