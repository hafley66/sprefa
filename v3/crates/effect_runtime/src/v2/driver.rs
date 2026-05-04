//! `drive` — the queue-backed loop, generic over carrier.
//!
//! Pull a runnable row → look up the component at `depth` → render →
//! flatten → enqueue children. Repeat until nothing is runnable.
//!
//! Synchronous Phase-3 form. Yield parks rows; the caller advances
//! the `EventBus` ready set (or the global tick) and re-enters `drive`
//! to make progress.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::component::{DynComponent, RenderCtx};
use super::event_bus::EventBus;
use super::next::Next;
use super::queue::{
    DriveTick, InstanceId, PipeHash, QueueBackend, QueueRow,
};
use super::wake::Wake;

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
pub struct DriveOpts {
    pub bus:       Arc<EventBus>,
    pub batch_cap: usize,
}

pub const DEFAULT_BATCH_CAP: usize = 256;

impl Default for DriveOpts {
    fn default() -> Self {
        Self {
            bus:       Arc::new(EventBus::new()),
            batch_cap: DEFAULT_BATCH_CAP,
        }
    }
}

impl DriveOpts {
    pub fn new() -> Self { Self::default() }

    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.bus = bus;
        self
    }

    pub fn with_batch_cap(mut self, cap: usize) -> Self {
        self.batch_cap = cap.max(1);
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct DriveStats {
    pub rendered: u64,
    pub emitted:  u64,
    pub terminal: u64,
    pub parked:   u64,
}

static GLOBAL_TICK: AtomicU64 = AtomicU64::new(1);

fn bump_global_tick() -> DriveTick {
    GLOBAL_TICK.fetch_add(1, Ordering::SeqCst)
}

/// Drive a single pipe instance until nothing is runnable. Pass empty
/// `seed` to resume an already-seeded queue.
pub fn drive<N: Next>(
    pipe:  &PipeInstance<N>,
    queue: Arc<dyn QueueBackend<N>>,
    seed:  Vec<Arc<N>>,
    opts:  DriveOpts,
) -> DriveStats {
    let drive_tick = bump_global_tick();

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
            drive_tick,
            enqueued_at_ns: 0,
        });
    }

    let mut stats = DriveStats::default();

    loop {
        let batch = queue.pull_runnable_batch(drive_tick, opts.batch_cap);
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
        let ctx  = RenderCtx::new(batch[0].pipe_hash, head_depth, drive_tick);

        // PHASE E (deferred): reconciliation hook lives inside
        // `dispatch`. Before enqueueing new children, an override can
        // multiset-diff this row's prior child NextKeys against the
        // new ones via `queue.cascade_delete(child_id)`. Today every
        // render is a fresh row, so there's no prior set to diff;
        // Phase E lights up when Memoize+Yield get composed.

        let depth_before = queue.depth();
        let batch_len    = batch.len() as u64;
        comp.dispatch(&ctx, &batch, queue.as_ref(), opts.bus.as_ref());
        let depth_after  = queue.depth();

        stats.rendered += batch_len;
        stats.emitted  += depth_after.saturating_sub(depth_before);
    }

    stats
}
