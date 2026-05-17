//! Phase 6 — dirty-source → re-render-owner loop (plan Diagram A).
//!
//! This is the DRed RE-DERIVE half. Phase 5 built the delete half
//! (`cascade_retract`). On a source change the write order (plan
//! Section 4) is exactly:
//!
//!   1. event layer: `clock.bump(s)` → new `SOURCE_GEN[s]`;
//!   2. reverse-lookup `MEMO_DEPS` for owners whose recorded dep set
//!      contains `s` (SCOPED — `read_where(source_id)`, never a
//!      full-graph scan; that is the constant-RSS property);
//!   3. `mark_memo_owner_dirty(owner, s, gen)` into `RUNTIME_DIRTY`;
//!   4. sweep: re-render each dirty owner over its stale input via the
//!      installed `MemoSeam`. The seam (Phase 6 source-keyed in_key)
//!      probes STALE for the changed owner (same owner key, newer
//!      source gen) and `reconcile` emits the per-row deltas: a
//!      `Retract` routes through `cascade_retract` (Phase 5 DRed),
//!      an `Assert` enqueues downstream, then `memo.put`. Owners whose
//!      sources did NOT move probe `Replay` and the op dispatches 0×.
//!   5. quiescence: `RUNTIME_DIRTY` empty.
//!
//! Re-running is driven from the pipe seed: the seam short-circuits
//! every unchanged owner with `Replay` (op 0×), so the only owner that
//! actually re-renders + reconciles is the one whose source moved.
//! That keeps the work proportional to the changed slice without a
//! per-owner continuation reconstruction.
//!
//! The sweep loop has a HARD round cap. Exceeding it is a loud error
//! (`DirtySweepError::RoundCap`), never a hang.

use std::sync::Arc;

use effect_runtime::v2::{expand, ExpandOpts, MemQueue, PipeInstance, QueueBackend};

use crate::memo_seam_impl::V4MemoSeam;
use crate::runtime_graph::RuntimeGraph;
use crate::source_clock::{SourceClock, SourceId};
use crate::Cursor;

/// Loud failure of the dirty sweep. The plan forbids a loop that can
/// hang; the cap converts non-settling into an error with a diagnostic
/// count instead of spinning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirtySweepError {
    /// The worklist did not drain within `rounds` sweeps. `handled` is
    /// the number of dirty rows retired before the cap tripped.
    RoundCap { rounds: usize, handled: usize },
}

impl std::fmt::Display for DirtySweepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DirtySweepError::RoundCap { rounds, handled } => write!(
                f,
                "dirty-source sweep did not reach quiescence within {rounds} rounds \
                 ({handled} owners handled); aborting to avoid a hang"
            ),
        }
    }
}

impl std::error::Error for DirtySweepError {}

/// Default hard round cap for the dirty sweep. Generous; a real
/// stratified re-derive settles in a handful of rounds. The cap exists
/// only to fail loud on a pathological non-settling graph.
pub const DEFAULT_ROUND_CAP: usize = 10_000;

/// The Phase-6 dirty-source driver. Holds the pipe (the literal
/// `fs > read > re` chain for the binding test), the runtime graph,
/// and the source-keyed `MemoSeam` (Piece 1) that turns a content
/// edit into a STALE probe rather than a MISS.
pub struct DirtySourceDriver {
    pipe: Arc<PipeInstance<Cursor>>,
    graph: Arc<RuntimeGraph>,
    seam: Arc<V4MemoSeam>,
    seed: Vec<Arc<Cursor>>,
}

impl DirtySourceDriver {
    pub fn new(
        pipe: Arc<PipeInstance<Cursor>>,
        graph: Arc<RuntimeGraph>,
        seam: Arc<V4MemoSeam>,
        seed: Vec<Arc<Cursor>>,
    ) -> Self {
        Self {
            pipe,
            graph,
            seam,
            seed,
        }
    }

    /// Cold/initial run: expand the pipe once with the seam installed
    /// so every owner's memo + recorded deps are populated. After this
    /// an unchanged re-run is pure `Replay` (op dispatch 0×).
    pub fn prime(&self) {
        self.run_once();
    }

    fn run_once(&self) {
        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        expand(
            self.pipe.as_ref(),
            queue,
            self.seed.clone(),
            ExpandOpts::default()
                .with_runtime(self.graph.clone())
                .with_memo_seam::<Cursor>(self.seam.clone()),
        );
    }

    /// Step 1+2+3 of Diagram A for a changed FILE source. Bumps the
    /// clock, reverse-looks-up `MEMO_DEPS` SCOPED to this source only,
    /// and marks each affected owner dirty at the new generation.
    /// Returns the number of owners marked (0 ⇒ nothing depended on
    /// this source; the sweep is a no-op).
    pub fn on_file_changed(&self, path: &str) -> usize {
        let sid = SourceId::for_file(path);
        let gen = self.graph.clock().bump(sid);
        let source_hex = sid.hex();
        // SCOPED reverse-lookup: only MEMO_DEPS rows whose source_id ==
        // this file. No full-graph scan (constant-RSS invariant).
        let owners = self.graph.memo_dep_owners_for_source(sid);
        for (owner_hex, _in_key) in &owners {
            self.graph
                .mark_memo_owner_dirty(owner_hex, &source_hex, gen);
        }
        owners.len()
    }

    /// Step 4+5: drain `RUNTIME_DIRTY` to quiescence with a HARD round
    /// cap. Each round re-runs the pipe once; the source-keyed seam
    /// makes the changed owner probe STALE → `reconcile` → DRed
    /// teardown + re-assert, and replays every unchanged owner (op
    /// 0×). A round retires the dirty rows it observed; the loop ends
    /// when the worklist is empty. Returns the count of dirty rows
    /// retired, or `RoundCap` if the worklist never drained.
    pub fn sweep_to_quiescence(
        &self,
        round_cap: usize,
    ) -> Result<usize, DirtySweepError> {
        let mut handled = 0usize;
        for round in 0..round_cap {
            let dirty = self.graph.dirty_memo_owners();
            if dirty.is_empty() {
                return Ok(handled);
            }
            // Re-render the affected slice. The seam decides per owner:
            // STALE (changed source) → reconcile → cascade_retract +
            // Assert; unchanged → Replay (op dispatch 0×).
            self.run_once();
            for (owner_hex, source_hex, generation) in &dirty {
                self.graph
                    .clear_memo_owner_dirty(owner_hex, source_hex, *generation);
                handled += 1;
            }
            // A re-render that itself dirtied more owners (a cascade)
            // is picked up on the next round; the cap bounds the total.
            if round + 1 == round_cap && !self.graph.dirty_memo_owners().is_empty() {
                return Err(DirtySweepError::RoundCap {
                    rounds: round_cap,
                    handled,
                });
            }
        }
        // Loop body returns Ok on an empty worklist; reaching here with
        // a non-empty list is the cap trip (handled above), so an
        // exhausted range with nothing left is quiescence.
        if self.graph.dirty_memo_owners().is_empty() {
            Ok(handled)
        } else {
            Err(DirtySweepError::RoundCap {
                rounds: round_cap,
                handled,
            })
        }
    }

    /// Convenience: full Diagram-A cycle for one changed file —
    /// bump+scope+mark, then capped sweep. Returns retired-row count.
    pub fn react_to_file_change(&self, path: &str) -> Result<usize, DirtySweepError> {
        self.on_file_changed(path);
        self.sweep_to_quiescence(DEFAULT_ROUND_CAP)
    }

    pub fn seam(&self) -> &Arc<V4MemoSeam> {
        &self.seam
    }
}
