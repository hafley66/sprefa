//! Our side of the head-to-head: on-disk weight cascade in SQLite.
//! stdout = sorted surviving node ids (the answer bytes).
//! stderr = two sections: SETUP (untimed-as-headline, one-time cost) and the
//! MEASURED op (the incremental retract) with ops counts, not just wall time.
//! `cargo run --release --example sqlite_reach -- <layers> <width>`

use std::time::Instant;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, Statement};
use sprefa_store::{benchgraph, cascade, memcap, stmt_counter};

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

/// SQLite's OWN C-allocator high-water, in MB. This memory is malloc'd by the
/// bundled C library, NOT through Rust's #[global_allocator], so the memcap
/// counter is BLIND to it. Reading it here is the honest accounting: the wall
/// claim needs SQLite's C heap measured, not assumed bounded.
fn sqlite_highwater_mb() -> f64 {
    // reset=0: report the peak, don't clear it.
    let hw = unsafe { libsqlite3_sys::sqlite3_memory_highwater(0) };
    hw as f64 / (1024.0 * 1024.0)
}

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        // Linux reports ru_maxrss in kilobytes; macOS/BSD in bytes.
        let bytes = if cfg!(target_os = "linux") {
            (ru.ru_maxrss as f64) * 1024.0
        } else {
            ru.ru_maxrss as f64
        };
        bytes / (1024.0 * 1024.0)
    }
}

#[tokio::main]
async fn main() {
    // Self-cap heap. Default 4 GiB (rust-analyzer-ish), override DL_MEMCAP_MB
    // (0 disables). Turns a runaway into a clean abort, never a swap storm.
    let cap_mb: u64 = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(4096);
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }

    // DL_SQLITE_MODE = mem (":memory:", resident) | disk (on-file, paged).
    let disk = std::env::var("DL_SQLITE_MODE").map(|m| m != "mem").unwrap_or(true);
    let engine = if disk { "sqlite-disk" } else { "sqlite-mem" };

    let args: Vec<String> = std::env::args().collect();
    // Medium by default; clamp generously (max ~2M nodes) but not unbounded.
    let layers: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8).clamp(1, 20);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20_000).clamp(1, 500_000);

    // Multi-relation graph: 3 tagged relations, cross-relation edges. `(tag,id)`
    // is load-bearing (local ids collide across relations).
    let g = benchgraph::gen_multi(layers, width);
    let n = g.rows.len();

    // ---- SETUP (one-time; reported but NOT the measured number) -------------
    // disk: real on-file db with a BOUNDED 32 MB page cache (state paged to disk,
    //       evictable). mem: ":memory:" (whole db resident, like dd/dbsp).
    let db_path = std::env::temp_dir().join(format!("sqlite_reach_{}.sqlite", std::process::id()));
    // ONE connection: mandatory for `:memory:` (each connection is its own db)
    // and ideal for a single WAL writer. Avoids all pool round-robin surprises.
    let url = if disk {
        let _ = std::fs::remove_file(&db_path);
        format!("sqlite://{}?mode=rwc", db_path.display())
    } else {
        "sqlite::memory:".to_string()
    };
    let mut opt = ConnectOptions::new(url);
    opt.max_connections(1).min_connections(1);
    let db = Database::connect(opt).await.unwrap();
    // temp_store=MEMORY: the per-round GROUP BY temp b-tree is delta-sized (the
    // wavefront), so keeping it in RAM stays bounded and skips temp-file I/O.
    if disk {
        db.execute_unprepared(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; \
             PRAGMA cache_size=-32000; PRAGMA mmap_size=0; PRAGMA temp_store=MEMORY;",
        )
        .await
        .unwrap();
    } else {
        db.execute_unprepared("PRAGMA synchronous=OFF; PRAGMA temp_store=MEMORY;")
            .await
            .unwrap();
    }
    // Independent cap on SQLite's C heap (the memcap allocator cannot touch it).
    // DL_SQLITE_HEAP_MB > 0 sets PRAGMA hard_heap_limit: past it SQLite returns
    // SQLITE_NOMEM (a clean error, never swap). Completing a retract under a tight
    // limit PROVES SQLite's own footprint is bounded, not secretly ballooning.
    let heap_mb: u64 = std::env::var("DL_SQLITE_HEAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    if heap_mb != 0 {
        db.execute_unprepared(&format!("PRAGMA hard_heap_limit={};", heap_mb * 1024 * 1024))
            .await
            .unwrap();
    }
    cascade::create_schema(&db).await.unwrap();

    let rows: Vec<(i64, i64, i64)> =
        g.rows.iter().map(|(t, id, w)| (*t as i64, *id, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> = g
        .edges
        .iter()
        .map(|(pt, pid, ct, cid)| (*pt as i64, *pid, *ct as i64, *cid))
        .collect();
    let n_edges = deps.len();
    let seed = (g.seed.0 as i64, g.seed.1);

    let t_setup = Instant::now();
    cascade::insert_rows(&db, &rows).await.unwrap();
    cascade::insert_deps(&db, &deps).await.unwrap();
    let setup = t_setup.elapsed();
    let setup_stmts = stmt_counter::get();
    // SQLite C-heap peak for SETUP alone, then RESET the highwater (arg=1) so the
    // next read isolates the RETRACT's own C-heap from setup's.
    let setup_sqlite_hw = unsafe { libsqlite3_sys::sqlite3_memory_highwater(1) } as f64 / (1024.0 * 1024.0);

    // The corpus now lives on disk; drop the Rust-side staging so the measured
    // retract works against the db alone (this is the whole disk-resident point:
    // the engine does NOT keep the graph in RAM).
    drop(rows);
    drop(deps);

    // ---- MEASURED: the incremental retract ----------------------------------
    stmt_counter::reset();
    let t = Instant::now();
    let rounds = cascade::retract(&db, &[seed]).await.unwrap();
    let retract = t.elapsed();
    let retract_stmts = stmt_counter::get();

    // Snapshot memory HERE — right after the retract, BEFORE the survivor output
    // query materializes 4.5M rows (a reporting artifact that would otherwise
    // inflate rust_live). sqlite_highwater is a monotonic peak so it already
    // covers setup + retract; rust_live is instantaneous so it must be read now.
    let sqlite_hw = sqlite_highwater_mb();
    let rust_live = memcap::live_bytes() as f64 / (1024.0 * 1024.0);

    // Survivor COUNT is a scalar aggregate (one integer, tiny allocation) so the
    // reported killed/survivors is correct even when a C-heap cap is set — it is
    // NOT derived from materializing millions of rows. This is the honest engine
    // result: it lives on disk in cx_row.weight, read back with O(1) memory.
    let survivors = db
        .query_one_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT count(*) FROM cx_row WHERE weight > 0".to_string(),
        ))
        .await
        .unwrap()
        .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0))
        .unwrap_or(0) as usize;
    let killed = n - survivors;

    // Full survivor DUMP (stdout) for the byte-identical head-to-head. This is the
    // one memory-heavy step — it materializes every survivor id into a Rust Vec
    // (counted by memcap) — so it is done AFTER the memory snapshot above and is a
    // REPORTING artifact, not part of the measured retract. `key` IS the rowid and
    // equals tag*STRIDE+id (E2), so ORDER BY key is a no-sort ordered scan.
    let out = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT key FROM cx_row WHERE weight > 0 ORDER BY key".to_string(),
        ))
        .await
        .unwrap();
    let mut buf = String::new();
    for r in &out {
        buf.push_str(&r.try_get_by_index::<i64>(0).unwrap().to_string());
        buf.push('\n');
    }
    print!("{buf}");

    // On-disk footprint AFTER the retract (WAL folded via checkpoint). The space
    // axis: how many bytes the corpus occupies on disk, independent of RSS.
    let mut db_mb = 0.0f64;
    if disk {
        db.execute_unprepared("PRAGMA wal_checkpoint(TRUNCATE);").await.ok();
        let db_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
        let wal_bytes = std::fs::metadata(format!("{}-wal", db_path.display()))
            .map(|m| m.len())
            .unwrap_or(0);
        db_mb = (db_bytes + wal_bytes) as f64 / (1024.0 * 1024.0);
    }

    let rss = peak_rss_mb();
    eprintln!(
        "[{engine}] SETUP  nodes={n} edges={n_edges} | {:?} | {} stmts (build the corpus once)",
        setup, setup_stmts
    );
    eprintln!(
        "[{engine}] RETRACT  killed={killed} survivors={survivors} | {:?} | {} stmts, {} rounds \
         | peak_rss {:.1} MB | db {:.1} MB",
        retract, retract_stmts, rounds, rss, db_mb
    );
    // The memory split the memcap allocator alone cannot show: Rust live heap
    // (what memcap DOES cap) vs SQLite's C-allocator high-water (what it CANNOT).
    eprintln!(
        "[{engine}] MEMORY  rust_live {:.1} MB (memcap sees) | sqlite_c_heap setup {:.1} / \
         retract {:.1} MB (memcap BLIND) | peak_rss {:.1} MB",
        rust_live, setup_sqlite_hw, sqlite_hw, rss
    );
    // Machine-parseable: engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb,db_mb,sqlite_hw_mb
    eprintln!(
        "CSV,{engine},{n},{n_edges},{killed},{:.3},{:.3},{},{:.1},{:.1},{:.1}",
        setup.as_secs_f64() * 1e3,
        retract.as_secs_f64() * 1e3,
        retract_stmts,
        rss,
        db_mb,
        sqlite_hw
    );

    drop(db);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}
