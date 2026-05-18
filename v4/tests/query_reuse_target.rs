//! RED → GREEN spec for the "second identical `r?()` query yields zero
//! rows" defect.
//!
//! Two syntactically-identical rule QUERY calls `r?(...)` in the SAME
//! `render.markdown` body must each emit the full row set. The defect:
//! the first projection emits all rows, the second emits none, silently
//! (no diagnostic). Confirmed empirically in
//! `v4/examples/arch-d2-map.sprf` before this fix.
//!
//! See chat_log/20260517.query-memoization-ramifications.md for the
//! correctness/scope analysis behind the fix this test pins.

use std::fs;

use v4::app::{build_in_process, RunReq};
use v4::app::{RunReport, SprfClient, SprfDiag};

fn diag_lines(diags: &[SprfDiag]) -> String {
    diags
        .iter()
        .map(|d| format!("{}:{}:{}", d.severity, d.code, d.message))
        .collect::<Vec<_>>()
        .join("\n")
}

fn report_lines(report: &RunReport) -> String {
    format!(
        "parse:\n{}\nwalk:\n{}\nruntime:\n{}",
        diag_lines(&report.parse_diags),
        diag_lines(&report.walk_diags),
        diag_lines(&report.runtime_diags),
    )
}

/// Two identical `thing?(K?, V?)` projection sub-pipes in ONE
/// `render.markdown` body. Each sub-pipe renders its own block. Both
/// blocks must contain every fact row.
#[tokio::test]
async fn two_identical_queries_in_one_render_body_both_yield_all_rows() {
    let root = tempfile::tempdir().unwrap();
    let out = root.path().join("qdup_out");
    let sprf = root.path().join("query_reuse.sprf");
    let src = format!(
        r#"
rule(:thing, K?, V?);

`a|1
b|2`
  > split`
`
  > re`(?P<K>[^|]+)\|(?P<V>.+)`
  > thing(K, V);

`out`
  > render.markdown`${{ thing?(K?, V?) > render.markdown`x ${{K}}=${{V}}
` }}
${{ thing?(K?, V?) > render.markdown`y ${{K}}=${{V}}
` }}`
  > write_file`{}`;
"#,
        out.display()
    );
    fs::write(&sprf, &src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq {
            path: sprf,
            root: Some(root.path().to_path_buf()),
        })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "must lower cleanly:\n{}",
        report_lines(&report)
    );

    let got = fs::read_to_string(&out).expect("write_file should create target file");

    // First projection (x block): both rows.
    assert!(
        got.contains("x a=1") && got.contains("x b=2"),
        "first query projection lost rows:\n{got}"
    );
    // Second projection (y block): both rows — this is the regression.
    assert!(
        got.contains("y a=1") && got.contains("y b=2"),
        "second identical query projection yielded zero rows (the defect):\n{got}"
    );
}

/// Two sequential identical `r?()` query pipes (separate statements,
/// not nested in one render body). Verifies whether the defect also
/// reproduces at statement scope or is render-body-local.
#[tokio::test]
async fn two_sequential_identical_query_pipes_both_yield_all_rows() {
    let root = tempfile::tempdir().unwrap();
    let out_a = root.path().join("seq_a");
    let out_b = root.path().join("seq_b");
    let sprf = root.path().join("query_reuse_seq.sprf");
    let src = format!(
        r#"
rule(:thing, K?, V?);

`a|1
b|2`
  > split`
`
  > re`(?P<K>[^|]+)\|(?P<V>.+)`
  > thing(K, V);

thing?(K?, V?) > render.markdown`a ${{K}}=${{V}}` > write_file`{}`;
thing?(K?, V?) > render.markdown`b ${{K}}=${{V}}` > write_file`{}`;
"#,
        out_a.display(),
        out_b.display(),
    );
    fs::write(&sprf, &src).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    let report = client
        .run(RunReq {
            path: sprf,
            root: Some(root.path().to_path_buf()),
        })
        .await
        .unwrap();

    assert!(
        report.parse_diags.is_empty() && report.walk_diags.is_empty(),
        "must lower cleanly:\n{}",
        report_lines(&report)
    );

    let a = fs::read_to_string(&out_a).expect("seq_a should exist");
    let b = fs::read_to_string(&out_b).expect("seq_b should exist");
    assert!(
        a.contains("a a=1") && a.contains("a b=2"),
        "first sequential query lost rows:\n{a}"
    );
    assert!(
        b.contains("b a=1") && b.contains("b b=2"),
        "second sequential query lost rows:\n{b}"
    );
}
