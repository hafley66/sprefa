//! Cycle-safe sqlite-dd (DRed) head-to-head, on a CYCLIC random graph (edges go any
//! direction, so cycles exist — the counting model would be wrong here). Compares
//! SqliteReachDRed vs a RAM BFS oracle vs dd, and SWEEPS scale to find where the
//! sequential per-round cost breaks. Tracks SQLite's own heap.
//!
//!   cargo run --release --example dred_head2head
//!   cargo run --release --example dred_head2head --features with-dd

#[global_allocator]
static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;

use labkit::reach_dred::SqliteReachDRed;
use labkit::reach_inc::reach_oracle;
use labkit::{gun, sqlmem};
use std::collections::HashSet;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 16
    }
    fn below(&mut self, n: i64) -> i64 {
        (self.next() % n as u64) as i64
    }
}

fn run_scale(label: &str, nodes: i64, init_edges: usize, rounds: usize, batch: usize, with_dd: bool) {
    let root = 0i64;
    let mut rng = Lcg(0xD5ED ^ nodes as u64);
    // CYCLIC: u and v both random and distinct — the graph has cycles.
    let mk_edge = |rng: &mut Lcg| -> (i64, i64) {
        let u = rng.below(nodes);
        let mut v = rng.below(nodes);
        if v == u {
            v = (v + 1) % nodes;
        }
        (u, v)
    };

    let mut live: HashSet<(i64, i64)> = HashSet::new();
    while live.len() < init_edges {
        live.insert(mk_edge(&mut rng));
    }
    let init_vec: Vec<(i64, i64)> = live.iter().copied().collect();

    sqlmem::reset_peak();
    gun::reset_peak();
    let mut sql = SqliteReachDRed::new();
    sql.setup(&[root], &init_vec);

    let mut dd = if with_dd {
        #[cfg(feature = "with-dd")]
        {
            let mut d = labkit::DdBfs::new();
            d.setup(root, &init_vec);
            Some(d)
        }
        #[cfg(not(feature = "with-dd"))]
        {
            None::<()>
        }
    } else {
        None
    };
    let _ = &mut dd;

    let (od0, _) = reach_oracle(&[root], &live);
    let (sd0, _) = sql.reachable();
    let setup_ok = od0 == sd0;

    let mut mism = 0u64;
    let mut apply_ms = 0.0f64;
    for _ in 0..rounds {
        let dels: Vec<(i64, i64)> = live.iter().copied().take(batch).collect();
        let mut adds: Vec<(i64, i64)> = Vec::with_capacity(batch);
        while adds.len() < batch {
            let e = mk_edge(&mut rng);
            if !live.contains(&e) && !adds.contains(&e) {
                adds.push(e);
            }
        }
        let t = std::time::Instant::now();
        sql.del_batch(&dels);
        for &e in &dels {
            live.remove(&e);
        }
        sql.add_batch(&adds);
        for &e in &adds {
            live.insert(e);
        }
        apply_ms += t.elapsed().as_secs_f64() * 1000.0;

        #[cfg(feature = "with-dd")]
        if let Some(d) = dd.as_mut() {
            d.batch(&adds, &dels);
        }

        let (od, _) = reach_oracle(&[root], &live);
        let (sd, _) = sql.reachable();
        let mut ok = od == sd;
        #[cfg(feature = "with-dd")]
        if let Some(d) = dd.as_ref() {
            ok = ok && od == d.reachable().0;
        }
        if !ok {
            mism += 1;
        }
    }

    let (_, reach_n) = sql.reachable();
    let pct = 100.0 * reach_n as f64 / nodes as f64;
    println!(
        "  {:<7} {:>8} {:>9} {:>9.2} {:>8} {:>5.0}% {:>9.1} {:>9.1} {:>6}",
        label,
        nodes,
        init_vec.len(),
        apply_ms / rounds as f64,
        reach_n,
        pct,
        sqlmem::peak_mb(),
        gun::peak_rss_mb(),
        if setup_ok && mism == 0 { "✓" } else { "✗" }
    );
}

fn main() {
    gun::install(5120);
    let with_dd = cfg!(feature = "with-dd");

    println!("cycle-safe sqlite-dd (DRed) — CYCLIC graph, dd's bfs.rs rhythm, 20 rounds x 100 ins+del");
    println!("  engines: SqliteReachDRed vs RAM oracle{}  (all cycle-capable)\n", if with_dd { " vs dd" } else { "" });
    println!(
        "  {:<7} {:>8} {:>9} {:>9} {:>8} {:>6} {:>9} {:>9} {:>6}",
        "density", "nodes", "edges", "ms/round", "reach", "reach%", "sqliteMB", "rssMB", "equiv"
    );

    // SPARSE (1.2 edges/node): shallow cones -> DRed stays delta-proportional as corpus grows.
    run_scale("sparse", 50_000, 60_000, 20, 100, with_dd);
    run_scale("sparse", 200_000, 240_000, 20, 100, with_dd);
    run_scale("sparse", 800_000, 960_000, 20, 100, with_dd);
    println!();
    // DENSE (3 edges/node): one giant SCC -> the over-delete cone ~= the whole reachable
    // set, so DRed cost rises with the corpus. This is where the on-disk engine breaks.
    run_scale("dense", 20_000, 60_000, 20, 100, with_dd);
    run_scale("dense", 80_000, 240_000, 20, 100, with_dd);
    run_scale("dense", 200_000, 600_000, 20, 100, with_dd);

    println!("\n  sparse: ms/round ~flat as corpus grows 16x = delta-proportional (cone stays small).");
    println!("  dense:  ms/round rises with corpus = the wavefront wall (over-delete cone ~= reachable set).");
    println!("  cycle-safe throughout (equiv vs oracle). reach% shows how much of the graph the root sees.");
}
