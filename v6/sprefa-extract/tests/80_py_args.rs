//! Parameter-called sites resolve when every call site in the file passes the
//! same single named function in that slot (positional or keyword). HEAD-
//! FAILURE receipt: `a()` inside `func` keyed on the callee name, and no def
//! bears `a` -- the PyCG bench's args category sat at 42.86%, kwargs at 20%.
//!
//! Uniqueness only: two call sites passing different functions resolve to
//! nothing. A callee shadowed by an enclosing def's parameter never falls
//! through to a module-level alias.

use std::process::Command;

const MAIN: &str = "tests/fixtures/py_findings/args/main.py";

fn resolved_edges(paths: &[&str]) -> Vec<serde_json::Value> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let fact: serde_json::Value = serde_json::from_str(line).ok()?;
            (fact["record"] == "resolved_edge").then_some(fact)
        })
        .collect()
}

#[test]
fn positional_param_call_resolves_the_unique_candidate() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges
            .iter()
            .any(|e| e["callee_name"] == "param_func" && e["caller_name"] == "func"),
        "func(a) calls a(); unique site passes param_func, got {edges:?}"
    );
}

#[test]
fn positional_and_keyword_args_map_their_slots() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges
            .iter()
            .any(|e| e["caller_name"] == "kw" && e["callee_name"] == "other"),
        "kw's param a, passed positionally as other"
    );
    assert!(
        edges
            .iter()
            .any(|e| e["caller_name"] == "kw" && e["callee_name"] == "param_func"),
        "kw's b arrives by keyword: b=param_func"
    );
}

#[test]
fn module_level_calls_keep_the_null_caller_contract() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    let module_rows: Vec<&serde_json::Value> = edges
        .iter()
        .filter(|fact| fact["caller_name"].is_null())
        .collect();
    assert!(
        module_rows.iter().any(|row| row["callee_name"] == "func"),
        "the func(param_func) site stays a module-level call, got {edges:?}"
    );
}
