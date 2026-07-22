//! GraphStore Epic 1 answer at scale: split-vs-collapsed on-disk bytes.
//! `just storage`, or `cargo run --release --example storage_delta -- LAYERS WIDTH`.
//!
//! Streams the benchgraph layered-DAG shape straight into the tables — Rust heap
//! stays near zero even at multi-GB scale (the v6 law). Same rows + edges, same
//! monotonic keys, two table sets. Split = the live cx_/rx_ pair; Collapsed = one
//! g_node (carrying every plane's value column) + g_edge. The difference is the
//! dead-column tax vs the second table+index overhead.

use sprefa_store::measure::{measure_storage_scaled, parents_of};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (layers, width): (usize, usize) = match args.len() {
        3 => (args[1].parse().expect("layers"), args[2].parse().expect("width")),
        _ => (40, 40),
    };
    let n: i64 = 2 + (layers * width) as i64;
    let edges: i64 = (0..n).map(|g| parents_of(g, width).len() as i64).sum();

    let t = std::time::Instant::now();
    let delta = measure_storage_scaled(layers, width).await;
    let secs = t.elapsed().as_secs_f64();

    println!("shape     : layers={layers} width={width}  ->  {n} nodes, {edges} edges");
    println!("split     : {:>14} bytes", delta.split_bytes);
    println!("collapsed : {:>14} bytes", delta.collapsed_bytes);
    println!(
        "ratio     : collapsed/split = {:.3}   (delta {:+} bytes, {:+.1}%)",
        delta.collapsed_bytes as f64 / delta.split_bytes as f64,
        delta.delta(),
        delta.delta() as f64 / delta.split_bytes as f64 * 100.0,
    );
    println!("wall      : {secs:.1}s");
}
