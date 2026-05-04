//! `drive` — the queue-backed loop, generic over carrier.
//!
//! Pull a runnable row → look up the component at `pc` → render →
//! flatten → enqueue children. Repeat until nothing is runnable.
//!
//! Synchronous Phase-3 form. Suspense parks rows; the caller advances
//! the `EventBus` ready set (or the global tick) and re-enters `drive`
//! to make progress.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::component::{DynComponent, RenderCtx};
use super::event_bus::EventBus;
use super::flatten::flatten;
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
/// consult ready keys per loop iteration.
#[derive(Clone)]
pub struct DriveOpts {
    pub bus: Arc<EventBus>,
}

impl Default for DriveOpts {
    fn default() -> Self {
        Self { bus: Arc::new(EventBus::new()) }
    }
}

impl DriveOpts {
    pub fn new() -> Self { Self::default() }

    pub fn with_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.bus = bus;
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
            pc:             0,
            value,
            wake:           Wake::Immediate,
            drive_tick,
            enqueued_at_ns: 0,
        });
    }

    let mut stats = DriveStats::default();

    loop {
        let ready = opts.bus.snapshot_ready();
        let row = match queue.pull_runnable(&ready, drive_tick) {
            Some(r) => r,
            None    => {
                stats.parked = queue.depth();
                break;
            }
        };
        // Forget the key now that the row has been pulled. Keeps the
        // ready set bounded; future Suspense for the same logical
        // pause uses a fresh key.
        if let Wake::Key(k) = &row.wake {
            opts.bus.forget(*k);
        }

        if row.pc as usize >= pipe.components.len() {
            stats.terminal += 1;
            continue;
        }

        let comp = &pipe.components[row.pc as usize];
        let ctx = RenderCtx::new(row.pipe_hash, row.pc);

        let node = comp.render(&ctx, &row.value);
        let next_pc = row.pc + 1;
        let children = flatten(node, &row, next_pc, drive_tick);
        stats.rendered += 1;
        stats.emitted += children.len() as u64;

        for child in children {
            queue.enqueue(child);
        }
    }

    stats
}
