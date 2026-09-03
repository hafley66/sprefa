//! In-file decorator applications emit the decorator call edge, and the
//! syntactic `def wrapper... return wrapper` shape rebinds the decorated name
//! to the wrapper. HEAD-FAILURE receipt: a decorator expression is not a
//! `call` node, so `@dec` produced no edge and `func()` resolved only to the
//! original def -- the PyCG bench's decorators category sat at 36.36%.
//!
//! Identity decorators (`dec_id` returns `f` unchanged) rebind nothing: the
//! def keeps its own body as the call target.

use std::process::Command;

const MAIN: &str = "tests/fixtures/py_findings/decorators/main.py";

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
fn decorator_application_is_itself_a_call_edge() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges
            .iter()
            .any(|e| e["caller_name"].is_null() && e["callee_name"] == "dec"),
        "the @dec application calls dec, got {edges:?}"
    );
    assert!(
        edges.iter().any(|e| e["callee_name"] == "dec_id"),
        "@dec_id application likewise"
    );
}

#[test]
fn wrapper_return_rebinds_the_decorated_name() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges.iter().any(|e| e["callee_name"] == "wrapper"),
        "func() now calls the wrapper dec returned, got {edges:?}"
    );
}

#[test]
fn wrapper_calls_through_to_the_original_def() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges
            .iter()
            .any(|e| e["caller_name"] == "wrapper" && e["callee_name"] == "func"),
        "f() inside the wrapper is a call to func"
    );
}

#[test]
fn identity_decorator_keeps_the_def_its_own_target() {
    let edges = resolved_edges(&[MAIN, MAIN]);
    assert!(
        edges.iter().any(|e| e["callee_name"] == "func2"),
        "func2() resolves to func2 itself"
    );
    assert!(
        !edges
            .iter()
            .any(|e| e["callee_name"] == "wrapper" && e["caller_name"] == "func2"),
        "no wrapper fan-out for the identity decorator"
    );
}
