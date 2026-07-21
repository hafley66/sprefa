//! Print a tiny multi-relation instance so the `(tag, id)` design is concrete:
//! three relations, colliding local ids, cross-relation edges.
//! `cargo run --release --example dump_multi -- <per_rel>`

use sprefa_store::benchgraph;

fn name(tag: u32) -> &'static str {
    match tag {
        0 => "module",
        1 => "fn    ",
        2 => "type  ",
        _ => "?",
    }
}

fn main() {
    let layers: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(4);
    let width: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(2);
    let g = benchgraph::gen_multi(layers, width);

    println!(
        "rows  (tag, id, weight)   [rel0={} rel1={} rel2={}]",
        g.per_tag[0], g.per_tag[1], g.per_tag[2]
    );
    for (tag, id, w) in &g.rows {
        println!("  ({tag}, {id}, w={w})   {} {id}", name(*tag));
    }
    println!("\nedges  (parent_tag,parent_id) -> (child_tag,child_id)   [every edge crosses relations]");
    for (pt, pid, ct, cid) in &g.edges {
        println!("  ({pt},{pid}) -> ({ct},{cid})   {} {pid} supports {} {cid}", name(*pt), name(*ct));
    }
    println!("\nretract seed = (tag {}, id {})  = {} {}", g.seed.0, g.seed.1, name(g.seed.0), g.seed.1);
    println!("note: id 0 exists in ALL THREE relations — only (tag,id) tells them apart.");
}
