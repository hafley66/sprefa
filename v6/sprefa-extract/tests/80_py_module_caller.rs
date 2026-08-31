//! Module-level python callers emit resolved_edge rows. HEAD-FAILURE receipt:
//! `extract --resolve` over this fixture emitted zero resolved_edge rows before
//! the fix — `PythonSource::resolve` skipped every site with no covering def
//! (`covering_def` -> None at module level, `continue`), while `--family call`
//! still listed the sites. The PyCG bench scorer (plans/extract-bench-2026-08-29/
//! python-oracle/) lost 185 of 225 oracle rows to exactly this gap.
//!
//! Caller choice, decided WITH the bench in mind: the oracle's module-level
//! rows carry src_name = "" (PYCG-SUITE.md mapping rule 3, "our arm reports a
//! null caller_name there"), so the fix gives a module-level site the module as
//! caller with caller_name = null — the 4-col bench join then matches the
//! oracle's (src_path, "") rows exactly. The module caller is a CallF def node
//! spanning the whole file (CallKind::Module, nameless), skipped in call_def
//! wire rows so v5 parity baselines stay byte-identical.

use std::process::Command;

const MAIN: &str = "tests/fixtures/py_findings/module_caller/main.py";
const HELPER: &str = "tests/fixtures/py_findings/module_caller/helper.py";

fn resolved_edges(paths: &[&str]) -> Vec<serde_json::Value> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "extract exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let fact: serde_json::Value = serde_json::from_str(line).ok()?;
            (fact["record"] == "resolved_edge").then_some(fact)
        })
        .collect()
}

#[test]
fn module_level_caller_resolves_local_def() {
    let edges = resolved_edges(&[MAIN, HELPER]);
    let local = edges
        .iter()
        .find(|edge| edge["callee_name"] == "local_fn")
        .expect("module-level call to a local def resolves");
    assert_eq!(local["caller_path"], MAIN);
    assert_eq!(local["callee_path"], MAIN);
    // The module-as-caller contract: null caller_name, so the bench join's
    // `caller_name or ""` lands on the oracle's empty src_name for module rows.
    assert!(
        local["caller_name"].is_null(),
        "module caller_name must be null, got {}",
        local["caller_name"]
    );
}

#[test]
fn module_level_caller_resolves_imported_def() {
    let edges = resolved_edges(&[MAIN, HELPER]);
    let imported = edges
        .iter()
        .find(|edge| edge["callee_name"] == "imported_fn")
        .expect("module-level call to an imported def resolves");
    assert_eq!(imported["caller_path"], MAIN);
    assert_eq!(imported["callee_path"], HELPER);
    assert!(imported["caller_name"].is_null());
}
