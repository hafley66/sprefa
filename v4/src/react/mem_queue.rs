// MemQueue — in-RAM QueueBackend impl.
//
// Single Mutex<State>. Runnable rows live in a VecDeque (FIFO).
// SinkGen-parked rows are bucketed by sink_id for fast wake fanout.
// Tick-parked rows live in a separate vec, scanned per pull (rare).
// External-parked rows live in a HashMap<token, row_id>.
// Mounts: HashMap<instance_id, HashSet<row_id>> for unmount-by-tag.
//
// Lock-step trade: one mutex on the whole structure. Fine for POC; a
// SqliteQueue impl will be the throughput backend when push comes to
// shove. If MemQueue itself becomes a bottleneck under one-thread, we
// shard later.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;

use crate::Gen;
use super::queue::{
    DriveTick, InstanceId, QueueBackend, QueueId, QueueRow, SinkGenView,
};
use super::wake::{ExternalToken, SinkId, Wake};

pub struct MemQueue {
    next_id: AtomicU64,
    state:   Mutex<State>,
}

struct State {
    /// FIFO of immediately-runnable rows. Pulled in order.
    runnable: VecDeque<QueueRow>,
    /// Tick-parked rows. Scanned linearly (small expected size).
    tick_parked: Vec<QueueRow>,
    /// SinkGen-parked rows, bucketed by sink id for index-style lookup.
    by_sink: HashMap<SinkId, Vec<QueueRow>>,
    /// External-parked rows, keyed by their token.
    by_external: HashMap<ExternalToken, QueueRow>,
    /// Mount membership: which row ids belong to which instance.
    mounts: HashMap<InstanceId, HashSet<QueueId>>,
    /// Total row count (sum of the buckets above) — kept as a counter
    /// to avoid scanning on `depth()`.
    depth: u64,
}

impl MemQueue {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            state: Mutex::new(State {
                runnable:    VecDeque::new(),
                tick_parked: Vec::new(),
                by_sink:     HashMap::new(),
                by_external: HashMap::new(),
                mounts:      HashMap::new(),
                depth:       0,
            }),
        }
    }
}

impl Default for MemQueue {
    fn default() -> Self { Self::new() }
}

fn assign_id(q: &MemQueue) -> QueueId {
    q.next_id.fetch_add(1, Ordering::SeqCst)
}

fn track_mount(state: &mut State, row: &QueueRow) {
    state.mounts.entry(row.instance_id).or_default().insert(row.id);
}

fn untrack_mount(state: &mut State, row_id: QueueId, instance_id: InstanceId) {
    if let Some(set) = state.mounts.get_mut(&instance_id) {
        set.remove(&row_id);
        if set.is_empty() { state.mounts.remove(&instance_id); }
    }
}

impl QueueBackend for MemQueue {
    fn enqueue(&self, mut row: QueueRow) -> Result<QueueId> {
        if row.id == 0 { row.id = assign_id(self); }
        let mut s = self.state.lock().unwrap();
        track_mount(&mut s, &row);
        s.depth += 1;
        match &row.wake {
            Wake::Immediate    => s.runnable.push_back(row),
            Wake::Tick { .. }  => s.tick_parked.push(row),
            Wake::SinkGen { sink, .. } => {
                let sink = *sink;
                s.by_sink.entry(sink).or_default().push(row);
            }
            Wake::External(tok) => {
                s.by_external.insert(*tok, row);
            }
        }
        // can't return id from row after move; recompute via next_id-1
        Ok(self.next_id.load(Ordering::SeqCst) - 1)
    }

    fn pull_runnable(&self, sink_gens: SinkGenView<'_>, global_tick: DriveTick)
        -> Result<Option<QueueRow>>
    {
        let mut s = self.state.lock().unwrap();

        // 1. Immediately runnable.
        if let Some(row) = s.runnable.pop_front() {
            untrack_mount(&mut s, row.id, row.instance_id);
            s.depth -= 1;
            return Ok(Some(row));
        }

        // 2. Tick-parked: any whose past_tick < global_tick.
        let tick_idx = s.tick_parked.iter().position(|r|
            matches!(&r.wake, Wake::Tick { past_tick } if *past_tick < global_tick)
        );
        if let Some(i) = tick_idx {
            let row = s.tick_parked.swap_remove(i);
            untrack_mount(&mut s, row.id, row.instance_id);
            s.depth -= 1;
            return Ok(Some(row));
        }

        // 3. SinkGen-parked: scan each sink bucket for a row whose
        //    past_gen < current sink_gens[sink]. Empty key_pfx wakes
        //    on any commit; non-empty is checked here (in MemQueue we
        //    don't track per-key commit history, so non-empty key_pfx
        //    matches any commit until SinkGenTracker tracks last keys).
        let mut found: Option<(SinkId, usize)> = None;
        for (sink, vec) in s.by_sink.iter() {
            let s_idx = (*sink) as usize;
            if s_idx >= sink_gens.len() { continue; }
            let cur = sink_gens[s_idx];
            for (i, r) in vec.iter().enumerate() {
                if let Wake::SinkGen { past_gen, .. } = &r.wake {
                    if *past_gen < cur {
                        found = Some((*sink, i));
                        break;
                    }
                }
            }
            if found.is_some() { break; }
        }
        if let Some((sink, i)) = found {
            let bucket = s.by_sink.get_mut(&sink).unwrap();
            let row = bucket.swap_remove(i);
            if bucket.is_empty() { s.by_sink.remove(&sink); }
            untrack_mount(&mut s, row.id, row.instance_id);
            s.depth -= 1;
            return Ok(Some(row));
        }

        Ok(None)
    }

    fn delete(&self, id: QueueId) -> Result<()> {
        // No-op for rows already pulled (pull_runnable removes them).
        // For rows not yet pulled (rare: external API to drop a row by
        // id), search across buckets.
        let mut s = self.state.lock().unwrap();
        // runnable
        if let Some(pos) = s.runnable.iter().position(|r| r.id == id) {
            let row = s.runnable.remove(pos).unwrap();
            untrack_mount(&mut s, row.id, row.instance_id);
            s.depth -= 1;
            return Ok(());
        }
        if let Some(pos) = s.tick_parked.iter().position(|r| r.id == id) {
            let row = s.tick_parked.swap_remove(pos);
            untrack_mount(&mut s, row.id, row.instance_id);
            s.depth -= 1;
            return Ok(());
        }
        let mut sink_hit: Option<SinkId> = None;
        let mut sink_pos: usize = 0;
        for (sink, vec) in s.by_sink.iter() {
            if let Some(pos) = vec.iter().position(|r| r.id == id) {
                sink_hit = Some(*sink);
                sink_pos = pos;
                break;
            }
        }
        if let Some(sink) = sink_hit {
            let bucket = s.by_sink.get_mut(&sink).unwrap();
            let row = bucket.swap_remove(sink_pos);
            if bucket.is_empty() { s.by_sink.remove(&sink); }
            untrack_mount(&mut s, row.id, row.instance_id);
            s.depth -= 1;
            return Ok(());
        }
        let ext_hit = s.by_external.iter()
            .find(|(_, r)| r.id == id).map(|(t, _)| *t);
        if let Some(t) = ext_hit {
            let row = s.by_external.remove(&t).unwrap();
            untrack_mount(&mut s, row.id, row.instance_id);
            s.depth -= 1;
        }
        Ok(())
    }

    fn wake_index_lookup(&self, sink: SinkId, _committed_key: &[u8], new_gen: Gen)
        -> Result<Vec<QueueId>>
    {
        let s = self.state.lock().unwrap();
        let mut out = Vec::new();
        if let Some(bucket) = s.by_sink.get(&sink) {
            for r in bucket {
                if let Wake::SinkGen { past_gen, .. } = &r.wake {
                    if *past_gen < new_gen { out.push(r.id); }
                }
            }
        }
        Ok(out)
    }

    fn unmount(&self, instance_id: InstanceId) -> Result<u64> {
        let mut s = self.state.lock().unwrap();
        let ids: Vec<QueueId> = s.mounts.get(&instance_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        let n = ids.len() as u64;
        for id in ids {
            // Remove from whichever bucket it lives in. Reuse delete().
            // delete() does its own mount untrack + depth decrement.
            drop(s);
            let _ = self.delete(id);
            s = self.state.lock().unwrap();
        }
        // mounts entry already cleaned by untrack_mount
        Ok(n)
    }

    fn depth(&self) -> Result<u64> {
        Ok(self.state.lock().unwrap().depth)
    }
}
