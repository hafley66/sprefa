//! Correctness oracles: dd differential, salsa red-green, hand-rolled Rust.
//! - `dd`    : ported long ago (the cascade/reach oracle).
//! - `salsa` : ported 2026-07-23 from the folded labkit (SalsaReconciler, the resident
//!             salsa-crate oracle for the reconcile plane). Its parity test
//!             (tests/reconcile.rs) is GREEN: engine::reconcile driven through `propagate`
//!             (the ascending topo sweep) is byte-identical to salsa on DAGs w/ diamonds.
//! TODO remaining: move the tarjan/walk oracle out of tests/covering.rs to here.

pub mod dd {
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

    pub fn mix(k: i64) -> i64 {
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

}

pub mod salsa {
    //! SalsaReconciler — the resident salsa-crate oracle for the SQLite reconcile plane
    //! (`engine::reconcile`). Same role as `dd::DdBfs` for the cascade plane: an
    //! independent, respected implementation of the same algorithm, diffed against the
    //! SQLite formulation in tests/reconcile.rs.
    //!
    //! The algorithm (defined once; matches engine::reconcile's rx_memo/rx_dep):
    //!   digest(i) = mix(value[i]) XOR XOR_{j in deps[i]} mix(digest(j))
    //! Ascending id is a topological order (deps have smaller ids), so one sweep
    //! converges. Early cutoff: a node recomputes only when a dependency's digest
    //! actually moved (salsa's WillExecute event is the recompute meter).
    //!
    //! Parity proof (tests/reconcile.rs): the SQLite plane and this oracle agree on
    //! (a) the ANSWER digest every edit tick, and (b) the RECOMPUTE COUNT. That
    //! count-equality is what "salsa = reconciliation you can do in SQL" means.

    use std::sync::{Arc, Mutex};

    use crate::oracle::dd::mix;

    /// A rel reads deps within the previous WIN ids.
    pub const WIN: u32 = 8;
    /// Up to DEG deps per rel.
    pub const DEG: usize = 3;

    fn cell_hash(i: u32, salt: i64) -> i64 {
        mix((i as i64) << 12 ^ salt)
    }

    /// One node's digest from its value and its deps' (already-current) digests.
    pub fn node_digest(value: i64, dep_digests: impl Iterator<Item = i64>) -> i64 {
        dep_digests.fold(mix(value), |acc, d| acc ^ mix(d))
    }

    /// Layered dep DAG: node i reads up to DEG distinct j in [i-WIN, i). Real rule
    /// graphs are shallow, sparse, mostly reading recently-defined rels; a DAG keeps
    /// the oracle exact and ascending id a valid topo order.
    pub fn reconcile_graph(n: usize) -> Vec<Vec<u32>> {
        let mut deps = vec![Vec::new(); n];
        for i in 0..n as u32 {
            let mut seen = std::collections::HashSet::new();
            for d in 0..DEG as u32 {
                let span = (mix((i as i64) << 8 ^ d as i64).unsigned_abs() % WIN as u64) as u32 + 1;
                if let Some(j) = i.checked_sub(span) {
                    if seen.insert(j) {
                        deps[i as usize].push(j);
                    }
                }
            }
            deps[i as usize].sort_unstable();
        }
        deps
    }

    /// The deterministic edit stream + the independent from-scratch oracle answer.
    pub struct RStream {
        pub init: Vec<i64>,
        pub edits: Vec<Vec<(u32, i64)>>,
        pub oracle_answer: i64,
    }

    /// Deterministic stream: `ticks` rounds, each editing `per` cells. 1 in 4 edits
    /// re-writes the SAME value (exercises early cutoff / backdating). `oracle_answer`
    /// is the from-scratch ascending digest after ALL edits (independent of both engines).
    pub fn reconcile_stream(n: usize, deps: &[Vec<u32>], seed: u64, ticks: usize, per: usize) -> RStream {
        let mut val: Vec<i64> = (0..n as u32).map(|i| cell_hash(i, 0)).collect();
        let init = val.clone();
        let mut rng = seed ^ (n as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let mut next = || {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            rng >> 16
        };
        let mut edits = Vec::with_capacity(ticks);
        for _ in 0..ticks {
            let mut e = Vec::with_capacity(per);
            for _ in 0..per {
                let i = (next() % n as u64) as usize;
                // 1 in 4 edits re-writes the SAME value: exercises early-cutoff / backdating
                // (the node re-executes but its digest does not move, so the wave stops).
                let same = next() % 4 == 0;
                let nv = if same { val[i] } else { cell_hash(i as u32, next() as i64 | 1) };
                val[i] = nv;
                e.push((i as u32, nv));
            }
            edits.push(e);
        }
        // oracle: from-scratch, ascending (topo). Independent of both engines.
        let mut memo = vec![0i64; n];
        for i in 0..n {
            memo[i] = node_digest(val[i], deps[i].iter().map(|&j| memo[j as usize]));
        }
        let oracle_answer = memo.iter().fold(0i64, |a, &d| a ^ d);
        RStream { init, edits, oracle_answer }
    }

    /// The seam. The parity test runs two impls under it and compares.
    pub trait Reconciler {
        fn name(&self) -> &'static str;
        fn build(&mut self, deps: Vec<Vec<u32>>, init: Vec<i64>);
        fn edit(&mut self, changes: &[(u32, i64)]);
        /// Total node recomputes (bodies executed) since build: the early-cutoff meter.
        fn recomputes(&self) -> u64;
        /// XOR of every node's current digest: the equivalence key.
        fn answer(&mut self) -> i64;
    }

    // ---- the salsa crate, resident, the oracle ----
    #[salsa::db]
    #[derive(Clone)]
    struct Db {
        storage: salsa::Storage<Self>,
        execs: Arc<Mutex<u64>>,
    }
    impl Default for Db {
        fn default() -> Self {
            let execs = Arc::new(Mutex::new(0));
            let e2 = execs.clone();
            Self {
                storage: salsa::Storage::new(Some(Box::new(move |ev| {
                    if matches!(ev.kind, salsa::EventKind::WillExecute { .. }) {
                        *e2.lock().unwrap() += 1;
                    }
                }))),
                execs,
            }
        }
    }
    #[salsa::db]
    impl salsa::Database for Db {}

    #[salsa::input]
    struct Cell {
        value: i64,
    }
    #[salsa::input]
    struct World {
        cells: Arc<Vec<Cell>>,
        deps: Arc<Vec<Vec<u32>>>,
    }
    #[salsa::interned]
    struct Node<'db> {
        idx: u32,
    }

    /// digest(i) = mix(value[i]) XOR over deps mix(digest(j)). Reading `cells[i].value`
    /// records a dependency on cell i ONLY; reading `world.deps` depends on `World`,
    /// which never changes after build, so it never triggers invalidation.
    #[salsa::tracked]
    fn node_val<'db>(db: &'db dyn salsa::Database, world: World, node: Node<'db>) -> i64 {
        let i = *node.idx(db) as usize;
        let cells = world.cells(db);
        let deps = world.deps(db);
        node_digest(
            *cells[i].value(db),
            deps[i].iter().map(|&j| *node_val(db, world, Node::new(db, j))),
        )
    }

    /// The salsa-crate oracle. ORACLE ONLY; the shipping SQLite path never links salsa
    /// except through this module.
    pub struct SalsaReconciler {
        db: Db,
        cells: Vec<Cell>,
        world: Option<World>,
        n: usize,
        exec_baseline: u64, // execs at end of cold build, so recomputes() counts EDITS only
    }
    impl Default for SalsaReconciler {
        fn default() -> Self {
            Self { db: Db::default(), cells: Vec::new(), world: None, n: 0, exec_baseline: 0 }
        }
    }
    impl Reconciler for SalsaReconciler {
        fn name(&self) -> &'static str {
            "salsa (resident oracle)"
        }
        fn build(&mut self, deps: Vec<Vec<u32>>, init: Vec<i64>) {
            self.n = init.len();
            self.cells = init.iter().map(|&v| Cell::new(&self.db, v)).collect();
            self.world = Some(World::new(
                &self.db,
                Arc::new(self.cells.clone()),
                Arc::new(deps),
            ));
            self.answer(); // prime the memo table (cold build)
            self.exec_baseline = *self.db.execs.lock().unwrap(); // exclude the build
        }
        fn edit(&mut self, changes: &[(u32, i64)]) {
            use salsa::Setter;
            for &(i, v) in changes {
                self.cells[i as usize].set_value(&mut self.db).to(v);
            }
            self.answer(); // re-drive: salsa recomputes only the invalidated + un-backdated
        }
        fn recomputes(&self) -> u64 {
            *self.db.execs.lock().unwrap() - self.exec_baseline
        }
        fn answer(&mut self) -> i64 {
            let world = self.world.unwrap();
            (0..self.n as u32)
                .fold(0i64, |a, i| a ^ *node_val(&self.db, world, Node::new(&self.db, i)))
        }
    }
}
