//! The project-mode CLI contract. Every golden here PINS a JSONL record shape:
//! the v6 host decodes these rows by top-level key, so adding or renaming a
//! field is a breaking change and has to show up as a golden diff.

use std::process::Command;

const CALLER: &str = "tests/fixtures/resolve/0_caller.ts";
const CALLEE: &str = "tests/fixtures/resolve/1_callee.ts";
const GOLDEN: &str = include_str!("fixtures/resolve/2_resolved_edges.jsonl");
const KOTLIN: &str = "tests/fixtures/resolve/3_kotlin.kt";
const KOTLIN_GOLDEN: &str = include_str!("fixtures/resolve/4_kotlin_resolved_edges.jsonl");
const TYPE_EDGE_GOLDEN: &str = include_str!("fixtures/resolve/5_resolved_type_edges.jsonl");
const GO_TYPE_EDGE_GOLDEN: &str = include_str!("fixtures/resolve/6_go_resolved_type_edges.jsonl");
const CLOSURE_CALLER: &str = "tests/fixtures/resolve/7_closure_caller.rs";
const CLOSURE_CALLEE: &str = "tests/fixtures/resolve/8_closure_callee.rs";
const CLOSURE_GOLDEN: &str = include_str!("fixtures/resolve/9_closure_resolved_edges.jsonl");

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

#[test]
fn resolve_mode_streams_cross_file_edges() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(["--resolve", CALLER, CALLEE])
        .output()
        .expect("extract binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), GOLDEN);
}

#[test]
fn resolve_mode_dispatches_kotlin_call_edges() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args(["--resolve", KOTLIN])
        .output()
        .expect("extract binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), KOTLIN_GOLDEN);
}

/// `record=resolved_type_edge`, the `Resolve<TypeF>` arm's wire shape. It joined
/// the contract with this lane, so this golden is what pins its field names.
/// Kinds here are the TS arm's: field, param, returns.
#[test]
fn resolve_type_arm_streams_resolved_type_edges() {
    assert_eq!(
        run(&[
            "--resolve",
            "--family",
            "type",
            "tests/fixtures/ts/sample.ts",
            "tests/fixtures/ts/consts.ts",
        ]),
        TYPE_EDGE_GOLDEN
    );
}

/// The same record from a different language arm, so the golden pins the shape
/// rather than one projector's habits. Go contributes the field/impl/generic
/// kinds TS does not emit.
#[test]
fn resolve_type_arm_covers_the_go_edge_kinds() {
    assert_eq!(
        run(&[
            "--resolve",
            "--family",
            "type",
            "tests/fixtures/go/edges.go",
            "tests/fixtures/go/sample.go",
        ]),
        GO_TYPE_EDGE_GOLDEN
    );
}

/// FAIL-FIRST RECEIPT: the pre-fix binary emits this same row with
/// `"caller_name":null`, and the `text` column on `resolve_at` drops it, so the
/// call inside the closure has no target in `call_target` at all.
#[test]
fn resolve_names_a_closure_caller() {
    assert_eq!(
        run(&["--resolve", CLOSURE_CALLER, CLOSURE_CALLEE]),
        CLOSURE_GOLDEN
    );
    assert!(!CLOSURE_GOLDEN.contains("\"caller_name\":null"));
}

/// The default arm is unchanged by the arrival of `--family` in project mode:
/// bare `--resolve` is still call edges only. This is the back-compat pin for
/// every existing caller of the flag.
#[test]
fn resolve_default_arm_stays_call_only() {
    let default = run(&["--resolve", CALLER, CALLEE]);
    assert_eq!(
        default,
        run(&["--resolve", "--family", "call", CALLER, CALLEE])
    );
    assert!(!default.contains("resolved_type_edge"));
}
