//! Call goal step 1 (plans/v8/2026-10-01-call-goal.step1.brief.md): examples 1-4 and 7 with
//! every `call` branch written by hand, through the public compiler on all five engines.

use super::_9e_generic_rules::{render_row, ENGINES};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn compile(fixture: &str, engine: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(root.join("fixtures/call").join(fixture))
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

/// Rendered compiler rows of `relations`, sorted, a trailing prelude `reason` cut.
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

/// 1 `(User, Named)` passes, `(Robot, Named)` fails at `name`. 2 `(Account, Owner)` passes
/// covariantly, `(Account, StrictOwner)` has no row. 3 `(Robot, Serial)` passes by the
/// `implements` fact, `(User, Serial)` has no row. 4 `(User, Display)` returns `Display`.
/// 7 the `user_history` obligation is discharged, `robot_history` is not.
#[test]
fn hand_expanded_call_agrees_on_every_engine() {
    let expected = vec![
        "discharged: const(\"user_history\"), const(0), ref(User)",
        "extends_diagnostic: ref(Robot), ref(Named), const(name)",
        "extends_diagnostic: ref(Robot), ref(UserContract), const(name)",
        "extends_diagnostic: ref(User), ref(Serial), const(serial)",
        "passes: ref(Account), ref(Owner), ref(Account)",
        "passes: ref(Robot), ref(Serial), ref(Robot)",
        "passes: ref(User), ref(Display), ref(Display)",
        "passes: ref(User), ref(Named), ref(User)",
        "passes: ref(User), ref(UserContract), ref(User)",
    ];
    for engine in ENGINES {
        let program = compile("0_hand_expanded.dl7", engine);
        let rows = select(&program, &["passes", "discharged", "extends_diagnostic"]);
        assert_eq!(rows, expected, "{engine}");
    }
}

/// Fifty products and one ask: the walk reaches the asked pair and its one shared field.
#[test]
fn hand_expanded_call_checks_only_the_asked_pair() {
    let expected = vec![
        "Extends: ref(User), ref(Named), ref(User)",
        "candidate: ref(User), ref(Named)",
        "candidate: ref(primitive(str)), ref(primitive(str))",
        "passes: ref(User), ref(Named), ref(User)",
    ];
    for engine in ENGINES {
        let program = compile("1_demand.dl7", engine);
        let rows = select(&program, &["passes", "Extends", "candidate", "fails_at", "fails"]);
        assert_eq!(rows, expected, "{engine}");
    }
}
