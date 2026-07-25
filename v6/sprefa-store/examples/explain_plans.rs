//! EXPLAIN QUERY PLAN for EVERY DML statement the cascade executes — the full
//! plan set, nothing summarized. Populates a realistic cyclic graph + the working
//! tables so the planner sees the same shapes it does at runtime (we never ANALYZE,
//! so these plans match the real prepared plans). Each statement is labelled with
//! the function + phase it comes from, and flagged if it contains a nested subquery.
//!
//!   cargo run --release --example explain_plans

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, Statement};
use sprefa_store::{benchgraph, cascade};

async fn explain(db: &sea_orm::DatabaseConnection, label: &str, has_subquery: bool, sql: &str) {
    let flag = if has_subquery { "  [NESTED SUBQUERY]" } else { "" };
    println!("\n── {label}{flag}");
    // print the SQL compactly
    let compact = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    println!("   SQL: {compact}");
    let rows = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!("EXPLAIN QUERY PLAN {sql}"),
        ))
        .await
        .unwrap();
    for r in &rows {
        let detail: String = r.try_get_by_index::<String>(3).unwrap_or_default();
        println!("   PLAN: {detail}");
    }
}

#[tokio::main]
async fn main() {
    let path = std::env::temp_dir().join(format!("explain_plans_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let db = Database::connect(opt).await.unwrap();
    db.execute_unprepared(sprefa_store::unfuck_sqlite::OPEN_PRAGMAS).await.unwrap();
    let ns = sprefa_store::relstore::GraphNs::default();
    cascade::create_schema(&db, &ns).await.unwrap();

    // realistic populated graph (cyclic so the shapes are the hard case) + working tables.
    let g = benchgraph::gen_multi_cyclic(6, 20_000, 7);
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    cascade::insert_rows(&db, &ns, &rows).await.unwrap();
    cascade::insert_deps(&db, &ns, &deps).await.unwrap();
    // seed the working tables so the planner sees non-empty temp relations.
    db.execute_unprepared(
        "INSERT INTO cx_frontier SELECT key FROM cx_row LIMIT 500;
         INSERT INTO cx_next SELECT key FROM cx_row LIMIT 500;
         INSERT INTO cx_cone SELECT key FROM cx_row LIMIT 500;
         INSERT INTO cx_hits SELECT key, 1 FROM cx_row LIMIT 500;",
    )
    .await
    .unwrap();

    println!("EXPLAIN QUERY PLAN — every DML statement in the cascade (no ANALYZE, matches runtime)");
    println!("graph: nodes={} edges={}  |  legend: SCAN=full table read, SEARCH ... USING = index seek", g.rows.len(), g.edges.len());

    // ===================== counting retract() =====================
    println!("\n========== retract() — counting Z-set (correct on DAGs) ==========");
    explain(&db, "retract seed decrement", false,
        "UPDATE cx_row SET weight = weight - 1 WHERE key IN (1,2,3)").await;
    explain(&db, "retract seed frontier fill", false,
        "INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN (1,2,3) AND weight <= 0").await;
    explain(&db, "retract round: hits = children + lost-support count", false,
        "INSERT INTO cx_hits(key,dec) SELECT d.child_key, count(*) FROM cx_frontier f \
         CROSS JOIN cx_dep d ON d.parent_key = f.key GROUP BY d.child_key").await;
    explain(&db, "retract round: decrement each hit child", true,
        "UPDATE cx_row SET weight = weight - (SELECT dec FROM cx_hits h WHERE h.key = cx_row.key) \
         WHERE key IN (SELECT key FROM cx_hits)").await;
    explain(&db, "retract round: next = crossed-zero children", false,
        "INSERT INTO cx_next(key) SELECT h.key FROM cx_hits h CROSS JOIN cx_row r ON r.key = h.key \
         WHERE r.weight <= 0 AND r.weight + h.dec > 0").await;
    explain(&db, "retract round: frontier <- next", false,
        "INSERT INTO cx_frontier SELECT key FROM cx_next").await;

    // ===================== assert() =====================
    println!("\n========== assert() — forward add ==========");
    explain(&db, "assert seed frontier", false,
        "INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN (1,2,3)").await;
    explain(&db, "assert round: next = dead children of frontier", false,
        "INSERT OR IGNORE INTO cx_next(key) SELECT d.child_key FROM cx_frontier f \
         CROSS JOIN cx_dep d ON d.parent_key = f.key CROSS JOIN cx_row r ON r.key = d.child_key \
         WHERE r.weight = 0").await;
    explain(&db, "assert mark alive", true,
        "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_next)").await;

    // ===================== retract_dred() over-delete =====================
    println!("\n========== retract_dred() — over-delete phase ==========");
    explain(&db, "od seed frontier (alive seeds)", false,
        "INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key IN (1,2,3) AND weight>0").await;
    explain(&db, "od kill frontier", true,
        "UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_frontier)").await;
    explain(&db, "od cone record", false,
        "INSERT INTO cx_cone SELECT key FROM cx_frontier").await;
    explain(&db, "od round: next = alive children of frontier (THE HOT JOIN)", false,
        "INSERT OR IGNORE INTO cx_next(key) SELECT d.child_key FROM cx_frontier f \
         CROSS JOIN cx_dep d ON d.parent_key = f.key CROSS JOIN cx_row r ON r.key = d.child_key \
         WHERE r.weight > 0").await;
    explain(&db, "od round: kill next", true,
        "UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_next)").await;
    explain(&db, "od round: cone += next", false,
        "INSERT OR IGNORE INTO cx_cone SELECT key FROM cx_next").await;

    // ===================== retract_dred() rederive =====================
    println!("\n========== retract_dred() — rederive phase ==========");
    explain(&db, "rd base: cone nodes with a surviving parent (REVERSE JOIN)", false,
        "INSERT OR IGNORE INTO cx_frontier(key) SELECT c.key FROM cx_cone c \
         CROSS JOIN cx_dep d ON d.child_key = c.key CROSS JOIN cx_row p ON p.key = d.parent_key \
         WHERE p.weight > 0").await;
    explain(&db, "rd mark alive", true,
        "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_frontier)").await;
    explain(&db, "rd round: alive children still in cone", false,
        "INSERT OR IGNORE INTO cx_next(key) SELECT d.child_key FROM cx_frontier f \
         CROSS JOIN cx_dep d ON d.parent_key = f.key CROSS JOIN cx_row r ON r.key = d.child_key \
         CROSS JOIN cx_cone c ON c.key = d.child_key WHERE r.weight = 0").await;

    // ===================== retract_dred_cte() =====================
    // Lab hook (H4): DL_LAB_ANALYZE=1 runs ANALYZE first, so the CTE plans can be
    // diffed with and without sqlite_stat1 (the loop engines pin joins with CROSS
    // JOIN; these two statements use plain JOIN and are the only planner-free SQL).
    if std::env::var("DL_LAB_ANALYZE").map(|v| v == "1").unwrap_or(false) {
        db.execute_unprepared("ANALYZE;").await.unwrap();
        println!("\n========== ANALYZE RAN: plans below use sqlite_stat1 ==========");
    }
    println!("\n========== retract_dred_cte() — the two recursive CTEs ==========");
    explain(&db, "cte phase 1: over-delete cone walk", false,
        "INSERT INTO cx_cone(key) WITH RECURSIVE cone(key) AS ( \
            SELECT key FROM cx_row WHERE key IN (1,2,3) AND weight>0 \
            UNION \
            SELECT d.child_key FROM cone \
              JOIN cx_dep d ON d.parent_key = cone.key \
              JOIN cx_row r ON r.key = d.child_key \
             WHERE r.weight>0 ) \
         SELECT key FROM cone").await;
    explain(&db, "cte phase 2: rederive walk", false,
        "INSERT INTO cx_frontier(key) WITH RECURSIVE alive(key) AS ( \
            SELECT c.key FROM cx_cone c \
              JOIN cx_dep d ON d.child_key = c.key \
              JOIN cx_row p ON p.key = d.parent_key \
             WHERE p.weight>0 \
            UNION \
            SELECT d.child_key FROM alive \
              JOIN cx_dep d ON d.parent_key = alive.key \
              JOIN cx_cone c ON c.key = d.child_key ) \
         SELECT key FROM alive").await;

    println!("\n(working-table DELETEs like `DELETE FROM cx_next` are unconditional whole-table clears — trivial, omitted.)");

    drop(db);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}
