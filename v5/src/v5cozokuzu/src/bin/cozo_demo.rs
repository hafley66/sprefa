//! Cozo: embeddable Datalog + built-in graph algorithms, written in Rust.
//! We load the call graph, run transitive reach as a recursive rule (compare
//! to your dl `reaches`), then run PageRank as a built-in "fixed rule" — the
//! thing your dl engine does NOT have. The algorithm's output is just a relation.

use std::collections::BTreeMap;
use cozo::{DbInstance, ScriptMutability};
use v5cozokuzu::EDGES;

fn show(label: &str, rows: &cozo::NamedRows) {
    println!("{label}");
    for row in &rows.rows {
        let cells: Vec<String> = row.iter().map(|v| format!("{v:?}")).collect();
        println!("  {}", cells.join("\t"));
    }
    println!();
}

fn main() {
    // "mem" engine = in-memory. Swap to "sqlite" + a path for a durable file,
    // which is the backend that ties Cozo back into your facts-in-SQLite world.
    let db = DbInstance::new("mem", "", Default::default()).expect("open cozo");

    // a stored relation calls(caller, callee)
    db.run_script(
        ":create calls {caller: String, callee: String}",
        BTreeMap::new(),
        ScriptMutability::Mutable,
    ).unwrap();

    // load the edges
    let tuples: Vec<String> = EDGES.iter().map(|(a, b)| format!("['{a}', '{b}']")).collect();
    let put = format!("?[caller, callee] <- [{}]\n:put calls {{caller, callee}}", tuples.join(", "));
    db.run_script(&put, BTreeMap::new(), ScriptMutability::Mutable).unwrap();

    // transitive reach from main — a recursive Datalog rule, same shape as dl
    let reach = r#"
        reach[n] := *calls{caller: "main", callee: n}
        reach[n] := reach[m], *calls{caller: m, callee: n}
        ?[n] := reach[n]
    "#;
    let r = db.run_script(reach, BTreeMap::new(), ScriptMutability::Immutable).unwrap();
    show("reach from main (recursive rule):", &r);

    // PageRank — a built-in algorithm invoked as a fixed rule. dl can't express this.
    let pr = "?[node, rank] <~ PageRank(*calls[caller, callee])";
    match db.run_script(pr, BTreeMap::new(), ScriptMutability::Immutable) {
        Ok(r) => show("pagerank (built-in algorithm operator):", &r),
        Err(e) => println!("pagerank skipped (syntax differs in this cozo version): {e}\n"),
    }

    // shortest path is likewise a built-in (ShortestPathDijkstra / BFS).
}
