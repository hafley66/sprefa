//! Deterministic evidence for the reverse-traversal index claim behind
//! `declare.rs`'s `traversal_edge_cols` (storage-diet, 2026-07-20): a
//! recursive CTE walking a graph edge relation BACKWARD, with no index on
//! the reverse column, makes SQLite build a throwaway `AUTOMATIC COVERING
//! INDEX` per query instead of seeking a real one. First measured by hand
//! against a copy of the live root's `rel_map_edge` (139,709 rows) and
//! independently by the `graph-libs:sqlite-native:opus-redo` research arc
//! against `rel_df_edge` (~4,000x, 160ms unindexed vs 0.04ms indexed). Hand
//! timing against a live db is not a repeatable receipt — this file is the
//! repeatable one: a fixed, seeded fixture, no live-db dependency, no
//! wall-clock dependence in what gets ASSERTED.
//!
//! Two things are asserted, and they are asserted separately because they
//! have different reliability:
//!   - the QUERY PLAN text (deterministic: SQLite's plan choice for a fixed
//!     schema, fixed query, and fixed row shape does not vary run to run on
//!     a given SQLite build) — `AUTOMATIC COVERING INDEX` appears with no
//!     index on the reverse column and disappears once one exists;
//!   - the TIMING ratio is measured and printed as an advisory receipt
//!     (`[advisory]` in the output) but NOT asserted against a numeric
//!     floor: wall-clock timing on a shared CI runner is noisy enough that a
//!     hard threshold is a flaky test waiting to happen, and the plan
//!     assertion above already proves the mechanism (SQLite itself says it
//!     is building a throwaway index) without needing a stopwatch. The one
//!     timing invariant asserted IS reliable regardless of machine noise:
//!     the indexed run cannot be slower than the unindexed run by more than
//!     a trivial margin, which would indicate the fixture stopped
//!     exercising the mechanism at all.
//!
//! Fixture: a 3-column ROWID table shaped like the production edge rels
//! (`rel_map_edge`/`rel_bom_edge`: `src, dst, kind`, full-row PRIMARY KEY,
//! not `WITHOUT ROWID` — no `.dl`-parsed decl ever gets the `pk_never_null`
//! vouch `wants_without_rowid` requires). `NBUCKETS` destinations each
//! receive `FANIN` distinct sources, so a reverse walk from one destination
//! has a real, non-trivial (`FANIN`-sized) result set — a bushy fan-in, not
//! a chain (a 1-in-degree chain does not reproduce the automatic-index plan
//! at any size tried; see the `low_fanin_..` note on the fixture fn). Row
//! count and structure are both fixed constants, not randomized.

use rusqlite::Connection;
use std::time::Instant;

/// Destinations. Each gets `FANIN` distinct sources: `NBUCKETS * FANIN`
/// total rows. 200 * 50 = 10,000 rows reproduces the automatic-index plan
/// reliably (checked down to 2,500 rows by hand; kept well above that floor
/// so the fixture is not living right at the edge of a SQLite version's
/// planner heuristics).
const NBUCKETS: i64 = 200;
const FANIN: i64 = 50;

/// Builds the fixture table `edge(src, dst, kind)` with `NBUCKETS * FANIN`
/// rows: a bushy fan-in graph, NOT a chain. A 1-in-degree chain (`i -> i+1`)
/// was tried first and never produces `AUTOMATIC COVERING INDEX` in this
/// SQLite build at any row count tried up to 100,000 — each reverse step
/// matches at most one row, which the planner apparently costs as too cheap
/// to bother building a temp index for. A real fan-in (many sources per
/// destination, matching how `rel_map_edge`'s busiest destinations look on
/// the live root: up to ~1,366 sources for one node) is what triggers it.
fn build_fixture(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE edge (src INTEGER, dst INTEGER, kind INTEGER, PRIMARY KEY(src, dst, kind))",
    )
    .unwrap();
    conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    {
        let mut stmt = conn.prepare("INSERT INTO edge VALUES (?1, ?2, 1)").unwrap();
        let mut src = 0i64;
        for dst in 0..NBUCKETS {
            for _ in 0..FANIN {
                src += 1;
                stmt.execute(rusqlite::params![src, dst]).unwrap();
            }
        }
    }
    conn.execute_batch("COMMIT").unwrap();
}

/// One reverse hop from a fixed, deterministic seed (the middle bucket):
/// walks `dst = seed` back to its `FANIN` sources via a recursive CTE, the
/// same join shape a backward `port_of_reach_rec`/`mark_reach`-style rule
/// or a find-refs point query compiles to.
const REVERSE_WALK_SQL: &str = "
WITH RECURSIVE walk(node, depth) AS (
  SELECT ?1, 0
  UNION
  SELECT e.src, w.depth + 1 FROM walk w JOIN edge e ON e.dst = w.node WHERE w.depth < 1
)
SELECT count(*) FROM walk";

fn explain_plan(conn: &Connection, seed: i64) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!("EXPLAIN QUERY PLAN {REVERSE_WALK_SQL}"))
        .unwrap();
    stmt.query_map([seed], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn run_walk(conn: &Connection, seed: i64) -> i64 {
    conn.query_row(REVERSE_WALK_SQL, [seed], |row| row.get(0))
        .unwrap()
}

#[test]
fn reverse_walk_without_index_builds_an_automatic_covering_index() {
    let conn = Connection::open_in_memory().unwrap();
    build_fixture(&conn);
    let seed = NBUCKETS / 2;

    let plan = explain_plan(&conn, seed);
    let plan_text = plan.join("\n");
    assert!(
        plan_text.contains("AUTOMATIC COVERING INDEX"),
        "with no index on `dst`, SQLite must fall back to building a \
         throwaway automatic covering index for the reverse join — this is \
         the exact mechanism the reverse-traversal claim rests on. Plan:\n{plan_text}"
    );

    // Sanity: the walk actually reaches the expected fan-in.
    let reached = run_walk(&conn, seed);
    assert_eq!(reached, 1 + FANIN, "seed + its FANIN sources");
}

#[test]
fn reverse_walk_with_index_avoids_the_automatic_covering_index() {
    let conn = Connection::open_in_memory().unwrap();
    build_fixture(&conn);
    conn.execute_batch("CREATE INDEX idx_edge_dst ON edge(dst)")
        .unwrap();
    let seed = NBUCKETS / 2;

    let plan = explain_plan(&conn, seed);
    let plan_text = plan.join("\n");
    assert!(
        !plan_text.contains("AUTOMATIC COVERING INDEX"),
        "a real index on `dst` must remove the automatic-index fallback \
         entirely. Plan:\n{plan_text}"
    );
    assert!(
        plan_text.contains("idx_edge_dst"),
        "the plan must name the real index it used instead. Plan:\n{plan_text}"
    );

    let reached = run_walk(&conn, seed);
    assert_eq!(reached, 1 + FANIN);
}

/// Timing is reported, not asserted against a numeric ratio floor (see the
/// file header): wall-clock noise on a shared CI runner makes a hard
/// threshold flaky in a way the plan assertions above are not. The one
/// bound asserted here is a sanity check that would only fail if the
/// fixture stopped exercising the mechanism (e.g. a SQLite version change
/// that no longer builds the automatic index for this shape) — in which
/// case the plan tests above already fail first and more informatively.
#[test]
fn reverse_walk_timing_is_reported_as_advisory_evidence() {
    let without_index = Connection::open_in_memory().unwrap();
    build_fixture(&without_index);
    let with_index = Connection::open_in_memory().unwrap();
    build_fixture(&with_index);
    with_index
        .execute_batch("CREATE INDEX idx_edge_dst ON edge(dst)")
        .unwrap();
    let seed = NBUCKETS / 2;

    // Warm-up runs so both connections' page caches and query plans are hot
    // before the measured runs — the receipt is about the index, not about
    // cold-cache disk IO.
    for _ in 0..3 {
        run_walk(&without_index, seed);
        run_walk(&with_index, seed);
    }

    const ROUNDS: u32 = 50;
    let t0 = Instant::now();
    for _ in 0..ROUNDS {
        run_walk(&without_index, seed);
    }
    let without_index_elapsed = t0.elapsed();

    let t0 = Instant::now();
    for _ in 0..ROUNDS {
        run_walk(&with_index, seed);
    }
    let with_index_elapsed = t0.elapsed();

    eprintln!(
        "[advisory, not asserted] reverse walk over {ROUNDS} rounds: \
         without index {:.3}ms/query, with index {:.3}ms/query, ratio {:.1}x \
         (task-level claim, independently measured against the live \
         rel_map_edge/rel_df_edge data: ~4,000x — see \
         i:graph-libs:sqlite-native:opus-redo H3b)",
        without_index_elapsed.as_secs_f64() * 1000.0 / ROUNDS as f64,
        with_index_elapsed.as_secs_f64() * 1000.0 / ROUNDS as f64,
        without_index_elapsed.as_secs_f64() / with_index_elapsed.as_secs_f64().max(1e-9),
    );

    // Sanity floor only: the automatic-index build is strictly extra work
    // on top of what the indexed run does, so it cannot be faster. A
    // numeric ratio assertion (e.g. ">100x") is deliberately NOT here.
    assert!(
        without_index_elapsed >= with_index_elapsed,
        "the unindexed run (which pays for building a throwaway index on \
         every single query) must not be faster than the indexed run: \
         without={without_index_elapsed:?} with={with_index_elapsed:?}"
    );
}
