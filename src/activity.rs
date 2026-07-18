//! Process-global "what is the engine doing right now" slot, read by the
//! daemon's `ping` handler and rendered by `dl daemon status`. Lives OFF the
//! engine Mutex: a tick swaps it at phase boundaries (microsecond lock), `ping`
//! reads a snapshot, so status never blocks on a mid-tick eng lock.
//!
//! One `static` written by the engine (tick.rs / daemon.rs) and read by ping.
//! Matches the existing process-global telemetry pattern (`CMD_COUNT` /
//! `TICK_AUDIT` in engine/mod.rs): the tick is too deep (reconcile → per-file
//! extract families → derived strata → operators → effect drain) to thread a
//! handle to every call site, so a single process slot is the O(1) plumbing.
//! Ticks are serialized by the eng lock, so in practice there is one writer;
//! `ping` handlers (each on its own connection thread) take the lock only long
//! enough to clone the small struct.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Coarse step the current tick is in. `as_str` is the wire form `ping` ships.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    ColdTick,
    Declare,
    Reconcile,
    ParseExtract,
    Derived,
    Operators,
    Effects,
    Query,
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::ColdTick => "cold-tick",
            Phase::Declare => "declare",
            Phase::Reconcile => "reconcile",
            Phase::ParseExtract => "extract",
            Phase::Derived => "derived",
            Phase::Operators => "operators",
            Phase::Effects => "effects",
            Phase::Query => "query",
        }
    }
}

struct Activity {
    phase: Phase,
    detail: String,
    program: String,
    /// The repo root currently being served/ticked, or empty when none is known
    /// (idle, config view, or one-shot CLI path).
    root: String,
    tick: u64,
    since: Instant,
    /// Active-root accounting so concurrent jobs on different roots do not
    /// stomp each other's `why.jsonl` / `perf.jsonl` attribution. A count map
    /// handles overlapping stamps; the stack gives a deterministic "current"
    /// root (the most recent still-active stamp).
    root_counts: HashMap<PathBuf, usize>,
    root_stack: Vec<PathBuf>,
    /// The root pushed by the current `begin_tick`/`set_root` pair; `end_tick`
    /// pops it so open/tick lifecycle stays balanced.
    tick_root: Option<PathBuf>,
    /// Set by `begin_tick`, read by `end_tick` for the whole-tick duration
    /// logged there — distinct from `since`, which `set()` resets at every
    /// phase boundary and so only ever holds the CURRENT phase's start.
    tick_since: Instant,
}

impl Activity {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            detail: String::new(),
            program: String::new(),
            root: String::new(),
            tick: 0,
            since: Instant::now(),
            root_counts: HashMap::new(),
            root_stack: Vec::new(),
            tick_root: None,
            tick_since: Instant::now(),
        }
    }

    fn push_root(&mut self, root: &Path) {
        *self.root_counts.entry(root.to_path_buf()).or_insert(0) += 1;
        self.root_stack.push(root.to_path_buf());
    }

    fn pop_root(&mut self, root: &Path) {
        let key = root.to_path_buf();
        if let Some(c) = self.root_counts.get_mut(&key) {
            if *c > 0 {
                *c -= 1;
            }
            if *c == 0 {
                self.root_counts.remove(&key);
            }
        }
        while let Some(top) = self.root_stack.last() {
            if self.root_counts.contains_key(top) {
                break;
            }
            self.root_stack.pop();
        }
    }

    /// Sync `self.root` and the perf log's current-root resolver to the top of
    /// the active-root stack.
    fn sync_root(&mut self) {
        let current = self.root_stack.last().cloned();
        self.root = current
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        crate::perflog::set_current_root(current.as_deref());
    }
}

static SLOT: OnceLock<Mutex<Activity>> = OnceLock::new();

fn slot() -> &'static Mutex<Activity> {
    SLOT.get_or_init(|| Mutex::new(Activity::new()))
}

/// A cheap point-in-time read of the activity slot (cloned under the brief
/// lock). `elapsed_ms` is how long the current phase has been running — the
/// caller sees "stuck in extract for 4.3s".
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub phase: Phase,
    pub detail: String,
    pub program: String,
    /// Repo root being served/ticked; empty when unknown.
    pub root: String,
    pub tick: u64,
    pub elapsed_ms: u64,
}

/// Mark a phase boundary with a fresh detail string. Resets the phase timer.
/// Closes out the previous phase by emitting a perf-log record of how long it
/// ran (driven from the same transition `ping`/`status` read, so the perf log
/// costs no extra instrumentation points).
pub fn set(phase: Phase, detail: impl Into<String>) {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    if a.phase != Phase::Idle && a.phase != phase {
        let ms = a.since.elapsed().as_millis() as u64;
        crate::perflog::emit_phase(a.tick, a.phase.as_str(), ms, &a.detail);
        // The seam for "extract family begin/end" / "derived rebuild
        // begin/end": every phase transition (Declare -> Reconcile ->
        // ParseExtract -> Derived -> Operators -> Query) closes out the
        // phase that just ended with how long it ran and its last detail
        // string (per-family name for ParseExtract, per-component rel list
        // for Derived — see the `activity::detail()` call sites in
        // `engine/tick.rs` and `engine/derive.rs`).
        tracing::info!(
            tick = a.tick, phase = a.phase.as_str(), ms, detail = %a.detail,
            "[tick] phase done"
        );
    }
    a.phase = phase;
    a.detail = detail.into();
    a.since = Instant::now();
}

/// Set the repo root currently being served. Called by the daemon when it opens
/// a root so `why.jsonl` samples and `perf.jsonl` records identify which root
/// the work belongs to. The root stays active until the matching `end_tick`.
/// Pass `None` to clear the root pushed by the most recent `set_root`.
pub fn set_root(root: Option<&Path>) {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    if let Some(r) = root {
        a.push_root(r);
        a.tick_root = Some(r.to_path_buf());
    } else if let Some(r) = a.tick_root.take() {
        a.pop_root(&r);
    }
    a.sync_root();
}

/// Update only the detail within the current phase (per-file / per-rel). Leaves
/// the phase and its timer alone so a phase spanning many items reports one
/// duration, not one-per-item.
pub fn detail(detail: impl Into<String>) {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    a.detail = detail.into();
}

/// Record the tick number + program + served root at tick start. Does not
/// touch the phase; the first `set` inside the tick establishes it. Between
/// `begin_tick` and that first `set`, a reader sees the previous tick's
/// terminal `Idle`.
pub fn begin_tick(tick: u64, program: &str, root: &Path) {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    a.tick = tick;
    a.program = program.to_string();
    a.push_root(root);
    a.tick_root = Some(root.to_path_buf());
    a.sync_root();
    a.since = Instant::now();
    a.tick_since = a.since;
    tracing::info!(tick, root = %root.display(), program, "[tick] begin");
}

/// Mark the tick done: phase back to Idle, detail + root cleared. Emits a
/// final perf-log record for the phase that was running when the tick ended.
pub fn end_tick() {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    if a.phase != Phase::Idle {
        let ms = a.since.elapsed().as_millis() as u64;
        crate::perflog::emit_phase(a.tick, a.phase.as_str(), ms, &a.detail);
    }
    let tick = a.tick;
    let root = a.tick_root.clone();
    let total_ms = a.tick_since.elapsed().as_millis() as u64;
    a.phase = Phase::Idle;
    a.detail.clear();
    a.program.clear();
    if let Some(r) = a.tick_root.take() {
        a.pop_root(&r);
    }
    a.sync_root();
    if let Some(root) = root {
        tracing::info!(tick, root = %root.display(), ms = total_ms, "[tick] end");
    }
}

/// Stamp the repo root for a scope (e.g. a daemon job). The root stays active
/// until the guard drops; concurrent stamps stack, so a job finishing does not
/// clear a root still being ticked by another job.
pub struct RootGuard {
    root: PathBuf,
}

impl RootGuard {
    fn new(root: PathBuf) -> Self {
        {
            let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
            a.push_root(&root);
            a.sync_root();
        }
        Self { root }
    }
}

impl Drop for RootGuard {
    fn drop(&mut self) {
        let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
        a.pop_root(&self.root);
        a.sync_root();
    }
}

/// Stamp `root` as active for the current scope. Returns a guard that restores
/// the previous active root (if any) when dropped.
pub fn stamp_root(root: &Path) -> RootGuard {
    RootGuard::new(root.to_path_buf())
}

/// Read a snapshot for ping / `dl daemon status`.
pub fn snapshot() -> Snapshot {
    let a = slot().lock().unwrap_or_else(|p| p.into_inner());
    Snapshot {
        phase: a.phase,
        detail: a.detail.clone(),
        program: a.program.clone(),
        root: a.root.clone(),
        tick: a.tick,
        elapsed_ms: a.since.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The slot is process-global, so every test that mutates it must hold
    // this lock or parallel runs stomp each other mid-assertion (seen live:
    // lifecycle_round_trips reading "" after root_stamp's end_tick()).
    static SLOT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn lifecycle_round_trips() {
        let _slot = SLOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        end_tick(); // start clean
        begin_tick(7, ".dl/*.dl", Path::new("/tmp/root"));
        set(Phase::Declare, "");
        let s = snapshot();
        assert_eq!(s.phase, Phase::Declare);
        assert_eq!(s.program, ".dl/*.dl");
        assert_eq!(s.tick, 7);
        assert_eq!(s.root, "/tmp/root");

        set(Phase::ParseExtract, "type family");
        assert_eq!(snapshot().phase, Phase::ParseExtract);
        assert_eq!(snapshot().detail, "type family");

        detail("call: src/foo.rs");
        let mid = snapshot();
        assert_eq!(mid.phase, Phase::ParseExtract); // phase unchanged by detail()
        assert_eq!(mid.detail, "call: src/foo.rs");

        std::thread::sleep(std::time::Duration::from_millis(12));
        let later = snapshot();
        assert!(later.elapsed_ms >= mid.elapsed_ms,
            "elapsed should not go backwards ({} -> {})", mid.elapsed_ms, later.elapsed_ms);

        end_tick();
        let done = snapshot();
        assert_eq!(done.phase, Phase::Idle);
        assert!(done.detail.is_empty());
        assert!(done.root.is_empty());
    }

    /// P2 (per-root perf/why attribution): a `stamp_root` guard routes both the
    /// activity snapshot and the perf-log path to that root while held, and
    /// reverts to the prior state when dropped. The slot is process-global, so
    /// the test brackets against its own prior state instead of assuming idle.
    #[test]
    fn root_stamp_routes_activity_and_perf_path() {
        let _slot = SLOT_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        end_tick(); // start clean
        let active = PathBuf::from(format!("/tmp/dl_stamp_{}", std::process::id()));
        std::env::remove_var("DL_PERF_LOG");
        let before_root = snapshot().root;
        let before_path = crate::perflog::path();

        {
            let _g = stamp_root(&active);
            assert_eq!(snapshot().root, active.to_string_lossy());
            assert_eq!(
                crate::perflog::path(),
                Some(active.join(".dl").join("perf.jsonl")),
                "perf path should follow the stamped root"
            );
        }

        assert_eq!(snapshot().root, before_root, "root should revert after guard drop");
        assert_eq!(crate::perflog::path(), before_path, "perf path should revert after guard drop");
    }
}
