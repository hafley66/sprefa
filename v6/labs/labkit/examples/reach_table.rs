//! The blast-radius (transitive closure) golden table — the real product query.
//! Recompute-from-scratch engines (ram, sqlite) show the scaling incrementality
//! exists to fix. Scales are node counts (a per-module call graph is hundreds of
//! functions); TC is O(V·E), so the shape shows fast even at modest V.
//!
//!   cargo run --release --example reach_table

#[global_allocator]
static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;

use labkit::{Experiment, Harness, RamReach, SqliteReach};

fn main() {
    labkit::gun::install(5120);

    let harness = Harness {
        scales: vec![100, 200, 400, 800],
        ticks: 20,
        edits_per_tick: 2, // one function's out-edges rewritten per edit
        seed: 0xB1A57,
    };

    let mut exps: Vec<Box<dyn Experiment>> = vec![
        Box::new(RamReach::default()),
        Box::new(SqliteReach::default()),
    ];
    // The resident incremental yardstick — only when built with --features with-dd.
    #[cfg(feature = "with-dd")]
    exps.push(Box::new(labkit::DdReach::default()));

    harness.run(&labkit::reach_workload(), exps);
}
