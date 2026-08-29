//! Defects the rust-analyzer entrypoint crawl measured
//! (`plans/extract-crawl-2026-08-29/rust.REPORT.md` section 7, kinks 3, 5, 6).
//! Fixtures and their expected/observed headers are in
//! `tests/fixtures/rust_findings/`.
//!
//! FAIL-FIRST RECEIPT, all five red before the fix (binary at c60e5c4cc):
//!   const_block_fns_mint_call_defs
//!     left: {}   right: {"inner", "outer"}
//!   const_block_call_resolves_to_its_sibling
//!     left: []   right: [Edge { caller: "outer", callee: "inner", site: 869,
//!                               kind: "name_resolve" }]
//!   initializer_calls_carry_the_const_or_static_item_as_caller
//!     left: []   right: ["ROW", "TABLE"]
//!   a_closure_caller_edge_mirrors_onto_the_enclosing_fn
//!     left: {"closure@1182"}   right: {"closure@1182", "entry"}
//!   one_mirror_edge_per_closure_caller_edge
//!     no mirror for Edge { caller: "closure@1014", callee: "spawn", site: 1017 }
//!     among 5 rows, 3 of them closure-caller
//!
//! SABOTAGE RECEIPT: deleting the `syn::Item::Const` arm from
//! `call_defs_in_items` restores rows 1 and 2; returning `false` from
//! `initializer_defs` without minting the item def restores row 3; dropping
//! the `enclosing_named_def` push in `Resolve<CallF>` restores rows 4 and 5.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;
use sprefa_extract::{FamilyMask, RustSource, Source};

const CONST_BLOCK_PATH: &str = "tests/fixtures/rust_findings/const_block_defs.rs";
const CONST_BLOCK: &[u8] = include_bytes!("fixtures/rust_findings/const_block_defs.rs");

/// One `resolved_edge` row, reduced to the four fields these cases grade on.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    caller: String,
    callee: String,
    site: u64,
    kind: String,
}

fn resolved_edges(paths: &[&str]) -> Vec<Edge> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(["--family", "call"])
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("stdout is UTF-8");
    let mut edges: Vec<Edge> = text
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a flat fact is JSON"))
        .filter(|fact| fact["record"] == "resolved_edge")
        .map(|fact| Edge {
            caller: fact["caller_name"].as_str().unwrap_or("").to_string(),
            callee: fact["callee_name"].as_str().unwrap_or("").to_string(),
            site: fact["caller_site_start"].as_u64().unwrap_or(0),
            kind: fact["kind"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    edges.sort();
    edges
}

/// Kink 5. A `const _: () = { .. }` block is where a derive macro puts whole
/// impl blocks and free fns; the def walker stopped at the item.
#[test]
fn const_block_fns_mint_call_defs() {
    let output = RustSource.extract(CONST_BLOCK_PATH, CONST_BLOCK, FamilyMask::ALL);
    let call = output.call.as_ref().expect("the call family is projected");
    let named: BTreeSet<&str> = call
        .nodes
        .iter()
        .filter_map(|node| node.name.map(|id| output.strings.lookup(id)))
        .collect();
    assert_eq!(named, BTreeSet::from(["inner", "outer"]));
}

/// Kink 5, the edge the defs unblock. `outer` covers the `inner()` site, so the
/// caller binding needs no const-item node here.
#[test]
fn const_block_call_resolves_to_its_sibling() {
    let edges = resolved_edges(&[CONST_BLOCK_PATH]);
    assert_eq!(
        edges,
        vec![Edge {
            caller: "outer".to_string(),
            callee: "inner".to_string(),
            site: 869,
            kind: "name_resolve".to_string(),
        }]
    );
}

/// Kink 6. A call directly in a `static` or `const` initializer sits outside
/// every fn, so the item itself is the caller.
#[test]
fn initializer_calls_carry_the_const_or_static_item_as_caller() {
    let edges = resolved_edges(&["tests/fixtures/rust_findings/static_init_call.rs"]);
    let callers: Vec<&str> = edges.iter().map(|edge| edge.caller.as_str()).collect();
    assert_eq!(callers, vec!["ROW", "TABLE"]);
    assert!(
        edges.iter().all(|edge| edge.callee == "helper"),
        "both initializer edges name helper: {edges:?}"
    );
}

/// Kink 6's other half: an item with no call in its initializer stays out of
/// the call plane, so the `const GREETING: &str = "hello"` shape mints no node
/// and the committed wire golden does not move.
#[test]
fn a_const_with_no_call_in_its_initializer_mints_no_call_def() {
    let source = b"pub const GREETING: &str = \"hello\";\nstatic ROOT: u32 = 7;\n";
    let output = RustSource.extract("scratch.rs", source, FamilyMask::ALL);
    let call = output.call.as_ref().expect("the call family is projected");
    assert!(
        call.nodes.is_empty(),
        "a call-free initializer minted {} call def(s)",
        call.nodes.len()
    );
}

/// Kink 3. A closure caller ends every walk, so the edge is ALSO emitted from
/// the innermost enclosing named def and a BFS over named defs passes through.
/// The closure edge stays: it is the only row that says where the call is.
#[test]
fn a_closure_caller_edge_mirrors_onto_the_enclosing_fn() {
    let edges = resolved_edges(&["tests/fixtures/rust_findings/closure_caller_chain.rs"]);
    let worker_callers: BTreeSet<&str> = edges
        .iter()
        .filter(|edge| edge.callee == "worker")
        .map(|edge| edge.caller.as_str())
        .collect();
    assert_eq!(worker_callers, BTreeSet::from(["closure@1182", "entry"]));
    assert_eq!(edges.len(), 3, "2 primary edges + 1 mirror: {edges:?}");
}

/// Kink 3's COUNT rail: exactly one mirror per closure-caller edge, never one
/// per closure and never one per named def. Three closures, one nested inside
/// another, so a mirror-per-closure-frame walk would over-count.
#[test]
fn one_mirror_edge_per_closure_caller_edge() {
    let edges = resolved_edges(&["tests/fixtures/rust_findings/closure_mirror_count.rs"]);
    let closure_edges: Vec<&Edge> = edges
        .iter()
        .filter(|edge| edge.caller.starts_with("closure@"))
        .collect();
    let mirrors: Vec<&Edge> = closure_edges
        .iter()
        .map(|closure| {
            edges
                .iter()
                .find(|edge| {
                    edge.site == closure.site
                        && edge.callee == closure.callee
                        && !edge.caller.starts_with("closure@")
                })
                .unwrap_or_else(|| panic!("no mirror for {closure:?} among {edges:?}"))
        })
        .collect();
    assert_eq!(closure_edges.len(), 3, "{edges:?}");
    assert_eq!(mirrors.len(), 3);
    assert!(
        mirrors.iter().all(|mirror| mirror.caller == "entry"),
        "every mirror names the innermost enclosing NAMED def: {mirrors:?}"
    );
    // 5 primaries (one per site) + 3 mirrors. Before the fix: 5.
    assert_eq!(edges.len(), 8, "{edges:?}");
}
