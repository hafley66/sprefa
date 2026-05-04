//! `Component` — three-tier render surface.
//!
//! Pipes are `Vec<Arc<dyn Component<Next = N>>>`. Trait-object form
//! pins the associated type, so a pipe is homogeneous in N even though
//! the components themselves are heterogeneous.
//!
//! Override the tier that fits the work:
//!
//!   tier 1 — `render(&self, ctx, &N) -> Node<N>`
//!     Per-row pure transform. Default = `Node::Done` (drop input).
//!
//!   tier 2 — `render_batch(&self, ctx, &[&N]) -> Vec<Node<N>>`
//!     Per-batch pure transform. Default = loop `render`.
//!     Override when work amortizes across the batch (rayon, SIMD,
//!     shared pattern compile, sqlite IN(...) lookup).
//!
//!   tier 3 — `dispatch(&self, ctx, rows, queue, bus)`
//!     Full substrate interaction. Default = call `render_batch` and
//!     splice each result via `flatten` + `queue.enqueue`. Override to
//!     control fanout shape (mergeMap with custom concurrency,
//!     switchMap-style cancellation by `KeyDirty(prev)`, parker
//!     enqueue with `Wake::Key`, debounce via `Wake::Tick`, Spawner
//!     handoff for Mutation, etc.).
//!
//! No method is mandatory. A Component that overrides nothing is a
//! no-op (drops every input). Defaults flow inner → outer:
//! `dispatch` → `render_batch` → `render`. No recursion: `render`'s
//! terminal default is `Node::Done`.

use std::sync::Arc;

use super::event_bus::EventBus;
use super::flatten::splice_into;
use super::next::Next;
use super::node::Node;
use super::queue::{DriveTick, PipeHash, QueueBackend, QueueRow};

pub trait Component: Send + Sync + 'static {
    type Next: Next;

    /// Tier 1. Per-row pure transform. Terminal default = drop input.
    fn render(&self, _ctx: &RenderCtx, _c: &Self::Next) -> Node<Self::Next> {
        Node::Done
    }

    /// Tier 2. Per-batch pure transform. Default = loop `render`.
    fn render_batch(
        &self,
        ctx:   &RenderCtx,
        batch: &[&Self::Next],
    ) -> Vec<Node<Self::Next>> {
        batch.iter().map(|c| self.render(ctx, c)).collect()
    }

    /// Tier 3. Full substrate-interaction surface. Default = call
    /// `render_batch` and splice each child row into `queue` with
    /// `Wake::Immediate` via `flatten`. Override to control fanout
    /// shape, parker enqueue, domain dispatch, etc.
    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Self::Next>],
        queue: &dyn QueueBackend<Self::Next>,
        _bus:  &EventBus,
    ) {
        let inputs: Vec<&Self::Next> =
            rows.iter().map(|r| r.value.as_ref()).collect();
        let nodes = self.render_batch(ctx, &inputs);
        for (row, node) in rows.iter().zip(nodes) {
            splice_into(row, node, ctx.depth + 1, ctx.drive_tick, queue);
        }
    }

    /// Driver hint for `pull_runnable_batch`: max rows to hand to
    /// `dispatch` at once. `None` lets the driver pick a default.
    /// `Some(1)` forces per-row.
    fn batch_hint(&self) -> Option<usize> { None }
}

/// Render-call context. Carries the pipe identity, the depth being
/// rendered, and the current drive tick (needed by `flatten` to mint
/// child `enqueued_at_ns` and stamp `drive_tick`).
#[derive(Clone, Debug)]
pub struct RenderCtx {
    pub pipe:       PipeHash,
    pub depth:      u32,
    pub drive_tick: DriveTick,
}

impl RenderCtx {
    pub fn new(pipe: PipeHash, depth: u32, drive_tick: DriveTick) -> Self {
        Self { pipe, depth, drive_tick }
    }
}

/// Type alias for the trait-object form a pipe stores. Reduces
/// `Vec<Arc<dyn Component<Next = N>>>` noise at use sites.
pub type DynComponent<N> = Arc<dyn Component<Next = N>>;

/// Rayon-parallel `render_batch` helper. Maps `f` across `batch` and
/// returns `Vec<Node<N>>` in input order. The rxjs-mergeMap analog for
/// sync sub-tasks. Use inside a `render_batch` override:
///
/// ```ignore
/// fn render_batch(&self, _ctx: &RenderCtx, batch: &[&MyCarrier])
///     -> Vec<Node<MyCarrier>>
/// {
///     par_render(batch, |c| Node::Emit(Arc::new(do_cpu_heavy(c))))
/// }
/// ```
///
/// Mirror of v3's `per_cursor` (`v3/crates/pipeline/src/_1_op.rs`),
/// rewritten sync over rayon's work-stealing pool instead of
/// `futures::buffer_unordered(64)`. Preserves the lift contract:
/// per-row work amortizes across the rayon thread pool, results land
/// back in the same order as inputs.
pub fn par_render<N, F>(batch: &[&N], f: F) -> Vec<Node<N>>
where
    N: Next,
    F: Fn(&N) -> Node<N> + Send + Sync,
{
    use rayon::prelude::*;
    batch.par_iter().map(|c| f(*c)).collect()
}
