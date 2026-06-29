//! Tier-1 proof on real Python via real `scip-python`: the same language-blind
//! dispatch query (bench/flow/dispatch_flow.dl) runs over a Python index
//! unchanged. The fixture is a generated-client shape — a `PetAPI` ABC with one
//! abstractmethod per OpenAPI operationId, a `PetClient` subclass overriding each
//! method, and a shared `httpExec` helper both overrides call.
//!
//! scip-python emits the override as a METHOD-LEVEL `is_implementation`
//! relationship (`PetClient#getPet().` -> `PetAPI#getPet().`), surfaced as
//! `scip_impl`. The query crosses that hop to find `httpExec` on the call-graph
//! path of BOTH ops. This is also the tier that exposed the parameter-vs-method
//! fn-def attribution fix in scip_import (scip-python emits parameter symbols;
//! RA/scip-go inline them), now locked by a loader unit test.
//!
//! Skips (does not fail) when no `scip-python` is found — set SPREFA_SCIP_PYTHON
//! or put it on PATH. Install with `npm install -g @sourcegraph/scip-python`.
//!
//! Unlike the Go test, this indexes the fixture IN PLACE rather than from a /tmp
//! copy: scip-python walks parent directories for Python/environment config and
//! fatally errors (`indexOf of undefined`) when run from a bare temp dir with no
//! such config above it. Running in the in-repo fixture sidesteps that. The index
//! is written to a temp file (not the repo) and fed to dl via SPREFA_SCIP_INDEX,
//! and the db lives in temp, so the only thing touched in-tree is a read.

use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/flow");
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/py_flow");

fn find_scip_python() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_SCIP_PYTHON") {
        let p = PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    let out = Command::new("which").arg("scip-python").output().ok()?;
    if !out.status.success() { return None; }
    let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
    p.is_file().then_some(p)
}

/// rows of a `? rel` block from dl stdout (tab-split values).
fn query_block<'a>(stdout: &'a str, header: &str) -> Vec<Vec<&'a str>> {
    let mut rows = Vec::new();
    let mut in_q = false;
    for line in stdout.lines() {
        if line.starts_with(header) { in_q = true; continue; }
        if line.starts_with('?') { in_q = false; continue; }
        if in_q {
            let t = line.trim();
            if t.is_empty() || t.starts_with('(') { continue; }
            rows.push(t.split('\t').collect());
        }
    }
    rows
}

#[test]
fn py_interface_dispatch_finds_function_on_two_op_paths() {
    let Some(scip_python) = find_scip_python() else {
        eprintln!("SKIP flow_py_dispatch: no scip-python (set SPREFA_SCIP_PYTHON)");
        return;
    };
    let pid = std::process::id();
    let index = std::env::temp_dir().join(format!("flow_py_{pid}.scip"));
    let dbp = std::env::temp_dir().join(format!("flow_py_{pid}.db"));
    let _ = std::fs::remove_file(&index);
    let _ = std::fs::remove_file(&dbp);

    let run = Command::new(&scip_python)
        .args(["index", ".", "--project-name", "petstore", "--output"])
        .arg(&index)
        .current_dir(FIXTURE)
        .output().expect("run scip-python");
    if !index.is_file() {
        eprintln!("SKIP flow_py_dispatch: scip-python produced no index: {}",
            String::from_utf8_lossy(&run.stderr));
        return;
    }

    let out = Command::new(DL)
        .arg(PathBuf::from(FLOW).join("dispatch_flow.dl"))
        .args(["--root", FIXTURE, "--db", dbp.to_str().unwrap()])
        .env("SPREFA_SCIP_INDEX", &index)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "dl failed: {}\n{stdout}", String::from_utf8_lossy(&out.stderr));

    let ops = query_block(&stdout, "? spec_op");
    assert!(ops.iter().any(|r| r == &["getPet"]) && ops.iter().any(|r| r == &["createPet"]),
        "both spec ops present: {ops:?}");

    let entries = query_block(&stdout, "? op_entry");
    assert!(entries.iter().any(|r| r.first() == Some(&"getPet") && r.get(1).is_some_and(|m| m.contains("PetAPI#getPet"))),
        "getPet entry is the ABC method: {entries:?}\n{stdout}");

    // the shared helper on BOTH op paths. Without the param-vs-method fn-def fix
    // this set would not contain httpExec (the call would attribute to a param).
    let multi = query_block(&stdout, "? multi_op");
    assert!(multi.iter().any(|r| r.first().is_some_and(|f| f.contains("httpExec"))),
        "httpExec is on both op paths (dispatch hop fired, attribution correct): {multi:?}\n{stdout}");
}
