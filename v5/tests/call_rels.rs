//! Phase D: the CALL_RELS built-in family (call_def / call_site / call_edge /
//! call_edge_rev) wired as a lazy indexer alongside TYPE_RELS. The Rust
//! extractor (syn) is live; Kotlin and TS still use the TypeLang default
//! (empty CallFacts) until their extract_calls bodies land.
//!
//! Tests: (1) the four relations are reserved, (2) a language with no
//! call extractor keeps the wiring live with zero rows, (3) the Rust
//! extractor + the engine's caller/callee resolution pass produce a real
//! resolved call graph and closure(call_edge) walks it transitively.

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

/// A language whose extract_calls is still the empty default (Kotlin today)
/// keeps the wiring live: the lazy indexer runs, the relations are queryable
/// and empty, and closure(call_edge) is a legal (empty) edge rel. This is the
/// no-op gate for any not-yet-implemented front-end.
#[test]
fn empty_call_extractors_keep_wiring_live() {
    let d = sandbox("empty");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/lib.kt"), "fun main() { helper(1) }\nfun helper(x: Int) {}\n").unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.kt\", path, rev), match(path, rev, /./, line).\n",
        "rel reaches(a: text, b: text).\n",
        "reaches(a, b) <- closure(call_edge).\n",
        "? call_def(sym, kind, file, line, end).\n",
        "? call_site(caller, callee, file, line).\n",
        "? reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "empty extractors must not error:\n{err}");
    assert!(out.contains("(0 rows)"), "expected zero-row footers:\n{out}");
    assert!(
        !out.contains("(1 rows)") && !out.contains("(2 rows)"),
        "empty Kotlin extractors produced rows:\n{out}"
    );
}

/// The Phase D gate: the Rust extractor emits call defs with body spans, the
/// engine's resolution pass attaches each call site to its enclosing def and
/// resolves bare callees to def syms, and closure(call_edge) walks the result
/// transitively. `main -> helper -> leaf` must reach `main -> leaf`.
#[test]
fn rust_call_graph_extracts_resolves_and_closes() {
    let d = sandbox("rust");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(
        d.join("src/lib.rs"),
        "fn main() { helper(1); }\nfn helper(x: i32) { leaf(x); }\nfn leaf(x: i32) {}\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel reaches(a: text, b: text).\n",
        "reaches(a, b) <- closure(call_edge).\n",
        "? call_def(sym, kind, file, line, end).\n",
        "? reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog, &[]);
    assert_eq!(code, 0, "Rust extraction must not error:\n{err}");

    let main = "src/lib.rs::function::main";
    let helper = "src/lib.rs::function::helper";
    let leaf = "src/lib.rs::function::leaf";

    // call_def: all three callables present.
    assert!(out.contains(main), "main def missing:\n{out}");
    assert!(out.contains(helper), "helper def missing:\n{out}");
    assert!(out.contains(leaf), "leaf def missing:\n{out}");
    assert!(
        out.contains("(3 rows)"),
        "expected exactly 3 call_def rows:\n{out}"
    );

    // closure(call_edge): direct main -> helper, plus transitive main -> leaf.
    assert!(
        out.contains(&format!("{main}\t{helper}")),
        "direct edge main -> helper missing from closure:\n{out}"
    );
    assert!(
        out.contains(&format!("{main}\t{leaf}")),
        "transitive reach main -> leaf missing from closure:\n{out}"
    );
}
