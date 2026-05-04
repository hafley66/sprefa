//! In-RAM `QueueBackend<N>` impl. Single mutex on a struct of buckets.
//!
//! Lab-quality: one big lock, no sharding. Park subscriptions live in
//! the queue itself, indexed by `(domain, key)`.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::next::Next;
use super::next_key::NextKey;
use super::queue::{
    DriveTick, QueueBackend, QueueId, QueueRow,
};
use super::wake::Wake;

pub struct MemQueue<N: Next> {
    next_id: AtomicU64,
    state:   Mutex<State<N>>,
}

struct State<N: Next> {
    runnable:    VecDeque<QueueRow<N>>,
    tick_parked: Vec<QueueRow<N>>,
    by_key:      HashMap<(Cow<'static, str>, NextKey), Vec<QueueRow<N>>>,
    depth:       u64,
}

impl<N: Next> MemQueue<N> {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            state: Mutex::new(State {
                runnable:    VecDeque::new(),
                tick_parked: Vec::new(),
                by_key:      HashMap::new(),
                depth:       0,
            }),
        }
    }
}

impl<N: Next> Default for MemQueue<N> {
    fn default() -> Self { Self::new() }
}

impl<N: Next> QueueBackend<N> for MemQueue<N> {
    fn enqueue(&self, mut row: QueueRow<N>) -> QueueId {
        if row.id == 0 {
            row.id = self.next_id.fetch_add(1, Ordering::SeqCst);
        }
        let id = row.id;
        let mut s = self.state.lock().unwrap();
        s.depth += 1;
        match &row.wake {
            Wake::Immediate    => s.runnable.push_back(row),
            Wake::Tick { .. }  => s.tick_parked.push(row),
            Wake::Key { domain, key } => {
                let k = (domain.clone(), *key);
                s.by_key.entry(k).or_default().push(row);
            }
        }
        id
    }

    fn pull_runnable(
        &self,
        global_tick: DriveTick,
    ) -> Option<QueueRow<N>> {
        let mut s = self.state.lock().unwrap();

        if let Some(row) = s.runnable.pop_front() {
            s.depth -= 1;
            return Some(row);
        }

        let tick_idx = s.tick_parked.iter().position(|r|
            matches!(&r.wake, Wake::Tick { past_tick } if *past_tick < global_tick)
        );
        if let Some(i) = tick_idx {
            let row = s.tick_parked.swap_remove(i);
            s.depth -= 1;
            return Some(row);
        }

        None
    }

    fn depth(&self) -> u64 {
        self.state.lock().unwrap().depth
    }

    fn dispatch_park(&self, domain: &str, key: Option<NextKey>) -> u64 {
        let mut s = self.state.lock().unwrap();
        // Collect matching keys to drain — by_key is keyed by
        // (Cow<str>, NextKey) so domain-only sweeps walk the whole map.
        let matching: Vec<(Cow<'static, str>, NextKey)> = s.by_key.keys()
            .filter(|(d, k)| {
                d.as_ref() == domain && key.map_or(true, |want| *k == want)
            })
            .cloned()
            .collect();
        let mut promoted = 0u64;
        for k in matching {
            if let Some(rows) = s.by_key.remove(&k) {
                for mut row in rows {
                    row.wake = Wake::Immediate;
                    s.runnable.push_back(row);
                    promoted += 1;
                }
            }
        }
        promoted
    }

    fn pull_runnable_batch(
        &self,
        global_tick: DriveTick,
        n:           usize,
    ) -> Vec<QueueRow<N>> {
        self.pull_runnable_batch_impl(global_tick, n)
    }
}

impl<N: Next> MemQueue<N> {
    /// Drain `Wake::Key` rows where `pred(row)` returns true. Returns
    /// the owned rows; depth is decremented for each. Used by
    /// `HybridQueue::tick_flush` to evict aged parks down to sqlite.
    pub fn drain_parks_where(
        &self,
        mut pred: impl FnMut(&QueueRow<N>) -> bool,
    ) -> Vec<QueueRow<N>> {
        let mut s = self.state.lock().unwrap();
        let mut out = Vec::new();
        let keys: Vec<(Cow<'static, str>, NextKey)> = s.by_key.keys().cloned().collect();
        for k in keys {
            // Partition each bucket: keep non-matching, take matching.
            let bucket = s.by_key.remove(&k).unwrap();
            let mut keep = Vec::with_capacity(bucket.len());
            for row in bucket {
                if pred(&row) {
                    s.depth -= 1;
                    out.push(row);
                } else {
                    keep.push(row);
                }
            }
            if !keep.is_empty() {
                s.by_key.insert(k, keep);
            }
        }
        out
    }

    /// Park count (Wake::Key rows only). Cap-enforcement helper.
    pub fn park_count(&self) -> usize {
        let s = self.state.lock().unwrap();
        s.by_key.values().map(|v| v.len()).sum()
    }

    fn pull_runnable_batch_impl(
        &self,
        global_tick: DriveTick,
        n:           usize,
    ) -> Vec<QueueRow<N>> {
        if n == 0 { return Vec::new(); }
        let mut s = self.state.lock().unwrap();

        // Hot path: runnable VecDeque. Drain a homogeneous prefix
        // without reordering — peek front, pop only if `(pipe_hash,
        // depth)` matches the head's.
        if let Some(head) = s.runnable.front() {
            let head_pipe  = head.pipe_hash;
            let head_depth = head.depth;
            let mut out = Vec::with_capacity(n);
            while out.len() < n {
                let matches = s.runnable.front()
                    .map(|r| r.pipe_hash == head_pipe && r.depth == head_depth)
                    .unwrap_or(false);
                if !matches { break; }
                let row = s.runnable.pop_front().unwrap();
                s.depth -= 1;
                out.push(row);
            }
            return out;
        }

        // Cold path: tick-parked. Fall through to single-row pull.
        drop(s);
        match self.pull_runnable(global_tick) {
            Some(r) => vec![r],
            None    => Vec::new(),
        }
    }
}
