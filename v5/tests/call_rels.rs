//! Phase D scaffold: the CALL_RELS built-in family (call_def / call_site /
//! call_edge / call_edge_rev) is wired as a lazy indexer alongside TYPE_RELS.
//! Extractors return empty CallFacts today (the TypeLang default), so these
//! tests prove the plumbing, not the data: the four relations are reserved,
//! the lazy refresh runs against a real source file without error, and
//! closure(call_edge) is a legal (empty) edge rel. Per-language extractor
//! bodies fill the rows in follow-up commits.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("call_rels_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--root",
            dir.to_str().unwrap(),
            "--db",
            dir.join("db").to_str().unwrap(),
        ])
        .args(extra)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// The four CALL_RELS are reserved: a program that declares one by hand errors
/// out, matching the TYPE_RELS / MODULE_RELS contract.
#[test]
fn call_rels_are_reserved() {
    let d = sandbox("reserved");
    for rel in ["call_def", "call_site", "call_edge", "call_edge_rev"] {
        let prog = format!("rel {rel}(a: text).\n? {rel}(\"x\").\n");
        let (code, _out, err) = run(&d, &prog, &[]);
        assert_ne!(code, 0, "{rel} must be reserved (expected error):\n{err}");
        assert!(
            err.contains("built-in call-graph relation"),
            "{rel} reservation message missing:\n{err}"
        );
    }
}

/// Wiring is live even with empty extractors: scanning a real Rust file, the
/// lazy indexer runs, the relations are queryable and empty, and
/// closure(call_edge) is a legal edge rel. This is the Phase D no-op gate:
/// every part of the plumbing runs, no rows are produced yet.
#[test]
fn empty_extractors_keep_wiring_live() {
    let d = sandbox("empty");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn main() { helper(1); }\nfn helper(x: i32) {}\n",
    )
    .unwrap();
    let prog = concat!(
        "rel reaches(a: text, b: text).\n",
        "reaches(a, b) <- closure(call_edge).\n",
        "? call_def(sym, kind, file, line, end).\n",
        "? call_site(caller, callee, file, line).\n",
        "? reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "empty extractors must not error:\n{err}");
    // Each query prints a "(N rows)" footer; with empty extractors all three
    // are zero. No positive row count should appear anywhere in the output.
    assert!(out.contains("(0 rows)"), "expected zero-row footers:\n{out}");
    assert!(
        !out.contains("(1 rows)") && !out.contains("(2 rows)"),
        "empty extractors produced rows:\n{out}"
    );
}
