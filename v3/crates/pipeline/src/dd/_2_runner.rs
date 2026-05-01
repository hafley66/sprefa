//! DdRunner facade.
//!
//! Card A keeps this thin: a constructor + the synthetic-smoke entry. The
//! long-lived multi-tick runner — `push` / `advance_and_flush` /
//! `drain_sinks` — needs the worker thread to stay alive between ticks.
//! That lifetime story (Worker handle owned by the runner, channels for
//! cross-thread submission, dataflow scope built once and held) is card E.
//!
//! Surface laid out below; methods that require the long-lived worker
//! return `Unimplemented` for now so call sites in card E compile against
//! the intended shape.

use std::sync::atomic::{AtomicU64, Ordering};

use super::_0_row::{Diff, FactName, Row, Time};
use super::_1_scope::{run_synthetic_scope, SmokeReport};

/// Errors surfaced by the runner. v0 only has the unimplemented placeholder.
#[derive(Debug, Eq, PartialEq)]
pub enum DdRunnerError {
    /// Method requires the long-lived worker scaffolding from card E.
    Unimplemented(&'static str),
}

/// Facade for the DD-backed runtime. Card A: the long-lived worker lives
/// inside `run_synthetic_scope` and exits when that call returns. Card E
/// promotes this to own a Worker handle and route push/advance through
/// channels.
pub struct DdRunner {
    current_gen: AtomicU64,
}

impl DdRunner {
    pub fn new() -> Self {
        Self {
            current_gen: AtomicU64::new(0),
        }
    }

    /// Monotonic generation counter. RtCtx::current_gen will alias this
    /// once card D wires the runtime.
    pub fn current_gen(&self) -> Time {
        self.current_gen.load(Ordering::SeqCst)
    }

    /// Advance the gen counter. Card E will couple this to the worker's
    /// `advance_to(T) + step_while(probe < T)` cycle.
    pub fn bump_gen(&self) -> Time {
        self.current_gen.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Run the synthetic smoke scenario inline. Owns + drops a timely
    /// worker for the duration. Returns the report for assertion.
    pub fn run_smoke(&self, long_line_threshold: usize) -> SmokeReport {
        run_synthetic_scope(long_line_threshold)
    }

    /// Submit a row diff into the named fact. Card E.
    pub fn push(&mut self, _fact: &FactName, _row: Row, _diff: Diff) -> Result<(), DdRunnerError> {
        Err(DdRunnerError::Unimplemented(
            "DdRunner::push lands in card E (long-lived Worker)",
        ))
    }

    /// Seal the current gen and flush sinks. Card E.
    pub fn advance_and_flush(&mut self) -> Result<(), DdRunnerError> {
        Err(DdRunnerError::Unimplemented(
            "DdRunner::advance_and_flush lands in card E",
        ))
    }
}

impl Default for DdRunner {
    fn default() -> Self {
        Self::new()
    }
}
