//! Type-system prototype (2026-07-09): enum brands + named shapes, both
//! typecheck-time only (columns stay text at runtime).
//!   RUNG 1 — `type severity = "error" | "warn" | ...` closed literal set; a
//!            literal outside the set is an `enum-variant-unknown` error with a
//!            nearest-variant suggestion.
//!   RUNG 2 — `type finding(path: text, line: int, sev: severity)` named shape,
//!            referenced by `rel finding_rel: finding.`, expanded at load into a
//!            plain RelDecl so a rule can write rows and a query read them.
//! Same sandbox harness as tests/it/facts.rs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_decls_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A valid enum literal in a branded head + query pin passes and returns rows.
#[test]
fn enum_brand_accepts_a_known_variant() {
    let dir = sandbox("enum_ok");
    let prog = concat!(
        "type severity = \"error\" | \"warn\" | \"info\" | \"hint\".\n",
        "rel finding(path: text, line: int, sev: severity).\n",
        "finding(\"a.rs\", 10, \"error\").\n",
        "finding(\"b.rs\", 20, \"warn\").\n",
        "? finding(path, line, sev).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "known enum variants must pass:\n{err}");
    assert!(out.contains("(2 rows)"), "both findings selected:\n{out}");
    assert!(out.contains("error") && out.contains("warn"), "{out}");
}

/// An unknown enum literal is an `enum-variant-unknown` error whose message
/// suggests the nearest variant. Exit is non-zero.
#[test]
fn enum_brand_rejects_unknown_variant_with_suggestion() {
    let dir = sandbox("enum_bad");
    let prog = concat!(
        "type severity = \"error\" | \"warn\" | \"info\" | \"hint\".\n",
        "rel finding(path: text, line: int, sev: severity).\n",
        "finding(\"a.rs\", 10, \"wrn\").\n",
        "? finding(path, line, sev).\n");
    let (code, out, err) = run(&dir, prog);
    let all = format!("{out}{err}");
    assert_ne!(code, 0, "an unknown enum variant must fail:\n{all}");
    assert!(all.contains("enum-variant-unknown"), "diag code expected:\n{all}");
    assert!(all.contains("did you mean \"warn\"?"), "nearest-variant suggestion expected:\n{all}");
}

/// A named shape expands into a working rel: a rule writes rows, a query reads
/// them. The shape's `sev` column carries the enum brand end to end.
#[test]
fn named_shape_expands_into_a_working_rel() {
    let dir = sandbox("shape_ok");
    let prog = concat!(
        "type severity = \"error\" | \"warn\".\n",
        "type finding(path: text, line: int, sev: severity).\n",
        "rel finding_rel: finding.\n",
        "rel raw(path: text, line: int).\n",
        "raw(\"a.rs\", 10).\n",
        "raw(\"b.rs\", 20).\n",
        // A derived rule fills the shape-declared rel, branding every row \"warn\".
        "finding_rel(path, line, \"warn\") <- raw(path, line).\n",
        "? finding_rel(path, line, sev).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "shape rel must derive rows:\n{err}");
    assert!(out.contains("(2 rows)"), "both raw rows flow into the shape rel:\n{out}");
    assert!(out.contains("a.rs") && out.contains("warn"), "{out}");
}

/// A `rel <name>: <shape>.` naming a shape that was never declared is an
/// `unknown-shape` load error that names the fix.
#[test]
fn unknown_shape_is_a_load_error() {
    let dir = sandbox("shape_unknown");
    let prog = concat!(
        "rel finding_rel: finding.\n",
        "finding_rel(\"a.rs\").\n");
    let (code, out, err) = run(&dir, prog);
    let all = format!("{out}{err}");
    assert_ne!(code, 0, "an unknown shape must fail:\n{all}");
    assert!(all.contains("unknown-shape"), "diag code expected:\n{all}");
    assert!(all.contains("declare `type finding(...)`"), "message must name the fix:\n{all}");
}

/// Regression: the pre-existing `type X <: Y` nominal brand still works —
/// a valid text value passes, and the brand round-trips through a query.
#[test]
fn plain_nominal_brand_still_works() {
    let dir = sandbox("brand_regress");
    let prog = concat!(
        "type sha <: text.\n",
        "rel commit(id: sha, msg: text).\n",
        "commit(\"abc123\", \"init\").\n",
        "? commit(id, msg).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "nominal brand must still pass:\n{err}");
    assert!(out.contains("abc123") && out.contains("init"), "{out}");
}
