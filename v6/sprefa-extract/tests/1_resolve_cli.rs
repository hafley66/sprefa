use std::process::Command;

const CALLER: &str = "tests/fixtures/resolve/0_caller.ts";
const CALLEE: &str = "tests/fixtures/resolve/1_callee.ts";
const GOLDEN: &str = include_str!("fixtures/resolve/2_resolved_edges.jsonl");

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
