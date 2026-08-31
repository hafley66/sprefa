//! A bound and an impl header name their generic ARGUMENTS too.
//!
//! `bound_candidate` and the impl `trait_` leg both read `path_name` and
//! stopped there, so `T: Carrier<Payload>` named `Carrier` and lost `Payload`;
//! `impl Carrier<Payload> for Boxed<Other>` lost both `Payload` and `Other`.
//! `type_refs` already recurses a field type this way. Section 26.2 of
//! `rust.REPORT.md` prices class D at 387 rows (D1 362, D2's generic-argument
//! half 25).
//!
//! The impl self type's own HEAD stays unwalked: it names the owner, and the
//! owner-to-itself row is the excluded X2 class.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, at 0e29983b7): all three tests red against
//! the same three rows, `[("Boxed","Carrier"),("Boxed","Plain"),
//! ("Holder","Carrier")]` — every head bound, no argument did.
//!
//! Fixtures: `tests/fixtures/rust_findings/generic_args/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/generic_args";

const FILES: &[&str] = &["lib.rs", "parts.rs", "holder.rs"];

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

/// (owner_name, target_name, target file stem) per `resolved_type_edge`.
fn type_edges() -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_type_edge")
        .map(|row| {
            (
                text(row, "owner_name"),
                text(row, "target_name"),
                text(row, "target_path")
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
fn bound_generic_argument_is_named() {
    let rows = type_edges();
    assert!(
        rows.iter().any(|(owner, target, stem)| {
            owner == "Holder" && target == "Payload" && stem == "parts"
        }),
        "`Holder<T: Carrier<Payload>>` names Payload: {rows:?}"
    );
}

#[test]
fn impl_trait_generic_argument_is_named() {
    let rows = type_edges();
    assert!(
        rows.iter().any(|(owner, target, stem)| {
            owner == "Boxed" && target == "Payload" && stem == "parts"
        }),
        "`impl Carrier<Payload> for Boxed<Other>` names Payload: {rows:?}"
    );
}

#[test]
fn impl_self_type_generic_argument_is_named() {
    let rows = type_edges();
    assert!(
        rows.iter()
            .any(|(owner, target, stem)| owner == "Boxed" && target == "Other" && stem == "parts"),
        "`impl .. for Boxed<Other>` names Other: {rows:?}"
    );
    assert!(
        !rows
            .iter()
            .any(|(owner, target, _)| owner == "Boxed" && target == "Boxed"),
        "the self type's own head is the owner, never a target: {rows:?}"
    );
}
