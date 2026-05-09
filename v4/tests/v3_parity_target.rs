use std::fs;

use v4::app::{
    build_in_process, GetDiagsReq, RunReq, SprfClient, SprfDiag, RunReport,
    LspOpenReq,
};

fn diag_lines(diags: &[SprfDiag]) -> String {
    diags
        .iter()
        .map(|d| format!("{}:{}:{}", d.severity, d.code, d.message))
        .collect::<Vec<_>>()
        .join("\n")
}

fn report_lines(report: &RunReport) -> String {
    let mut lines = Vec::new();
    lines.push("parse:".to_string());
    lines.push(diag_lines(&report.parse_diags));
    lines.push("walk:".to_string());
    lines.push(diag_lines(&report.walk_diags));
    lines.join("\n")
}

#[tokio::test]
async fn runtime_lsp_warn_publishes_diagnostics_for_open_buffer() {
    let root = tempfile::tempdir().unwrap();
    let (_state, client) = build_in_process(root.path().to_path_buf());

    let uri = "file:///v3-parity-runtime-diag.sprf".to_string();
    let src = r#"
        rule(:warns) {
          `alpha`
          > lsp_warn(:v3_parity)`runtime warning`
        };
    "#;

    client
        .lsp_open(LspOpenReq { uri: uri.clone(), text: src.to_string(), version: 1 })
        .await
        .unwrap();

    let diags = client.get_diags(GetDiagsReq { uri }).await.unwrap();
    assert!(
        diags.iter().any(|d| {
            d.severity == "warning"
                && d.code == "v3_parity"
                && d.message == "runtime warning"
        }),
        "runtime lsp_warn should surface through get_diags, got:\n{}",
        diag_lines(&diags)
    );
}

#[tokio::test]
async fn write_file_backtick_path_writes_cursor_value() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("out.md");
    let sprf = root.path().join("write_file_backtick_path.sprf");
    let src = format!(
        "`hello from sprf` > write_file`{}`;",
        out.display()
    );
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "write_file backtick path should parse/lower cleanly:\n{}",
        report_lines(&report)
    );

    let written = fs::read_to_string(&out).expect("write_file should create target file");
    assert_eq!(written, "hello from sprf");
}

#[tokio::test]
async fn render_markdown_aggregate_writes_file() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("RECAP.md");
    let sprf = root.path().join("render_markdown_aggregate.sprf");
    let src = format!(
        r#"
        rule(:items, NAME?);

        `alpha` > term_bind(:NAME) > rule(:items, NAME: NAME);
        `beta`  > term_bind(:NAME) > rule(:items, NAME: NAME);

        items(NAME?)
        > render(:markdown)`- ${{NAME}}`
        > write_file`{}`;
        "#,
        out.display()
    );
    fs::write(&sprf, src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq { path: sprf.clone(), root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "markdown render/write should parse/lower cleanly:\n{}",
        report_lines(&report)
    );

    let first = fs::read_to_string(&out).expect("render should create markdown file");
    assert_eq!(first, "- alpha\n- beta\n");

    let report = client
        .run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) })
        .await
        .unwrap();
    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "second render/write run should stay clean:\n{}",
        report_lines(&report)
    );
    let second = fs::read_to_string(&out).expect("render output should still exist");
    assert_eq!(second, first, "render/write should be idempotent");
}
