//! Corpus-battery regression: same-name helper in two files, unqualified call
//! inside an inline `mod`. Rust scoping says the callee is the same file's
//! crate-root fn; the resolver used to hand the call to the other file.

use std::process::Command;

const A: &str = "tests/fixtures/resolve/12_rust_scope_helper_a.rs";
const B: &str = "tests/fixtures/resolve/13_rust_scope_helper_b.rs";

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
fn same_file_scope_beats_foreign_top_level_def() {
    let out = run(&["--resolve", A, B]);
    let mut edges = out.lines().filter(|l| l.contains("resolved_edge"));
    let edge = edges.next().expect("one resolved edge");
    assert!(
        edge.contains(r#""callee_path":"tests/fixtures/resolve/13_rust_scope_helper_b.rs""#),
        "callee should be the same file's helper, got: {edge}"
    );
}
