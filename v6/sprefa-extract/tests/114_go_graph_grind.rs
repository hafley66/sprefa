//! Go graph grind against the typescript-go oracles
//! (`plans/extract-bench-2026-08-29/RATCHET.tsv`, the four go rows).
//!
//! Type plane: a `type` spec whose underlying type is a func, slice, map,
//! alias, or an interface with method specs mentions named types the old
//! candidate walk dropped (`go_edge_candidates` matched struct fields,
//! embeds and interface embeds only). 792 of the 1,683 typedecl oracle rows
//! missing at 1b2464c9b were this shape.
//!
//! Call plane: `x = f()` on a name the receiver walk has not typed recorded no
//! bind-plan row (OPEN-PROBLEMS.md row 3), so a method call on the reassigned
//! name never bound.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.
//!
//! TEST HEADER, HEAD failure before the fix
//! (`cargo test --release --features cli --test 114_go_graph_grind`, 4 of 5):
//!   func_slice_map_alias_specs_ref_their_named_types  FAILED  Handler -> Req: []
//!   interface_method_spec_refs_its_param_and_result   FAILED  Visitor -> Item: []
//!   reassignment_from_call_types_the_name             FAILED  one Tag edge: []
//!   pair_reassignment_from_call_types_the_first_name  FAILED  one Tag edge: []
//!
//! Review receipt (PR #701, cf46c15cc): the `assignment_statement` arm and
//! the `:=` arm collected `children()` unfiltered, so the `,` tokens sat in
//! `targets` / `rhss`. `a, b = f()` bound `b` at result position 2, and
//! `a, b := f(), g()` saw 2 names against 3 rhs nodes and bound nothing:
//!   second_name_of_a_pair_reassignment_types_from_the_second_result  FAILED  one Total edge: []
//!   paired_declare_types_each_name_from_its_own_call                  FAILED  one Total edge: []

use std::process::Command;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn walk(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("fixture dir readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            out.extend(walk(&path.to_string_lossy()));
        } else if path.extension().is_some_and(|ext| ext == "go") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    out
}

fn resolve() -> Vec<serde_json::Value> {
    let mut paths = walk(&fixture("go_grind"));
    paths.sort();
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg("--family")
        .arg("call,type")
        .args(&paths)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf8 wire")
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// (owner_name, target_name, target_path) per resolved type edge.
fn type_edges(rows: &[serde_json::Value]) -> Vec<(String, String, String)> {
    rows.iter()
        .filter(|row| row["record"] == "resolved_type_edge")
        .map(|row| {
            (
                row["owner_name"].as_str().unwrap_or("").to_string(),
                row["target_name"].as_str().unwrap_or("").to_string(),
                row["target_path"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

/// (caller_name, callee_name, callee_path) per resolved call edge.
fn call_edges(rows: &[serde_json::Value]) -> Vec<(String, String, String)> {
    rows.iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                row["caller_name"].as_str().unwrap_or("").to_string(),
                row["callee_name"].as_str().unwrap_or("").to_string(),
                row["callee_path"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

fn one_edge(edges: &[(String, String, String)], src: &str, dst: &str, file: &str) {
    let hit: Vec<_> = edges.iter().filter(|e| e.0 == src && e.1 == dst).collect();
    assert_eq!(hit.len(), 1, "one {src} -> {dst} edge: {hit:?}");
    assert!(hit[0].2.ends_with(file), "{src} -> {dst} bound in {file}: {hit:?}");
}

#[test]
fn func_slice_map_alias_specs_ref_their_named_types() {
    let edges = type_edges(&resolve());
    one_edge(&edges, "Handler", "Req", "shapes.go");
    one_edge(&edges, "Handler", "Resp", "shapes.go");
    one_edge(&edges, "ItemList", "Item", "shapes.go");
    one_edge(&edges, "KeyedItems", "Key", "shapes.go");
    one_edge(&edges, "KeyedItems", "Item", "shapes.go");
    one_edge(&edges, "ItemAlias", "Item", "shapes.go");
}

#[test]
fn interface_method_spec_refs_its_param_and_result() {
    let edges = type_edges(&resolve());
    one_edge(&edges, "Visitor", "Item", "shapes.go");
    one_edge(&edges, "Visitor", "Resp", "shapes.go");
}

#[test]
fn reassignment_from_call_types_the_name() {
    let edges = call_edges(&resolve());
    one_edge(&edges, "Reassign", "Widget", "store.go");
    one_edge(&edges, "Reassign", "Tag", "store.go");
}

#[test]
fn pair_reassignment_from_call_types_the_first_name() {
    let edges = call_edges(&resolve());
    one_edge(&edges, "ReassignPair", "Lookup", "store.go");
    one_edge(&edges, "ReassignPair", "Tag", "store.go");
}

/// The lhs of a compound assignment is still walked: the `Widget` and `Tag`
/// sites inside the index target keep their rows.
#[test]
fn compound_assignment_keeps_lhs_sites() {
    let edges = call_edges(&resolve());
    one_edge(&edges, "Compound", "Widget", "store.go");
    one_edge(&edges, "Compound", "Tag", "store.go");
}

#[test]
fn second_name_of_a_pair_reassignment_types_from_the_second_result() {
    let edges = call_edges(&resolve());
    one_edge(&edges, "ReassignSecond", "Sell", "store.go");
    one_edge(&edges, "ReassignSecond", "Total", "store.go");
}

#[test]
fn paired_declare_types_each_name_from_its_own_call() {
    let edges = call_edges(&resolve());
    one_edge(&edges, "PairedDeclare", "Receipt", "store.go");
    one_edge(&edges, "PairedDeclare", "Total", "store.go");
}
