//! DdBfs — differential-dataflow single-source reachability, the resident oracle for the
//! sqlite-dd head-to-head. Same dataflow as the store's `examples/dd_reach.rs`:
//!   reach = roots.iterate(|inner| edges.semijoin(inner).map(child).concat(roots).distinct())
//! i.e. the reachable-from-roots set, maintained incrementally under edge inserts/deletes.
//! Long-lived worker fed per-round over a channel (the harness applies edits tick by tick).
//! Digest uses the SAME splitmix as reach_inc, so dd, sqlite, and the RAM oracle agree.

use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate; // semijoin/map/concat/distinct are inherent
use timely::dataflow::operators::probe::Handle as ProbeHandle;

fn mix(k: i64) -> i64 {
    let mut z = (k as u64).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    (z ^ (z >> 31)) as i64
}

#[derive(Default)]
struct Shared {
    counts: HashMap<i64, isize>,
    digest: i64,
    card: u64,
}

enum Cmd {
    Batch { edges: Vec<((i64, i64), isize)>, roots: Vec<(i64, isize)>, ack: SyncSender<()> },
    Stop,
}

pub struct DdBfs {
    tx: Option<SyncSender<Cmd>>,
    handle: Option<std::thread::JoinHandle<()>>,
    shared: Arc<Mutex<Shared>>,
}

impl Default for DdBfs {
    fn default() -> Self {
        Self::new()
    }
}

impl DdBfs {
    pub fn new() -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let shared_worker = shared.clone();
        let (tx, rx): (SyncSender<Cmd>, Receiver<Cmd>) = sync_channel(64);
        let rx = Arc::new(Mutex::new(rx));

        let handle = std::thread::spawn(move || {
            timely::execute_directly(move |worker| {
                let acc = shared_worker.clone();
                let rx = rx.lock().unwrap();
                let mut probe = ProbeHandle::new();

                let (mut edges_in, mut roots_in) = worker.dataflow::<u64, _, _>(|scope| {
                    let (edges_in, edges) = scope.new_collection::<(i64, i64), isize>();
                    let (roots_in, roots) = scope.new_collection::<i64, isize>();
                    let edges_c = edges.clone();
                    let roots_c = roots.clone();

                    let reach = roots.iterate(move |scope, inner| {
                        let edges = edges_c.enter(scope);
                        let roots = roots_c.enter(scope);
                        edges
                            .semijoin(inner)
                            .map(|(_parent, child)| child)
                            .concat(roots)
                            .distinct()
                    });

                    reach
                        .consolidate()
                        .inspect(move |(node, _t, diff)| {
                            let mut s = acc.lock().unwrap();
                            let e = s.counts.entry(*node).or_insert(0);
                            let before = *e > 0;
                            *e += *diff;
                            let after = *e > 0;
                            if before != after {
                                s.digest ^= mix(*node);
                                if after {
                                    s.card += 1;
                                } else {
                                    s.card = s.card.saturating_sub(1);
                                }
                            }
                        })
                        .probe_with(&mut probe);

                    (edges_in, roots_in)
                });

                let mut t: u64 = 0;
                loop {
                    match rx.recv() {
                        Ok(Cmd::Batch { edges, roots, ack }) => {
                            for (e, d) in edges {
                                edges_in.update(e, d);
                            }
                            for (r, d) in roots {
                                roots_in.update(r, d);
                            }
                            t += 1;
                            edges_in.advance_to(t);
                            roots_in.advance_to(t);
                            edges_in.flush();
                            roots_in.flush();
                            worker.step_while(|| probe.less_than(edges_in.time()));
                            let _ = ack.send(());
                        }
                        Ok(Cmd::Stop) | Err(_) => break,
                    }
                }
            });
        });

        Self { tx: Some(tx), handle: Some(handle), shared }
    }

    fn feed(&mut self, edges: Vec<((i64, i64), isize)>, roots: Vec<(i64, isize)>) {
        let (ack_tx, ack_rx) = sync_channel(1);
        self.tx.as_ref().unwrap().send(Cmd::Batch { edges, roots, ack: ack_tx }).unwrap();
        ack_rx.recv().unwrap();
    }

    pub fn setup(&mut self, root: i64, edges: &[(i64, i64)]) {
        let e: Vec<((i64, i64), isize)> = edges.iter().map(|&e| (e, 1isize)).collect();
        self.feed(e, vec![(root, 1isize)]);
    }
    pub fn add_edge(&mut self, u: i64, v: i64) {
        self.feed(vec![((u, v), 1)], vec![]);
    }
    pub fn del_edge(&mut self, u: i64, v: i64) {
        self.feed(vec![((u, v), -1)], vec![]);
    }
    /// Batch a whole round of deltas into one dd step (dd's natural mode).
    pub fn batch(&mut self, adds: &[(i64, i64)], dels: &[(i64, i64)]) {
        let mut e: Vec<((i64, i64), isize)> = Vec::with_capacity(adds.len() + dels.len());
        e.extend(adds.iter().map(|&x| (x, 1isize)));
        e.extend(dels.iter().map(|&x| (x, -1isize)));
        self.feed(e, vec![]);
    }
    pub fn reachable(&self) -> (i64, u64) {
        let s = self.shared.lock().unwrap();
        (s.digest, s.card)
    }
}

impl Drop for DdBfs {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Cmd::Stop);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
