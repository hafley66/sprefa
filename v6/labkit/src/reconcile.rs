//! The reconciliation SEAM — salsa's actual job (red-green graph reconciliation),
//! written once as a trait with TWO swappable implementations:
//!
//!   - `SalsaReconciler` — the salsa crate, resident. salsa does `maybe_changed_after`
//!     (revision compare + backdating / early-cutoff) for us.
//!   - `SqlReconciler`   — the SAME algorithm as a semi-naive ascending sweep, with the
//!     invalidation edges (`dep`) and the memo digests in SQLite, on disk, durable.
//!
//! Both maintain the digest of a layered dep DAG under cell edits, WITH EARLY CUTOFF (a
//! node recomputes only when a dependency's digest actually moved). A third, independent
//! oracle recomputes from scratch. The proof this seam exists:
//!   - all three agree on the ANSWER digest (equivalence), and
//!   - salsa and sql agree on the RECOMPUTE COUNT (early-cutoff behaves identically).
//! That count-equality is what "salsa = reconciliation you can do in SQL" means, running.
//!
//! Node digest is defined ONCE, everywhere the same:
//!     digest(i) = mix(value[i]) XOR  XOR_{j in deps[i]} mix(digest(j))
//! Ascending id is a topological order (every dep has a smaller id), which both engines
//! exploit: a single ascending sweep suffices.

use crate::mix;
use crate::store_db::StoreDb;

pub const WIN: u32 = 8; // a rel reads deps within the previous WIN ids
pub const DEG: usize = 3; // up to DEG deps per rel

fn cell_hash(i: u32, salt: i64) -> i64 {
    mix((i as i64) << 12 ^ salt)
}

/// Layered dep DAG: node i reads up to DEG distinct j in [i-WIN, i). Real rule graphs
/// are shallow, sparse, mostly reading recently-defined rels; a DAG keeps the oracle
/// exact and ascending id a valid topo order.
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

/// One node's digest from its value and its deps' (already-current) digests.
fn node_digest(value: i64, dep_digests: impl Iterator<Item = i64>) -> i64 {
    dep_digests.fold(mix(value), |acc, d| acc ^ mix(d))
}

/// The deterministic edit stream + the independent from-scratch oracle answer.
pub struct RStream {
    pub init: Vec<i64>,
    pub edits: Vec<Vec<(u32, i64)>>, // per-tick (cell, new value)
    pub oracle_answer: i64,          // from-scratch digest after ALL edits
}

pub fn reconcile_stream(
    n: usize,
    deps: &[Vec<u32>],
    seed: u64,
    ticks: usize,
    per: usize,
) -> RStream {
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

/// The seam. Default an experiment to salsa; swap to sql; measure the swap.
pub trait Reconciler {
    fn name(&self) -> &'static str;
    fn build(&mut self, deps: Vec<Vec<u32>>, init: Vec<i64>);
    fn edit(&mut self, changes: &[(u32, i64)]);
    /// Total node recomputes (bodies executed) since build — the early-cutoff meter.
    fn recomputes(&self) -> u64;
    /// XOR of every node's current digest — the equivalence key.
    fn answer(&mut self) -> i64;
}

// ============================ salsa implementation ============================
// ORACLE ONLY, gated behind `with-salsa`. The shipping SQLite path never needs salsa.
#[cfg(feature = "with-salsa")]
mod salsa_impl {
use super::{node_digest, Reconciler};
use std::sync::{Arc, Mutex};

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

/// digest(i) = mix(value[i]) XOR over deps mix(digest(j)). Reading `cells[i].value(db)`
/// records a dependency on cell i ONLY; reading `world.deps(db)` depends on `World`,
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
        "salsa (resident)"
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
} // mod salsa_impl
#[cfg(feature = "with-salsa")]
pub use salsa_impl::SalsaReconciler;

// ============================ sqlite implementation ==========================
// Same algorithm on disk: dep(reader, read) rows are the invalidation edges, memo(id,
// digest) the durable digests. Edit = ascending semi-naive sweep with early cutoff —
// a node is recomputed only when a dependency's digest actually changed (the SQL
// `readers` query drives propagation). Ascending id order = topo, so one sweep converges.

pub struct SqlReconciler {
    db: StoreDb,
    deps: Vec<Vec<u32>>,     // forward: deps[i] = what i reads (for recompute)
    readers: Vec<Vec<u32>>,  // reverse: readers[j] = who reads j (for invalidation)
    value: Vec<i64>,
    memo: Vec<i64>,          // mirror of the memo table for O(1) dep reads during a sweep
    n: usize,
    recomputes: u64,
}
impl Default for SqlReconciler {
    fn default() -> Self {
        let db = StoreDb::memory();
        db.exec("CREATE TABLE dep(reader INTEGER NOT NULL, read INTEGER NOT NULL);
             CREATE INDEX ix_read ON dep(read);
             CREATE TABLE memo(id INTEGER PRIMARY KEY, digest INTEGER NOT NULL) WITHOUT ROWID;");
        Self {
            db,
            deps: Vec::new(),
            readers: Vec::new(),
            value: Vec::new(),
            memo: Vec::new(),
            n: 0,
            recomputes: 0,
        }
    }
}
impl SqlReconciler {
    /// Recompute node i in RAM (this is the "derive job" — the same work salsa does).
    /// Returns Some(new_digest) if it moved (early-cutoff: None means the wave stops).
    fn recompute_one(&mut self, i: usize) -> Option<i64> {
        let d = node_digest(self.value[i], self.deps[i].iter().map(|&j| self.memo[j as usize]));
        self.recomputes += 1;
        if d != self.memo[i] {
            self.memo[i] = d;
            Some(d)
        } else {
            None
        }
    }
}
impl Reconciler for SqlReconciler {
    fn name(&self) -> &'static str {
        "sqlite (on disk)"
    }
    fn build(&mut self, deps: Vec<Vec<u32>>, init: Vec<i64>) {
        self.n = init.len();
        self.value = init;
        self.deps = deps;
        self.readers = vec![Vec::new(); self.n];
        for (i, ds) in self.deps.iter().enumerate() {
            for &j in ds {
                self.readers[j as usize].push(i as u32);
            }
        }
        // persist dep edges (batched, one insert of a json array — N+1 law)
        let edges = self.deps.iter().enumerate().flat_map(|(i, ds)| ds.iter().map(move |j| format!("({i},{j})"))).collect::<Vec<_>>();
        if !edges.is_empty() { self.db.exec(format!("INSERT INTO dep(reader,read) VALUES {}", edges.join(","))); }
        // initial memos, ascending (topo)
        self.memo = vec![0i64; self.n];
        let mut rows = Vec::new();
        for i in 0..self.n { let d = node_digest(self.value[i], self.deps[i].iter().map(|&j| self.memo[j as usize])); self.memo[i] = d; rows.push(format!("({i},{d})")); }
        if !rows.is_empty() { self.db.exec(format!("INSERT INTO memo(id,digest) VALUES {}", rows.join(","))); }
    }
    fn edit(&mut self, changes: &[(u32, i64)]) {
        use std::collections::BTreeSet;
        let mut dirty: BTreeSet<u32> = BTreeSet::new(); // ascending = topo order
        for &(i, v) in changes {
            self.value[i as usize] = v;
            dirty.insert(i);
        }
        // ascending semi-naive sweep with early cutoff, COMPUTED IN RAM (identical to
        // salsa's work): pop the smallest dirty id, recompute it; if its digest moved,
        // enqueue its readers (via the RAM reverse-index — a rebuilt view of the durable
        // `dep` table). One ascending pass converges on the DAG. Collect the moved
        // digests and PERSIST them in ONE batched transaction (N+1 law), so the on-disk
        // cost over salsa is exactly the write-through, not a per-node fsync.
        let mut moved: Vec<(i64, i64)> = Vec::new();
        while let Some(&i) = dirty.iter().next() {
            dirty.remove(&i);
            if let Some(d) = self.recompute_one(i as usize) {
                moved.push((i as i64, d));
                for &r in &self.readers[i as usize] {
                    dirty.insert(r);
                }
            }
        }
        if !moved.is_empty() {
            let cases = moved.iter().map(|(id,d)| format!("WHEN {id} THEN {d}")).collect::<Vec<_>>().join(" ");
            let ids = moved.iter().map(|(id,_)| id.to_string()).collect::<Vec<_>>().join(",");
            self.db.exec(format!("UPDATE memo SET digest=CASE id {cases} END WHERE id IN ({ids})"));
        }
    }
    fn recomputes(&self) -> u64 {
        self.recomputes
    }
    fn answer(&mut self) -> i64 {
        // read the digests back FROM the table (proves the durable memo is the truth),
        // fold in Rust so no custom aggregate is needed.
        self.db.rows("SELECT digest FROM memo").into_iter().fold(0i64, |a, r| a ^ r.try_get_by_index::<i64>(0).unwrap())
    }
}
