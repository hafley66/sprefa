//! TEST df node identity as the WIRE spells it, on the Go and Kotlin `return
//! f()` shape. FAIL-PRE-FIX: go.rs/kotlin.rs `df_push` minted every value node
//! with `len: 0`, so every span is zero-width and a span-keyed consumer cannot
//! tell `call_res` from the `ret` that covers the same tail expression.
//!
//! (Mirror of tests/12_df_identity.rs for rust.rs. The `from_kind`/`to_kind`
//! edge arms are already populated by the shared flatten_df, so the acyclicity
//! and endpoint tests pass before AND after the span fix; the zero-width test is
//! the fail-first that proves the extent change.)

use std::collections::{BTreeSet, HashMap, HashSet};

use serde_json::Value;
use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask};

const GO_SOURCE: &str = r#"func f() int {
	return g()
}

func g() int {
	return 42
}
"#;

const KT_SOURCE: &str = r#"fun f(): Int {
    return g()
}

fun g(): Int {
    return 42
}
"#;

const DF_ONLY: FamilyMask = FamilyMask {
    cst: false,
    types: false,
    call: false,
    df: true,
    data: false,
};

fn df_facts(path: &str, source: &str) -> Vec<Value> {
    let out = dispatch(path, source.as_bytes(), DF_ONLY).expect("a Source matches the path");
    flatten_jsonl(&out)
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).expect("a flat fact is JSON"))
        .filter(|fact| fact["family"] == "df")
        .collect()
}

fn endpoint(fact: &Value, side: &str) -> (u64, u64, String) {
    let span = &fact[side];
    (
        span["start"].as_u64().expect("span start"),
        span["end"].as_u64().expect("span end"),
        fact[format!("{side}_kind")]
            .as_str()
            .unwrap_or("")
            .to_string(),
    )
}

/// A value node spans its expression. A zero-width span is an anchor, and two
/// anchors at one offset are one node to every span-keyed consumer.
#[test]
fn go_and_kotlin_df_nodes_carry_their_full_extent() {
    for (path, source) in [("sample.go", GO_SOURCE), ("sample.kt", KT_SOURCE)] {
        let zero_width: Vec<String> = df_facts(path, source)
            .iter()
            .filter(|fact| fact["record"] == "node")
            .filter(|fact| fact["span"]["start"] == fact["span"]["end"])
            .map(|fact| fact.to_string())
            .collect();
        assert!(
            zero_width.is_empty(),
            "[{path}] zero-width df node spans: {zero_width:?}"
        );
    }
}

/// Every edge endpoint names a node fact: the (span, kind) identity is the
/// declared contract, and the wire must hand the consumer both halves of it.
#[test]
fn go_and_kotlin_wire_edges_carry_endpoint_kinds() {
    for (path, source) in [("sample.go", GO_SOURCE), ("sample.kt", KT_SOURCE)] {
        let facts = df_facts(path, source);
        let nodes: HashSet<(u64, u64, String)> = facts
            .iter()
            .filter(|fact| fact["record"] == "node")
            .map(|fact| {
                (
                    fact["span"]["start"].as_u64().expect("span start"),
                    fact["span"]["end"].as_u64().expect("span end"),
                    fact["kind"].as_str().expect("node kind").to_string(),
                )
            })
            .collect();
        let edges: Vec<&Value> = facts
            .iter()
            .filter(|fact| fact["record"] == "edge")
            .collect();
        assert!(!edges.is_empty(), "[{path}] the shape emits value edges");
        for edge in edges {
            for side in ["from", "to"] {
                let ep = endpoint(edge, side);
                assert!(
                    !ep.2.is_empty(),
                    "[{path}] edge {edge:?} names no {side}_kind"
                );
                assert!(
                    nodes.contains(&ep),
                    "[{path}] edge endpoint {ep:?} matches no node fact"
                );
            }
        }
    }
}

/// The repro: the returned `f()`'s `call_res` and the `ret` that covers it
/// share a start, and the receiver/tail edges close a 2-cycle the moment a
/// span-only consumer merges them. Value flow inside one callable is a DAG.
#[test]
fn go_and_kotlin_wire_edge_graph_is_acyclic() {
    for (path, source) in [("sample.go", GO_SOURCE), ("sample.kt", KT_SOURCE)] {
        let facts = df_facts(path, source);
        let mut adjacency: HashMap<(u64, u64, String), BTreeSet<(u64, u64, String)>> =
            HashMap::new();
        for edge in facts.iter().filter(|fact| fact["record"] == "edge") {
            adjacency
                .entry(endpoint(edge, "from"))
                .or_default()
                .insert(endpoint(edge, "to"));
        }
        let mut seen = HashSet::new();
        let mut stack = Vec::new();
        let mut cycle = None;
        for root in adjacency.keys() {
            walk(root, &adjacency, &mut seen, &mut stack, &mut cycle);
        }
        assert!(cycle.is_none(), "[{path}] df edge cycle: {cycle:?}");
    }
}

type Node = (u64, u64, String);

fn walk(
    node: &Node,
    adjacency: &HashMap<Node, BTreeSet<Node>>,
    seen: &mut HashSet<Node>,
    stack: &mut Vec<Node>,
    cycle: &mut Option<Vec<Node>>,
) {
    if stack.contains(node) {
        let mut found = stack.clone();
        found.push(node.clone());
        *cycle = Some(found);
        return;
    }
    if !seen.insert(node.clone()) {
        return;
    }
    stack.push(node.clone());
    for next in adjacency.get(node).into_iter().flatten() {
        walk(next, adjacency, seen, stack, cycle);
    }
    stack.pop();
}
