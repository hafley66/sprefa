//! ISOLATED experiment: manual, scalable cascade retraction over a generic
//! `(tag, id)` reference graph, with Z-set weights.
//!
//! Not wired into the spine. This proves the mechanism the book calls for:
//! borrow the DBSP algebra (weights, retraction = subtraction) but keep the
//! state on disk in SQLite, and cascade by hand so we owe nothing to a resident
//! engine.
//!
//! The polymorphic FK is metadata: a row is addressed by `(tag, id)` — `tag`
//! is "which logical table", `id` is the row within it — so ONE dep edge table
//! expresses every cross-table reference without per-relationship cascade
//! wiring:
//!
//!   cx_row(tag, id, weight)              -- every row; weight = # of supports
//!   cx_dep(parent_tag,parent_id,         -- child depends on parent; deleting
//!          child_tag, child_id)             the parent decrements the child
//!
//! Retraction is a Z-set subtraction: a row's `weight` is how many derivations
//! support it. Removing a support decrements it; the row dies only when weight
//! reaches 0 (its LAST support is gone) — so a row derived two ways survives the
//! loss of one. That is why this is not naive reachability: a child dies when
//! its last parent dies, never its first.
//!
//! Scalability: the cascade runs as a breadth-first fixpoint where each ROUND is
//! a fixed handful of set-based SQL statements over the whole current frontier.
//! The number of rounds is the DAG depth, not the row count — a 100k-row graph
//! of depth 3 retracts in 3 rounds.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, TransactionTrait};

use crate::stmt_counter;

/// Rows per multi-row INSERT. Values are inlined integer literals (injection-
/// safe — they are numbers), so the bound-parameter ceiling does not apply; the
/// only limit is SQL length, so we use a big chunk to cut statement count.
const CHUNK: usize = 4000;

/// E1 single-key encoding: `(tag, id)` -> one dense i64 so `cx_row` can be a
/// rowid table clustered on an INTEGER PRIMARY KEY (the rowid itself, zero PK
/// storage, fastest possible SQLite lookup) and `cx_dep` a 2-column key instead
/// of a 4-column composite. `tag`/`id` stay as plain output columns on cx_row.
/// Stride must exceed any local id (local ids are per-relation, < a few million).
const KEY_STRIDE: i64 = 1_000_000_000;

#[inline]
fn key(tag: i64, id: i64) -> i64 {
    tag * KEY_STRIDE + id
}

// Helpers take `&impl ConnectionTrait` so they run on either the pooled
// connection OR a single-connection transaction. The transaction path is the
// point: it pins every statement to ONE connection (correctness under pooling)
// and batches all the WAL writes into a single commit (the big speed win).
/// Per-statement wall-time trace, opt-in via `DL_CASCADE_TRACE=1`. Off by default
/// (one env read per statement, 29 total — negligible). Prints ms + a SQL prefix
/// to stderr so an experiment can see which statement in the round dominates.
fn traced() -> bool {
    std::env::var("DL_CASCADE_TRACE").map(|v| v != "0" && !v.is_empty()).unwrap_or(false)
}

async fn exec(db: &impl ConnectionTrait, sql: &str) -> Result<(), DbErr> {
    stmt_counter::incr();
    if traced() {
        let t = std::time::Instant::now();
        db.execute_unprepared(sql).await?;
        let head: String = sql.chars().take(50).collect::<String>().replace('\n', " ");
        eprintln!("[cascade] {:>8.2} ms  {}", t.elapsed().as_secs_f64() * 1e3, head);
        return Ok(());
    }
    db.execute_unprepared(sql).await?;
    Ok(())
}

async fn scalar(db: &impl ConnectionTrait, sql: &str) -> Result<i64, DbErr> {
    stmt_counter::incr();
    Ok(db
        .query_one_raw(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Sqlite,
            sql.to_owned(),
        ))
        .await?
        .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0))
        .unwrap_or(0))
}

/// Create the generic cascade schema. Working tables (`cx_frontier`, `cx_next`)
/// are regular tables (not TEMP) so they are visible regardless of which pooled
/// connection runs a statement.
pub async fn create_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    // cx_row is a ROWID table clustered on `key` (INTEGER PRIMARY KEY = the
    // rowid alias): the key costs zero extra bytes and every lookup is a native
    // rowid search. tag/id ride along as plain output columns. cx_dep collapses
    // from a 4-column composite to a 2-column (parent_key, child_key) WITHOUT
    // ROWID, still parent-prefix-ordered for the delta traversal.
    db.execute_unprepared(
        "CREATE TABLE cx_row (
            key    INTEGER PRIMARY KEY,
            weight INTEGER NOT NULL DEFAULT 1,
            tag    INTEGER GENERATED ALWAYS AS (key / 1000000000) VIRTUAL,
            id     INTEGER GENERATED ALWAYS AS (key % 1000000000) VIRTUAL
         );
         CREATE TABLE cx_dep (
            parent_key INTEGER NOT NULL,
            child_key  INTEGER NOT NULL,
            PRIMARY KEY (parent_key, child_key)
         ) WITHOUT ROWID;
         CREATE TABLE cx_frontier (key INTEGER PRIMARY KEY);
         CREATE TABLE cx_next     (key INTEGER PRIMARY KEY);
         CREATE TABLE cx_hits (key INTEGER PRIMARY KEY, dec INTEGER NOT NULL);",
    )
    .await?;
    Ok(())
}

/// Batch-insert rows `(tag, id, weight)`. One transaction = one WAL commit for
/// the whole load, instead of a commit per chunk.
pub async fn insert_rows(db: &DatabaseConnection, rows: &[(i64, i64, i64)]) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    for chunk in rows.chunks(CHUNK) {
        let vals: Vec<String> = chunk
            .iter()
            .map(|(t, i, w)| format!("({},{w})", key(*t, *i)))
            .collect();
        exec(&txn, &format!("INSERT INTO cx_row(key,weight) VALUES {}", vals.join(","))).await?;
    }
    txn.commit().await?;
    Ok(())
}

/// Batch-insert dependency edges `(parent_tag, parent_id, child_tag, child_id)`.
pub async fn insert_deps(
    db: &DatabaseConnection,
    edges: &[(i64, i64, i64, i64)],
) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    for chunk in edges.chunks(CHUNK) {
        let vals: Vec<String> = chunk
            .iter()
            .map(|(pt, pi, ct, ci)| format!("({},{})", key(*pt, *pi), key(*ct, *ci)))
            .collect();
        exec(
            &txn,
            &format!("INSERT INTO cx_dep(parent_key,child_key) VALUES {}", vals.join(",")),
        )
        .await?;
    }
    txn.commit().await?;
    Ok(())
}

/// Retract `seeds` (each `(tag, id)` loses one unit of weight). Cascade the
/// consequence and return the number of rounds (= the depth reached). Every
/// round is a fixed set of set-based statements over the whole frontier, so the
/// statement count is O(rounds), never O(rows).
pub async fn retract(db: &DatabaseConnection, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
    // The WHOLE cascade runs in ONE transaction: one connection (correct under
    // pooling) and one WAL commit for every round, instead of a commit per
    // statement. This is the largest single retract speedup.
    let txn = db.begin().await?;

    exec(&txn, "DELETE FROM cx_frontier").await?;
    exec(&txn, "DELETE FROM cx_next").await?;

    // Apply the -1 to each seed, then the frontier is the seeds that hit <= 0.
    let seed_vals: Vec<String> = seeds.iter().map(|(t, i)| key(*t, *i).to_string()).collect();
    let seed_in = format!("({})", seed_vals.join(","));
    exec(
        &txn,
        &format!("UPDATE cx_row SET weight = weight - 1 WHERE key IN {seed_in}"),
    )
    .await?;
    exec(
        &txn,
        &format!(
            "INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN {seed_in} AND weight <= 0"
        ),
    )
    .await?;

    // Each round is DELTA-PROPORTIONAL: every statement is driven from the small
    // working set (frontier / hits) into the big tables via their PRIMARY KEY, so
    // work scales with the wavefront, not the corpus. `CROSS JOIN` pins the join
    // order (the transient working tables have no stats, so the planner would
    // otherwise scan the corpus). Verified with EXPLAIN QUERY PLAN: every step is
    // SCAN <small> -> SEARCH <big> USING PRIMARY KEY.
    let mut rounds = 0u64;
    loop {
        if scalar(&txn, "SELECT count(*) FROM cx_frontier").await? == 0 {
            break;
        }
        rounds += 1;

        // 1. hits = the frontier's children + how many supports each loses now.
        exec(&txn, "DELETE FROM cx_hits").await?;
        exec(&txn,
            "INSERT INTO cx_hits(key,dec) \
             SELECT d.child_key, count(*) \
             FROM cx_frontier f CROSS JOIN cx_dep d \
               ON d.parent_key = f.key \
             GROUP BY d.child_key",
        )
        .await?;

        // 2. decrement each hit child by its lost-support count (indexed by rowid).
        exec(&txn,
            "UPDATE cx_row SET weight = weight - \
                (SELECT dec FROM cx_hits h WHERE h.key = cx_row.key) \
             WHERE key IN (SELECT key FROM cx_hits)",
        )
        .await?;

        // 3. next frontier = hits that CROSSED zero THIS round: dead now
        //    (weight <= 0) but alive before this decrement (weight + dec > 0).
        //    The transition guard means a node enters the frontier exactly once,
        //    so we never delete dead rows/edges to avoid re-processing — a re-hit
        //    of an already-dead node just drives its weight more negative and is
        //    filtered out here. Killing the two big DELETEs (dead rows + dead
        //    edges out of the WITHOUT ROWID b-trees) is the retract's dominant
        //    cost, so this is the main speedup.
        exec(&txn, "DELETE FROM cx_next").await?;
        exec(&txn,
            "INSERT INTO cx_next(key) \
             SELECT h.key FROM cx_hits h CROSS JOIN cx_row r \
               ON r.key = h.key \
             WHERE r.weight <= 0 AND r.weight + h.dec > 0",
        )
        .await?;

        // 4. frontier <- next. Dead rows STAY in cx_row (weight <= 0); the
        //    survivor query filters on weight > 0.
        exec(&txn, "DELETE FROM cx_frontier").await?;
        exec(&txn, "INSERT INTO cx_frontier SELECT key FROM cx_next").await?;
    }
    txn.commit().await?;
    Ok(rounds)
}
