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

    // Output the survivor set as dense encoded ids (tag*STRIDE+id), so all three
    // engines emit byte-identical identifiers.
    let out = db
        .query_all_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!(
                "SELECT tag*{stride}+id AS key FROM cx_row WHERE weight > 0 ORDER BY key",
                stride = benchgraph::TAG_STRIDE
            ),
        ))
        .await
        .unwrap();
    let survivors = out.len();
    let killed = n - survivors;

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
    // Machine-parseable: engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb,db_mb
    eprintln!(
        "CSV,{engine},{n},{n_edges},{killed},{:.3},{:.3},{},{:.1},{:.1}",
        setup.as_secs_f64() * 1e3,
        retract.as_secs_f64() * 1e3,
        retract_stmts,
        rss,
        db_mb
    );

    drop(db);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}
