//! An enum variant's payload types are edges of the ENUM.
//!
//! `item_edge_candidates` minted one `Enum::Variant` text candidate per variant
//! and walked no payload (`rust.rs`, the `Item::Enum` arm). The section comment
//! above it called the class unrepresentable because v5's owner is the
//! synthetic `Owner::Variant` text; the oracle's owner is the enum's own
//! declaration (`owner_of` stops at `ast::Enum`), which every variant already
//! has an entity for. Section 26.2 of `rust.REPORT.md` prices class B at 1,270
//! of the 6,063 missing `rust.oracle.type.typedecl.tsv` rows.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, at 9db85027e): both tests red against an
//! EMPTY `resolved_type_edge` set — the enum's only candidates were its two
//! `Shape::Variant` text rows, which resolve to the zero dst leg.
//!
//! Fixtures: `tests/fixtures/rust_findings/variant_payload/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/variant_payload";

const FILES: &[&str] = &["lib.rs", "payload.rs", "shapes.rs"];

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

/// A tuple variant and a struct variant alike; the payload declared in another
/// file binds through the `use`.
#[test]
fn variant_payload_types_are_enum_edges() {
    let rows = type_edges();
    for want in ["Point", "Label"] {
        assert!(
            rows.iter().any(|(owner, target, stem)| {
                owner == "Shape" && target == want && stem == "payload"
            }),
            "Shape's variant payload names {want} in payload.rs: {rows:?}"
        );
    }
}

/// The payload is walked as a field type is, so a generic argument counts.
#[test]
fn variant_payload_generic_argument_is_named() {
    let rows = type_edges();
    assert!(
        rows.iter()
            .any(|(owner, target, stem)| owner == "Shape" && target == "Wrapped" && stem == "shapes"),
        "`Many(Wrapped<Point>)` names Wrapped: {rows:?}"
    );
}
