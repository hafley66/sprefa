//! Phase 2 of `v4/plans/lsp-diags-to-claude-code-full.md`: a lint that
//! targets non-`.sprf` files must publish its diagnostics on those
//! files' URIs, not on the `.sprf` URI that defined the lint.
//!
//! Drives the in-process axum router via `build_in_process`. Writes
//! `a.rs` and `b.rs` fixtures, opens a `.sprf` document via
//! `lsp_open`, then verifies that `lsp_diags_by_uri` returns a map
//! keyed by the `.rs` `file://` URIs, with at least one `SprfDiag`
//! per file. The `.sprf` URI itself must NOT be a key (the lint
//! routes every diag to a non-`.sprf` target).

use v4::app::{
    build_in_process, LspDiagsByUriReq, LspOpenReq, SprfClient,
};

#[tokio::test]
async fn lsp_diags_by_uri_routes_to_rs_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let a_rs = tmp.path().join("a.rs");
    let b_rs = tmp.path().join("b.rs");
    std::fs::write(&a_rs, "fn alpha() {}\n").unwrap();
    std::fs::write(&b_rs, "fn beta() {}\n").unwrap();

    let lint_sprf = tmp.path().join("lint.sprf");
    let lint_src = r#"
fs > glob`**/*.rs`
   > ast_yaml(:rs)`
       rule:
         kind: function_item
     `
   > lsp_error(:test-cross-uri)`fn at ${FS}`;
"#;
    std::fs::write(&lint_sprf, lint_src).unwrap();

    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let source_uri = url::Url::from_file_path(&lint_sprf).unwrap().to_string();
    client
        .lsp_open(LspOpenReq {
            uri: source_uri.clone(),
            text: lint_src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let resp = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: source_uri.clone(),
        })
        .await
        .unwrap();

    let a_uri = url::Url::from_file_path(&a_rs).unwrap().to_string();
    let b_uri = url::Url::from_file_path(&b_rs).unwrap().to_string();

    let keys: Vec<&String> = resp.by_uri.keys().collect();
    assert!(
        resp.by_uri.contains_key(&a_uri),
        "expected key {a_uri} in {keys:?}",
    );
    assert!(
        resp.by_uri.contains_key(&b_uri),
        "expected key {b_uri} in {keys:?}",
    );
    assert!(
        !resp.by_uri.contains_key(&source_uri),
        "the .sprf URI must not appear as a target; keys={keys:?}",
    );

    let a_diags = resp.by_uri.get(&a_uri).unwrap();
    assert!(!a_diags.is_empty(), "no diags routed to {a_uri}");
    assert_eq!(a_diags[0].severity, "error");
    assert_eq!(a_diags[0].code, "test-cross-uri");
    assert_eq!(a_diags[0].uri.as_deref(), Some(a_uri.as_str()));

    let b_diags = resp.by_uri.get(&b_uri).unwrap();
    assert!(!b_diags.is_empty(), "no diags routed to {b_uri}");
    assert_eq!(b_diags[0].uri.as_deref(), Some(b_uri.as_str()));
}
