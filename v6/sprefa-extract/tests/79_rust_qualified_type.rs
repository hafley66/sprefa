//! A qualified type path binds through its TRAILING segment.
//!
//! `path_name` joins every segment (`rust.rs`), so `inner::Marker` is interned
//! as the literal string `inner::Marker` — and the checker index, the module
//! plane and the `DefIndex` are all keyed on a bare declaration name, so no leg
//! could ever bind it. Section 26.2 of `rust.REPORT.md` prices class C at 461
//! of the missing `rust.oracle.type.typedecl.tsv` rows.
//!
//! The candidate text stays as written (the 4b-iii discipline); the RESOLVE
//! side gains the trailing-segment key, narrowed by the qualifier's module
//! path so a bare name shared by two modules still binds per file.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, at 012f784ae): both tests red against an
//! EMPTY `resolved_type_edge` set — every candidate the fixture mints is
//! qualified, and each one resolved to the zero dst leg.
//!
//! Fixtures: `tests/fixtures/rust_findings/qualified_type/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/qualified_type";

const FILES: &[&str] = &["lib.rs", "inner.rs", "user.rs"];

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
fn qualified_field_type_binds_its_declaration() {
    let rows = type_edges();
    assert!(
        rows.iter().any(|(owner, target, stem)| {
            owner == "Holder" && target == "Marker" && stem == "inner"
        }),
        "`marker: inner::Marker` binds Marker in inner.rs: {rows:?}"
    );
}

/// The generic argument of a qualified path is itself a qualified path.
#[test]
fn qualified_generic_argument_binds() {
    let rows = type_edges();
    assert!(
        rows.iter()
            .any(|(owner, target, stem)| owner == "Holder" && target == "Slot" && stem == "inner"),
        "`slot: inner::Slot<inner::Marker>` binds Slot in inner.rs: {rows:?}"
    );
}
