//! `expand` — the queue-backed loop, generic over carrier.
//!
//! Pull a runnable row → look up the component at `depth` → render →
//! flatten → enqueue children. Repeat until nothing is runnable.
//!
//! Synchronous Phase-3 form. Yield parks rows; the caller advances
//! the `EventBus` ready set (or the global tick) and re-enters `expand`
//! to make progress.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::component::{DynComponent, RenderCtx};
use super::diag::{DiagSink, NoopDiagSink};
use super::event_bus::EventBus;
use super::next::Next;
use super::queue::{
    ExpandTick, InstanceId, PipeHash, QueueBackend, QueueRow,
};
use super::wake::Wake;

/// Unbuilt pipe value. The shape DSL holes resolve to at lower-time:
/// a flat sequence of components over carrier `N`. CST grammars can
/// produce this without a runtime dep beyond the `Component` trait.
///
/// Build into a runnable form via `into_instance()`.
pub struct Pipe<N: Next> {
    pub steps: Vec<DynComponent<N>>,
}

impl<N: Next> Pipe<N> {
    pub fn new() -> Self { Self { steps: Vec::new() } }

    pub fn from_steps(steps: Vec<DynComponent<N>>) -> Self {
        Self { steps }
    }

    /// Append a step. Builder form: `Pipe::new().step(a).step(b)`.
    pub fn step(mut self, c: DynComponent<N>) -> Self {
        self.steps.push(c);
        self
    }

    /// Append all steps from another pipe. Used when a DSL hole's
    /// inner pipe is spliced into an outer one at lower-time.
    pub fn extend(mut self, other: Pipe<N>) -> Self {
        self.steps.extend(other.steps);
        self
    }

    pub fn len(&self) -> usize { self.steps.len() }
    pub fn is_empty(&self) -> bool { self.steps.is_empty() }

    /// Convert to a mounted, runnable instance. Pipe-hash and
    /// instance-id default to 0 today; assign once the lower-pass
    /// has identity stamping.
    pub fn into_instance(self) -> PipeInstance<N> {
        PipeInstance::new(self.steps)
    }
}

impl<N: Next> Default for Pipe<N> {
    fn default() -> Self { Self::new() }
}

/// One mounted pipe instance. Pipe homogeneous in `N`; components
/// pinned via `dyn Component<Next = N>`.
pub struct PipeInstance<N: Next> {
    pub pipe_hash:   PipeHash,
    pub instance_id: InstanceId,
    pub components:  Vec<DynComponent<N>>,
}

impl<N: Next> PipeInstance<N> {
    pub fn new(components: Vec<DynComponent<N>>) -> Self {
        Self { pipe_hash: 0, instance_id: 0, components }
    }
}

/// Caller-supplied state. Holds the `EventBus` so the driver can
/// consult ready keys per loop iteration. `batch_cap` is the max rows
/// the driver pulls per dispatch — Component::render_batch sees up to
/// that many homogeneous rows in one call.
#[derive(Clone)]
pub struct ExpandOpts {
    pub bus:       Arc<EventBus>,
    pub diag:      Arc<dyn DiagSink>,
    pub batch_cap: usize,
}

pub const DEFAULT_BATCH_CAP: usize = 256;

impl Default for ExpandOpts {
    fn default() -> Self {
        Self {
            bus:       Arc::new(EventBus::new()),
            diag:      Arc::new(NoopDiagSink),
            batch_cap: DEFAULT_BATCH_CAP,
        }
    }
}

impl ExpandOpts {
    pub fn new() -> Self { Self::default() }

    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.bus = bus;
        self
    }

    pub fn with_diag(mut self, diag: Arc<dyn DiagSink>) -> Self {
        self.diag = diag;
        self
    }

    pub fn with_batch_cap(mut self, cap: usize) -> Self {
        self.batch_cap = cap.max(1);
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct ExpandStats {
    pub rendered: u64,
    pub emitted:  u64,
    pub terminal: u64,
    pub parked:   u64,
}

static GLOBAL_TICK: AtomicU64 = AtomicU64::new(1);

fn bump_global_tick() -> ExpandTick {
    GLOBAL_TICK.fetch_add(1, Ordering::SeqCst)
}

/// Drive a single pipe instance until nothing is runnable. Pass empty
/// `seed` to resume an already-seeded queue.
pub fn expand<N: Next>(
    pipe:  &PipeInstance<N>,
    queue: Arc<dyn QueueBackend<N>>,
    seed:  Vec<Arc<N>>,
    opts:  ExpandOpts,
) -> ExpandStats {
    let expand_tick = bump_global_tick();

    for value in seed {
        queue.enqueue(QueueRow {
            id:             0,
            parent_id:      None,
            batch_idx:      0,
            path:           Vec::new(),
            pipe_hash:      pipe.pipe_hash,
            instance_id:    pipe.instance_id,
            depth:             0,
            value,
            wake:           Wake::Immediate,
            expand_tick,
            enqueued_at_ns: 0,
        });
    }

    let mut stats = ExpandStats::default();

    loop {
        let batch = queue.pull_runnable_batch(expand_tick, opts.batch_cap);
        if batch.is_empty() {
            stats.parked = queue.depth();
            break;
        }

        let head_depth = batch[0].depth;
        if head_depth as usize >= pipe.components.len() {
            stats.terminal += batch.len() as u64;
            continue;
        }

        let comp = &pipe.components[head_depth as usize];
        let ctx  = RenderCtx::new(batch[0].pipe_hash, head_depth, expand_tick)
            .with_bus(opts.bus.clone())
            .with_diag(opts.diag.clone());

        // PHASE E (deferred): reconciliation hook lives inside
        // `dispatch`. Before enqueueing new children, an override can
        // multiset-diff this row's prior child NextKeys against the
        // new ones via `queue.cascade_delete(child_id)`. Today every
        // render is a fresh row, so there's no prior set to diff;
        // Phase E lights up when Memoize+Yield get composed.

        let depth_before = queue.depth();
        let batch_len    = batch.len() as u64;
        comp.dispatch(&ctx, &batch, queue.as_ref());
        let depth_after  = queue.depth();

        stats.rendered += batch_len;
        stats.emitted  += depth_after.saturating_sub(depth_before);
    }

    stats
}
