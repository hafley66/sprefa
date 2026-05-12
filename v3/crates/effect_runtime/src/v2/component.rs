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

use std::any::Any;
use std::hash::Hasher;
use std::sync::Arc;

use super::diag::{DiagSink, NoopDiagSink};
use super::event_bus::EventBus;
use super::flatten::splice_into;
use super::next::Next;
use super::node::Node;
use super::queue::{
    BarrierScope, ExpandTick, PendingSummary, PipeHash, QueueBackend,
    QueueRow,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ComponentLifecycle {
    Streaming,
    Barrier,
}

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
    /// shape, parker enqueue, domain dispatch, etc. The bus and diag
    /// sink hang off `ctx`.
    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Self::Next>],
        queue: &dyn QueueBackend<Self::Next>,
    ) {
        let inputs: Vec<&Self::Next> =
            rows.iter().map(|r| r.value.as_ref()).collect();
        let nodes = self.render_batch(ctx, &inputs);
        for (row, node) in rows.iter().zip(nodes) {
            splice_into(row, node, ctx.depth + 1, ctx.expand_tick, queue);
        }
    }

    /// Driver hint for `pull_runnable_batch`: max rows to hand to
    /// `dispatch` at once. `None` lets the driver pick a default.
    /// `Some(1)` forces per-row.
    fn batch_hint(&self) -> Option<usize> { None }

    /// Barrier components buffer rows during `dispatch`, then flush
    /// from `idle` or `complete` once the scheduler observes upstream
    /// state. Streaming components ignore both hooks.
    fn lifecycle(&self) -> ComponentLifecycle { ComponentLifecycle::Streaming }

    /// Called when the scheduler has no runnable rows but upstream rows
    /// before this barrier are still parked.
    fn idle(
        &self,
        _ctx:     &RenderCtx,
        _scope:   BarrierScope,
        _pending: PendingSummary,
        _queue:   &dyn QueueBackend<Self::Next>,
    ) {
    }

    /// Called when the scheduler has no upstream rows before this
    /// barrier. Barrier implementations usually flush buffered rows
    /// to `depth + 1` here.
    fn complete(
        &self,
        _ctx:   &RenderCtx,
        _scope: BarrierScope,
        _queue: &dyn QueueBackend<Self::Next>,
    ) {
    }

    // ── introspection (default-empty, framework-neutral) ──────────────
    //
    // Mirrors saga effect descriptors / react hook deps — analysis is
    // data, not a closure. Userland (sprf, etc.) downcasts `describe()`
    // to its own typed descriptors; effect_runtime stays sprf-blind.

    /// Stable kind tag for tooling. Mirrors saga effect.type / redux
    /// action.type / react devtools hook kind. Default = Rust type
    /// name (debug-only — author-supplied override is preferred for
    /// tools that surface this label).
    fn kind(&self) -> &'static str { std::any::type_name::<Self>() }

    /// Coarse purity tag. Drives downstream memoize/replay/cancellation.
    ///   `Pure`      — render = f(input). Memoizable.
    ///   `Read`      — observes external state but emits no effect.
    ///   `Effectful` — emits writes / IO descriptions.
    fn purity(&self) -> Purity { Purity::Pure }

    /// Fold this component's identity into a hasher. Returns `true` if
    /// the component contributed identity bytes; `false` if it declines
    /// (callers treat that as "do not memoize across this step"). v3's
    /// `Op::cache_key` shape, generalized.
    fn cache_key(&self, _h: &mut dyn Hasher) -> bool { false }

    /// Static-analysis blob. Userland trait extensions downcast to
    /// recover strongly-typed configuration. Mirror of react devtools'
    /// `_debugHookType` + `memoizedState`. `None` = no descriptor.
    ///
    /// Components that compute a descriptor lazily must cache it on
    /// `self` (e.g. via `OnceLock`); the borrow is `&self`-bound.
    fn describe(&self) -> Option<&dyn Any> { None }
}

/// Coarse purity classification. Three levels for now; widen to the
/// 6-level Pure/Param/Relational/IO/Effectful/Stateful taxonomy when
/// the closure-analysis / SQL-hoist work needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Purity {
    Pure,
    Read,
    Effectful,
}

/// Render-call context. Per-call handle that every Component sees.
/// Carries pipe identity, the depth being rendered, the current expand
/// tick, plus the cross-cutting handles (`bus`, `diag`). Arc-shaped so
/// it clones cheaply across `flatten` and override sites.
#[derive(Clone)]
pub struct RenderCtx {
    pub pipe:        PipeHash,
    pub depth:       u32,
    pub expand_tick: ExpandTick,
    pub bus:         Arc<EventBus>,
    pub diag:        Arc<dyn DiagSink>,
    runtime:         Option<Arc<dyn Any + Send + Sync>>,
}

impl RenderCtx {
    pub fn new(pipe: PipeHash, depth: u32, expand_tick: ExpandTick) -> Self {
        Self {
            pipe,
            depth,
            expand_tick,
            bus:  Arc::new(EventBus::new()),
            diag: Arc::new(NoopDiagSink),
            runtime: None,
        }
    }

    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.bus = bus;
        self
    }

    pub fn with_diag(mut self, diag: Arc<dyn DiagSink>) -> Self {
        self.diag = diag;
        self
    }

    pub fn with_runtime<T: Any + Send + Sync>(mut self, runtime: Arc<T>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn with_runtime_any(mut self, runtime: Arc<dyn Any + Send + Sync>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    pub fn runtime<T: Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.runtime
            .as_ref()
            .and_then(|runtime| runtime.clone().downcast::<T>().ok())
    }

    pub fn put<E: RuntimePut>(&self, effect: E) -> E::Output {
        effect.apply(self)
    }
}

pub trait RuntimePut {
    type Output;

    fn apply(self, ctx: &RenderCtx) -> Self::Output;
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
