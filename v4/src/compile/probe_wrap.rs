//! `SpannedComponent` — walk-time wrapper that tags emits with a span.
//!
//! The walker, given a `LowerCtx::probe`, wraps every step in a lowered
//! `Pipe<Cursor>` with a `SpannedComponent` carrying the source byte
//! range of the originating `OpCall`. On each render the wrapper
//! delegates to its inner Component but routes queue enqueues through
//! a `ProbingQueue` that fires `ProbeSink::on_emit` per child row.
//!
//! Invariant: with `probe = None`, the walker emits the inner
//! components unchanged. With `probe = Some(_)`, semantics are
//! identical except for the side-effect of firing per-emit probes.
//! No row is dropped, reordered, or duplicated.
//!
//! Implementation note: the wrapper overrides `dispatch` only. That
//! captures every emit path (Emit / Many / Yield-with-value) because
//! all of them flow through `QueueBackend::enqueue`. Overriding
//! `render` or `render_batch` would miss components that override
//! `dispatch` for fanout shape (Memoize, parker writes, etc.).

use std::sync::Arc;

use effect_runtime::v2::{
    ByteRange, Component, DynComponent, ExpandTick, Node, Probe,
    ProbeSink, QueueBackend, QueueId, QueueRow, RenderCtx,
};
use effect_runtime::v2::next_key::NextKey;

use crate::Cursor;

/// Wraps a `DynComponent<Cursor>` with the byte range of the OpCall
/// that produced it. On dispatch, every queue enqueue goes through a
/// `ProbingQueue` that fires `probe.on_emit(Probe { span, cursor })`.
pub struct SpannedComponent {
    pub inner: DynComponent<Cursor>,
    pub span:  ByteRange,
    pub probe: Arc<dyn ProbeSink<Cursor>>,
}

impl Component for SpannedComponent {
    type Next = Cursor;

    fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        self.inner.render(ctx, c)
    }

    fn render_batch(
        &self,
        ctx:   &RenderCtx,
        batch: &[&Cursor],
    ) -> Vec<Node<Cursor>> {
        self.inner.render_batch(ctx, batch)
    }

    fn dispatch(
        &self,
        ctx:   &RenderCtx,
        rows:  &[QueueRow<Cursor>],
        queue: &dyn QueueBackend<Cursor>,
    ) {
        let wrapped = ProbingQueue {
            inner: queue,
            probe: self.probe.as_ref(),
            span:  self.span,
        };
        self.inner.dispatch(ctx, rows, &wrapped);
    }

    fn batch_hint(&self) -> Option<usize> { self.inner.batch_hint() }
}

/// `&dyn QueueBackend<Cursor>` adaptor that fires a probe on every
/// `enqueue` and forwards to the underlying backend. All other methods
/// pass through. Lifetime-bound to one `dispatch` call.
struct ProbingQueue<'a> {
    inner: &'a dyn QueueBackend<Cursor>,
    probe: &'a dyn ProbeSink<Cursor>,
    span:  ByteRange,
}

impl<'a> QueueBackend<Cursor> for ProbingQueue<'a> {
    fn enqueue(&self, row: QueueRow<Cursor>) -> QueueId {
        self.probe.on_emit(Probe {
            span:   self.span,
            cursor: row.value.clone(),
        });
        self.inner.enqueue(row)
    }

    fn pull_runnable(
        &self,
        global_tick: ExpandTick,
    ) -> Option<QueueRow<Cursor>> {
        self.inner.pull_runnable(global_tick)
    }

    fn depth(&self) -> u64 { self.inner.depth() }

    fn dispatch_park(&self, domain: &str, key: Option<NextKey>) -> u64 {
        self.inner.dispatch_park(domain, key)
    }

    fn pull_runnable_batch(
        &self,
        global_tick: ExpandTick,
        n:           usize,
    ) -> Vec<QueueRow<Cursor>> {
        self.inner.pull_runnable_batch(global_tick, n)
    }

    fn cascade_delete(&self, root: QueueId) -> u64 {
        self.inner.cascade_delete(root)
    }
}

// ProbingQueue is `Send + Sync` only as long as its borrows are; the
// trait bound `Send + Sync + 'static` on QueueBackend conflicts. We
// dodge it by passing `&dyn QueueBackend<Cursor>` directly through
// `dispatch`'s signature, which requires `&dyn QueueBackend<Cursor>`
// without a 'static bound. The trait object is created on the stack
// per dispatch call.
//
// (compile-time check: dispatch takes `&dyn QueueBackend<Self::Next>`
// not `&'static dyn ...`, so 'a-lifetime trait objects are accepted.)

// ── helpers ──────────────────────────────────────────────────────────

/// Wrap each step in `pipe.steps` with a `SpannedComponent` carrying
/// `span`. Used at walk-time when `LowerCtx::probe` is set.
pub fn wrap_pipe_with_span(
    pipe:  effect_runtime::v2::Pipe<Cursor>,
    span:  ByteRange,
    probe: Arc<dyn ProbeSink<Cursor>>,
) -> effect_runtime::v2::Pipe<Cursor> {
    let wrapped: Vec<DynComponent<Cursor>> = pipe.steps.into_iter()
        .map(|inner| {
            let sc: Arc<dyn Component<Next = Cursor>> = Arc::new(SpannedComponent {
                inner,
                span,
                probe: probe.clone(),
            });
            sc
        })
        .collect();
    effect_runtime::v2::Pipe::from_steps(wrapped)
}

