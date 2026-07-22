//! GraphStore Epic 1 answer: split-vs-collapsed on-disk bytes for one corpus.
//! `just storage`.
//!
//! Same rows + edges, same dense keys, two table sets. Split = the live cx_/rx_
//! pair; Collapsed = one g_node (carrying every plane's value column) + g_edge.
//! The difference is the dead-column tax vs the two-table overhead. This prints
//! the number; the decision to retarget (Epic 2) is the user's, not this binary's.

use sprefa_store::measure::{benchgraph, measure_storage};

#[tokio::main]
async fn main() {
    // 40 layers x 40 width ~ 1602 nodes across 3 relations, every edge crosses a
    // relation boundary, roots 0 and 1 with mixed support. Large enough that the
    // per-row overhead dominates the fixed schema headroom.
    let corpus = benchgraph::gen_multi(40, 40);
    let rows = corpus.rows.len();
    let edges = corpus.edges.len();
    let delta = measure_storage(&corpus).await;
    println!("corpus    : {rows} rows, {edges} edges");
    println!("split     : {:>12} bytes", delta.split_bytes);
    println!("collapsed : {:>12} bytes", delta.collapsed_bytes);
    println!("delta     : {:>+12} bytes  (collapsed - split; negative = collapsed wins)", delta.delta());
}
