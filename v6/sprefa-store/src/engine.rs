//! The ONE semi-naive cascade: frontier -> one hop -> prune -> fixpoint.
//! Three prune modes: reconcile=digest(A) . retract/assert=weight(B) . reach/temporal=reached(C).
//! SQLite is production; dd/salsa live in `oracle.rs`. See ../AGENTS.md.

pub mod cascade {
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

}
pub mod reach {
//! The v5 graph covering set, on-disk over `cx_dep(parent_key, child_key)`.
//!
//! Each function here has a resident pure-Rust ORACLE in the repo root
//! (`src/graph/scc.rs`, `src/graph/walk.rs`) and MUST agree with it byte-for-byte
//! (partition-canonical for SCC). `tests/covering.rs` is the standing check.
//!
//! FROZEN CONTRACT + exact inclusion semantics: `v6/findings/INSIGHTS.md` §3, §A.
//! Recursive-CTE RAM is NOT guessable — every perf run goes through `measure.rs`.

use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

static WALK_TABLE_ID: AtomicU64 = AtomicU64::new(0);

fn statement(sql: String) -> Statement {
    Statement::from_string(DatabaseBackend::Sqlite, sql)
}

const REACH_CTE: &str = "WITH RECURSIVE reach(src,dst) AS (\
    SELECT parent_key,child_key FROM cx_dep \
    UNION \
    SELECT reach.src, dep.child_key FROM reach \
    JOIN cx_dep dep ON dep.parent_key = reach.dst\
)";

/// Condensation, all component ids expressed as MIN-member representative keys.
pub struct Condensed {
    pub comp_of: Vec<(i64, i64)>, // (node_key, comp_repr)
    pub size: Vec<(i64, i64)>,    // (comp_repr, member_count)
    pub cyclic: Vec<(i64, bool)>, // (comp_repr, is_cyclic)
    pub cadj: Vec<(i64, i64)>,    // (parent_comp_repr, child_comp_repr), deduped, no self
}

/// Forward transitive closure from `start` (strict; includes start iff its SCC is cyclic).
pub async fn reaches_from(db: &DatabaseConnection, start: i64) -> Result<Vec<i64>, DbErr> {
    let sql = format!(
        "WITH RECURSIVE reach(key) AS (\
            SELECT child_key FROM cx_dep WHERE parent_key = {start} \
            UNION \
            SELECT dep.child_key FROM cx_dep dep JOIN reach ON dep.parent_key = reach.key\
        ) SELECT key FROM reach ORDER BY key"
    );
    db.query_all_raw(statement(sql))
        .await?
        .iter()
        .map(|row| row.try_get_by_index::<i64>(0))
        .collect()
}

/// Reverse transitive closure into `target` (rides ix_cx_dep_child).
pub async fn reached_by(db: &DatabaseConnection, target: i64) -> Result<Vec<i64>, DbErr> {
    let sql = format!(
        "WITH RECURSIVE reach(key) AS (\
            SELECT parent_key FROM cx_dep WHERE child_key = {target} \
            UNION \
            SELECT dep.parent_key FROM cx_dep dep JOIN reach ON dep.child_key = reach.key\
        ) SELECT key FROM reach ORDER BY key"
    );
    db.query_all_raw(statement(sql))
        .await?
        .iter()
        .map(|row| row.try_get_by_index::<i64>(0))
        .collect()
}

/// Multi-source min-depth BFS. See INSIGHTS §3 for halt/depth_cap semantics.
pub async fn multi_source_walk(
    db: &DatabaseConnection,
    starts: &[(i64, i64, i64)],
    halt: Option<&[i64]>,
    depth_cap: Option<i64>,
) -> Result<Vec<(i64, i64, i64)>, DbErr> {
    let table_id = WALK_TABLE_ID.fetch_add(1, Ordering::Relaxed);
    let reached_table = format!("_reached_{table_id}");
    let halt_table = format!("_halt_{table_id}");
    let txn = db.begin().await?;

    txn.execute_unprepared(&format!(
        "CREATE TEMP TABLE {reached_table} (\
            tag INTEGER NOT NULL, node INTEGER NOT NULL, depth INTEGER NOT NULL, round INTEGER NOT NULL,\
            PRIMARY KEY(tag,node)\
        )"
    ))
    .await?;
    txn.execute_unprepared(&format!("CREATE TEMP TABLE {halt_table} (node INTEGER PRIMARY KEY)"))
        .await?;

    if let Some(halt_nodes) = halt {
        for &node in halt_nodes {
            txn.execute_unprepared(&format!("INSERT OR IGNORE INTO {halt_table} VALUES ({node})"))
                .await?;
        }
    }

    let mut ordered_starts = starts.to_vec();
    ordered_starts.sort_unstable();
    for (tag, node, depth) in ordered_starts {
        txn.execute_unprepared(&format!(
            "INSERT OR IGNORE INTO {reached_table}(tag,node,depth,round) VALUES ({tag},{node},{depth},0)"
        ))
        .await?;
    }

    let mut round = 0i64;
    loop {
        let expand_guard = match depth_cap {
            Some(cap) => format!(" AND reached.depth < {cap}"),
            None => String::new(),
        };
        txn.execute_unprepared(&format!(
            "INSERT OR IGNORE INTO {reached_table}(tag,node,depth,round) \
             SELECT reached.tag, dep.child_key, reached.depth + 1, {} \
             FROM {reached_table} reached JOIN cx_dep dep ON dep.parent_key = reached.node \
             WHERE reached.round = {round} \
             AND NOT EXISTS (SELECT 1 FROM {halt_table} halt WHERE halt.node = reached.node){}",
            round + 1,
            expand_guard,
        ))
        .await?;

        let inserted = txn
            .query_one_raw(statement(format!(
                "SELECT count(*) FROM {reached_table} WHERE round = {}",
                round + 1
            )))
            .await?
            .map(|row| row.try_get_by_index::<i64>(0))
            .transpose()?
            .unwrap_or(0);
        if inserted == 0 {
            break;
        }
        round += 1;
    }

    let rows = txn
        .query_all_raw(statement(format!(
            "SELECT tag,node,depth FROM {reached_table} ORDER BY tag,node"
        )))
        .await?;
    let result = rows
        .iter()
        .map(|row| {
            Ok((
                row.try_get_by_index::<i64>(0)?,
                row.try_get_by_index::<i64>(1)?,
                row.try_get_by_index::<i64>(2)?,
            ))
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    txn.execute_unprepared(&format!("DROP TABLE {reached_table}; DROP TABLE {halt_table}"))
        .await?;
    txn.commit().await?;
    Ok(result)
}

/// halt-only, depth-agnostic special case of `multi_source_walk`.
pub async fn multi_source_halt_bfs(
    db: &DatabaseConnection,
    starts: &[(i64, i64)],
    halt: &[i64],
) -> Result<Vec<(i64, i64)>, DbErr> {
    let starts3: Vec<(i64, i64, i64)> = starts.iter().map(|&(t, n)| (t, n, 0)).collect();
    Ok(multi_source_walk(db, &starts3, Some(halt), None)
        .await?
        .into_iter()
        .map(|(t, n, _)| (t, n))
        .collect())
}

/// SCC partition as (node_key, comp_repr = MIN member key). Compare on the partition.
pub async fn scc_labels(db: &DatabaseConnection) -> Result<Vec<(i64, i64)>, DbErr> {
    let sql = format!(
        "{REACH_CTE} \
         SELECT node.key, COALESCE(MIN(CASE WHEN backward.src IS NOT NULL THEN forward.dst END), node.key) \
         FROM cx_row node \
         LEFT JOIN reach forward ON forward.src = node.key \
         LEFT JOIN reach backward ON backward.src = forward.dst AND backward.dst = node.key \
         GROUP BY node.key ORDER BY node.key"
    );
    db.query_all_raw(statement(sql))
        .await?
        .iter()
        .map(|row| Ok((row.try_get_by_index::<i64>(0)?, row.try_get_by_index::<i64>(1)?)))
        .collect()
}

/// Condensation derived from `scc_labels` + cx_dep group-bys.
pub async fn build_condensed(db: &DatabaseConnection) -> Result<Condensed, DbErr> {
    let comp_of = scc_labels(db).await?;
    let repr_by_node: BTreeMap<i64, i64> = comp_of.iter().copied().collect();
    let mut member_counts: BTreeMap<i64, i64> = BTreeMap::new();
    for &(_, repr) in &comp_of {
        *member_counts.entry(repr).or_default() += 1;
    }

    let edges = db
        .query_all_raw(statement("SELECT parent_key,child_key FROM cx_dep".to_owned()))
        .await?;
    let mut self_loops = BTreeSet::new();
    let mut condensed_edges = BTreeSet::new();
    for edge in &edges {
        let parent = edge.try_get_by_index::<i64>(0)?;
        let child = edge.try_get_by_index::<i64>(1)?;
        let parent_repr = repr_by_node[&parent];
        let child_repr = repr_by_node[&child];
        if parent == child {
            self_loops.insert(parent_repr);
        }
        if parent_repr != child_repr {
            condensed_edges.insert((parent_repr, child_repr));
        }
    }

    let size: Vec<(i64, i64)> = member_counts.iter().map(|(&repr, &count)| (repr, count)).collect();
    let cyclic = member_counts
        .iter()
        .map(|(&repr, &count)| (repr, count > 1 || self_loops.contains(&repr)))
        .collect();
    Ok(Condensed {
        comp_of,
        size,
        cyclic,
        cadj: condensed_edges.into_iter().collect(),
    })
}

/// Reachable ordered-pair count; matches scc::count_pairs byte-for-byte. Counts over
/// the CONDENSATION (ncomp components, bitset reach) exactly like v5's scc.rs:103 —
/// it does NOT materialize the Θ(V²) node-pair table. The answer can be Θ(V²) but is
/// COMPUTED in ncomp²/64 words in Rust; the on-disk graph stays on disk, only the
/// small condensed structure comes to Rust. (SCC labeling inside build_condensed is
/// the remaining lever — see INSIGHTS §3.)
pub async fn count_pairs(db: &DatabaseConnection) -> Result<i128, DbErr> {
    let cond = build_condensed(db).await?;

    // dense-index the min-member component reps to 0..ncomp
    let reprs: BTreeSet<i64> = cond.size.iter().map(|&(repr, _)| repr).collect();
    let idx: BTreeMap<i64, usize> = reprs.iter().enumerate().map(|(i, &r)| (r, i)).collect();
    let ncomp = reprs.len();
    if ncomp == 0 {
        return Ok(0);
    }
    let mut size = vec![0u128; ncomp];
    for &(repr, n) in &cond.size {
        size[idx[&repr]] = n as u128;
    }
    let mut cyclic = vec![false; ncomp];
    for &(repr, is_cyclic) in &cond.cyclic {
        cyclic[idx[&repr]] = is_cyclic;
    }
    let mut cadj = vec![Vec::new(); ncomp];
    for &(parent, child) in &cond.cadj {
        cadj[idx[&parent]].push(idx[&child]);
    }

    // v5 scc.rs:103 verbatim in ncomp space: topo order, bitset-propagate reach,
    // total = Σ cyclic·size² + Σ size·(reachable component sizes).
    let mut indeg = vec![0u32; ncomp];
    for cc in 0..ncomp {
        for &s in &cadj[cc] {
            indeg[s] += 1;
        }
    }
    let mut topo: Vec<usize> = (0..ncomp).filter(|&x| indeg[x] == 0).collect();
    let mut qi = 0;
    while qi < topo.len() {
        let x = topo[qi];
        qi += 1;
        for &s in &cadj[x] {
            indeg[s] -= 1;
            if indeg[s] == 0 {
                topo.push(s);
            }
        }
    }
    let words = (ncomp + 63) / 64;
    let mut reach = vec![0u64; ncomp * words];
    for &cc in topo.iter().rev() {
        for si in 0..cadj[cc].len() {
            let s = cadj[cc][si];
            reach[cc * words + s / 64] |= 1u64 << (s % 64);
            for w in 0..words {
                reach[cc * words + w] |= reach[s * words + w];
            }
        }
    }
    let mut total: i128 = 0;
    for cc in 0..ncomp {
        if cyclic[cc] {
            total += (size[cc] * size[cc]) as i128;
        }
        let mut wsum: u128 = 0;
        for w in 0..words {
            let mut bits = reach[cc * words + w];
            while bits != 0 {
                let b = w * 64 + bits.trailing_zeros() as usize;
                wsum += size[b];
                bits &= bits - 1;
            }
        }
        total += (size[cc] * wsum) as i128;
    }
    Ok(total)
}

}
pub mod reconcile {
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

}
pub mod temporal {
//! Append-only bitemporal fact storage backed by SQLite.

use sea_orm::{
    ConnectionTrait, DatabaseBackend, DatabaseConnection, DbErr, Statement, TransactionTrait,
};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::stmt_counter;

const SOFT_HEAP_LIMIT: &str = "PRAGMA soft_heap_limit=4294967296;";

fn mix_key(key: i64) -> i64 {
    let mut mixed = (key as u64).wrapping_add(0x9E3779B97F4A7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D049BB133111EB);
    (mixed ^ (mixed >> 31)) as i64
}

async fn execute(db: &impl ConnectionTrait, sql: &str) -> Result<(), DbErr> {
    stmt_counter::incr();
    db.execute_unprepared(sql).await?;
    Ok(())
}

async fn execute_statement(db: &impl ConnectionTrait, statement: Statement) -> Result<(), DbErr> {
    stmt_counter::incr();
    db.execute_raw(statement).await?;
    Ok(())
}

async fn scalar(db: &impl ConnectionTrait, sql: &str) -> Result<i64, DbErr> {
    stmt_counter::incr();
    Ok(db
        .query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, sql.to_owned()))
        .await?
        .map(|row| row.try_get_by_index::<i64>(0).unwrap_or(0))
        .unwrap_or(0))
}

async fn create_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    execute(
        db,
        "CREATE TABLE fact(key INTEGER NOT NULL, tt_from INTEGER NOT NULL,
            tt_to INTEGER, weight INTEGER NOT NULL, PRIMARY KEY(key,tt_from)) WITHOUT ROWID;
         CREATE INDEX ix_live ON fact(key) WHERE tt_to IS NULL;
         CREATE TEMP TABLE d(key INTEGER PRIMARY KEY, dw INTEGER);",
    )
    .await
}

fn delta_json(deltas: &[(i64, i64)]) -> String {
    let mut json = String::from("[");
    for (index, (key, weight)) in deltas.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!("[{key},{weight}]"));
    }
    json.push(']');
    json
}

pub struct TemporalStore {
    db: DatabaseConnection,
    revision: AtomicI64,
}

impl TemporalStore {
    pub async fn attach(db: DatabaseConnection) -> Result<Self, DbErr> {
        execute(&db, crate::unfuck_sqlite::OPEN_PRAGMAS).await?;
        execute(&db, SOFT_HEAP_LIMIT).await?;
        create_schema(&db).await?;
        let revision = scalar(&db, "SELECT COALESCE(MAX(tt_from), 0) FROM fact").await?;
        Ok(Self {
            db,
            revision: AtomicI64::new(revision),
        })
    }

    pub async fn commit(&self, deltas: &[(i64, i64)]) -> Result<(), DbErr> {
        if deltas.is_empty() {
            return Ok(());
        }
        let revision = self.revision.fetch_add(1, Ordering::Relaxed) + 1;
        let delta_json = delta_json(deltas);
        let transaction = self.db.begin().await?;
        execute(&transaction, "DELETE FROM d").await?;
        execute_statement(
            &transaction,
            Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "INSERT INTO d(key,dw) SELECT json_extract(value,'$[0]'), sum(json_extract(value,'$[1]'))
             FROM json_each(?1) GROUP BY 1",
                [delta_json.into()],
            ),
        )
        .await?;
        execute_statement(
            &transaction,
            Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "INSERT INTO fact(key,tt_from,tt_to,weight)
             SELECT d.key, ?1, NULL, 0 FROM d LEFT JOIN fact f ON f.key=d.key AND f.tt_to IS NULL
             WHERE f.key IS NULL AND d.dw>0",
                [revision.into()],
            ),
        )
        .await?;
        execute(
            &transaction,
            "UPDATE fact SET weight = weight + (SELECT dw FROM d WHERE d.key=fact.key)
             WHERE tt_to IS NULL AND key IN (SELECT key FROM d)",
        )
        .await?;
        execute_statement(
            &transaction,
            Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                "UPDATE fact SET tt_to=?1 WHERE tt_to IS NULL AND weight<=0 AND key IN (SELECT key FROM d)",
                [revision.into()],
            ),
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn live(&self) -> Result<i64, DbErr> {
        scalar(&self.db, "SELECT count(*) FROM fact WHERE tt_to IS NULL").await
    }

    pub async fn total_rows(&self) -> Result<i64, DbErr> {
        scalar(&self.db, "SELECT count(*) FROM fact").await
    }

    pub async fn digest(&self) -> Result<i64, DbErr> {
        stmt_counter::incr();
        let rows = self
            .db
            .query_all_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT key FROM fact WHERE tt_to IS NULL".to_owned(),
            ))
            .await?;
        Ok(rows.into_iter().fold(0, |digest, row| {
            digest ^ mix_key(row.try_get_by_index::<i64>(0).unwrap_or(0))
        }))
    }

    pub fn conn(&self) -> &DatabaseConnection {
        &self.db
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    async fn open() -> TemporalStore {
        static NEXT_TEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "temporal_store_test_{}_{unique}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut options = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        options.max_connections(1).min_connections(1);
        TemporalStore::attach(Database::connect(options).await.unwrap())
            .await
            .unwrap()
    }

    fn statement_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        &LOCK
    }

    #[tokio::test]
    async fn commit_opens_live_intervals_for_a_batch() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        let deltas = [(10, 1), (20, 1), (30, 1)];
        store.commit(&deltas).await.unwrap();
        assert_eq!(store.live().await.unwrap(), deltas.len() as i64);
        assert_eq!(store.total_rows().await.unwrap(), deltas.len() as i64);
    }

    #[tokio::test]
    async fn retract_closes_the_live_interval_and_keeps_history() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        store.commit(&[(10, 1), (20, 1)]).await.unwrap();
        store.commit(&[(10, -1)]).await.unwrap();
        assert_eq!(store.live().await.unwrap(), 1);
        assert_eq!(store.total_rows().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn readd_opens_a_new_interval_after_a_close() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        store.commit(&[(10, 1)]).await.unwrap();
        store.commit(&[(10, -1)]).await.unwrap();
        store.commit(&[(10, 1)]).await.unwrap();
        assert_eq!(store.live().await.unwrap(), 1);
        assert_eq!(store.total_rows().await.unwrap(), 2);
    }

    // `commit_statement_count_is_constant` lives in tests/stmt_count.rs (its own
    // process): stmt_counter is a process-global atomic, so an exact-count assert
    // races the cascade/reach/reconcile tests in the shared lib binary, and a
    // failed assert here panics while holding statement_lock — poisoning it and
    // cascading into every other temporal test.

    #[tokio::test]
    async fn digest_matches_the_expected_live_key_mix() {
        let _statement_guard = statement_lock().lock().unwrap();
        let store = open().await;
        store.commit(&[(10, 1), (20, 1), (30, 1)]).await.unwrap();
        store.commit(&[(20, -1), (40, 1)]).await.unwrap();
        let expected = [10, 30, 40]
            .into_iter()
            .fold(0, |digest, key| digest ^ mix_key(key));
        assert_eq!(store.digest().await.unwrap(), expected);
    }
}

}
