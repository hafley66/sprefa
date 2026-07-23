//! The parity surface — Definition of DONE as a literal trait.
//!
//! ## The one engine (what ships)
//! SQLite is the engine: its C VM/bytecode for everything it can express, Rust
//! with arena-bounded allocation for the parts it can't. State lives on disk;
//! RSS is the page cache (a knob: tune `cache_size`, then stream/chunk the
//! frontier when a live node count would breach the budget). Rust heap stays
//! near zero. This is the controlled-RAM answer to v5's resident 36 GB swap.
//!
//! ## The oracles (what we match, not ship)
//! salsa, differential-dataflow, petgraph, GraphBLAS, LadybugDB — graph-
//! algorithm engines at heart. They are INSPIRATIONS (the function surface and
//! the invariants we re-express in SQL) and ORACLES (resident ground truth we
//! run on small inputs and diff byte-for-byte). We never ship their resident
//! runtimes. Parity with their functions is the DONE bar.
//!
//! ## Everything is an event
//! Every lifecycle transition is a `tracing` event — including a frontier's
//! run / pause / resume. A true frontier is batched and resumable: run in
//! memory, persist the frontier state, pause, resume. That lands on top of the
//! event stream later; it is not blocking the parity contract below.
//!
//! ## DONE = a green parity test through this trait
//! Each family is one trait with two impls: the SQLite engine (production) and
//! a resident oracle (ground truth). A parity test runs shared shapes through
//! both and asserts byte-identical results. That green test is the DONE cell —
//! no hand-edited status can lie past it.
//!
//! This module starts with the reach family. cascade (weight / Z-set / dd role)
//! and reconcile (digest / salsa role) follow the same shape.

#![allow(async_fn_in_trait)]
use sea_orm::{DatabaseConnection, DbErr};

use crate::relstore::GraphNs;

/// Reachability and structure over a directed graph. Node keys are dense `i64`.
///
/// Two impls: [`SqliteReach`] (the on-disk engine over `cx_dep`) and a resident
/// oracle (a vendored pure-Rust condensation, small inputs). A parity test runs
/// the same query through either and compares — that is the DONE check.
///
/// `scc_labels` returns the partition canonicalized to MIN-member reps so it is
/// directly comparable across impls (component id order is impl-defined).
pub trait Reach {
    /// Forward transitive closure from `start` (strict; includes `start` iff its
    /// SCC is cyclic — a path returns to it).
    async fn reaches_from(&self, start: i64) -> Result<Vec<i64>, DbErr>;
    /// Reverse transitive closure into `target` (the mirror, over reversed edges).
    async fn reached_by(&self, target: i64) -> Result<Vec<i64>, DbErr>;
    /// SCC partition as `(node_key, comp_repr)` where `comp_repr = MIN member key`.
    async fn scc_labels(&self) -> Result<Vec<(i64, i64)>, DbErr>;
    /// `|{ (u, v) : u reaches v }|` where reach includes `u == v` iff `u` is in a
    /// cyclic SCC. `i128` because the count exceeds `i64` on wide graphs.
    async fn count_pairs(&self) -> Result<i128, DbErr>;
}

/// The SQLite engine: every method is the on-disk `cx_dep` formulation in
/// [`crate::reach`]. Borrows the connection and the store's [`GraphNs`]; state is
/// on disk, not in `self`.
pub struct SqliteReach<'a> {
    db: &'a DatabaseConnection,
    ns: &'a GraphNs,
}

impl<'a> SqliteReach<'a> {
    pub fn new(db: &'a DatabaseConnection, ns: &'a GraphNs) -> Self {
        Self { db, ns }
    }
}

impl Reach for SqliteReach<'_> {
    async fn reaches_from(&self, start: i64) -> Result<Vec<i64>, DbErr> {
        crate::reach::reaches_from(self.db, &self.ns, start).await
    }
    async fn reached_by(&self, target: i64) -> Result<Vec<i64>, DbErr> {
        crate::reach::reached_by(self.db, &self.ns, target).await
    }
    async fn scc_labels(&self) -> Result<Vec<(i64, i64)>, DbErr> {
        crate::reach::scc_labels(self.db, &self.ns).await
    }
    async fn count_pairs(&self) -> Result<i128, DbErr> {
        crate::reach::count_pairs(self.db, &self.ns).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relstore::RelStore;
    use sea_orm::{ConnectOptions, Database};

    async fn open() -> RelStore {
        let path = std::env::temp_dir().join(format!(
            "algo_smoke_{}_{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        opt.max_connections(1).min_connections(1);
        let db = Database::connect(opt).await.unwrap();
        RelStore::attach(db).await.unwrap()
    }

    // 3-cycle 0→1→2→0: ONE cyclic SCC of size 3 ⇒ every node reaches every node,
    // count_pairs = 3² = 9, scc partition = {(0,0),(1,0),(2,0)} (repr = min = 0).
    // Proves the trait is wired to the real engine, not just signatures.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sqlite_reach_smoke() {
        let store = open().await;
        store.add_rows(&[(0, 0, 1), (0, 1, 1), (0, 2, 1)]).await.unwrap();
        store
            .add_deps(&[(0, 0, 0, 1), (0, 1, 0, 2), (0, 2, 0, 0)])
            .await
            .unwrap();
        let r = SqliteReach::new(store.conn(), store.ns());
        let mut labels = r.scc_labels().await.unwrap();
        labels.sort_unstable();
        assert_eq!(labels, vec![(0, 0), (1, 0), (2, 0)], "one SCC, repr = 0");
        assert_eq!(r.count_pairs().await.unwrap(), 9, "3-cycle ⇒ 9 reachable pairs");
        let mut fwd = r.reaches_from(0).await.unwrap();
        fwd.sort_unstable();
        assert_eq!(fwd, vec![0, 1, 2], "0 reaches all");
    }
}
