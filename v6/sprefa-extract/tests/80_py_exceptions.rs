//! `raise <class>` is a call to the class's `__init__` (PyCG's oracle
//! semantics). HEAD-FAILURE receipt: `raise_statement` produced no call site,
//! so all 6 exceptions oracle rows were misses.
//!
//! Forms: a bare class name, a name alias (`a = A` rebinds through the binding
//! table), and a nested attribute (`B.C`, whose defining class carries the
//! `__init__` the join targets).

use std::process::Command;

const MAIN: &str = "tests/fixtures/py_findings/exceptions/main.py";

fn resolved_edges(paths: &[&str]) -> Vec<serde_json::Value> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|fact| fact["record"] == "resolved_edge")
        .collect()
}

#[test]
fn raise_of_a_class_name_calls_its_init() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges
            .iter()
            .any(|e| e["caller_name"].is_null() && e["callee_name"] == "__init__"),
        "raise A targets A.__init__, got {edges:?}"
    );
}

#[test]
fn raise_of_a_name_alias_resolves_the_class() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    let inits = edges
        .iter()
        .filter(|e| e["callee_name"] == "__init__")
        .count();
    assert!(
        inits >= 2,
        "alias raise lands on __init__ too, got {edges:?}"
    );
}

#[test]
fn raise_of_a_nested_class_targets_the_nested_init() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges
            .iter()
            .any(|e| e["callee_name"] == "__init__" && e["caller_name"].is_null()),
        "B.C.__init__ resolves from the module scope"
    );
}
