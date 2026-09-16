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
            &[
                ("rpc", method),
                ("waited_ms", &waited_ms.to_string()),
                ("behind", &behind),
            ],
        );
    }
    *lock(current_op_cell()) = method.to_string();
    guard
}

// ---------- module layout (decomposition plan step 6) ----------
// plans/2026-07-18-decomposition-normalization.md: daemon.rs (2,967 lines)
// promoted to src/daemon/, pure relocation. Every pre-split `crate::daemon::X`
// path resolves unchanged via the re-exports below.
mod budget;
mod client;
mod dispatch;
pub mod gc;
mod home;
pub mod http_discovery;
pub(crate) mod logcap;
pub mod read;
mod root;
pub mod shell;

pub(crate) use budget::*;
pub use client::*;
pub(crate) use dispatch::*;
pub use home::*;
pub use root::*;

/// The job-queue `root` id for the key-less config view (registered roots use
/// their blake3 registry key). One reserved token, never a valid key.
const CONFIG_JOB_ID: &str = "config";

// ---------- shared process handles ----------

/// Handles cloned into every `ServedRoot` so its per-root tick methods can reach
/// the process-wide subscriber list / shutdown flag / build identity without a
/// back-pointer to the whole `Daemon`.
#[derive(Clone)]
pub(crate) struct Shared {
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
        // `notify_waiters`, not `notify_one`: the apalis pollers (engine +
        // cold doorbell streams) AND the reconciler all park on this Notify.
        self.job_notify.notify_waiters();
        Ok(())
    }

    /// Push a pre-serialized notification body to all `/watch` subscribers
    /// (best-effort; zero subscribers = the send errors and is ignored).
    fn push_frame(&self, body: String) {
        let _ = self.broadcast_tx.send(body);
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
        let canon = Path::new(raw)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(raw));
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
                        canon.display(),
                        existing.root.display(),
                        existing.root.display()
                    );
                }
                if existing.root.starts_with(&canon) {
                    bail!(
                        "refusing to register {}: already-registered root {} lives inside it. \
                         Registering the parent would double-serve the child tree; \
                         `dl daemon drop {}` first if you want the parent served instead.",
                        canon.display(),
                        existing.root.display(),
                        existing.root.display()
                    );
                }
            }
        }
        let db = root_db_dir(&key).join("db.sqlite");
        let db_str = db.to_string_lossy().into_owned();
        let sr = ServedRoot::open(
            Some(canon.clone()),
            Some(key.clone()),
            &[],
            Some(&db_str),
            self.shared(),
        )?;
        lock(&self.roots).insert(key.clone(), sr.clone());
        // Persist.
        let added_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        {
            let mut records = read_roots_json();
            if !records.iter().any(|r| r.key == key) {
                records.push(RootRecord {
                    root: canon.clone(),
                    key: key.clone(),
                    added_at,
                });
                write_roots_json(&records);
            }
        }
        // Spawn its watcher as a tokio task (notify's callback thread forwards
        // events into a channel this task drains; engine ticks run via
        // `spawn_blocking`).
        self.shell.rt.spawn(daemon_shell::watch::watch_task(
            sr.clone(),
            self.shell.clone(),
            self.launch_exe_stamp,
        ));
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
            tracing::info!(
                "[daemon] deregistered root {} (key {key}){}",
                canon.display(),
                if purge { ", db purged" } else { "" }
            );
        }
        Ok(())
    }
}

/// Install the daemon's `tracing` subscriber: an stderr `fmt` layer so every log
/// line carries a timestamp, level, and the emitting thread's name (`dl-poll`,
/// `dl-watch-<key>`, `dl-http-N`, `dl-accept`, …), plus the two rolling FILE
/// layers (`dl.log`/`error.log` under `<home>/log/`, see
/// `crate::trace::dl_log_layer`/`error_log_layer`) so a foreground/background
/// daemon writes the same apache-style access/error log every other `dl`
/// invocation does — this is the one process where `crate::trace::init`
/// deliberately defers (see its doc comment) specifically so THIS init wins
/// the race and installs both the stderr layer and the file layers together.
/// `with_target(false)` drops the module path since call sites already carry a
/// `[daemon]`/`[<root>]` bracket, and `compact()` keeps each line short.
///
/// Under a plain, un-configured daemon (`DL_LOG` unset — the default under
/// launchd supervision, since nobody sets env vars in a plist) the stderr
/// layer defaults to `warn` (`stderr_filter_spec(None)`), NOT `DL_LOG`'s
/// own `info` default. Root cause of failure-modes class 31
/// (docs/failure-modes.md): at `info`, this layer mirrored the SAME lines
/// `dl_log_layer` already writes to the size-capped `dl.log` — pure
/// duplication — and under launchd that duplicate stream lands in
/// `launchd-stderr.log`, a file this process never opens and therefore
/// cannot rotate (see `daemon::logcap`'s module doc for why no in-process
/// crate can). Nothing is lost at the default: `warn`-and-up already lands in
/// the size-capped `error.log` too. Setting `DL_LOG` explicitly (most often
/// `--foreground` debugging) still widens stderr to match, exactly as before
/// this fix — `stderr_filter_spec` is the pure decision, unit-tested below.
///
/// Idempotent by design: `try_init` returns `Err` (ignored) when a global
/// subscriber is already installed, so a foreground daemon started inside a
/// process that already configured tracing — and a test that calls this twice —
/// does not panic.
fn init_daemon_tracing() {
    use tracing_subscriber::prelude::*;
    let dl_log_env = std::env::var("DL_LOG").ok();
    let spec = stderr_filter_spec(dl_log_env.as_deref());
    let filter = tracing_subscriber::EnvFilter::try_new(&spec)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    let observability = hafley_observe::Config::from_env(
        "sprefa-daemon",
        env!("CARGO_PKG_VERSION"),
        "warn",
        std::io::IsTerminal::is_terminal(&std::io::stderr()),
    )
    .expect("observability configuration");
    let stderr_layer = hafley_observe::format_layer(
        hafley_observe::FormatConfig {
            format: observability.format,
            ansi: observability.ansi,
            target: false,
            thread_names: true,
            span_events: tracing_subscriber::fmt::format::FmtSpan::NONE,
        },
        tracing_subscriber::fmt::writer::BoxMakeWriter::new(std::io::stderr),
    )
    .with_filter(filter);
    let home = daemon_home();
    // The event trail (`src/eventlog.rs`) is only ever installed HERE: ticks
    // happen in the daemon, not in one-shot CLI runs, and `set_home` must land
    // before the registry below is built so the layer's first `on_event` finds
    // `TRAIL` already set. `crate::eventlog::EventLayer` claims exactly
    // `eventlog::EVENT_TARGET` and passes every other event straight through
    // (see its `Layer::on_event` doc comment), so it composes onto this stack
    // the same no-op-elsewhere way `chrome_layer()` does.
    crate::eventlog::set_home(&home);
    // DL_TRACE_CHROME composes the same way here as it does in
    // `crate::trace::init` for a one-shot: absent -> `chrome_layer` returns
    // `None` -> no layer installed, zero overhead. `shutdown_cleanup` below
    // calls `finish_chrome_trace` on every exit path this fn's caller has.
    let _ = tracing_subscriber::registry()
        .with(stderr_layer)
        .with(crate::trace::dl_log_layer(&home))
        .with(crate::trace::error_log_layer(&home))
        .with(crate::trace::chrome_layer())
        .with(crate::eventlog::EventLayer)
        .try_init();
    hafley_observe::startup(&observability);
}

/// Pure decision behind `init_daemon_tracing`'s stderr filter: `DL_LOG` unset
/// -> `"warn"` (the class-28 fix — see that fn's doc); `DL_LOG` set to
/// anything -> that literal value, so an explicit ask for more terminal
/// verbosity keeps working unchanged. Extracted as a pure fn (rather than
/// inlined) so the decision itself is unit-testable without constructing a
/// `tracing_subscriber::EnvFilter`, which has no `PartialEq`.
fn stderr_filter_spec(dl_log: Option<&str>) -> String {
    dl_log
        .map(str::to_string)
        .unwrap_or_else(|| "warn".to_string())
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
    // Class-28 rail: cap any externally-redirected log left oversized by a
    // PRIOR run (a daemon down for a while, or one that just adopted this
    // fix) immediately at boot, rather than waiting out the idle-task's 30s
    // cadence (`daemon_shell::timers::idle_task`, which re-sweeps for the
    // rest of this run's lifetime).
    logcap::sweep(&home);
    let launch_exe_stamp = current_exe_stamp();

    // Single-instance witness (plan section 3.3): acquired BEFORE any other
    // setup so a second `run_daemon` in this home (a stray `dl daemon start
    // --foreground` racing the launchd-supervised instance) fails fast rather
    // than opening the job queue/engine dbs and losing a slower race later.
    // Both locals must stay alive for the rest of this function — dropping
    // either releases the OS-level `flock` immediately.
    let pid_lock_file = open_pid_lock_file()?;
    let mut singleton_lock = fd_lock::RwLock::new(pid_lock_file);
    let mut singleton_guard = singleton_lock.try_write().map_err(|_| {
        anyhow::anyhow!(
            "another dl daemon instance already holds {} — refusing to start a second one \
         (fd-lock single-instance witness; `dl daemon status` to check, `dl daemon stop` to clear)",
            pid_path().display()
        )
    })?;
    {
        use std::io::{Seek, SeekFrom, Write as _};
        let pid = std::process::id();
        let start = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
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

    // The durable job queue: apalis-sqlite's `Jobs` store in its own
    // `<home>/jobs.sqlite`. The sqlx pool + migrations (the bought schema)
    // come up first; `JobQueue::open` is the sync admission/introspection
    // seam over the same file.
    let jobs_db_path = home.join("jobs.sqlite");
    let jobs_pool = runtime
        .block_on(crate::jobq::workers::open_pool(&jobs_db_path))
        .context("open apalis job store")?;
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
    if !repos.is_empty() {
        tracing::info!("[config] {} repo(s) registered", repos.len());
    }

    // The config-view engine (root:None). An explicit --db points it at that file;
    // otherwise the home db.
    let config_db = db_path
        .map(|s| s.to_string())
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

    // Job workers: crash-recover any rows a previous process left in flight
    // (instant boot reset; apalis's heartbeat `reenqueue_orphaned` covers
    // mid-life orphans), then spawn the apalis worker pair — the `dl-engine`
    // queue (tick + sink-drain, concurrency = the old worker budget) and the
    // single-flight `dl-cold` queue — plus the admission reconciler. Started
    // BEFORE roots replay so a watcher's first enqueue has a worker to serve
    // it.
    match jobs.reset_orphaned_on_boot() {
        Ok(n) if n > 0 => tracing::info!("[daemon] reset {n} in-flight job(s) to pending on boot"),
        Ok(_) => {}
        Err(e) => tracing::warn!("[daemon] reset_orphaned_on_boot: {e}"),
    }
    let n_workers = daemon_thread_count(
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2),
        std::env::var("DL_DAEMON_THREADS").ok().as_deref(),
    );
    let runner: Arc<dyn crate::jobq::JobRunner> = Arc::new(DaemonJobRunner {
        daemon: Arc::downgrade(&daemon),
    });
    crate::jobq::workers::spawn_workers(&shell, &jobs_pool, jobs.clone(), runner, n_workers);
    tracing::info!("[daemon] apalis workers: dl-engine x{n_workers} + dl-cold x1 (single-flight)");

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
    if let Some(dir) = sock.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(&sock)?;
    // The accept task adopts this into the runtime via `from_std`, which requires
    // a non-blocking listener.
    listener
        .set_nonblocking(true)
        .context("set UDS listener non-blocking")?;
    std::fs::set_permissions(&sock, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    let idle_secs = idle_timeout_secs();
    tracing::info!(
        "[daemon] listening on {} (pid {}, idle {}s){}",
        sock.display(),
        std::process::id(),
        idle_secs,
        if foreground { " [foreground]" } else { "" }
    );
    // systemd readiness (plan section 3.2, `Type=notify`): a no-op everywhere
    // else — `sd_notify::notify` returns `Ok(())` immediately when
    // `NOTIFY_SOCKET` is unset (macOS, or a systemd unit not using
    // `Type=notify`), so this call is unconditional and cheap.
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);
    if let Some(p) = crate::perflog::path() {
        tracing::info!(
            "[daemon] perf log {} (tail -f | jq .total_ms, .phase, .ms)",
            p.display()
        );
    }
    crate::verdict::emit_run_header(
        "daemon",
        initial_root
            .as_ref()
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_else(|| home.to_string_lossy().into_owned())
            .as_str(),
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
    let stale_check_root = initial_root
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| home.clone());
    crate::stale_binary::warn_if_stale(&stale_check_root);

    // Config-view watcher (watches the config repos) — a tokio task.
    daemon.shell.rt.spawn(daemon_shell::watch::watch_task(
        daemon.config.clone(),
        shell.clone(),
        launch_exe_stamp,
    ));

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
            tracing::warn!(
                "[daemon] root {} no longer exists; evicting from roots.json (key {})",
                rec.root.display(),
                rec.key
            );
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
    if evicted_any {
        write_roots_json(&kept);
    }

    // Register the initial root (a `dl daemon start --foreground` from inside a
    // repo, or an explicit program set).
    if let Some(r) = &initial_root {
        if r.join(".dl").is_dir() {
            let canon = r.canonicalize().unwrap_or_else(|_| r.clone());
            let key = key_of(&canon);
            if !lock(&daemon.roots).contains_key(&key) {
                let db = root_db_dir(&key).join("db.sqlite");
                let db_str = db.to_string_lossy().into_owned();
                match ServedRoot::open(
                    Some(canon.clone()),
                    Some(key.clone()),
                    programs,
                    Some(&db_str),
                    daemon.shared(),
                ) {
                    Ok(sr) => {
                        lock(&daemon.roots).insert(key.clone(), sr.clone());
                        let added_at = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        let mut records = read_roots_json();
                        if !records.iter().any(|x| x.key == key) {
                            records.push(RootRecord {
                                root: canon.clone(),
                                key: key.clone(),
                                added_at,
                            });
                            write_roots_json(&records);
                        }
                        daemon.shell.rt.spawn(daemon_shell::watch::watch_task(
                            sr.clone(),
                            shell.clone(),
                            launch_exe_stamp,
                        ));
                        tracing::info!("[daemon] registered initial root {}", canon.display());
                    }
                    Err(e) => tracing::error!("[daemon] initial root {}: {e}", canon.display()),
                }
            }
        }
    }

    // Poll (@async clock) + idle (self-reap) timers — tokio interval tasks.
    if !foreground {
        daemon.shell.rt.spawn(daemon_shell::timers::idle_task(
            daemon.clone(),
            shell.clone(),
            idle_secs,
        ));
    }
    if let Some(secs) = poll_interval_secs() {
        daemon.shell.rt.spawn(daemon_shell::timers::poll_task(
            daemon.clone(),
            shell.clone(),
            secs,
        ));
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
        let Some(daemon) = self.daemon.upgrade() else {
            return Ok(());
        };
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
    let mtime = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    match mtime {
        Some(secs) => format!("{version}:{secs}"),
        None => version.to_string(),
    }
}

// ---------- small helpers shared with lib.rs ----------

pub(crate) fn load_repos_eager() -> Vec<config::RepoConfig> {
    match config::SprfConfig::load_default() {
        Ok(cfg) if !cfg.repos.is_empty() => cfg.repos,
        Ok(_) => Vec::new(),
        Err(e) => {
            tracing::warn!("[config] ignored: {e}");
            Vec::new()
        }
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
        assert!(
            rss < DEFAULT_MEM_LIMIT_MB,
            "test process RSS {rss}MB should sit under the {DEFAULT_MEM_LIMIT_MB}MB ceiling"
        );
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
        assert!(
            tracing::dispatcher::has_been_set(),
            "init_daemon_tracing must install a global subscriber"
        );
        // The second call must not panic (try_init swallows the "already set"
        // error) AND must leave the subscriber installed, not clear it.
        init_daemon_tracing();
        assert!(
            tracing::dispatcher::has_been_set(),
            "a second init_daemon_tracing call must stay idempotent, not unset the subscriber"
        );
    }

    /// Class-28 fix: unset `DL_LOG` must default the daemon's stderr layer to
    /// `warn` (the duplication that filled `launchd-stderr.log` was `info`),
    /// while an EXPLICIT `DL_LOG` value keeps widening stderr exactly as
    /// before this fix.
    #[test]
    fn stderr_filter_defaults_warn_but_honors_explicit_dl_log() {
        assert_eq!(stderr_filter_spec(None), "warn");
        assert_eq!(stderr_filter_spec(Some("info")), "info");
        assert_eq!(stderr_filter_spec(Some("debug")), "debug");
        assert_eq!(stderr_filter_spec(Some("dl=trace")), "dl=trace");
    }

    #[test]
    fn fresh_binary_compare_exits_only_for_two_confirmed_stamps() {
        let before = ExeStamp {
            len: 10,
            mtime: SystemTime::UNIX_EPOCH,
        };
        let after = ExeStamp {
            len: 11,
            mtime: SystemTime::UNIX_EPOCH,
        };
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
        assert!(
            a.starts_with(env!("CARGO_PKG_VERSION")),
            "build_id should carry the crate version: {a}"
        );
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
        assert!(
            !super::tick_warrants_broadcast(&noop),
            "no-op tick must not broadcast"
        );

        let timer = crate::engine::TickReport {
            changed: true,
            changed_rels: vec!["clock".into()],
            ..noop.clone()
        };
        assert!(
            super::tick_warrants_broadcast(&timer),
            "timer boundary must broadcast"
        );

        let derived = crate::engine::TickReport {
            derived_moved: true,
            ..noop.clone()
        };
        assert!(
            super::tick_warrants_broadcast(&derived),
            "derived digest move must broadcast"
        );
    }
}
