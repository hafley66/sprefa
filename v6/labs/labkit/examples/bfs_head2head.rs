//! The sqlite-dd head-to-head, shaped after differential-dataflow's own examples/bfs.rs:
//! single-source reachability from a root, with BATCHED edge inserts + deletes per round.
//! Engines maintain the reachable set incrementally; a RAM BFS recompute is the oracle
//! (and dd, when built with --features with-dd). Graph is a DAG (u<v) so the counting
//! model is sound. Tracks SQLite's OWN heap, not just the Rust gun.
//!
//!   cargo run --release --example bfs_head2head
//!   cargo run --release --example bfs_head2head --features with-dd

#[global_allocator]
static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;

use labkit::reach_inc::{reach_oracle, SqliteReachInc};
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

fn main() {
    gun::install(5120);
    sqlmem::reset_peak();

    let nodes: i64 = 20_000;
    let init_edges = 60_000usize;
    let rounds = 40usize;
    let batch = 200usize; // inserts AND deletes per round
    let root = 0i64;

    let mut rng = Lcg(0xB1A5E5);
    let mk_edge = |rng: &mut Lcg| -> (i64, i64) {
        // DAG: u < v (forward only), so counting-support reachability is sound.
        let u = rng.below(nodes - 1);
        let span = 1 + rng.below(nodes - 1 - u);
        (u, (u + span).min(nodes - 1))
    };

    // initial edge set (deduped)
    let mut live: HashSet<(i64, i64)> = HashSet::new();
    while live.len() < init_edges {
        live.insert(mk_edge(&mut rng));
    }
    let init_vec: Vec<(i64, i64)> = live.iter().copied().collect();

    println!("sqlite-dd head-to-head (dd's bfs.rs shape): {nodes} nodes, {} init edges, {rounds} rounds x {batch} ins+del", init_vec.len());
    println!("  root = {root}; graph is a DAG (u<v); incremental reachable set maintained under edge churn.\n");

    let mut sql = SqliteReachInc::new();
    sql.setup(&[root], &init_vec);

    #[cfg(feature = "with-dd")]
    let mut dd = {
        let mut d = labkit::DdBfs::new();
        d.setup(root, &init_vec);
        d
    };

    let (od, oc) = reach_oracle(&[root], &live);
    let (sd, sc) = sql.reachable();
    #[cfg(feature = "with-dd")]
    {
        let (dd_d, dd_c) = dd.reachable();
        println!("  setup:  oracle={oc} ({:x})  sqlite={sc} ({:x})  dd={dd_c} ({:x})  {}", od as u64, sd as u64, dd_d as u64,
            if od == sd && od == dd_d { "✓ all agree" } else { "✗ MISMATCH" });
    }
    #[cfg(not(feature = "with-dd"))]
    println!("  setup:  oracle reachable={oc} (digest {:x})   sqlite={sc} ({:x})  {}", od as u64, sd as u64, if od == sd { "✓" } else { "✗ MISMATCH" });

    let mut mism = 0u64;
    let t0 = std::time::Instant::now();
    let mut apply_ms = 0.0f64;
    for r in 0..rounds {
        // deletes: pick `batch` existing edges
        let dels: Vec<(i64, i64)> = live.iter().copied().take(batch).collect();
        // inserts: `batch` new DAG edges not currently live
        let mut adds: Vec<(i64, i64)> = Vec::with_capacity(batch);
        while adds.len() < batch {
            let e = mk_edge(&mut rng);
            if !live.contains(&e) && !adds.contains(&e) {
                adds.push(e);
            }
        }

        let t = std::time::Instant::now();
        for &(u, v) in &dels {
            sql.del_edge(u, v);
            live.remove(&(u, v));
        }
        for &(u, v) in &adds {
            sql.add_edge(u, v);
            live.insert((u, v));
        }
        apply_ms += t.elapsed().as_secs_f64() * 1000.0;

        #[cfg(feature = "with-dd")]
        dd.batch(&adds, &dels); // dd applies the whole round as ONE step (its natural mode)

        let (od, oc) = reach_oracle(&[root], &live);
        let (sd, sc) = sql.reachable();
        let mut ok = od == sd;
        #[cfg(feature = "with-dd")]
        let dd_sc = {
            let (dd_d, dd_c) = dd.reachable();
            ok = ok && od == dd_d;
            dd_c
        };
        #[cfg(not(feature = "with-dd"))]
        let dd_sc = 0u64;
        let _ = dd_sc;
        if !ok {
            mism += 1;
        }
        if r < 3 || r == rounds - 1 || !ok {
            #[cfg(feature = "with-dd")]
            println!("  round {r:>2}: oracle={oc:>6} sqlite={sc:>6} dd={dd_sc:>6}  {}", if ok { "✓" } else { "✗ MISMATCH" });
            #[cfg(not(feature = "with-dd"))]
            println!("  round {r:>2}: oracle={oc:>6} sqlite={sc:>6}  {}", if ok { "✓" } else { "✗ MISMATCH" });
        }
    }
    let wall = t0.elapsed().as_secs_f64() * 1000.0;

    println!("\n== result ==");
    println!("  rounds ✓: {}/{}   (mismatches {mism})", rounds - mism as usize, rounds);
    println!("  incremental apply: {:.1} ms total ({:.2} ms/round for {} edge ops)", apply_ms, apply_ms / rounds as f64, batch * 2);
    println!("  cascade rounds fired: {}   sql statements: {}", sql.rounds(), sql.statements());
    println!("  wall (incl. oracle recompute each round): {:.0} ms", wall);
    println!("\n== memory (the point: track SQLite's OWN heap, not just Rust) ==");
    println!("  SQLite heap   used {:.1} MB   peak {:.1} MB   (sqlite3_memory_highwater)", sqlmem::used_mb(), sqlmem::peak_mb());
    println!("  Rust heap     peak {:.1} MB   (the 5 GB gun)", gun::peak_mb());
    println!("  process RSS   peak {:.1} MB   (getrusage: Rust + SQLite C + page cache)", gun::peak_rss_mb());
}
