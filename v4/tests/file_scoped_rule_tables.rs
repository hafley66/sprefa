//! Phase A of `v4/plans/file-scoped-rule-tables.md`.
//!
//! Verifies that two `.sprf` files declaring the same rule name with
//! different column shapes lower to distinct backing tables in the
//! shared `FactStore`. This is the bug the LSP daemon hit on
//! `v4/bench/*.sprf` pre-warm before the fix.

use v4::app::{build_in_process, GetDiagsReq, LspOpenReq, SprfClient};

#[tokio::test]
async fn two_files_same_rule_name_different_shapes_dont_collide() {
    // Two synthetic .sprf URIs declaring `rule(:hits, ...)` with
    // distinct column lists. Both are head-of-pipe decl-only forms.
    // Before Phase A this panicked the FactStore at fact_store.rs:940;
    // after Phase A each lowers to a distinct prefixed table.
    let tmp = tempfile::tempdir().unwrap();
    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let uri_a = "file:///proj/a.sprf";
    let uri_b = "file:///proj/b.sprf";

    client
        .lsp_open(LspOpenReq {
            uri: uri_a.into(),
            text: "rule(:hits, FS?, LO?);\n".into(),
            version: 1,
        })
        .await
        .unwrap();
    // Before Phase A, this call would panic inside `ingest`'s declare
    // path. The test passing at all is the regression proof.
    client
        .lsp_open(LspOpenReq {
            uri: uri_b.into(),
            text: "rule(:hits, FS?, LO?, HI?);\n".into(),
            version: 1,
        })
        .await
        .unwrap();

    // Both docs are queryable and have zero diagnostics (decl-only rules
    // emit nothing; the test is about the absence of a panic, not output).
    let da = client
        .get_diags(GetDiagsReq {
            uri: uri_a.into(),
        })
        .await
        .unwrap();
    let db = client
        .get_diags(GetDiagsReq {
            uri: uri_b.into(),
        })
        .await
        .unwrap();
    assert_eq!(da.len(), 0, "uri_a parse/walk/runtime diags: {da:?}");
    assert_eq!(db.len(), 0, "uri_b parse/walk/runtime diags: {db:?}");
}

#[tokio::test]
async fn same_file_rule_reads_its_own_table_after_prefix() {
    // Smoke: a self-driving rule produces rows and a downstream sink
    // reads them in the SAME file. After Phase A the sink_table is
    // prefixed but `ctx.rules` lookup keys by user atom, so the call
    // dispatch is unchanged. The body sinks into the prefixed table
    // and the lsp_warn sink reads from the same prefixed table.
    let tmp = tempfile::tempdir().unwrap();
    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    // Decouple decl from sink-write, the v3_parity_target style: the
    // rule is declared once, then three pipe statements push rows
    // through, then a fourth statement queries it back.
    let src = r#"
rule(:counter, N?);

`1` > term_bind(:N) > rule(:counter, N: N);
`2` > term_bind(:N) > rule(:counter, N: N);
`3` > term_bind(:N) > rule(:counter, N: N);

counter?(N?) > lsp_warn(:cnt)`row ${N}`;
"#;

    let uri = "file:///proj/counter.sprf";
    client
        .lsp_open(LspOpenReq {
            uri: uri.into(),
            text: src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let diags = client
        .get_diags(GetDiagsReq {
            uri: uri.into(),
        })
        .await
        .unwrap();
    let warns: Vec<_> = diags
        .iter()
        .filter(|d| d.severity == "warning")
        .collect();
    assert_eq!(
        warns.len(),
        3,
        "expected 3 row diags (1,2,3); got {} warns; all diags: {diags:?}",
        warns.len(),
    );
}
