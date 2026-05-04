// drive_react — the queue-backed driver loop.
//
// One loop. Pull a runnable row, look up the component at `pc`, render
// it, flatten the result to N child rows, enqueue them. Repeat until
// the queue is empty.
//
// Phase 3 scope: synchronous, single-threaded, no Suspense / Mount /
// Effect handling. The driver does not block on anything; if no row is
// runnable it exits.
//
// Phase 4 will add SinkGenTracker integration so Suspense{SinkGen}
// rows wake. Phase 5 fills in Mount + Effect dispatch.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;

use crate::{Cursor, Interner, Telemetry};
use super::component::{Component, RenderCtx};
use super::flatten::flatten;
use super::node::PipeHash;
use super::queue::{DriveTick, InstanceId, QueueBackend, QueueRow};
use super::sink_gen_tracker::SinkGenTracker;
use super::wake::Wake;

/// One mounted pipe instance ready to drive. Components are indexed by
/// pc; pc out of range means "the last component emitted past the pipe
/// length, drop the row" (terminal).
pub struct PipeInstance {
    pub pipe_hash:   PipeHash,
    pub instance_id: InstanceId,
    pub components:  Vec<Arc<dyn Component>>,
}

impl PipeInstance {
    /// Default-id'd pipe instance from a component vec. Pipe hash and
    /// instance id default to 0; override via field assignment when the
    /// caller needs them.
    pub fn new(components: Vec<Arc<dyn Component>>) -> Self {
        Self { pipe_hash: 0, instance_id: 0, components }
    }
}

/// Runtime services and shared state passed to `drive_react`. Holds the
/// pieces every render needs (interner, telemetry, sink-gen tracker) so
/// the call site stays small. New fields land here, not in the function
/// signature — `DriveOpts::default()` keeps test sites stable.
#[derive(Clone)]
pub struct DriveOpts {
    pub interner:  Arc<Interner>,
    pub tele:      Telemetry,
    pub sink_gens: Arc<SinkGenTracker>,
}

impl Default for DriveOpts {
    fn default() -> Self {
        Self {
            interner:  Interner::new(),
            tele:      Telemetry::new(),
            sink_gens: Arc::new(SinkGenTracker::new()),
        }
    }
}

impl DriveOpts {
    pub fn new() -> Self { Self::default() }

    pub fn with_interner(mut self, i: Arc<Interner>)        -> Self { self.interner = i; self }
    pub fn with_tele(mut self, t: Telemetry)                -> Self { self.tele = t; self }
    pub fn with_sink_gens(mut self, s: Arc<SinkGenTracker>) -> Self { self.sink_gens = s; self }
}

/// Drive a single pipe instance until nothing is runnable.
///
/// `seed`: cursors to enqueue at pc=0. Pass `Vec::new()` to resume an
/// already-seeded queue (e.g., after advancing a sink gen).
///
/// Returns when `pull_runnable` returns None. Parked rows remain in the
/// queue; `DriveStats::parked` reports the count. Caller advances the
/// tracker (or pushes new seeds) and re-enters to make progress.
pub fn drive_react(
    pipe: &PipeInstance,
    queue: Arc<dyn QueueBackend>,
    seed: Vec<Arc<Cursor>>,
    opts: DriveOpts,
) -> Result<DriveStats> {
    let DriveOpts { interner, tele, sink_gens } = opts;
    let drive_tick = bump_global_tick();

    // Seed the queue.
    for cursor in seed {
        queue.enqueue(QueueRow {
            id:             0,
            parent_id:      None,
            batch_idx:      0,
            pipe_hash:      pipe.pipe_hash,
            instance_id:    pipe.instance_id,
            pc:             0,
            cursor,
            wake:           Wake::Immediate,
            drive_tick,
            enqueued_at_ns: 0,
        })?;
    }

    let mut stats = DriveStats::default();

    loop {
        let snap = sink_gens.snapshot();
        let row = match queue.pull_runnable(&snap, drive_tick)? {
            Some(r) => r,
            None    => {
                // Nothing runnable. Exit; caller advances tracker / global
                // tick and re-enters drive_react to pick up parked rows.
                stats.parked = queue.depth()?;
                break;
            }
        };

        if row.pc as usize >= pipe.components.len() {
            // Past the end — terminal. Drop the cursor.
            stats.terminal += 1;
            continue;
        }

        let comp = &pipe.components[row.pc as usize];
        let ctx = RenderCtx {
            interner: interner.clone(),
            tele:     tele.clone(),
            pipe:     row.pipe_hash,
            pc:       row.pc,
        };

        let node = comp.render(&ctx, &row.cursor);
        let next_pc = row.pc + 1;
        let children = flatten(node, &row, next_pc, drive_tick);
        stats.rendered += 1;
        stats.emitted += children.len() as u64;

        for child in children {
            queue.enqueue(child)?;
        }
    }

    Ok(stats)
}

#[derive(Debug, Default, Clone)]
pub struct DriveStats {
    pub rendered: u64,
    pub emitted:  u64,
    pub terminal: u64,
    /// Rows still resident in the queue at exit (parked, awaiting wake).
    pub parked:   u64,
}

static GLOBAL_TICK: AtomicU64 = AtomicU64::new(1);

fn bump_global_tick() -> DriveTick {
    GLOBAL_TICK.fetch_add(1, Ordering::SeqCst)
}
