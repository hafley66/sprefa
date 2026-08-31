//! Return-of-call chains (`f()(...)`) resolve through the called def's single
//! bare-identifier return. HEAD-FAILURE receipt: a `call`-in-function-position
//! produced no call site at all (`py_callee` answered None), so `func()()`
//! emitted only the inner `func()` edge -- the PyCG bench's direct_calls
//! category sat at 30% recall.
//!
//! The return value may name a same-file def (the nested wrapper counts), a
//! binding local to the returned def, or a parameter of it whose slot the
//! inner call passed (unique arg only).

use std::process::Command;

const MAIN: &str = "tests/fixtures/py_findings/direct_calls/main.py";

fn callee_names(paths: &[&str]) -> Vec<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(out.status.success());
    let mut names: Vec<String> = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(fact) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if fact["record"] != "resolved_edge" {
            continue;
        }
        names.push(fact["callee_name"].as_str().unwrap_or("").to_string());
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn inner_call_still_resolves_by_name() {
    let names = callee_names(&[MAIN, MAIN]);
    assert!(names.iter().any(|n| n == "func"), "func() itself");
}

#[test]
fn one_level_return_of_call_resolves() {
    let names = callee_names(&[MAIN, MAIN]);
    // func()() -- func returns return_func.
    assert!(names.iter().any(|n| n == "return_func"), "got {names:?}");
}

#[test]
fn chained_return_of_call_resolves() {
    let names = callee_names(&[MAIN, MAIN]);
    // func()()() -- return_func returns nested_return_func.
    assert!(
        names.iter().any(|n| n == "nested_return_func"),
        "got {names:?}"
    );
}
