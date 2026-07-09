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
    tick: u64,
    since: Instant,
}

impl Activity {
    fn new() -> Self {
        Self {
            phase: Phase::Idle,
            detail: String::new(),
            program: String::new(),
            tick: 0,
            since: Instant::now(),
        }
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
    }
    a.phase = phase;
    a.detail = detail.into();
    a.since = Instant::now();
}

/// Update only the detail within the current phase (per-file / per-rel). Leaves
/// the phase and its timer alone so a phase spanning many items reports one
/// duration, not one-per-item.
pub fn detail(detail: impl Into<String>) {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    a.detail = detail.into();
}

/// Record the tick number + program at tick start. Does not touch the phase;
/// the first `set` inside the tick establishes it. Between `begin_tick` and
/// that first `set`, a reader sees the previous tick's terminal `Idle`.
pub fn begin_tick(tick: u64, program: &str) {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    a.tick = tick;
    a.program = program.to_string();
    a.since = Instant::now();
}

/// Mark the tick done: phase back to Idle, detail cleared. Emits a final
/// perf-log record for the phase that was running when the tick ended.
pub fn end_tick() {
    let mut a = slot().lock().unwrap_or_else(|p| p.into_inner());
    if a.phase != Phase::Idle {
        let ms = a.since.elapsed().as_millis() as u64;
        crate::perflog::emit_phase(a.tick, a.phase.as_str(), ms, &a.detail);
    }
    a.phase = Phase::Idle;
    a.detail.clear();
}

/// Read a snapshot for ping / `dl daemon status`.
pub fn snapshot() -> Snapshot {
    let a = slot().lock().unwrap_or_else(|p| p.into_inner());
    Snapshot {
        phase: a.phase,
        detail: a.detail.clone(),
        program: a.program.clone(),
        tick: a.tick,
        elapsed_ms: a.since.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One sequential test: the slot is process-global, so parallel tests
    // mutating it would race. The full lifecycle in one function is race-free.
    #[test]
    fn lifecycle_round_trips() {
        end_tick(); // start clean
        begin_tick(7, ".dl/*.dl");
        set(Phase::Declare, "");
        let s = snapshot();
        assert_eq!(s.phase, Phase::Declare);
        assert_eq!(s.program, ".dl/*.dl");
        assert_eq!(s.tick, 7);

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
    }
}
