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

// Helpers take `&impl ConnectionTrait` so they run on either the pooled
// connection OR a single-connection transaction. The transaction path is the
// point: it pins every statement to ONE connection (correctness under pooling)
// and batches all the WAL writes into a single commit (the big speed win).
async fn exec(db: &impl ConnectionTrait, sql: &str) -> Result<(), DbErr> {
    stmt_counter::incr();
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
    db.execute_unprepared(
        "CREATE TABLE cx_row (
            tag    INTEGER NOT NULL,
            id     INTEGER NOT NULL,
            weight INTEGER NOT NULL DEFAULT 1,
            PRIMARY KEY (tag, id)
         ) WITHOUT ROWID;
         CREATE TABLE cx_dep (
            parent_tag INTEGER NOT NULL,
            parent_id  INTEGER NOT NULL,
            child_tag  INTEGER NOT NULL,
            child_id   INTEGER NOT NULL,
            PRIMARY KEY (parent_tag, parent_id, child_tag, child_id)
         ) WITHOUT ROWID;
         CREATE TABLE cx_frontier (tag INTEGER NOT NULL, id INTEGER NOT NULL, PRIMARY KEY(tag,id)) WITHOUT ROWID;
         CREATE TABLE cx_next     (tag INTEGER NOT NULL, id INTEGER NOT NULL, PRIMARY KEY(tag,id)) WITHOUT ROWID;
         CREATE TABLE cx_hits (tag INTEGER NOT NULL, id INTEGER NOT NULL, dec INTEGER NOT NULL, PRIMARY KEY(tag,id)) WITHOUT ROWID;",
    )
    .await?;
    Ok(())
}

/// Batch-insert rows `(tag, id, weight)`. One transaction = one WAL commit for
/// the whole load, instead of a commit per chunk.
pub async fn insert_rows(db: &DatabaseConnection, rows: &[(i64, i64, i64)]) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    for chunk in rows.chunks(CHUNK) {
        let vals: Vec<String> = chunk.iter().map(|(t, i, w)| format!("({t},{i},{w})")).collect();
        exec(&txn, &format!("INSERT INTO cx_row(tag,id,weight) VALUES {}", vals.join(","))).await?;
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
            .map(|(pt, pi, ct, ci)| format!("({pt},{pi},{ct},{ci})"))
            .collect();
        exec(
            &txn,
            &format!(
                "INSERT INTO cx_dep(parent_tag,parent_id,child_tag,child_id) VALUES {}",
                vals.join(",")
            ),
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
    let seed_vals: Vec<String> = seeds.iter().map(|(t, i)| format!("({t},{i})")).collect();
    let seed_in = format!("(VALUES {})", seed_vals.join(","));
    exec(
        &txn,
        &format!("UPDATE cx_row SET weight = weight - 1 WHERE (tag,id) IN {seed_in}"),
    )
    .await?;
    exec(
        &txn,
        &format!(
            "INSERT INTO cx_frontier SELECT tag,id FROM cx_row WHERE (tag,id) IN {seed_in} AND weight <= 0"
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
            "INSERT INTO cx_hits(tag,id,dec) \
             SELECT d.child_tag, d.child_id, count(*) \
             FROM cx_frontier f CROSS JOIN cx_dep d \
               ON d.parent_tag = f.tag AND d.parent_id = f.id \
             GROUP BY d.child_tag, d.child_id",
        )
        .await?;

        // 2. decrement each hit child by its lost-support count (indexed by PK).
        exec(&txn,
            "UPDATE cx_row SET weight = weight - \
                (SELECT dec FROM cx_hits h WHERE h.tag = cx_row.tag AND h.id = cx_row.id) \
             WHERE (tag,id) IN (SELECT tag,id FROM cx_hits)",
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
            "INSERT INTO cx_next(tag,id) \
             SELECT h.tag, h.id FROM cx_hits h CROSS JOIN cx_row r \
               ON r.tag = h.tag AND r.id = h.id \
             WHERE r.weight <= 0 AND r.weight + h.dec > 0",
        )
        .await?;

        // 4. frontier <- next. Dead rows STAY in cx_row (weight <= 0); the
        //    survivor query filters on weight > 0.
        exec(&txn, "DELETE FROM cx_frontier").await?;
        exec(&txn, "INSERT INTO cx_frontier SELECT tag,id FROM cx_next").await?;
    }
    txn.commit().await?;
    Ok(rounds)
}
