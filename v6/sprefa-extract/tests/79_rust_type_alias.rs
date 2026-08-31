//! A `type X = ..` declaration is a type, on both legs of a type edge.
//!
//! `item_entity` walked Struct/Enum/Union/Trait and stopped, so an alias was
//! invisible twice over: a candidate naming one joined to no definition, and
//! the alias itself minted no candidates for what its right-hand side names.
//! Section 26.2 of `rust.REPORT.md` prices the pair at 1,327 of the 6,063
//! missing `rust.oracle.type.typedecl.tsv` rows (A1 621, A2 680, A3 26).
//!
//! SABOTAGE RECEIPT (fail-pre-fix, at 907478908): all three tests red, each
//! against an EMPTY `resolved_type_edge` set — the fixture's only candidates
//! were the two alias-typed ones, and both resolved to the zero dst leg, which
//! `type_facts` drops.
//!
//! Fixtures: `tests/fixtures/rust_findings/type_alias/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/type_alias";

const FILES: &[&str] = &["lib.rs", "alias.rs", "holder.rs"];

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
fn alias_is_an_edge_destination() {
    let rows = type_edges();
    assert!(
        rows.iter().any(|(owner, target, stem)| {
            owner == "Holder" && target == "Handle" && stem == "alias"
        }),
        "Holder's field type Handle binds the alias declared in alias.rs: {rows:?}"
    );
}

#[test]
fn alias_names_its_right_hand_side() {
    let rows = type_edges();
    assert!(
        rows.iter().any(|(owner, target, stem)| {
            owner == "Handle" && target == "Inner" && stem == "alias"
        }),
        "`type Handle = Inner` is an edge Handle -> Inner: {rows:?}"
    );
}

/// The right-hand side is walked the way a field type is: head plus every
/// generic argument, not the head alone.
#[test]
fn alias_generic_argument_is_named() {
    let rows = type_edges();
    for want in ["Wrapped", "Inner"] {
        assert!(
            rows.iter()
                .any(|(owner, target, _)| owner == "Boxed" && target == want),
            "`type Boxed = Wrapped<Inner>` names {want}: {rows:?}"
        );
    }
}
