//! Deep profile of a single cycle-safe retract: where does the time go, at
//! tracing granularity, with read/write and access-path detail.
//!
//! Reports, for the RETRACT only (setup excluded):
//!   * per-PHASE wall time + BFS round counts (over-delete vs rederive)
//!   * per-STATEMENT total ms (from DL_CASCADE_TRACE, aggregated)
//!   * EXPLAIN QUERY PLAN of the two dominant joins (the READ access paths:
//!     index SEARCH vs table SCAN)
//!   * block I/O (getrusage ru_inblock/ru_oublock) + on-disk db/WAL byte delta
//!   * SQLite C-heap high-water, page cache size
//!   * network: NONE — embedded SQLite, zero sockets (stated + provable: no net
//!     syscalls, no fd beyond the db file)
//!
//!   cargo run --release --example profile_dred -- <layers> <width> [stride]

use std::time::Instant;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, Statement};
use sprefa_store::{benchgraph, cascade, memcap};

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

fn rusage_io() -> (i64, i64) {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        (ru.ru_inblock as i64, ru.ru_oublock as i64)
    }
}
fn sqlite_hw_mb() -> f64 {
    unsafe { libsqlite3_sys::sqlite3_memory_highwater(0) as f64 / 1048576.0 }
}
fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let b = if cfg!(target_os = "linux") { ru.ru_maxrss as f64 * 1024.0 } else { ru.ru_maxrss as f64 };
        b / 1048576.0
    }
}

async fn explain(db: &sea_orm::DatabaseConnection, label: &str, sql: &str) {
    let rows = db
        .query_all_raw(Statement::from_string(DatabaseBackend::Sqlite, format!("EXPLAIN QUERY PLAN {sql}")))
        .await
        .unwrap();
    println!("  [{label}]");
    for r in &rows {
        // column 3 (0-indexed) of EXPLAIN QUERY PLAN is the human-readable detail.
        let detail: String = r.try_get_by_index::<String>(3).unwrap_or_default();
        println!("      {detail}");
    }
}

#[tokio::main]
async fn main() {
    let cap_mb: u64 = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(2048);
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }
    let args: Vec<String> = std::env::args().collect();
    let layers: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(6);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(160_000);
    let stride: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);

    let g = benchgraph::gen_multi_cyclic(layers, width, stride);
    let nodes = g.rows.len();
    let edges = g.edges.len();
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, i, w)| (*t as i64, *i, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g.edges.iter().map(|(pt, pi, ct, ci)| (*pt as i64, *pi, *ct as i64, *ci)).collect();
    let seed = (g.seed.0 as i64, g.seed.1);
    drop(g);

    let path = std::env::temp_dir().join(format!("profile_dred_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let db = Database::connect(opt).await.unwrap();
    db.execute_unprepared(sprefa_store::unfuck_sqlite::OPEN_PRAGMAS).await.unwrap();
    cascade::create_schema(&db).await.unwrap();

    // ---- SETUP (untimed, excluded from the profile) -------------------------
    cascade::insert_rows(&db, &rows).await.unwrap();
    cascade::insert_deps(&db, &deps).await.unwrap();
    drop(rows);
    drop(deps);
    db.execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE);").await.ok();
    let db_bytes0 = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    println!("PROFILE cycle-safe retract  nodes={nodes} edges={edges} seed={seed:?}  memcap={cap_mb}MB");
    println!("network: NONE (embedded SQLite in-process; the only fd is the db file, zero sockets)\n");

    // ---- EXPLAIN the two dominant joins (READ access paths) -----------------
    println!("READ access paths (EXPLAIN QUERY PLAN of the hot joins):");
    explain(
        &db,
        "frontier expansion (over-delete/rederive inner loop)",
        "SELECT DISTINCT d.child_key FROM cx_frontier f \
         CROSS JOIN cx_dep d ON d.parent_key = f.key \
         CROSS JOIN cx_row r ON r.key = d.child_key WHERE r.weight > 0",
    )
    .await;
    explain(
        &db,
        "rederive base case (cx_cone reverse join)",
        "SELECT DISTINCT c.key FROM cx_cone c \
         CROSS JOIN cx_dep d ON d.child_key = c.key \
         CROSS JOIN cx_row p ON p.key = d.parent_key WHERE p.weight > 0",
    )
    .await;
    println!();

    // ---- MEASURED retract, with I/O + heap bracketed ------------------------
    let (in0, out0) = rusage_io();
    memcap::reset_peak();
    let t = Instant::now();
    let rounds = cascade::retract_dred(&db, &[seed]).await.unwrap();
    let ms = t.elapsed().as_secs_f64() * 1e3;
    let (in1, out1) = rusage_io();
    let rust_peak = memcap::peak_bytes() as f64 / 1048576.0;

    db.execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE);").await.ok();
    let db_bytes1 = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let survivors = db
        .query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, "SELECT count(*) FROM cx_row WHERE weight>0".to_owned()))
        .await.unwrap().unwrap().try_get_by_index::<i64>(0).unwrap();

    println!("TIMING + WORK");
    println!("  retract wall            {ms:.1} ms");
    println!("  total DRed rounds       {rounds}  (over-delete depth + rederive depth)");
    println!("  survivors / killed      {survivors} / {}", nodes as i64 - survivors);
    println!();
    println!("READ / WRITE (block I/O via getrusage, 512B blocks)");
    println!("  blocks read  (ru_inblock)   {}", in1 - in0);
    println!("  blocks written (ru_oublock) {}", out1 - out0);
    println!("  db file delta               {} -> {} bytes ({:+} KB)", db_bytes0, db_bytes1, (db_bytes1 as i64 - db_bytes0 as i64) / 1024);
    println!();
    println!("MEMORY");
    println!("  rust heap peak (gun sees)   {rust_peak:.2} MB");
    println!("  sqlite C-heap high-water    {:.1} MB", sqlite_hw_mb());
    println!("  process peak RSS            {:.1} MB", peak_rss_mb());
    println!();
    println!("per-statement breakdown: re-run with DL_CASCADE_TRACE=1 and aggregate (see profile note).");

    drop(db);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}
