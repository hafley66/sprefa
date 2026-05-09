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

use super::component::{ComponentLifecycle, DynComponent, RenderCtx};
use super::diag::{DiagSink, NoopDiagSink};
use super::event_bus::EventBus;
use super::next::Next;
use super::queue::{
    BarrierScope, ExpandTick, InstanceId, PipeHash, QueueBackend, QueueRow,
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

// Cloning a Pipe deep-copies the step Vec but shares each component
// via Arc clone. Lower-time call sites need this when a Value::Pipe
// argument is consumed in more than one place (e.g. validate scans the
// shape, lower then folds the steps in).
impl<N: Next> Clone for Pipe<N> {
    fn clone(&self) -> Self {
        Self { steps: self.steps.iter().cloned().collect() }
    }
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
        let batch = queue.pull_runnable_batch_for(
            pipe.pipe_hash,
            pipe.instance_id,
            expand_tick,
            opts.batch_cap,
        );
        if batch.is_empty() {
            if drive_barriers(pipe, queue.as_ref(), expand_tick, &opts) {
                continue;
            }
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

        // Gate every tracing-related cost on whether a subscriber wants
        // it. `event_enabled!` is one atomic load when no subscriber is
        // installed; the Instant::now() and span allocation only run
        // when DEBUG events on the `expand` target are sampled.
        let trace_on = tracing::event_enabled!(
            target: "expand", tracing::Level::DEBUG
        );

        if trace_on {
            let kind = comp.kind();
            let span = tracing::debug_span!(
                target: "expand",
                "render_batch",
                op    = kind,
                depth = head_depth as u32,
                n     = batch_len,
            );
            let _g = span.enter();
            let t0 = std::time::Instant::now();
            comp.dispatch(&ctx, &batch, queue.as_ref());
            let elapsed = t0.elapsed();
            let depth_after = queue.depth();
            let emitted_in_batch = depth_after.saturating_sub(depth_before);

            tracing::debug!(
                target: "expand",
                op       = kind,
                depth    = head_depth as u32,
                n        = batch_len,
                emitted  = emitted_in_batch,
                wall_us  = elapsed.as_micros() as u64,
                "batch"
            );

            stats.rendered += batch_len;
            stats.emitted  += emitted_in_batch;
        } else {
            comp.dispatch(&ctx, &batch, queue.as_ref());
            let depth_after = queue.depth();
            stats.rendered += batch_len;
            stats.emitted  += depth_after.saturating_sub(depth_before);
        }
    }

    stats
}

fn drive_barriers<N: Next>(
    pipe:        &PipeInstance<N>,
    queue:       &dyn QueueBackend<N>,
    expand_tick: ExpandTick,
    opts:        &ExpandOpts,
) -> bool {
    let mut emitted = false;

    for (depth, comp) in pipe.components.iter().enumerate() {
        if comp.lifecycle() != ComponentLifecycle::Barrier {
            continue;
        }

        let depth = depth as u32;
        let scope = BarrierScope {
            pipe_hash: pipe.pipe_hash,
            instance_id: pipe.instance_id,
            expand_tick,
            depth,
        };
        let ctx = RenderCtx::new(pipe.pipe_hash, depth, expand_tick)
            .with_bus(opts.bus.clone())
            .with_diag(opts.diag.clone());
        let pending = queue.pending_summary_before_or_at(
            pipe.pipe_hash,
            pipe.instance_id,
            expand_tick,
            depth,
        );

        let before = queue.depth();
        if pending.parked > 0 {
            comp.idle(&ctx, scope, pending, queue);
        } else if pending.total() == 0 {
            comp.complete(&ctx, scope, queue);
        }
        emitted |= queue.depth() > before;
    }

    emitted
}
