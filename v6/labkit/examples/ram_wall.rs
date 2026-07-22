//! Where does sqlite hit 2-3 GB of RAM? The bitemporal fact table (temporal-lab's
//! schema) built IN MEMORY, grown until peak RSS crosses 3 GB. Each scale runs in a
//! FRESH child process so getrusage RSS is isolated (the C allocator does not return
//! freed pages, so a single-process sweep would overcount later scales).
//!
//!   cargo run --release --example ram_wall
//!
//! The gun (5 GB) caps the RUST heap; sqlite's C heap is invisible to it, so this probe
//! reads getrusage RSS directly — the honest number for "how much RAM is sqlite using".

#[global_allocator]
static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;

use rusqlite::Connection;

fn peak_rss_mb() -> f64 {
    labkit::gun::peak_rss_mb()
}

/// Build an in-memory bitemporal fact table with `n` live rows; report peak RSS.
fn build_and_report(n: usize) {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "PRAGMA soft_heap_limit=4294967296;
         PRAGMA cache_size=-2000000;
         CREATE TABLE fact(
            key INTEGER NOT NULL, tt_from INTEGER NOT NULL, tt_to INTEGER,
            weight INTEGER NOT NULL, PRIMARY KEY(key, tt_from)) WITHOUT ROWID;",
    )
    .unwrap();
    conn.execute_batch("BEGIN").unwrap();
    {
        let mut ins = conn
            .prepare("INSERT INTO fact(key,tt_from,tt_to,weight) VALUES(?1,0,NULL,1)")
            .unwrap();
        for i in 0..n as i64 {
            ins.execute([i]).unwrap();
        }
    }
    conn.execute_batch("COMMIT").unwrap();
    // the live-row partial index temporal-lab carries (representative footprint)
    conn.execute_batch("CREATE INDEX ix_live ON fact(key) WHERE tt_to IS NULL").unwrap();

    let rows: i64 = conn.query_row("SELECT count(*) FROM fact", [], |r| r.get(0)).unwrap();
    let dbpages: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0)).unwrap();
    let pagesize: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap();
    let db_mb = (dbpages * pagesize) as f64 / (1024.0 * 1024.0);
    let rss = peak_rss_mb();
    // machine line the orchestrator parses:  DATA <rows> <rss_mb> <db_mb>
    println!("DATA {} {:.1} {:.1}", rows, rss, db_mb);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(n) = args.get(1).and_then(|s| s.parse::<usize>().ok()) {
        build_and_report(n);
        return;
    }

    labkit::gun::install(5120);
    let exe = std::env::current_exe().unwrap();
    let scales: [usize; 7] = [
        2_000_000, 5_000_000, 10_000_000, 20_000_000, 40_000_000, 60_000_000, 80_000_000,
    ];

    println!("sqlite RAM wall — in-memory bitemporal fact table, isolated child per scale");
    println!("  {:>12} {:>12} {:>12} {:>10}", "rows", "RSS MB", "db MB", "bytes/row");
    let mut json = String::from("[");
    for (i, &n) in scales.iter().enumerate() {
        let t = std::time::Instant::now();
        let out = std::process::Command::new(&exe).arg(n.to_string()).output().unwrap();
        let s = String::from_utf8_lossy(&out.stdout);
        let line = s.lines().find(|l| l.starts_with("DATA ")).unwrap_or("DATA 0 0 0");
        let f: Vec<&str> = line.split_whitespace().collect();
        let rows: f64 = f[1].parse().unwrap_or(0.0);
        let rss: f64 = f[2].parse().unwrap_or(0.0);
        let db: f64 = f[3].parse().unwrap_or(0.0);
        let bpr = if rows > 0.0 { rss * 1024.0 * 1024.0 / rows } else { 0.0 };
        println!(
            "  {:>12.0} {:>12.1} {:>12.1} {:>10.1}   ({:.1}s)",
            rows, rss, db, bpr, t.elapsed().as_secs_f64()
        );
        if i > 0 {
            json.push(',');
        }
        json.push_str(&format!("{{\"rows\":{rows},\"rss\":{rss},\"db\":{db}}}"));
        if rss > 3072.0 {
            println!("\n>>> crossed 3 GB at {:.0} rows ({:.2} GB RSS). stopping.", rows, rss / 1024.0);
            break;
        }
    }
    json.push(']');
    std::fs::write("ram_wall.json", &json).unwrap();
    println!("\nwrote ram_wall.json ({} bytes) for the chart.", json.len());
}
