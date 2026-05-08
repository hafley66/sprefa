//! `QueueBackend<N>` — pluggable storage for queue rows.
//!
//! Generic over carrier `N: Next`. One trait, multiple impls
//! (`MemQueue<N>` here, `SqliteQueue<N>` for the durable tier). The
//! driver code is identical against any backend.

use std::sync::Arc;

use super::next::Next;
use super::next_key::NextKey;
use super::wake::Wake;

pub type QueueId    = u64;
pub type InstanceId = u64;
pub type ExpandTick  = u64;
pub type PipeHash   = u64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BarrierScope {
    pub pipe_hash:   PipeHash,
    pub instance_id: InstanceId,
    pub expand_tick: ExpandTick,
    pub depth:       u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PendingSummary {
    pub runnable: u64,
    pub parked:   u64,
}

impl PendingSummary {
    pub fn total(self) -> u64 { self.runnable + self.parked }
}

/// One in-flight or parked value. `path` is the position trail from the
/// pipe root: each segment is the `batch_idx` taken at that depth. Roots
/// have `path = vec![]`.
#[derive(Debug, Clone)]
pub struct QueueRow<N: Next> {
    pub id:             QueueId,
    pub parent_id:      Option<QueueId>,
    pub batch_idx:      u32,
    pub path:           Vec<u32>,
    pub pipe_hash:      PipeHash,
    pub instance_id:    InstanceId,
    pub depth:          u32,
    pub value:          Arc<N>,
    pub wake:           Wake,
    pub expand_tick:     ExpandTick,
    pub enqueued_at_ns: u64,
}

pub trait QueueBackend<N: Next>: Send + Sync {
    /// Insert a new row. Backend assigns the QueueId. Returns it.
    fn enqueue(&self, row: QueueRow<N>) -> QueueId;

    /// Pull one runnable row. "Runnable" means:
    ///   - `Wake::Immediate`, OR
    ///   - `Wake::Tick { past_tick }` AND `past_tick < global_tick`.
    ///
    /// `Wake::Key` rows become runnable only after `dispatch_park`
    /// flips them to `Immediate`.
    fn pull_runnable(
        &self,
        global_tick: ExpandTick,
    ) -> Option<QueueRow<N>>;

    /// Total rows resident in the queue.
    fn depth(&self) -> u64;

    /// Promote every parked row whose `Wake::Key { domain, key }`
    /// matches the given domain (and key, if `Some`) to
    /// `Wake::Immediate`. Returns the number of rows promoted.
    ///
    /// Default impl scans linearly via repeated single-row pulls of
    /// `Wake::Key` rows; backends override with an indexed UPDATE
    /// (sqlite) or by-key map promotion (mem). The default exists so
    /// the trait stays implementable by ad-hoc backends; production
    /// impls override it.
    fn dispatch_park(&self, _domain: &str, _key: Option<NextKey>) -> u64 {
        0
    }

    /// Pull up to `n` runnable rows in storage order, all sharing the
    /// same `(pipe_hash, depth)` so the driver can hand them to one
    /// Component's `dispatch` as a homogeneous batch.
    ///
    /// Default = single-row pull (`n` ignored, `min(1, n)`). Backends
    /// that can peek-then-pop without reordering override to actually
    /// fill the batch.
    fn pull_runnable_batch(
        &self,
        global_tick: ExpandTick,
        n:           usize,
    ) -> Vec<QueueRow<N>> {
        if n == 0 { return Vec::new(); }
        match self.pull_runnable(global_tick) {
            Some(r) => vec![r],
            None    => Vec::new(),
        }
    }

    /// Count rows in the same mounted pipe before or at `max_depth`.
    /// Used by barrier components (`collect`, materialized query sinks)
    /// to decide whether upstream has completed or is parked.
    fn pending_summary_before_or_at(
        &self,
        _pipe_hash:   PipeHash,
        _instance_id: InstanceId,
        _global_tick: ExpandTick,
        _max_depth:   u32,
    ) -> PendingSummary {
        PendingSummary::default()
    }

    /// Delete `root` (if present) and every descendant reachable
    /// through the `parent_id` chain. Returns the count of rows
    /// removed. Default impl is a no-op (returns 0); real backends
    /// override with a recursive walk.
    ///
    /// `root` need not still be in the queue — the typical caller is
    /// Memoize-on-evict, where the row that produced the cache entry
    /// has long since been popped and only its descendants linger as
    /// parked subscriptions or queued futures. The walk seeds on
    /// `id = root OR parent_id = root` so descendants are found even
    /// when the root row is absent.
    fn cascade_delete(&self, _root: QueueId) -> u64 { 0 }
}
