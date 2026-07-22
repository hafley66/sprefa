//! CORE, batch form — the baseline that WORKS.
//!
//!   cargo run --example core_batch
//!
//! Borrowed `Hit<'r>` fan across rayon, project to owned `Edge` at the source, one
//! owned set comes back. No lifetime escapes the derive; the runtime that holds the
//! result is `struct { edges: BTreeSet<Edge> }` — no `'r`, nothing to infect.

use frp_lab::{derive_family_batch, File};

fn main() {
    let files = vec![
        File { path: "a.rs".into(), text: "main parse\nmain lower\nlower emit".into() },
        File { path: "b.rs".into(), text: "parse lex\nlex read".into() },
        File { path: "c.ts".into(), text: "handler validate\nvalidate main".into() },
    ];

    let edges = derive_family_batch(&files);

    println!("CORE batch — {} edges derived (rayon over {} files):", edges.len(), files.len());
    for e in &edges {
        println!("  {} -> {}", e.from, e.to);
    }

    // The runtime that owns this has NO lifetime parameter. That is the payoff.
    struct Runtime {
        edges: std::collections::BTreeSet<frp_lab::Edge>,
    }
    let rt = Runtime { edges };
    assert_eq!(rt.edges.len(), 7);
    println!("\nRuntime {{ edges: BTreeSet<Edge> }}  — no 'r, owned, done.");
}
