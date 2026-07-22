//! Reconciliation in SQLite — salsa's red-green dirty-check, done as a recursive CTE
//! over a dep table instead of a resident memo graph. Backported from the v6 labkit.
//!
//! This is the "salsa replaced by SQLite" piece: the control plane that decides WHICH
//! rels are stale after an input moves, without a resident dependency graph. The exact
//! salsa idea, in tables:
//!   rx_memo(id, digest, changed_at, verified_at)  -- one row per reactive rel
//!   rx_dep(reader, read)                           -- reader READS read (the deps array)
//!
//! - `mark_changed`  : an input's digest moved at revision `rev` (changed_at = rev).
//! - `dirty`         : the invalidation query — every rel transitively downstream of
//!                     something whose changed_at > verified_at. One recursive CTE.
//! - `verify`        : after the caller recomputes a rel, record its new digest; if the
//!                     digest actually MOVED, changed_at = rev (its readers stay dirty);
//!                     if not, changed_at is left (EARLY CUTOFF — the wave stops here).
//!                     verified_at = rev either way.
//!
//! One transaction per batch; every step is set-based (no per-row round trip beyond the
//! caller's own recompute).

use sea_orm::{ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait};

use crate::stmt_counter;

async fn exec(db: &impl ConnectionTrait, sql: &str) -> Result<(), DbErr> {
    stmt_counter::incr();
    db.execute_unprepared(sql).await?;
    Ok(())
}

async fn query_ids(db: &impl ConnectionTrait, sql: &str) -> Result<Vec<i64>, DbErr> {
    stmt_counter::incr();
    let rows = db
        .query_all_raw(Statement::from_string(DatabaseBackend::Sqlite, sql.to_owned()))
        .await?;
    Ok(rows.iter().map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0)).collect())
}

pub async fn create_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    db.execute_unprepared(
        "CREATE TABLE rx_memo (
            id          INTEGER PRIMARY KEY,
            digest      INTEGER NOT NULL,
            changed_at  INTEGER NOT NULL DEFAULT 0,
            verified_at INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE rx_dep (
            reader INTEGER NOT NULL,
            read   INTEGER NOT NULL,
            PRIMARY KEY (reader, read)
         ) WITHOUT ROWID;
         CREATE INDEX ix_rx_read ON rx_dep (read);",
    )
    .await?;
    Ok(())
}

/// Seed a rel's memo (its output digest and the deps it read), at revision `rev`.
pub async fn seed(
    db: &DatabaseConnection,
    id: i64,
    digest: i64,
    deps: &[i64],
    rev: i64,
) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    exec(
        &txn,
        &format!("INSERT INTO rx_memo(id,digest,changed_at,verified_at) VALUES ({id},{digest},{rev},{rev})"),
    )
    .await?;
    for &d in deps {
        exec(&txn, &format!("INSERT OR IGNORE INTO rx_dep(reader,read) VALUES ({id},{d})")).await?;
    }
    txn.commit().await?;
    Ok(())
}

/// An input's digest moved at `rev`: bump its changed_at so the CTE sees it stale.
pub async fn mark_changed(db: &DatabaseConnection, ids: &[i64], rev: i64) -> Result<(), DbErr> {
    let in_list = ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",");
    exec(db, &format!("UPDATE rx_memo SET changed_at={rev} WHERE id IN ({in_list})")).await
}

/// The invalidation query, in SQL: the current stale FRONTIER — every derived rel that
/// READS something whose digest changed after that rel was last verified. This is exactly
/// salsa's rule (a rel is stale iff a dependency moved past its verified_at). It is a
/// FRONTIER, not the full closure: a rel one hop further only becomes stale AFTER its dep
/// recomputes and actually moves (see `verify`) — that lazy step is what gives early
/// cutoff. The caller loops: `while let frontier = dirty(); recompute+verify each`.
pub async fn dirty(db: &DatabaseConnection) -> Result<Vec<i64>, DbErr> {
    query_ids(
        db,
        "SELECT DISTINCT dep.reader
         FROM rx_dep dep
         JOIN rx_memo d ON d.id = dep.read
         JOIN rx_memo s ON s.id = dep.reader
         WHERE d.changed_at > s.verified_at
         ORDER BY dep.reader",
    )
    .await
}

/// Record a recomputed rel's new digest at `rev`. Returns whether the digest MOVED.
/// If it moved, changed_at = rev (readers stay dirty). If not, changed_at is untouched
/// (EARLY CUTOFF: downstream never re-runs). verified_at = rev either way.
pub async fn verify(
    db: &DatabaseConnection,
    id: i64,
    new_digest: i64,
    rev: i64,
) -> Result<bool, DbErr> {
    stmt_counter::incr();
    let rows = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!("SELECT digest FROM rx_memo WHERE id={id}"),
        ))
        .await?;
    let old = rows.first().map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0)).unwrap_or(0);
    let moved = old != new_digest;
    let set_changed = if moved { format!(", changed_at={rev}") } else { String::new() };
    exec(
        db,
        &format!("UPDATE rx_memo SET digest={new_digest}, verified_at={rev}{set_changed} WHERE id={id}"),
    )
    .await?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    async fn open() -> DatabaseConnection {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("rx_test_{}_{uniq}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        opt.max_connections(1).min_connections(1);
        let db = Database::connect(opt).await.unwrap();
        db.execute_unprepared(crate::unfuck_sqlite::OPEN_PRAGMAS).await.unwrap();
        create_schema(&db).await.unwrap();
        db
    }

    // drive the reconcile loop: while there's a stale frontier, recompute each rel
    // (digest from `recompute`) and verify it. Returns the ids recomputed, in order —
    // the count/order is the early-cutoff meter.
    async fn reconcile_loop(db: &DatabaseConnection, recompute: impl Fn(i64) -> i64) -> Vec<i64> {
        let mut order = Vec::new();
        loop {
            let front = dirty(db).await.unwrap();
            if front.is_empty() {
                break;
            }
            for id in front {
                let dg = recompute(id);
                verify(db, id, dg, 1).await.unwrap();
                order.push(id);
            }
        }
        order
    }

    // chain 1 <- 2 <- 3. Change input 1. The stale FRONTIER is just {2} (one hop).
    #[tokio::test]
    async fn dirty_is_the_frontier() {
        let db = open().await;
        seed(&db, 1, 100, &[], 0).await.unwrap();
        seed(&db, 2, 200, &[1], 0).await.unwrap();
        seed(&db, 3, 300, &[2], 0).await.unwrap();
        assert_eq!(dirty(&db).await.unwrap(), Vec::<i64>::new());
        mark_changed(&db, &[1], 1).await.unwrap();
        assert_eq!(dirty(&db).await.unwrap(), vec![2]);
    }

    // early cutoff: input 1 changes, but recompute of 2 lands the SAME digest -> the wave
    // stops; 3 is NEVER recomputed. Only {2} runs.
    #[tokio::test]
    async fn early_cutoff_stops_the_wave() {
        let db = open().await;
        seed(&db, 1, 100, &[], 0).await.unwrap();
        seed(&db, 2, 200, &[1], 0).await.unwrap();
        seed(&db, 3, 300, &[2], 0).await.unwrap();
        mark_changed(&db, &[1], 1).await.unwrap();
        let ran = reconcile_loop(&db, |id| match id {
            2 => 200, // unchanged -> early cutoff
            other => other * 1000,
        })
        .await;
        assert_eq!(ran, vec![2], "2 recomputes, 3 never runs (early cutoff)");
    }

    // real change all the way down: 2 moves, so 3 becomes stale and also runs. {2,3}.
    #[tokio::test]
    async fn real_change_propagates() {
        let db = open().await;
        seed(&db, 1, 100, &[], 0).await.unwrap();
        seed(&db, 2, 200, &[1], 0).await.unwrap();
        seed(&db, 3, 300, &[2], 0).await.unwrap();
        mark_changed(&db, &[1], 1).await.unwrap();
        let ran = reconcile_loop(&db, |id| id * 7).await; // every recompute moves
        assert_eq!(ran, vec![2, 3], "2 moves -> 3 runs");
    }
}
