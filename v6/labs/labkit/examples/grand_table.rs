//! The golden table: every engine, every scale, everything counted, plan snapshot,
//! declared-vs-measured Big-O, equivalence — under a 5 GB gun-to-head allocator.
//!
//!   cargo run --release --example grand_table

#[global_allocator]
static GLOBAL: labkit::gun::Gun = labkit::gun::Gun;

use labkit::{Experiment, Harness, RamZset, SalsaRows, SqliteTemporal};

fn main() {
    labkit::gun::install(5120); // 5 GB gun to the head of every Rust allocation

    let harness = Harness {
        scales: vec![100_000, 300_000, 1_000_000, 3_000_000],
        ticks: 200,
        edits_per_tick: 100,
        seed: 0xC0FFEE,
    };

    let exps: Vec<Box<dyn Experiment>> = vec![
        Box::new(RamZset::default()),
        Box::new(SqliteTemporal::default()),
        Box::new(SalsaRows::default()),
    ];

    harness.run(&labkit::live_set_workload(), exps);
}
