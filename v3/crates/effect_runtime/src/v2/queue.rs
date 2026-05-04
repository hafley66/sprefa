//! `QueueBackend<N>` — pluggable storage for queue rows.
//!
//! Generic over carrier `N: Next`. One trait, multiple impls
//! (`MemQueue<N>` here; a sqlite impl slots in for Phase F). The driver
//! code is identical against any backend.

use std::sync::Arc;

use super::next::Next;
use super::next_key::NextKey;
use super::wake::Wake;

pub type QueueId    = u64;
pub type InstanceId = u64;
pub type DriveTick  = u64;
pub type PipeHash   = u64;

/// One in-flight or parked value. `path` is the position trail from the
/// pipe root: each segment is the `batch_idx` taken at that depth. Roots
/// have `path = vec![]`. Used for prefix-cascade invalidation and (in
/// Phase F) sqlite prefix indices.
#[derive(Debug, Clone)]
pub struct QueueRow<N: Next> {
    pub id:             QueueId,
    pub parent_id:      Option<QueueId>,
    pub batch_idx:      u32,
    pub path:           Vec<u32>,
    pub pipe_hash:      PipeHash,
    pub instance_id:    InstanceId,
    pub depth:             u32,
    pub value:          Arc<N>,
    pub wake:           Wake,
    pub drive_tick:     DriveTick,
    pub enqueued_at_ns: u64,
}

/// View into ready keys, supplied to `pull_runnable`. Backend uses it
/// to decide which `Wake::Key(k)` rows are unblocked.
pub type ReadyKeys<'a> = &'a [NextKey];

pub trait QueueBackend<N: Next>: Send + Sync + 'static {
    /// Insert a new row. Backend assigns the QueueId. Returns it.
    fn enqueue(&self, row: QueueRow<N>) -> QueueId;

    /// Pull one runnable row. "Runnable" means:
    ///   - `Wake::Immediate`, OR
    ///   - `Wake::Tick { past_tick }` AND `past_tick < global_tick`, OR
    ///   - `Wake::Key(k)` AND `k` ∈ `ready_keys`.
    ///
    /// Returns None when no row is currently runnable.
    fn pull_runnable(
        &self,
        ready_keys:  ReadyKeys<'_>,
        global_tick: DriveTick,
    ) -> Option<QueueRow<N>>;

    /// Total rows resident in the queue.
    fn depth(&self) -> u64;

    // PHASE E (deferred): reconciliation / cascade-delete.
    //
    // fn cascade_delete(&self, root: QueueId) -> u64;
    //
    // Delete the row whose id == root, plus every descendant by
    // parent_id chain. Returns the number of rows removed.
    //
    //   MemQueue impl: BFS over by_subject + tick_parked + runnable
    //     buckets, collect ids whose parent chain hits root, delete.
    //   SqliteQueue impl: recursive CTE keyed on parent_id, single
    //     DELETE statement.
    //
    // Used by Memoize cache invalidation (drop the children that the
    // prior cached render emitted) and by Query Success→Idle
    // transitions (drop downstream rows derived from a now-stale
    // result). Needs a per-parent prior-children index — see the
    // PHASE E note in driver.rs and memoize.rs.
}
