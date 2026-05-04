//! In-RAM `QueueBackend<N>` impl. Single mutex on a struct of buckets.
//!
//! Lab-quality: one big lock, no sharding. Fine for tests.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::next::Next;
use super::next_key::NextKey;
use super::queue::{
    DriveTick, QueueBackend, QueueId, QueueRow, ReadyKeys,
};
use super::wake::Wake;

pub struct MemQueue<N: Next> {
    next_id: AtomicU64,
    state:   Mutex<State<N>>,
}

struct State<N: Next> {
    runnable:    VecDeque<QueueRow<N>>,
    tick_parked: Vec<QueueRow<N>>,
    by_key:      HashMap<NextKey, Vec<QueueRow<N>>>,
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
            Wake::Key(k)       => {
                let k = *k;
                s.by_key.entry(k).or_default().push(row);
            }
        }
        id
    }

    fn pull_runnable(
        &self,
        ready_keys:  ReadyKeys<'_>,
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

        for k in ready_keys {
            if let Some(bucket) = s.by_key.get_mut(k) {
                if let Some(row) = bucket.pop() {
                    if bucket.is_empty() { s.by_key.remove(k); }
                    s.depth -= 1;
                    return Some(row);
                }
            }
        }

        None
    }

    fn depth(&self) -> u64 {
        self.state.lock().unwrap().depth
    }
}
