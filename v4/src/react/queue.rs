// QueueBackend — pluggable storage for cursor queue rows.
//
// One trait, two impls (MemQueue here, SqliteQueue in Phase 7). The
// driver code is identical against any backend. The choice is between
// throughput / RAM / durability profiles, not semantics.
//
// Hard rule: ops never touch the queue. Only the driver does. The
// `Cursor` type stays as `Arc<Cursor>` in MemQueue and gets encoded to
// bytes in SqliteQueue — the trait abstracts both as `QueueRow`.

use std::sync::Arc;
use anyhow::Result;

use crate::Cursor;
use crate::Gen;
use super::node::PipeHash;
use super::wake::{Wake, SinkId};

pub type QueueId    = u64;
pub type InstanceId = u64;
pub type DriveTick  = u64;

/// One in-flight or parked cursor instance. Identifies what to render
/// next, with what cursor, where to put the result, and what wakes it.
#[derive(Debug, Clone)]
pub struct QueueRow {
    pub id:             QueueId,
    pub parent_id:      Option<QueueId>,
    pub batch_idx:      u32,
    pub pipe_hash:      PipeHash,
    pub instance_id:    InstanceId,
    pub pc:             u32,
    pub cursor:         Arc<Cursor>,
    pub wake:           Wake,
    pub drive_tick:     DriveTick,
    pub enqueued_at_ns: u64,
}

/// Sink-gen view passed to `pull_runnable` so SinkGen-parked rows can
/// be evaluated. Indexed by `SinkId`. Owned by `SinkGenTracker` (Phase 4).
pub type SinkGenView<'a> = &'a [Gen];

pub trait QueueBackend: Send + Sync + 'static {
    /// Insert a new row. Backend assigns the QueueId. Returns it.
    fn enqueue(&self, row: QueueRow) -> Result<QueueId>;

    /// Pull one runnable row. "Runnable" means:
    ///   - wake = Immediate, OR
    ///   - wake = Tick{past_tick} AND past_tick < global_tick (caller-supplied), OR
    ///   - wake = SinkGen{sink, key_pfx, past_gen} AND
    ///       sink_gens[sink as usize] > past_gen AND
    ///       (key_pfx is empty OR matches latest commit on `sink`)
    ///
    /// Returns None when no row is currently runnable. Caller is
    /// responsible for calling `delete` after rendering.
    fn pull_runnable(&self, sink_gens: SinkGenView<'_>, global_tick: DriveTick)
        -> Result<Option<QueueRow>>;

    /// Delete a row by id. Used after the driver has consumed a row's
    /// render result and enqueued the children.
    fn delete(&self, id: QueueId) -> Result<()>;

    /// Wake-index lookup: enumerate row ids parked on `sink` whose
    /// key_pfx matches `committed_key` and whose past_gen < new_gen.
    /// Optional helper; backends may use it for batch-wake fanout.
    fn wake_index_lookup(&self, sink: SinkId, committed_key: &[u8], new_gen: Gen)
        -> Result<Vec<QueueId>>;

    /// Delete all rows belonging to a mount instance. Used when a
    /// Mount with a new key reaps the prior instance's running and
    /// parked rows in one shot.
    fn unmount(&self, instance_id: InstanceId) -> Result<u64>;

    /// Total rows resident in the queue. Used by the driver to detect
    /// pipeline drain and by tests.
    fn depth(&self) -> Result<u64>;
}
