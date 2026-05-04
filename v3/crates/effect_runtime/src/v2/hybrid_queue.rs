//! `HybridQueue<N>` — write-back cache layered over `SqliteQueue<N>`
//! with `MemQueue<N>` as the hot tier.
//!
//! A long-running server holding 10k file watches should not pay a
//! sqlite fsync per park. Most parks resolve quickly and never need to
//! reach disk. `HybridQueue` keeps fresh parks in RAM and debounces
//! flushes of stale ones to cold storage. The crash window equals
//! `park_flush_interval`; that knob is the explicit durability dial.
//!
//! - `enqueue` always lands in the hot tier.
//! - `dispatch_park` fans both tiers; whichever has the row promotes.
//! - `pull_runnable_batch` falls through hot to cold.
//! - `tick_flush` evicts aged `Wake::Key` rows from hot to cold in one
//!   sqlite transaction. `Wake::Immediate` and `Wake::Tick` rows stay
//!   hot since they will run shortly.
//!
//! Setting `park_flush_interval = Duration::ZERO` makes every
//! `tick_flush` a write-through. Setting it to `Duration::MAX` caps
//! memory only (rows never age out, only the `park_mem_cap` backstop
//! triggers eviction).

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::codec::Codec;
use super::mem_queue::MemQueue;
use super::next::Next;
use super::next_key::NextKey;
use super::queue::{DriveTick, QueueBackend, QueueId, QueueRow};
use super::sqlite_queue::SqliteQueue;

#[derive(Clone, Debug)]
pub struct HybridCfg {
    /// Age threshold for hot→cold eviction. ZERO = write-through; MAX
    /// = mem-cap-only.
    pub park_flush_interval: Duration,
    /// RSS backstop. When hot's park count exceeds this, the oldest
    /// parks flush regardless of age.
    pub park_mem_cap:        usize,
    /// Max rows per `tick_flush` call.
    pub park_flush_batch:    usize,
}

impl Default for HybridCfg {
    fn default() -> Self {
        Self {
            park_flush_interval: Duration::from_millis(100),
            park_mem_cap:        10_000,
            park_flush_batch:    256,
        }
    }
}

#[allow(dead_code)]
struct HybridState {
    last_flush: Instant,
}

pub struct HybridQueue<N: Next + Codec> {
    hot:   Arc<MemQueue<N>>,
    cold:  Arc<SqliteQueue<N>>,
    cfg:   HybridCfg,
    #[allow(dead_code)]
    state: Mutex<HybridState>,
}

impl<N: Next + Codec> HybridQueue<N> {
    pub fn new(
        hot:  Arc<MemQueue<N>>,
        cold: Arc<SqliteQueue<N>>,
        cfg:  HybridCfg,
    ) -> Self {
        Self {
            hot, cold, cfg,
            state: Mutex::new(HybridState { last_flush: Instant::now() }),
        }
    }

    pub fn open_in_memory(cfg: HybridCfg) -> std::io::Result<Self> {
        Ok(Self::new(
            Arc::new(MemQueue::new()),
            Arc::new(SqliteQueue::open_in_memory()),
            cfg,
        ))
    }

    pub fn open_file(path: &Path, cfg: HybridCfg) -> std::io::Result<Self> {
        Ok(Self::new(
            Arc::new(MemQueue::new()),
            Arc::new(SqliteQueue::open_file(path)),
            cfg,
        ))
    }

    pub fn hot(&self)  -> &Arc<MemQueue<N>>    { &self.hot }
    pub fn cold(&self) -> &Arc<SqliteQueue<N>> { &self.cold }
    pub fn cfg(&self)  -> &HybridCfg           { &self.cfg }

    /// Drain `Wake::Key` rows older than `park_flush_interval` from
    /// hot, bulk-insert into cold, return the number flushed. Capped
    /// per call by `park_flush_batch`.
    pub fn tick_flush(&self) -> usize {
        let interval_ns = self.cfg.park_flush_interval.as_nanos() as u64;
        let cutoff = now_ns().saturating_sub(interval_ns);
        let cap = self.cfg.park_flush_batch;
        self.flush_with(|row| row.enqueued_at_ns <= cutoff, cap)
    }

    /// Force-flush to keep hot park count under `park_mem_cap`. Ignores
    /// the age threshold.
    pub fn enforce_cap(&self) -> usize {
        let cap_target = self.cfg.park_mem_cap;
        let park_count = self.hot.park_count();
        if park_count <= cap_target { return 0; }
        let to_evict = park_count - cap_target;
        let batch = to_evict.min(self.cfg.park_flush_batch.max(to_evict));
        self.flush_with(|_| true, batch)
    }

    fn flush_with(
        &self,
        pred: impl Fn(&QueueRow<N>) -> bool,
        cap:  usize,
    ) -> usize {
        if cap == 0 { return 0; }
        let mut taken = 0usize;
        let evicted = self.hot.drain_parks_where(|row| {
            if taken >= cap { return false; }
            if pred(row) { taken += 1; true } else { false }
        });
        if evicted.is_empty() { return 0; }
        let n = self.cold.bulk_enqueue(evicted);
        if let Ok(mut s) = self.state.lock() {
            s.last_flush = Instant::now();
        }
        n
    }
}

impl<N: Next + Codec> QueueBackend<N> for HybridQueue<N> {
    fn enqueue(&self, row: QueueRow<N>) -> QueueId {
        let id = self.hot.enqueue(row);
        // Cheap backstop check; bounded by mem_cap subtraction.
        if self.hot.park_count() > self.cfg.park_mem_cap {
            self.enforce_cap();
        }
        id
    }

    fn pull_runnable(&self, global_tick: DriveTick) -> Option<QueueRow<N>> {
        if let Some(r) = self.hot.pull_runnable(global_tick) {
            return Some(r);
        }
        self.cold.pull_runnable(global_tick)
    }

    fn pull_runnable_batch(
        &self,
        global_tick: DriveTick,
        n:           usize,
    ) -> Vec<QueueRow<N>> {
        let hot = self.hot.pull_runnable_batch(global_tick, n);
        if !hot.is_empty() { return hot; }
        self.cold.pull_runnable_batch(global_tick, n)
    }

    fn depth(&self) -> u64 {
        self.hot.depth() + self.cold.depth()
    }

    fn dispatch_park(&self, domain: &str, key: Option<NextKey>) -> u64 {
        let h = self.hot.dispatch_park(domain, key);
        let c = self.cold.dispatch_park(domain, key);
        h + c
    }
}

fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}
