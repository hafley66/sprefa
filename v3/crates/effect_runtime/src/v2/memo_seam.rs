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
    /// Phase 6 (source-keyed owner identity). The driver asks the seam
    /// for the memo `in_key` of one input row BEFORE probing, instead
    /// of hard-coding `raw.content_hash()`. A content-threaded op
    /// (e.g. `re` reading file bytes carried in the in-cursor focal
    /// value) must NOT key its memo on that transient content — an
    /// edit would change the in-cursor identity, the probe would MISS
    /// (look brand-new), and `reconcile` (which needs STALE = same
    /// owner key, newer source gen) would never fire. The seam returns
    /// an `in_key` derived from the input row's STABLE identity (the
    /// source-deriving terms), so it is invariant under edits to those
    /// sources and changes only when the op instance or its source set
    /// changes.
    ///
    /// Default: `raw.content_hash()` — the pre-Phase-6 behavior and,
    /// for an op that records NO deps, exactly `key_hash(&[])` (Phase 0
    /// whole-cursor identity). Only dep-recording owners override.
    fn in_key_for(&self, _owner: [u8; 32], raw: &N) -> [u8; 32]
    where
        N: super::next::Next,
    {
        raw.content_hash()
    }

    /// `owner` = stable opaque id of this lowered op call (the driver
    /// folds pipe_hash ++ instance_id ++ depth ++ kind). `in_key` =
    /// the input row's content digest (Phase 0: `key_hash(&[])` ==
    /// whole-cursor `content_hash`; Phase 6: `in_key_for`'s
    /// source-keyed digest for a dep-recording owner).
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
