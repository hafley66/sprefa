//! TEST df node identity as the WIRE spells it, on the `src/dispatch.rs` shape.
//! FAIL-PRE-FIX: with `len: 0` df spans and a kindless `FlatFact::Edge`, all
//! three cases go red -- every span is zero-width, `call_res`+`ret` and
//! `var_read`+`ret` collapse onto one offset each, and the collapsed pair reads
//! as a 2-cycle in a graph that is a DAG in the extractor's own node vec.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde_json::Value;
use sprefa_extract::{dispatch, flatten_jsonl, FamilyMask};

/// `src/dispatch.rs` verbatim, minus its imports: one call whose value is a
/// method receiver, a closure whose body is a call, and two implicit returns.
const SOURCE: &str = r#"pub fn dispatch(path: &str, content: &[u8], mask: FamilyMask) -> Option<ExtractOutput> {
    source_for(path).map(|src| src.extract(path, content, mask))
}
"#;

const DF_ONLY: FamilyMask = FamilyMask {
    cst: false,
    types: false,
    call: false,
    df: true,
};

fn df_facts() -> Vec<Value> {
    let out = dispatch("dispatch.rs", SOURCE.as_bytes(), DF_ONLY).expect("a Source matches .rs");
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
fn df_nodes_carry_their_full_extent() {
    let zero_width: Vec<String> = df_facts()
        .iter()
        .filter(|fact| fact["record"] == "node")
        .filter(|fact| fact["span"]["start"] == fact["span"]["end"])
        .map(|fact| fact.to_string())
        .collect();
    assert!(
        zero_width.is_empty(),
        "zero-width df node spans: {zero_width:?}"
    );
}

/// The declared identity is `(span, kind)`; an edge that names only spans hands
/// the consumer half of it.
#[test]
fn wire_edges_carry_endpoint_kinds() {
    let facts = df_facts();
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
    assert!(!edges.is_empty(), "the shape emits value edges");
    for edge in edges {
        for side in ["from", "to"] {
            let endpoint = endpoint(edge, side);
            assert!(
                !endpoint.2.is_empty(),
                "edge {edge:?} names no {side}_kind, so its endpoint is not identifiable"
            );
            assert!(
                nodes.contains(&endpoint),
                "edge endpoint {endpoint:?} matches no node fact"
            );
        }
    }
}

/// The repro: `source_for(path)` and the fn's implicit `ret` both start at the
/// tail expression, and the receiver edge plus the tail edge close a 2-cycle
/// the moment those two nodes merge. Value flow inside one callable is a DAG.
#[test]
fn wire_edge_graph_of_the_dispatch_shape_is_acyclic() {
    let facts = df_facts();
    let mut adjacency: HashMap<(u64, u64, String), BTreeSet<(u64, u64, String)>> = HashMap::new();
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
    assert!(cycle.is_none(), "df edge cycle: {:?}", cycle.expect("some"));
}

/// FAIL-PRE-FIX: an empty Rust statement (bare `;`) parses as
/// `Stmt::Expr(Expr::Verbatim(<empty>), Some(semi))`; the `_ =>` fallback in
/// flow_expr mints an Expr node whose empty TokenStream span resolves to (0,0).
/// The smallest offender from rust-analyzer (nocontentexpr.rs) emits 12 such
/// nodes: 12 zero-width spans and 12 duplicate (span,kind) keys at (0,0,'expr').
const EMPTY_STMT_SOURCE: &str = r#"fn foo(){ ;;;some_expr();;;;{;;;};;;;Ok(()) }"#;

fn empty_stmt_df_facts() -> Vec<Value> {
    let out = dispatch("nocontentexpr.rs", EMPTY_STMT_SOURCE.as_bytes(), DF_ONLY)
        .expect("a Source matches .rs");
    flatten_jsonl(&out)
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).expect("a flat fact is JSON"))
        .filter(|fact| fact["family"] == "df")
        .collect()
}

/// Empty statements must not collapse to a shared zero-width placeholder node.
#[test]
fn empty_statements_mint_no_zero_width_nodes() {
    let facts = empty_stmt_df_facts();
    let zero_width: Vec<String> = facts
        .iter()
        .filter(|fact| fact["record"] == "node")
        .filter(|fact| fact["span"]["start"] == fact["span"]["end"])
        .map(|fact| fact.to_string())
        .collect();
    assert!(
        zero_width.is_empty(),
        "zero-width df node spans from empty statements: {zero_width:?}"
    );
}

/// Every empty statement collapsing onto (0,0,'expr') must be gone: the node
/// key set has no duplicate (span,kind) pairs.
#[test]
fn empty_statements_mint_no_duplicate_node_keys() {
    let facts = empty_stmt_df_facts();
    let nodes: Vec<(u64, u64, String)> = facts
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
    let unique: HashSet<(u64, u64, String)> = nodes.iter().cloned().collect();
    assert_eq!(
        nodes.len(),
        unique.len(),
        "duplicate (span,kind) node keys from empty statements"
    );
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
