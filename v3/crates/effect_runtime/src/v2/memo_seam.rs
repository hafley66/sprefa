//! `MemoSeam<N>` — the object-safe memo/replay/reconcile seam.
//!
//! This is the "A-minimal" lift: the driver loop (`expand`) consults a
//! caller-supplied seam for EVERY component before dispatch. The seam
//! decides one of three things per input row:
//!
//!   - `Miss`            — never computed; run the op as usual.
//!   - `Stale(prior)`    — recorded deps moved; run the op, then call
//!                          `reconcile` with the prior output rows.
//!   - `Replay(rows)`    — deps unchanged; splice `rows` downstream and
//!                          SKIP `comp.dispatch` entirely (op runs 0×).
//!
//! Keys crossing this wall are OPAQUE `[u8; 32]` digests only — the
//! same trick Phase 2's `RenderCtx.deps: Arc<Mutex<Vec<[u8;32]>>>`
//! uses. `effect_runtime` never learns any v4 type (`Cursor`,
//! `SourceId`, `OpInstanceId`). The carrier `N` already crosses the
//! queue boundary (`QueueBackend<N>`); replay/reconcile hand back `N`
//! rows because the driver must splice real carrier values and `N`
//! cannot be reconstructed from a `[u8;32]` id inside this crate.
//!
//! `None` on `ExpandOpts`/`RenderCtx` ⇒ exactly the pre-Phase-4
//! behavior: the driver never probes, every component dispatches.

use std::sync::Arc;

/// Probe outcome for one `(owner, in_key)`.
pub enum MemoProbe<N> {
    /// Never computed (or not dep-tracked). Run the op.
    Miss,
    /// Recorded deps moved. Run the op; the carried rows are the prior
    /// render's output, for `reconcile` to diff against.
    Stale(Vec<Arc<N>>),
    /// Deps unchanged. Splice these downstream, skip `dispatch`.
    Replay(Vec<Arc<N>>),
}

/// One reconcile decision. `Retract` carries the opaque row-id digest
/// of a row the new render no longer produces (or replaced); the seam
/// owner (v4) maps it back to its sink-table teardown. `Assert` is an
/// index into the `fresh` slice handed to `reconcile`.
pub enum MemoDelta {
    Assert(usize),
    Retract([u8; 32]),
}

/// Object-safe. `dyn MemoSeam<N>` lives behind an `Arc` on
/// `ExpandOpts`/`RenderCtx`. Implemented in v4 over the Phase-3
/// `v4::Memo`.
pub trait MemoSeam<N>: Send + Sync {
    /// `owner` = stable opaque id of this lowered op call (the driver
    /// folds pipe_hash ++ instance_id ++ depth ++ kind). `in_key` =
    /// the input row's content digest (Phase 0: `key_hash(&[])` ==
    /// whole-cursor `content_hash`).
    fn probe(&self, owner: [u8; 32], in_key: [u8; 32]) -> MemoProbe<N>;

    /// Called after a `Stale`/`Miss` render. `prior` is `Some` only
    /// when probe returned `Stale`. Returns the per-row deltas: the
    /// seam performs `Retract` teardown internally (presence-based,
    /// Phase 4 — no mult/DRed yet) and records the new memo entry.
    /// `Assert(i)` tells the driver to splice `fresh[i]` downstream.
    fn reconcile(
        &self,
        owner: [u8; 32],
        in_key: [u8; 32],
        prior: Option<Vec<Arc<N>>>,
        fresh: &[Arc<N>],
    ) -> Vec<MemoDelta>;
}

/// Trait-object form carried on `ExpandOpts`/`RenderCtx`. `None` = the
/// pre-Phase-4 path (no probe, every component dispatches).
pub type DynMemoSeam<N> = Arc<dyn MemoSeam<N>>;
