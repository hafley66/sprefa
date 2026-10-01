//! `Extends` and `ExtendsAll` (plans/v8/2026-09-30-extends.test-plan.md,
//! cases 1-15 and 18) through the public compiler on all five engines. The
//! walk reads `edge_snapshot`, last round's edges, at comptime, so every
//! case reads `compiler_rows`.

use super::_9e_generic_rules::{render_row, ENGINES};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn compile(fixture: &str, engine: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(root.join("fixtures/extends").join(fixture))
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{engine}: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let program: Value = serde_json::from_slice(&compiled.stdout).unwrap();
    assert_eq!(program["diagnostics"], json!([]), "{engine}");
    program
}

/// Rendered compiler rows of `relations`, sorted. A trailing `reason` value
/// is a prelude variant node whose rendering carries reader offsets; it is
/// cut.
fn select(program: &Value, relations: &[&str]) -> Vec<String> {
    let names = &program["program"]["names"];
    let rendered: BTreeSet<String> = program["compiler_rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| render_row(row, names))
        .filter(|row| relations.iter().any(|relation| row.starts_with(&format!("{relation}: "))))
        .map(|row| match row.split_once(", ref(application(owner(prelude") {
            Some((kept, _)) => kept.to_owned(),
            None => row,
        })
        .collect();
    rendered.into_iter().collect()
}

/// Cases 1-15: 1 exact, 2 extra fields, 3 missing, 4 other field type, 5 empty
/// contract, 6 field order, 7 asymmetry, 8 same product field, 9 covariant
/// product field, 10 generic instance field, 11 recursive types, 12 every
/// product with `title: str`, 13 all pass, 14 one fails, 15 empty list.
#[test]
fn extends_cases_agree_on_every_engine() {
    let extends = vec![
        "Extends: ref(Book), ref(Titled), ref(Book)",
        "Extends: ref(Chain), ref(Node), ref(Chain)",
        "Extends: ref(Named), ref(Named), ref(Named)",
        "Extends: ref(Node), ref(Node), ref(Node)",
        "Extends: ref(OwnedByUser), ref(OwnerNamedContract), ref(OwnedByUser)",
        "Extends: ref(OwnedByUser), ref(OwnerUserContract), ref(OwnedByUser)",
        "Extends: ref(Post), ref(Titled), ref(Post)",
        "Extends: ref(Reordered), ref(Named), ref(Reordered)",
        "Extends: ref(StatusHolder), ref(StatusContract), ref(StatusHolder)",
        "Extends: ref(Titled), ref(Titled), ref(Titled)",
        "Extends: ref(User), ref(Aged), ref(User)",
        "Extends: ref(User), ref(Empty), ref(User)",
        "Extends: ref(User), ref(Named), ref(User)",
    ];
    let lists = vec![
        "ExtendsAll: ref(User), application(tuple, []), ref(User)",
        "ExtendsAll: ref(User), ref(application(tuple, [ref(Named), ref(Aged)])), ref(User)",
        "extends_all_checked: ref(User), ref(User)",
        "titled_type: ref(Book)",
        "titled_type: ref(Post)",
        "titled_type: ref(Titled)",
    ];
    // The failures at the asking site, contract `Titled` aside (case 12 asks
    // every product with a field).
    let diagnostics = vec![
        "extends_diagnostic: ref(Aged), ref(Named), const(name)",
        "extends_diagnostic: ref(Named), ref(Aged), const(age)",
        "extends_diagnostic: ref(Named), ref(User), const(age)",
        "extends_diagnostic: ref(NumericName), ref(Named), const(name)",
        "extends_diagnostic: ref(TaggedChain), ref(Tagged), const(next)",
        "extends_diagnostic: ref(TaggedChain), ref(Tagged), const(tag)",
    ];
    for engine in ENGINES {
        let program = compile("0_cases.dl7", engine);
        assert_eq!(select(&program, &["Extends"]), extends, "{engine}");
        assert_eq!(
            select(&program, &["ExtendsAll", "extends_all_checked", "titled_type"]),
            lists,
            "{engine}"
        );
        let failures: Vec<String> = select(&program, &["extends_diagnostic"])
            .into_iter()
            .filter(|row| !row.contains("ref(Titled)"))
            .collect();
        assert_eq!(failures, diagnostics, "{engine}");
    }
}

/// Case 18: fifty products and one ask. The walk reaches the asked pair and
/// its one shared field, nothing else.
#[test]
fn extends_checks_only_the_asked_pair() {
    let expected = vec![
        "Extends: ref(User), ref(Named), ref(User)",
        "candidate: ref(User), ref(Named)",
        "candidate: ref(primitive(str)), ref(primitive(str))",
    ];
    for engine in ENGINES {
        let program = compile("1_demand.dl7", engine);
        let rows = select(&program, &["Extends", "candidate", "fails_at", "fails"]);
        assert_eq!(rows, expected, "{engine}");
    }
}
