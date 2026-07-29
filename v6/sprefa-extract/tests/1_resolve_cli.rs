use std::process::Command;

const CALLER: &str = "tests/fixtures/resolve/0_caller.ts";
const CALLEE: &str = "tests/fixtures/resolve/1_callee.ts";
const GOLDEN: &str = include_str!("fixtures/resolve/2_resolved_edges.jsonl");
const KOTLIN: &str = "tests/fixtures/resolve/3_kotlin.kt";
const KOTLIN_GOLDEN: &str = include_str!("fixtures/resolve/4_kotlin_resolved_edges.jsonl");

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
