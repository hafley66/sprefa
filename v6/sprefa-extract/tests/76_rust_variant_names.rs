//! A variant constructor names the VARIANT, not its enum.
//!
//! `rust.oracle.call.tsv` (ra_ap_ide) spells `Alpha::First(3)` as an edge to
//! `First`; `variant_ctor_target` returned the enum's own def span, so the
//! `resolved_edge` read `callee_name = "Alpha"`. 1,344 of the 7,826 rust call
//! leak rows are exactly that shape (`rust.REPORT.md` section 22), and each
//! one costs twice: a missed oracle row plus an excess row naming the enum.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, at cbf7eb6da): both edge tests red,
//! `variant_ctor_names_the_variant` got `[("alpha_user", "Alpha", "alpha")]`
//! and `variant_ctor_through_Self_names_the_variant` got
//! `[("make", "Shape", "shape")]`.
//!
//! Fixtures: `tests/fixtures/rust_findings/variant_names/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/variant_names";

const FILES: &[&str] = &["lib.rs", "alpha.rs", "shape.rs", "user.rs"];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
    ];
    args.extend(FILES.iter().map(|name| format!("{DIR}/{name}")));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

fn text(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or("").to_string()
}

/// (caller_name, callee_name, callee file stem) per `resolved_edge`.
fn edges() -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                text(row, "callee_name"),
                text(row, "callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs")
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows.dedup();
    rows
}

#[test]
fn variant_ctor_names_the_variant() {
    let rows = edges();
    assert!(
        rows.iter().any(|(caller, callee, stem)| {
            caller == "alpha_user" && callee == "First" && stem == "alpha"
        }),
        "Alpha::First(3) names First, not Alpha: {rows:?}"
    );
    assert!(
        !rows
            .iter()
            .any(|(caller, callee, _)| caller == "alpha_user" && callee == "Alpha"),
        "the enum row must be gone, not doubled: {rows:?}"
    );
}

#[test]
fn variant_ctor_through_Self_names_the_variant() {
    let rows = edges();
    assert!(
        rows.iter().any(|(caller, callee, stem)| {
            caller == "make" && callee == "Round" && stem == "shape"
        }),
        "Self::Round(r) names Round: {rows:?}"
    );
}

/// A variant and a free fn sharing one name must not merge: the bare call
/// binds the fn, the qualified one binds the variant.
#[test]
fn a_variant_does_not_capture_a_same_named_free_fn() {
    let rows = edges();
    assert!(
        rows.iter().any(|(caller, callee, stem)| {
            caller == "collide_user" && callee == "Square" && stem == "shape"
        }),
        "Shape::Square(2) binds the variant: {rows:?}"
    );
    assert!(
        rows.iter().any(|(caller, callee, stem)| {
            caller == "collide_user" && callee == "square" && stem == "lib"
        }),
        "square(2) binds the free fn: {rows:?}"
    );
}
