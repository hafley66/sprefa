//! One question, one test: cut the anchor of a cycle, who gets it right?
//! Graph: 0->1, 1->2, 2->3, 3->1 (cycle 1-2-3). Cut 0->1. Correct: only {0} reachable.
//!   cargo run --release --example cycle_probe --features with-dd

use labkit::reach_dred::SqliteReachDRed;
use labkit::reach_inc::{reach_oracle, SqliteReachInc};
use std::collections::HashSet;

fn main() {
    let edges = [(0i64, 1i64), (1, 2), (2, 3), (3, 1)];
    let after: HashSet<(i64, i64)> = edges.iter().copied().filter(|&e| e != (0, 1)).collect();
    let (_, truth) = reach_oracle(&[0], &after);

    println!("cut 0->1 (anchor of cycle 1->2->3->1). correct reachable = {{0}} => count {truth}\n");
    println!("  {:<22} {:>10}  {}", "engine", "count after", "verdict");
    println!("  {:<22} {:>10}  {}", "RAM oracle (truth)", truth, "-");

    let mut c = SqliteReachInc::new();
    c.setup(&[0], &edges);
    c.del_edge(0, 1);
    let n = c.reachable().1;
    println!("  {:<22} {:>10}  {}", "sqlite counting", n, if n == truth { "correct" } else { "WRONG (phantom cycle)" });

    let mut d = SqliteReachDRed::new();
    d.setup(&[0], &edges);
    d.del_batch(&[(0, 1)]);
    let n = d.reachable().1;
    println!("  {:<22} {:>10}  {}", "sqlite DRed", n, if n == truth { "correct" } else { "WRONG" });

    #[cfg(feature = "with-dd")]
    {
        let mut dd = labkit::DdBfs::new();
        dd.setup(0, &edges);
        dd.del_edge(0, 1);
        let n = dd.reachable().1;
        println!("  {:<22} {:>10}  {}", "dd (differential)", n, if n == truth { "correct" } else { "WRONG" });
    }
    #[cfg(not(feature = "with-dd"))]
    println!("  {:<22} {:>10}  {}", "dd (differential)", "-", "rerun with --features with-dd");
}
