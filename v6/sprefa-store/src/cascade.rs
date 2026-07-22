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
pub const KEY_STRIDE: i64 = 1_000_000_000;

/// Dense E1 key: (rel, row) -> one i64 so cx_row is a rowid table clustered on an
/// INTEGER PRIMARY KEY. `rel` (= tag) picks the relation, `row` (= id) the tuple.
#[inline]
pub fn key(tag: i64, id: i64) -> i64 {
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

/// Create the generic cascade schema and apply the traversal tuning.
///
/// The store pins to ONE connection (`min=max=1`), so the churny working tables
/// are `TEMP`: with `temp_store=MEMORY` they live in RAM and are NEVER WAL-logged,
/// which is the whole per-round cost of the cascade (the labkit DRed proved this —
/// regular working tables WAL-log every `DELETE`/`INSERT` per round, a ~4x tax).
/// A real page cache + mmap lets the cone walk read `cx_row`/`cx_dep` from RAM
/// instead of the disk file. These match the proven feldera-lab DRed tuning.
pub async fn create_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    // 256 MB page cache (cache_size negative = KiB) + 1 GB read mmap. cache_size is
    // SQLite's own C heap (the memcap gun is blind to it — measured separately in
    // sqlite_reach's highwater); mmap is not a heap allocation at all.
    db.execute_unprepared("PRAGMA cache_size=-262144; PRAGMA mmap_size=1073741824;").await?;
    // cx_row is a ROWID table clustered on `key` (INTEGER PRIMARY KEY = the
    // rowid alias): the key costs zero extra bytes and every lookup is a native
    // rowid search. tag/id ride along as plain output columns. cx_dep collapses
    // from a 4-column composite to a 2-column (parent_key, child_key) WITHOUT
    // ROWID, still parent-prefix-ordered for the delta traversal. cx_row/cx_dep
    // are the persistent corpus; cx_frontier/next/hits/cone are RAM-only churn.
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
         CREATE TEMP TABLE cx_frontier (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE cx_next     (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE cx_hits (key INTEGER PRIMARY KEY, dec INTEGER NOT NULL);
         CREATE TEMP TABLE cx_cone (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE cx_scc_scope (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE cx_scc_frontier (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE cx_scc_next (key INTEGER PRIMARY KEY);
         CREATE TEMP TABLE cx_scc_live (key INTEGER PRIMARY KEY);
         CREATE INDEX ix_cx_dep_child ON cx_dep (child_key);",
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

/// Cycle-correct two-pass retraction. The first pass captures and tentatively
/// removes the affected cone. The second pass republishes cone members reached
/// from surviving external support. PK tables perform deduplication, and each
/// round fuses its weight, cone, and frontier mutations into one SQLite call.
///
/// All graph state, scope, and frontiers live in SQLite tables. Rust only drives
/// the fixed set of SQL rounds; no adjacency list or SCC partition is resident.
pub async fn retract_scc(db: &DatabaseConnection, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
    retract_scc_two_pass(db, seeds).await
}

async fn retract_scc_two_pass(db: &DatabaseConnection, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
    let txn = db.begin().await?;
    let seed_vals: Vec<String> = seeds.iter().map(|(t, i)| key(*t, *i).to_string()).collect();
    let seed_in = format!("({})", seed_vals.join(","));
    exec(&txn,
        &format!("DELETE FROM cx_frontier;
                  DELETE FROM cx_next;
                  DELETE FROM cx_cone;
                  INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN {seed_in} AND weight>0;
                  UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_frontier);
                  INSERT INTO cx_cone SELECT key FROM cx_frontier"),
    ).await?;

    let mut rounds = 0u64;
    loop {
        exec(&txn,
            "DELETE FROM cx_next;
             INSERT OR IGNORE INTO cx_next(key)
             SELECT d.child_key
             FROM cx_frontier f CROSS JOIN cx_dep d ON d.parent_key = f.key
             CROSS JOIN cx_row r ON r.key = d.child_key
             WHERE r.weight > 0",
        ).await?;
        if scalar(&txn, "SELECT count(*) FROM cx_next").await? == 0 { break; }
        rounds += 1;
        exec(&txn,
            "UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_next);
             INSERT OR IGNORE INTO cx_cone SELECT key FROM cx_next;
             DELETE FROM cx_frontier;
             INSERT INTO cx_frontier SELECT key FROM cx_next",
        ).await?;
    }

    exec(&txn,
        "DELETE FROM cx_frontier;
         DELETE FROM cx_next;
         INSERT OR IGNORE INTO cx_frontier(key)
         SELECT c.key
         FROM cx_cone c CROSS JOIN cx_dep d ON d.child_key = c.key
         CROSS JOIN cx_row p ON p.key = d.parent_key
         WHERE p.weight > 0;
         UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_frontier)",
    ).await?;
    loop {
        exec(&txn,
            "DELETE FROM cx_next;
             INSERT OR IGNORE INTO cx_next(key)
             SELECT d.child_key
             FROM cx_frontier f CROSS JOIN cx_dep d ON d.parent_key = f.key
             CROSS JOIN cx_row r ON r.key = d.child_key
             CROSS JOIN cx_cone c ON c.key = d.child_key
             WHERE r.weight = 0",
        ).await?;
        if scalar(&txn, "SELECT count(*) FROM cx_next").await? == 0 { break; }
        rounds += 1;
        exec(&txn,
            "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_next);
             DELETE FROM cx_frontier;
             INSERT INTO cx_frontier SELECT key FROM cx_next",
        ).await?;
    }
    txn.commit().await?;
    Ok(rounds)
}

// ============================================================================
// CYCLE-SAFE PAIR (backported from the v6 labkit). The counting `retract` above
// is correct only on an ACYCLIC support graph — on a cycle the members mutually
// support each other and never hit weight 0 (a phantom: cut the anchor and the
// cycle stays "alive"). These two treat `weight` as a BOOLEAN alive flag (0/1)
// and compute reachability-from-roots exactly, which is what retraction was
// always trying to approximate. Use this pair when the graph can contain cycles.
//   assert       = forward add (monotonic, cycle-safe by nature)
//   retract_dred = Delete-and-Rederive: over-delete the forward cone, then
//                  rederive any row still anchored to a surviving row.
// ============================================================================

/// Forward add: `seeds` become alive; propagate aliveness to everything reachable
/// from them that was dead. The opposite of retract. Monotonic, so cycle-safe.
/// Returns rounds (= depth of the newly-alive wavefront).
pub async fn assert(db: &DatabaseConnection, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
    let txn = db.begin().await?;
    exec(&txn, "DELETE FROM cx_frontier").await?;
    exec(&txn, "DELETE FROM cx_next").await?;
    let seed_in = {
        let v: Vec<String> = seeds.iter().map(|(t, i)| key(*t, *i).to_string()).collect();
        format!("({})", v.join(","))
    };
    // seed the wavefront from the seeds (alive or not), so an already-alive root still
    // pushes reachability into any newly-added dead children.
    exec(&txn, &format!("INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN {seed_in}")).await?;
    exec(&txn, &format!("UPDATE cx_row SET weight=1 WHERE key IN {seed_in}")).await?;
    let mut rounds = 0u64;
    loop {
        exec(
            &txn,
            "DELETE FROM cx_next; \
             INSERT INTO cx_next(key) \
             SELECT DISTINCT d.child_key \
             FROM cx_frontier f CROSS JOIN cx_dep d ON d.parent_key = f.key \
               CROSS JOIN cx_row r ON r.key = d.child_key \
             WHERE r.weight = 0",
        )
        .await?;
        if scalar(&txn, "SELECT count(*) FROM cx_next").await? == 0 {
            break;
        }
        rounds += 1;
        exec(&txn, "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_next)").await?;
        exec(&txn, "DELETE FROM cx_frontier").await?;
        exec(&txn, "INSERT INTO cx_frontier SELECT key FROM cx_next").await?;
    }
    txn.commit().await?;
    Ok(rounds)
}

/// Cycle-safe retraction via Delete-and-Rederive. `seeds` are retracted; then the
/// forward cone reachable from them is tentatively killed (over-delete), and any
/// cone row still reachable from a SURVIVING row is brought back (rederive). A dead
/// cycle has no surviving anchor, so it correctly stays dead. Returns total rounds.
pub async fn retract_dred(db: &DatabaseConnection, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
    let txn = db.begin().await?;
    exec(&txn, "DELETE FROM cx_frontier").await?;
    exec(&txn, "DELETE FROM cx_next").await?;
    exec(&txn, "DELETE FROM cx_cone").await?;

    // over-delete: seeds (alive) start the cone; kill them, walk forward killing the
    // reachable-and-alive cone. Every statement is driven from the small frontier into
    // the big tables by PRIMARY KEY (CROSS JOIN pins the order), so work ∝ the cone.
    let seed_in = {
        let v: Vec<String> = seeds.iter().map(|(t, i)| key(*t, *i).to_string()).collect();
        format!("({})", v.join(","))
    };
    exec(&txn, &format!("INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN {seed_in} AND weight>0")).await?;
    exec(&txn, "UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_frontier)").await?;
    exec(&txn, "INSERT INTO cx_cone SELECT key FROM cx_frontier").await?;
    let mut rounds = 0u64;
    loop {
        exec(
            &txn,
            "DELETE FROM cx_next; \
             INSERT INTO cx_next(key) \
             SELECT DISTINCT d.child_key \
             FROM cx_frontier f CROSS JOIN cx_dep d ON d.parent_key = f.key \
               CROSS JOIN cx_row r ON r.key = d.child_key \
             WHERE r.weight > 0",
        )
        .await?;
        if scalar(&txn, "SELECT count(*) FROM cx_next").await? == 0 {
            break;
        }
        rounds += 1;
        exec(&txn, "UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_next)").await?;
        exec(&txn, "INSERT OR IGNORE INTO cx_cone SELECT key FROM cx_next").await?;
        exec(&txn, "DELETE FROM cx_frontier").await?;
        exec(&txn, "INSERT INTO cx_frontier SELECT key FROM cx_next").await?;
    }

    // rederive: cone rows with a SURVIVING parent (weight>0, i.e. outside the cone)
    // come back; propagate forward within the cone. Uses ix_cx_dep_child (child->parent).
    exec(&txn, "DELETE FROM cx_frontier").await?;
    exec(&txn, "DELETE FROM cx_next").await?;
    exec(
        &txn,
        "INSERT INTO cx_frontier(key) \
         SELECT DISTINCT c.key \
         FROM cx_cone c CROSS JOIN cx_dep d ON d.child_key = c.key \
           CROSS JOIN cx_row p ON p.key = d.parent_key \
         WHERE p.weight > 0",
    )
    .await?;
    exec(&txn, "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_frontier)").await?;
    loop {
        exec(
            &txn,
            "DELETE FROM cx_next; \
             INSERT INTO cx_next(key) \
             SELECT DISTINCT d.child_key \
             FROM cx_frontier f CROSS JOIN cx_dep d ON d.parent_key = f.key \
               CROSS JOIN cx_row r ON r.key = d.child_key \
               CROSS JOIN cx_cone c ON c.key = d.child_key \
             WHERE r.weight = 0",
        )
        .await?;
        if scalar(&txn, "SELECT count(*) FROM cx_next").await? == 0 {
            break;
        }
        rounds += 1;
        exec(&txn, "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_next)").await?;
        exec(&txn, "DELETE FROM cx_frontier").await?;
        exec(&txn, "INSERT INTO cx_frontier SELECT key FROM cx_next").await?;
    }
    txn.commit().await?;
    Ok(rounds)
}

/// Cycle-safe retraction, Delete-and-Rederive expressed as TWO recursive CTEs so
/// SQLite runs the whole cone traversal AND the rederive inside its C engine — one
/// prepared statement each — instead of the Rust-driven round loop of `retract_dred`
/// (~6 `execute` round-trips per BFS round, ~180 for a depth-15 graph). Identical
/// semantics and result; the round-trip tax is gone. This is the form to use at
/// scale. Returns 0 (rounds are not meaningful for the set-at-once CTE form).
///
/// Phase 1 (over-delete): the forward cone of currently-alive nodes reachable from
/// the alive seeds, computed over unchanged weights in one recursive walk, then all
/// killed. Phase 2 (rederive): cone nodes anchored to a surviving (weight>0, hence
/// outside-cone) parent come back and propagate forward within the cone; a dead
/// cycle has no such anchor and stays dead.
pub async fn retract_dred_cte(db: &DatabaseConnection, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
    let txn = db.begin().await?;
    let seed_in = {
        let v: Vec<String> = seeds.iter().map(|(t, i)| key(*t, *i).to_string()).collect();
        format!("({})", v.join(","))
    };
    exec(&txn, "DELETE FROM cx_cone").await?;
    // Phase 1 — over-delete. `cone` = seeds ∪ everything forward-reachable from them
    // over still-alive edges. UNION (not UNION ALL) dedups nodes, so work is O(cone),
    // and the walk reads weights BEFORE the kill below, so every cone node qualifies.
    exec(
        &txn,
        &format!(
            "INSERT INTO cx_cone(key)
             WITH RECURSIVE cone(key) AS (
                SELECT key FROM cx_row WHERE key IN {seed_in} AND weight>0
                UNION
                SELECT d.child_key FROM cone
                  JOIN cx_dep d ON d.parent_key = cone.key
                  JOIN cx_row r ON r.key = d.child_key
                 WHERE r.weight>0
             )
             SELECT key FROM cone",
        ),
    )
    .await?;
    exec(&txn, "UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_cone)").await?;
    // Phase 2 — rederive. Base: cone nodes with a surviving (weight>0) parent — since
    // every cone node is now weight=0, weight>0 means the parent is outside the cone.
    // Step: propagate aliveness forward, staying inside the cone (JOIN cx_cone).
    exec(&txn, "DELETE FROM cx_frontier").await?; // reused as the alive-set sink
    exec(
        &txn,
        "INSERT INTO cx_frontier(key)
         WITH RECURSIVE alive(key) AS (
            SELECT c.key FROM cx_cone c
              JOIN cx_dep d ON d.child_key = c.key
              JOIN cx_row p ON p.key = d.parent_key
             WHERE p.weight>0
            UNION
            SELECT d.child_key FROM alive
              JOIN cx_dep d ON d.parent_key = alive.key
              JOIN cx_cone c ON c.key = d.child_key
         )
         SELECT key FROM alive",
    )
    .await?;
    exec(&txn, "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_frontier)").await?;
    txn.commit().await?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    async fn open() -> DatabaseConnection {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir()
            .join(format!("cx_dred_test_{}_{uniq}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        opt.max_connections(1).min_connections(1);
        let db = Database::connect(opt).await.unwrap();
        db.execute_unprepared(crate::unfuck_sqlite::OPEN_PRAGMAS).await.unwrap();
        create_schema(&db).await.unwrap();
        db
    }

    async fn alive(db: &DatabaseConnection) -> i64 {
        scalar(db, "SELECT count(*) FROM cx_row WHERE weight>0").await.unwrap()
    }

    // root R -> A, cycle A -> B -> C -> A. Cut R. Correct: everything dies (no anchor).
    // Counting retract would leave A,B,C alive (phantom cycle); DRed must give 0.
    #[tokio::test]
    async fn retract_dred_kills_a_cut_cycle() {
        let db = open().await;
        // rows R=(0,0) A=(0,1) B=(0,2) C=(0,3), all alive (weight 1)
        insert_rows(&db, &[(0, 0, 1), (0, 1, 1), (0, 2, 1), (0, 3, 1)]).await.unwrap();
        // R->A, A->B, B->C, C->A
        insert_deps(&db, &[(0, 0, 0, 1), (0, 1, 0, 2), (0, 2, 0, 3), (0, 3, 0, 1)]).await.unwrap();
        assert_eq!(alive(&db).await, 4);

        retract_dred(&db, &[(0, 0)]).await.unwrap();
        assert_eq!(alive(&db).await, 0, "cutting the root must kill the whole cone, cycle included");
    }

    // Same graph but ALSO a second root R2 -> B. Cut R. Now B (and via cycle A,C) stay
    // alive through R2. DRed must rederive them: survivors = R2,A,B,C = 4 (only R dies).
    #[tokio::test]
    async fn retract_dred_rederives_through_alternate_anchor() {
        let db = open().await;
        // R=(0,0) A=(0,1) B=(0,2) C=(0,3) R2=(0,4)
        insert_rows(&db, &[(0, 0, 1), (0, 1, 1), (0, 2, 1), (0, 3, 1), (0, 4, 1)]).await.unwrap();
        insert_deps(&db, &[(0, 0, 0, 1), (0, 1, 0, 2), (0, 2, 0, 3), (0, 3, 0, 1), (0, 4, 0, 2)]).await.unwrap();
        assert_eq!(alive(&db).await, 5);

        retract_dred(&db, &[(0, 0)]).await.unwrap();
        // R dies; R2 anchors B, and the cycle B->C->A keeps A,C alive. survivors = 4.
        assert_eq!(alive(&db).await, 4, "alternate anchor must rederive the cycle");
    }

    // The CTE form must match retract_dred exactly: cut the root, whole cone incl
    // the cycle dies (no anchor).
    #[tokio::test]
    async fn retract_dred_cte_kills_a_cut_cycle() {
        let db = open().await;
        insert_rows(&db, &[(0, 0, 1), (0, 1, 1), (0, 2, 1), (0, 3, 1)]).await.unwrap();
        insert_deps(&db, &[(0, 0, 0, 1), (0, 1, 0, 2), (0, 2, 0, 3), (0, 3, 0, 1)]).await.unwrap();
        assert_eq!(alive(&db).await, 4);
        retract_dred_cte(&db, &[(0, 0)]).await.unwrap();
        assert_eq!(alive(&db).await, 0, "CTE: cutting the root must kill the whole cone, cycle included");
    }

    // CTE form: alternate anchor rederives the cycle (only R dies, survivors = 4).
    #[tokio::test]
    async fn retract_dred_cte_rederives_through_alternate_anchor() {
        let db = open().await;
        insert_rows(&db, &[(0, 0, 1), (0, 1, 1), (0, 2, 1), (0, 3, 1), (0, 4, 1)]).await.unwrap();
        insert_deps(&db, &[(0, 0, 0, 1), (0, 1, 0, 2), (0, 2, 0, 3), (0, 3, 0, 1), (0, 4, 0, 2)]).await.unwrap();
        assert_eq!(alive(&db).await, 5);
        retract_dred_cte(&db, &[(0, 0)]).await.unwrap();
        assert_eq!(alive(&db).await, 4, "CTE: alternate anchor must rederive the cycle");
    }

    // assert is the inverse: bring a dead node alive and propagate forward.
    #[tokio::test]
    async fn assert_propagates_forward() {
        let db = open().await;
        // R alive, A/B dead; R->A->B. assert R's reach.
        insert_rows(&db, &[(0, 0, 1), (0, 1, 0), (0, 2, 0)]).await.unwrap();
        insert_deps(&db, &[(0, 0, 0, 1), (0, 1, 0, 2)]).await.unwrap();
        assert_eq!(alive(&db).await, 1);
        assert(&db, &[(0, 0)]).await.unwrap();
        assert_eq!(alive(&db).await, 3, "R->A->B all alive after assert");
    }
}
