//! Alternative to the weight cascade: recompute the survivor set FROM SCRATCH
//! with a single `WITH RECURSIVE` reachability CTE (reachable from the surviving
//! roots). This is the recompute baseline — O(survivors), not O(delta) — to
//! measure against the delta-proportional cascade in sqlite_reach.
//! stdout = sorted encoded survivor ids (byte-identical to sqlite_reach).
//! `cargo run --release --example sqlite_recursive -- <layers> <width>`

use std::time::Instant;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, Statement};
use sprefa_store::{benchgraph, cascade, memcap};

#[global_allocator]
static GLOBAL: memcap::CappedAlloc = memcap::CappedAlloc;

fn peak_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let bytes = if cfg!(target_os = "linux") { ru.ru_maxrss as f64 * 1024.0 } else { ru.ru_maxrss as f64 };
        bytes / (1024.0 * 1024.0)
    }
}

#[tokio::main]
async fn main() {
    let cap_mb: u64 = std::env::var("DL_MEMCAP_MB").ok().and_then(|s| s.parse().ok()).unwrap_or(4096);
    if cap_mb != 0 { memcap::cap_address_space_mb(cap_mb); }

    let args: Vec<String> = std::env::args().collect();
    let layers: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8).clamp(1, 20);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20_000).clamp(1, 500_000);

    let g = benchgraph::gen_multi(layers, width);
    let n = g.rows.len();

    let db_path = std::env::temp_dir().join(format!("sqlite_rec_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&db_path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", db_path.display()));
    opt.max_connections(1).min_connections(1);
    let db = Database::connect(opt).await.unwrap();
    db.execute_unprepared(
        "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; \
         PRAGMA cache_size=-32000; PRAGMA mmap_size=0; PRAGMA temp_store=MEMORY;",
    ).await.unwrap();
    cascade::create_schema(&db).await.unwrap();

    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, id, w)| (*t as i64, *id, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pid, ct, cid)| (*pt as i64, *pid, *ct as i64, *cid)).collect();
    let n_edges = deps.len();
    let (seed_tag, seed_id) = (g.seed.0 as i64, g.seed.1);

    let t_setup = Instant::now();
    cascade::insert_rows(&db, &rows).await.unwrap();
    cascade::insert_deps(&db, &deps).await.unwrap();
    let setup = t_setup.elapsed();
    drop(rows);
    drop(deps);

    // ---- MEASURED: full recompute of the alive set via WITH RECURSIVE --------
    // alive = surviving roots (rows with no incoming edge, minus the retracted
    // seed) then everything reachable from them. This re-derives ALL survivors;
    // it does not know about `weight` — it IS the reachability recompute.
    let stride = benchgraph::TAG_STRIDE;
    let sql = format!(
        "WITH RECURSIVE alive(tag,id) AS (
            SELECT r.tag, r.id FROM cx_row r
             WHERE NOT EXISTS (SELECT 1 FROM cx_dep d WHERE d.child_tag=r.tag AND d.child_id=r.id)
               AND NOT (r.tag={seed_tag} AND r.id={seed_id})
            UNION
            SELECT d.child_tag, d.child_id
              FROM alive a JOIN cx_dep d ON d.parent_tag=a.tag AND d.parent_id=a.id
         )
         SELECT tag*{stride}+id AS key FROM alive ORDER BY key"
    );
    let t = Instant::now();
    let out = db
        .query_all_raw(Statement::from_string(DatabaseBackend::Sqlite, sql))
        .await
        .unwrap();
    let recompute = t.elapsed();
    let survivors = out.len();

    let mut buf = String::new();
    for r in &out {
        buf.push_str(&r.try_get_by_index::<i64>(0).unwrap().to_string());
        buf.push('\n');
    }
    print!("{buf}");

    let rss = peak_rss_mb();
    eprintln!("[recursive] SETUP  nodes={n} edges={n_edges} | {:?}", setup);
    eprintln!(
        "[recursive] RECOMPUTE  survivors={survivors} killed={} | {:?} | 1 statement | peak_rss {:.1} MB",
        n - survivors, recompute, rss
    );
    eprintln!(
        "CSV,sqlite-recursive,{n},{n_edges},{},{:.3},{:.3},1,{:.1}",
        n - survivors, setup.as_secs_f64() * 1e3, recompute.as_secs_f64() * 1e3, rss
    );

    drop(db);
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}
