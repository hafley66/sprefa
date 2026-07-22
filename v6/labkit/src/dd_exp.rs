//! DdReach — the differential-dataflow actor. The RESIDENT incremental engine: it
//! maintains the all-pairs transitive closure with O(Δ) work per edit (dd's arrangements
//! do the delta), the yardstick the on-disk cascade must match on correctness and beat
//! on the memory wall. It is `with-dd`-gated because it is heavy, and it is resident:
//! at large scale it walks INTO the 5 GB gun and aborts, which is exactly the point —
//! the on-disk engines complete past where dd dies.
//!
//! Wiring: a long-lived timely worker on its own thread (edges fed tick-by-tick over a
//! channel), because the harness applies edits incrementally — unlike the store's
//! one-shot `dd_reach.rs`, which does a single retract inside one `execute_directly`.
//! Closure is ALL-PAIRS (labkit's oracle is all-pairs), so `iterate` uses `join`, not
//! the store example's single-source `semijoin`.

use crate::reach::{dec, initial_edges, MUL};
use crate::{mix, Complexity, Experiment};
use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};

use differential_dataflow::input::Input;
use differential_dataflow::operators::Iterate; // join/distinct/concat/consolidate are inherent on Collection
use timely::dataflow::operators::probe::Handle as ProbeHandle;

/// Order-independent XOR digest + cardinality of the live reachable-pair set, maintained
/// from the output diff stream on the worker thread. `records` = dd's unit of work.
#[derive(Default)]
struct Shared {
    counts: HashMap<(i64, i64), isize>,
    digest: i64,
    card: u64,
    records: u64,
}

enum Cmd {
    /// (edge, diff): +1 add, -1 remove. Batched per tick.
    Batch(Vec<((i64, i64), isize)>, SyncSender<()>),
    Stop,
}

pub struct DdReach {
    tx: Option<SyncSender<Cmd>>,
    handle: Option<std::thread::JoinHandle<()>>,
    shared: Arc<Mutex<Shared>>,
}

impl Default for DdReach {
    fn default() -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let shared_worker = shared.clone();
        let (tx, rx): (SyncSender<Cmd>, Receiver<Cmd>) = sync_channel(64);
        // execute_directly's closure must be Send+Sync; Receiver is Send but !Sync, so
        // wrap it (the archived v4 _attic/dd.rs pattern).
        let rx = Arc::new(Mutex::new(rx));

        let handle = std::thread::spawn(move || {
            timely::execute_directly(move |worker| {
                let acc = shared_worker.clone();
                let rx = rx.lock().unwrap();
                let mut probe = ProbeHandle::new();

                let mut edges_in = worker.dataflow::<u64, _, _>(|scope| {
                    let (edges_in, edges) = scope.new_collection::<(i64, i64), isize>();
                    let edges_c = edges.clone();

                    // all-pairs closure:
                    //   reach(a,c) :- edge(a,c)
                    //   reach(a,c) :- reach(a,b), edge(b,c)
                    let reach = edges.iterate(move |scope, inner| {
                        let edges = edges_c.enter(scope);
                        inner
                            .map(|(a, b)| (b, a)) // key on b
                            .join(edges.clone()) // edges keyed by from=b -> (b,(a,c))
                            .map(|(_b, (a, c))| (a, c))
                            .concat(edges)
                            .distinct()
                    });

                    reach
                        .consolidate()
                        .inspect(move |((a, c), _t, diff)| {
                            let mut s = acc.lock().unwrap();
                            s.records += 1;
                            let e = s.counts.entry((*a, *c)).or_insert(0);
                            let before = *e > 0;
                            *e += *diff;
                            let after = *e > 0;
                            if before != after {
                                s.digest ^= mix(a * MUL + c);
                                if after {
                                    s.card += 1;
                                } else {
                                    s.card = s.card.saturating_sub(1);
                                }
                            }
                        })
                        .probe_with(&mut probe);

                    edges_in
                });

                let mut t: u64 = 0;
                loop {
                    match rx.recv() {
                        Ok(Cmd::Batch(deltas, ack)) => {
                            for ((u, v), d) in deltas {
                                edges_in.update((u, v), d);
                            }
                            t += 1;
                            edges_in.advance_to(t);
                            edges_in.flush();
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
}

impl DdReach {
    fn feed(&mut self, deltas: Vec<((i64, i64), isize)>) {
        let (ack_tx, ack_rx) = sync_channel(1);
        self.tx.as_ref().unwrap().send(Cmd::Batch(deltas, ack_tx)).unwrap();
        ack_rx.recv().unwrap(); // block until the worker has stepped this batch to fixpoint
    }
}

impl Drop for DdReach {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Cmd::Stop);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Experiment for DdReach {
    fn name(&self) -> &'static str {
        "dd-reach"
    }
    fn complexity(&self) -> Complexity {
        Complexity { time: "O(Δ)/tick incremental", space: "O(closure) RESIDENT" }
    }
    fn rationale(&self) -> &'static str {
        "differential-dataflow: the resident incremental all-pairs closure. dd's arrangements maintain the delta in O(Δ). The yardstick for O(Δ) correctness, and the engine that walks into the gun at scale (resident) where the on-disk cascade survives."
    }
    fn reset(&mut self) {
        *self = DdReach::default();
    }
    fn setup(&mut self, base: usize) {
        let deltas: Vec<((i64, i64), isize)> =
            initial_edges(base).into_iter().map(|k| (dec(k), 1isize)).collect();
        self.feed(deltas);
    }
    fn tick(&mut self, adds: &[i64], removes: &[i64]) {
        let mut deltas: Vec<((i64, i64), isize)> = Vec::with_capacity(adds.len() + removes.len());
        deltas.extend(adds.iter().map(|&k| (dec(k), 1isize)));
        deltas.extend(removes.iter().map(|&k| (dec(k), -1isize)));
        self.feed(deltas);
    }
    fn digest(&self) -> i64 {
        self.shared.lock().unwrap().digest
    }
    fn live(&self) -> u64 {
        self.shared.lock().unwrap().card
    }
    fn recompute_units(&self) -> u64 {
        // dd has no SQL; its work unit is diff records emitted (like the store's dd_reach).
        self.shared.lock().unwrap().records
    }
}
