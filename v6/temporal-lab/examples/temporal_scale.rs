//! RSS-flat curve: push live facts to the limit, one count per process (clean peak).
//!
//!   cargo run --release --example temporal_scale -- 10000000
//!
//! The claim under test: the fact engine's resident memory is O(working set), NOT
//! O(facts) — because the facts live on disk. If true, peak RSS barely moves as the
//! live set goes 1M -> 10M. Driven by examples/scale.sh.

use temporal_lab::{peak_rss_mb, TemporalStore};

fn main() -> rusqlite::Result<()> {
    let n: i64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
    let base = peak_rss_mb();
    let path = std::env::temp_dir().join(format!("temporal_scale_{n}.db"));
    let _ = std::fs::remove_file(&path);

    let mut store = TemporalStore::open(Some(path.to_str().unwrap()))?;
    let t = std::time::Instant::now();
    let mut k = 0i64;
    while k < n {
        let end = (k + 20_000).min(n);
        let batch: Vec<(i64, i64)> = (k..end).map(|x| (x, 1)).collect();
        store.commit(&batch)?;
        k = end;
    }
    let insert = t.elapsed();

    // one retract+readd wave to exercise close/rebirth, then a live-digest scan.
    let t = std::time::Instant::now();
    let d = store.live_digest()?;
    let scan = t.elapsed();

    let db_mb = std::fs::metadata(&path).map(|m| m.len() as f64 / 1e6).unwrap_or(0.0);
    println!(
        "n={:>9}  live {:>9}  insert {:>7.2?}  digest-scan {:>7.2?}  peakRSS {:>6.0} MB (base {:.0})  dbfile {:.0} MB  d=0x{:x}",
        n, store.live_count()?, insert, scan, peak_rss_mb(), base, db_mb, d as u64
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    Ok(())
}
