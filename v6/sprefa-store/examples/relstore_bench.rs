//! Harness for the GENERIC RelStore — measures the real, incubating store code (not a
//! bespoke copy). Builds a multi-relation graph, loads it through RelStore's generic
//! (rel,row) API, and measures the CYCLE-SAFE retraction at scale under the memcap gun.
//!
//!   cargo run --release --example relstore_bench -- <layers> <width>
//!
//! Reports rounds (= depth), retract ms, survivors, peak RSS, and on-disk db MB — the
//! same receipts as sqlite_reach.rs, now over the generic facade.

use std::time::Instant;

use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseBackend, Statement};
use sprefa_store::{benchgraph, memcap, relstore::RelStore};

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
    if cap_mb != 0 {
        memcap::cap_address_space_mb(cap_mb);
    }
    let args: Vec<String> = std::env::args().collect();
    let layers: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(8).clamp(1, 20);
    let width: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20_000).clamp(1, 500_000);

    let g = benchgraph::gen_multi(layers, width);
    let n = g.rows.len();
    let rows: Vec<(i64, i64, i64)> = g.rows.iter().map(|(t, id, w)| (*t as i64, *id, *w)).collect();
    let deps: Vec<(i64, i64, i64, i64)> =
        g.edges.iter().map(|(pt, pid, ct, cid)| (*pt as i64, *pid, *ct as i64, *cid)).collect();
    let n_edges = deps.len();
    let seed = (g.seed.0 as i64, g.seed.1);

    let path = std::env::temp_dir().join(format!("relstore_bench_{}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let store = RelStore::attach(Database::connect(opt).await.unwrap()).await.unwrap();

    let t_setup = Instant::now();
    store.add_rows(&rows).await.unwrap();
    store.add_deps(&deps).await.unwrap();
    let setup = t_setup.elapsed();

    // MEASURED: cycle-safe retraction of the seed through the generic store.
    let t = Instant::now();
    let rounds = store.retract_dred(&[seed]).await.unwrap();
    let retract = t.elapsed();
    let survivors = store.alive().await.unwrap();

    let db_mb = {
        let pc = store.conn().query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, "PRAGMA page_count".to_owned())).await.unwrap().unwrap().try_get_by_index::<i64>(0).unwrap();
        let ps = store.conn().query_one_raw(Statement::from_string(DatabaseBackend::Sqlite, "PRAGMA page_size".to_owned())).await.unwrap().unwrap().try_get_by_index::<i64>(0).unwrap();
        (pc * ps) as f64 / (1024.0 * 1024.0)
    };

    eprintln!("[relstore] SETUP     rels·rows={n} edges={n_edges} | {:?}", setup);
    eprintln!(
        "[relstore] RETRACT   rounds={rounds} survivors={survivors} killed={} | {:?} | peak_rss {:.1} MB | db {:.1} MB",
        n as i64 - survivors, retract, peak_rss_mb(), db_mb
    );
    eprintln!(
        "CSV,relstore-dred,{n},{n_edges},{},{:.3},{:.3},{rounds},{:.1}",
        n as i64 - survivors, setup.as_secs_f64() * 1e3, retract.as_secs_f64() * 1e3, peak_rss_mb()
    );

    drop(store);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}
