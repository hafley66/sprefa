use std::process::Command;

const SOURCE: &str = "tests/fixtures/ast_pattern/0_rtkq.ts";

#[test]
fn ast_pattern_mode_batches_patterns_and_flattens_capture_spans() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args([
            "--ast-pattern",
            "create=createApi({ $$$BEFORE, endpoints: ($BUILDER) => ($BODY), $$$AFTER })",
            "--ast-capture",
            "create=BUILDER",
            "--ast-pattern",
            "inject=$API.injectEndpoints({ $$$BEFORE, endpoints: ($BUILDER) => ($BODY), $$$AFTER })",
            "--ast-capture",
            "inject=API",
            "--ast-capture",
            "inject=BUILDER",
            "--ast-pattern",
            "generic=const endpoints = { $ENDPOINT: $BUILDER.$KIND<$RESULT, $ARG>($CONFIG) }",
            "--ast-selector",
            "generic=pair",
            "--ast-capture",
            "generic=ENDPOINT",
            "--ast-capture",
            "generic=KIND",
            "--ast-pattern",
            "plain=const endpoints = { $ENDPOINT: $BUILDER.$KIND($CONFIG) }",
            "--ast-selector",
            "plain=pair",
            "--ast-capture",
            "plain=ENDPOINT",
            "--ast-capture",
            "plain=KIND",
            SOURCE,
        ])
        .output()
        .expect("extract binary runs");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        include_str!("fixtures/ast_pattern/1_expected.jsonl")
    );
}

#[test]
fn ast_pattern_mode_refuses_unknown_capture() {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .args([
            "--ast-pattern",
            "create=createApi($CONFIG)",
            "--ast-capture",
            "create=MISSING",
            SOURCE,
        ])
        .output()
        .expect("extract binary runs");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("ast pattern 'create' does not define capture 'MISSING'"));
}
