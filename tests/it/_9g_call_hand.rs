//! Call goal step 1 (plans/v8/2026-10-01-call-goal.step1.brief.md): examples 1-4 and 7 with
//! every `call` branch written by hand, through the public compiler on all five engines. Step 2
//! (plans/v8/2026-10-01-l2-variable-call.brief.md): the same rows through `(?Checker ..)`.

use super::_9e_generic_rules::{render, render_row, ENGINES};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

fn compile_any(fixture: &str, engine: &str) -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let compiled = Command::new(env!("CARGO_BIN_EXE_dl8"))
        .arg("compile")
        .arg(root.join("fixtures/call").join(fixture))
        .current_dir(root)
        .env("DL8_ENGINE", engine)
        .output()
        .unwrap();
    serde_json::from_slice(&compiled.stdout).unwrap_or_else(|_| {
        panic!("{engine}: {}", String::from_utf8_lossy(&compiled.stderr))
    })
}

fn compile(fixture: &str, engine: &str) -> Value {
    let program = compile_any(fixture, engine);
    assert_eq!(program["diagnostics"], json!([]), "{engine}");
    program
}

/// Rendered diagnostics, file paths cut.
fn diagnostics(fixture: &str, engine: &str) -> Vec<String> {
    let program = compile_any(fixture, engine);
    program["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| render(diagnostic, &Value::Null))
        .collect()
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
const CHECKED: [&str; 9] = [
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

#[test]
fn hand_expanded_call_agrees_on_every_engine() {
    for engine in ENGINES {
        let program = compile("0_hand_expanded.dl7", engine);
        let rows = select(&program, &["passes", "discharged", "extends_diagnostic"]);
        assert_eq!(rows, CHECKED, "{engine}");
    }
}

/// `(?Checker ..)` expands to the hand-written branches: the rows of `0_hand_expanded.dl7`.
#[test]
fn variable_call_agrees_with_the_hand_expansion() {
    for engine in ENGINES {
        let program = compile("2_variable_call.dl7", engine);
        let rows = select(&program, &["passes", "discharged", "extends_diagnostic"]);
        assert_eq!(rows, CHECKED, "{engine}");
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

/// Fifty products and one ask through `(?Checker ..)`: the same walk as `1_demand.dl7`.
#[test]
fn variable_call_checks_only_the_asked_pair() {
    let expected = vec![
        "Extends: ref(User), ref(Named), ref(User)",
        "candidate: ref(User), ref(Named)",
        "candidate: ref(primitive(str)), ref(primitive(str))",
        "passes: ref(User), ref(Named), ref(User)",
    ];
    for engine in ENGINES {
        let program = compile("3_variable_demand.dl7", engine);
        let rows = select(&program, &["passes", "Extends", "candidate", "fails_at", "fails"]);
        assert_eq!(rows, expected, "{engine}");
    }
}

/// `(?Policy.check ..)`: `LoosePolicy` checks with `Extends`, `StrictPolicy` with `Invariant`.
#[test]
fn dotted_call_reads_the_owner_edge() {
    let expected = vec!["passes: ref(Account), ref(Owner), ref(Account)"];
    for engine in ENGINES {
        let program = compile("4_dotted_call.dl7", engine);
        assert_eq!(select(&program, &["passes"]), expected, "{engine}");
    }
}

/// A sum reaches the call: no relation to run, so the call site reports it.
#[test]
fn variable_call_reports_a_name_that_is_no_relation() {
    let expected = vec![
        "diagnostic(check, reader_node(34), not_built_yet(call, ref(owner(file, expansion_node(reader_node(39), infix_colon, 1, 3)))))",
    ];
    for engine in ENGINES {
        assert_eq!(diagnostics("5_not_built_yet.dl7", engine), expected, "{engine}");
    }
}

/// `Pair` has two columns; the call passes three.
#[test]
fn variable_call_reports_an_arity_mismatch() {
    let expected = vec![
        "diagnostic(check, reader_node(34), arity_mismatch(ref(owner(file, expansion_node(reader_node(39), infix_colon, 1, 3))), 2, 3))",
    ];
    for engine in ENGINES {
        assert_eq!(diagnostics("6_arity_mismatch.dl7", engine), expected, "{engine}");
    }
}

/// `field` and `product` declared in a module are that module's relations; the imported
/// module keeps the kernel `product`.
#[test]
fn declared_kernel_names_shadow_in_their_module() {
    let expected = vec![
        "declared_type: ref(Gadget)",
        "declared_type: ref(Signup)",
        "listed: ref(Signup), const(\"listing\")",
        "required: ref(Signup), const(\"email\")",
    ];
    for engine in ENGINES {
        let program = compile("7_shadow.dl7", engine);
        let rows: Vec<String> = select(&program, &["required", "listed", "declared_type"])
            .into_iter()
            .filter(|row| !row.starts_with("declared_type") || row.contains("Gadget") || row.contains("Signup"))
            .collect();
        assert_eq!(rows, expected, "{engine}");
    }
}
