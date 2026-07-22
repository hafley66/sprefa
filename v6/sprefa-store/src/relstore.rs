//! RelStore — the generic incremental relation store. Lifts the cascade + reconcile
//! off the bespoke `cx_`/`rx_` scaffolding into one handle keyed by DENSE `(rel, row)`
//! integer ids (E1: `key = rel*KEY_STRIDE + row`, a rowid-clustered table). One store
//! holds ANY number of relations; `rel` is the relation discriminator, `row` the tuple.
//!
//! Two planes, both generic:
//!   FACT (Z-set):  add_rows / add_deps / assert / retract / retract_dred / alive
//!   CONTROL (salsa-in-sql): seed_memo / mark_changed / dirty / verify
//!
//! The `cx_*` / `rx_*` tables are just the default on-disk impl; callers speak only in
//! `(rel, row)` pairs. This is what the harness measures now, instead of a bespoke copy.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

use crate::{cascade, reconcile};

pub use cascade::{key, KEY_STRIDE};

pub struct RelStore {
    db: DatabaseConnection,
}

impl RelStore {
    /// Open (or create) a store at `db` with the store's tuning + both schemas.
    pub async fn attach(db: DatabaseConnection) -> Result<Self, DbErr> {
        db.execute_unprepared(crate::unfuck_sqlite::OPEN_PRAGMAS).await?;
        cascade::create_schema(&db).await?;
        reconcile::create_schema(&db).await?;
        Ok(Self { db })
    }

    pub fn conn(&self) -> &DatabaseConnection {
        &self.db
    }

    // ---- FACT plane (generic Z-set over (rel,row)) ----------------------------

    /// Insert `(rel, row, weight)` tuples.
    pub async fn add_rows(&self, rows: &[(i64, i64, i64)]) -> Result<(), DbErr> {
        cascade::insert_rows(&self.db, rows).await
    }
    /// Insert dependency edges `(parent_rel, parent_row, child_rel, child_row)`.
    pub async fn add_deps(&self, edges: &[(i64, i64, i64, i64)]) -> Result<(), DbErr> {
        cascade::insert_deps(&self.db, edges).await
    }
    /// Forward add: propagate aliveness from `seeds`. Returns rounds.
    pub async fn assert(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::assert(&self.db, seeds).await
    }
    /// Counting retraction (fast, correct on ACYCLIC support graphs). Returns rounds.
    pub async fn retract(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract(&self.db, seeds).await
    }
    /// Cycle-safe retraction (Delete-and-Rederive), Rust-driven round loop. Returns rounds.
    pub async fn retract_dred(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract_dred(&self.db, seeds).await
    }
    /// Cycle-safe retraction as two recursive CTEs (whole traversal in SQLite's C
    /// engine, no per-round round-trip). Same result as `retract_dred`; use at scale.
    pub async fn retract_dred_cte(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract_dred_cte(&self.db, seeds).await
    }
    /// Count live rows (weight > 0) across all relations.
    pub async fn alive(&self) -> Result<i64, DbErr> {
        use sea_orm::{DatabaseBackend, Statement};
        Ok(self
            .db
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT count(*) FROM cx_row WHERE weight>0".to_owned(),
            ))
            .await?
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0))
            .unwrap_or(0))
    }

    /// The live-row survivor SET as sorted encoded keys (`key = rel*KEY_STRIDE + row`).
    /// This is the answer bytes the head-to-head diffs against the oracle and dd. `key`
    /// IS the rowid and is stored ordered, so `ORDER BY key` is a no-sort ordered scan.
    pub async fn alive_keys(&self) -> Result<Vec<i64>, DbErr> {
        use sea_orm::{DatabaseBackend, Statement};
        Ok(self
            .db
            .query_all_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT key FROM cx_row WHERE weight>0 ORDER BY key".to_owned(),
            ))
            .await?
            .iter()
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0))
            .collect())
    }

    // ---- CONTROL plane (salsa-in-sql over (rel,row) memos) --------------------

    pub async fn seed_memo(&self, rel: i64, row: i64, digest: i64, deps: &[(i64, i64)], rev: i64) -> Result<(), DbErr> {
        let dep_keys: Vec<i64> = deps.iter().map(|&(r, w)| key(r, w)).collect();
        reconcile::seed(&self.db, key(rel, row), digest, &dep_keys, rev).await
    }
    pub async fn mark_changed(&self, cells: &[(i64, i64)], rev: i64) -> Result<(), DbErr> {
        let ks: Vec<i64> = cells.iter().map(|&(r, w)| key(r, w)).collect();
        reconcile::mark_changed(&self.db, &ks, rev).await
    }
    /// The stale frontier as `(rel, row)` pairs.
    pub async fn dirty(&self) -> Result<Vec<(i64, i64)>, DbErr> {
        Ok(reconcile::dirty(&self.db)
            .await?
            .into_iter()
            .map(|k| (k / KEY_STRIDE, k % KEY_STRIDE))
            .collect())
    }
    /// Record a recomputed rel's digest; returns whether it moved (early cutoff).
    pub async fn verify(&self, rel: i64, row: i64, digest: i64, rev: i64) -> Result<bool, DbErr> {
        reconcile::verify(&self.db, key(rel, row), digest, rev).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    async fn open() -> RelStore {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("relstore_test_{}_{uniq}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        opt.max_connections(1).min_connections(1);
        RelStore::attach(Database::connect(opt).await.unwrap()).await.unwrap()
    }

    // TWO relations in one store: rel 0 (roots/"files"), rel 1 ("derived"). A cross-rel
    // dep 0:0 -> 1:0, and a cycle inside rel 1 (1:0 ->1:1 ->1:2 ->1:0). Cut the root.
    // Cycle-safe retraction must kill the whole rel-1 cycle. Proves it's generic + cyclic.
    #[tokio::test]
    async fn generic_two_relation_cycle() {
        let s = open().await;
        s.add_rows(&[(0, 0, 1), (1, 0, 1), (1, 1, 1), (1, 2, 1)]).await.unwrap();
        s.add_deps(&[
            (0, 0, 1, 0), // file 0:0 supports derived 1:0
            (1, 0, 1, 1), // cycle in rel 1
            (1, 1, 1, 2),
            (1, 2, 1, 0),
        ])
        .await
        .unwrap();
        assert_eq!(s.alive().await.unwrap(), 4);

        s.retract_dred(&[(0, 0)]).await.unwrap();
        assert_eq!(s.alive().await.unwrap(), 0, "cutting the cross-rel anchor kills the rel-1 cycle");
    }
}
