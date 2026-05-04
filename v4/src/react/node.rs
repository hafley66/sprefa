// CursorNode — the discriminated union returned by Component::render.
//
// One enum, six (kind-of) variants. Every variant lowers to N queue rows
// in the driver. See ../../../llm-notes.md "Op = Component" section for
// the design write-up. Portal is intentionally commented out — Mount +
// default-flow + Many cover the routing patterns we have today.

use std::sync::Arc;

use crate::Cursor;
use super::wake::Wake;
use super::effect::ReactEffect;

#[derive(Debug, Clone)]
pub enum CursorNode {
    /// Leaf: cursor flows to next op (pc+1) immediately.
    Emit(Arc<Cursor>),

    /// Fragment: flatten N children, all flow downstream concurrently.
    /// Siblings have no implicit ordering; sequential = explicit
    /// `Effect{await prev_done, then}`.
    Many(Vec<CursorNode>),

    /// Throw-promise: park cursor at pc+1 with a wake condition.
    /// The same op never re-renders the same cursor; when the wake
    /// fires the parked row becomes runnable at pc+1.
    Suspense {
        cursor: Arc<Cursor>,
        wake:   Wake,
    },

    /// switchMap: instantiate a subpipe keyed by `key`. Mounting again
    /// with the same `(pipe_hash, key)` reuses the running instance;
    /// new key reaps the prior instance via queue.unmount(instance_id).
    Mount {
        subpipe: PipeRef,
        key:     Arc<Cursor>,
        input:   Arc<Cursor>,
    },

    /// useEffect: dispatch a side effect, runtime stitches the response
    /// into a synthetic cursor and re-renders `then` with that cursor
    /// as input.
    Effect {
        eff:  ReactEffect,
        then: Box<CursorNode>,
    },

    /// null: consume cursor, emit nothing.
    Done,

    // /// createPortal: route node to a different pipe. Deferred — Mount +
    // /// default-flow + Many+Mount tee cover the cases we have. Reactivate
    // /// if a real use case arrives that Mount can't express.
    // Portal {
    //     to:   PipeRef,
    //     node: Box<CursorNode>,
    // },
}

/// Reference to a pipe — by name (registered in PipeRegistry, Phase 7)
/// or by inline `PipeHash` (Phase 5 hardcoded subpipes).
#[derive(Debug, Clone)]
pub enum PipeRef {
    /// Inline subpipe identified by content hash. Looked up in the
    /// runtime's pipe table by hash.
    Hash(PipeHash),
}

/// blake3-truncated-to-u64 hash of a pipe's component identities, used
/// to address pipes in `mount_registry` and queue rows.
pub type PipeHash = u64;
