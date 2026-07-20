//! Tier-1 proof on real Go via real `scip-go`: the language-blind dispatch query
//! (bench/flow/dispatch_flow.dl) runs over a Go interface index unchanged. The
//! fixture is a generated-client shape — a `PetAPI` interface with one method per
//! OpenAPI operationId, a `petClient` struct that satisfies it structurally, and
//! a shared `httpExec` helper both methods call.
//!
//! What this proves that a hand-built index can't: scip-go emits the interface
//! satisfaction as a METHOD-LEVEL `is_implementation` relationship
//! (`petClient#getPet().` -> `PetAPI#getPet.`), which the loader surfaces as
//! `scip_impl`. The query crosses that hop and finds `httpExec` on the call-graph
//! path of BOTH ops. Go's structural satisfaction is invisible to a syntactic
//! pass; only the compiler-backed index carries it.
//!
//! `#[ignore]`d — no `scip-go` on PATH is a genuine environmental gap, not
//! something the default `cargo test` run should silently count as coverage.
//! Set SPREFA_SCIP_GO or put it on PATH / in ~/go/bin. Build with `go install
//! github.com/scip-code/scip-go/cmd/scip-go@latest`.

use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const FLOW: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/bench/flow");
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/go_flow");

fn find_scip_go() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SPREFA_SCIP_GO") {
        let p = PathBuf::from(p);
        if p.is_file() { return Some(p); }
    }
    if let Ok(out) = Command::new("which").arg("scip-go").output() {
        if out.status.success() {
            let p = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if p.is_file() { return Some(p); }
        }
    }
    let home = std::env::var("HOME").ok()?;
    let p = PathBuf::from(home).join("go/bin/scip-go");
    p.is_file().then_some(p)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let d = dst.join(e.file_name());
        if p.is_dir() { copy_dir(&p, &d); } else { std::fs::copy(&p, &d).unwrap(); }
    }
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
#[ignore = "needs scip-go on PATH (set SPREFA_SCIP_GO)"]
fn go_interface_dispatch_finds_function_on_two_op_paths() {
    let scip_go = find_scip_go().expect("needs scip-go on PATH (set SPREFA_SCIP_GO)");
    let root = std::env::temp_dir().join(format!("flow_go_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&PathBuf::from(FIXTURE), &root);
    std::fs::copy(PathBuf::from(FLOW).join("dispatch_flow.dl"), root.join("dispatch_flow.dl")).unwrap();

    let run = Command::new(&scip_go)
        .args(["--output", "index.scip"])
        .current_dir(&root)
        .output().expect("run scip-go");
    assert!(root.join("index.scip").is_file(), "scip-go produced no index: {}",
        String::from_utf8_lossy(&run.stderr));

    let out = Command::new(DL)
        .arg(root.join("dispatch_flow.dl"))
        .args(["--db", root.join("flow.db").to_str().unwrap()])
        .current_dir(root)
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "dl failed: {}\n{stdout}", String::from_utf8_lossy(&out.stderr));

    let ops = query_block(&stdout, "? spec_op");
    assert!(ops.iter().any(|r| r == &["getPet"]) && ops.iter().any(|r| r == &["createPet"]),
        "both spec ops present: {ops:?}");

    // each op entry is the INTERFACE method (descriptor `PetAPI#<op>`).
    let entries = query_block(&stdout, "? op_entry");
    assert!(entries.iter().any(|r| r.first() == Some(&"getPet") && r.get(1).is_some_and(|m| m.contains("PetAPI#getPet"))),
        "getPet entry is the interface method: {entries:?}\n{stdout}");

    // the shared helper is on BOTH op paths via the method-level dispatch hop.
    let multi = query_block(&stdout, "? multi_op");
    assert!(multi.iter().any(|r| r.first().is_some_and(|f| f.contains("httpExec"))),
        "httpExec is on both op paths (the dispatch hop fired): {multi:?}\n{stdout}");
}
