//! Phase 3 of `v4/plans/lsp-diags-to-claude-code-full.md`: `lsp_pre_warm`
//! walks a workspace, opens every `.sprf` file via the existing
//! `lsp_open` path, and (because of Phase 2 cross-URI routing)
//! produces per-target-file diagnostics that `lsp_diags_by_uri`
//! returns without any explicit editor-driven `lsp_open` call.

use v4::app::{
    build_in_process, LspDiagsByUriReq, LspPreWarmReq, SprfClient,
};

#[tokio::test]
async fn lsp_pre_warm_walks_workspace_and_publishes_cross_uri_diags() {
    let tmp = tempfile::tempdir().unwrap();
    let a_rs = tmp.path().join("a.rs");
    std::fs::write(&a_rs, "fn alpha() {}\n").unwrap();

    let lint_sprf = tmp.path().join("lint.sprf");
    let lint_src = r#"
fs > glob`**/*.rs`
   > ast_yaml(:rs)`
       rule:
         kind: function_item
     `
   > lsp_error(:test-pre-warm)`fn at ${FS}`;
"#;
    std::fs::write(&lint_sprf, lint_src).unwrap();

    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let resp = client
        .lsp_pre_warm(LspPreWarmReq {
            root: tmp.path().to_path_buf(),
            scm_branch: "main".into(),
        })
        .await
        .unwrap();

    let lint_uri = url::Url::from_file_path(&lint_sprf).unwrap().to_string();
    assert!(
        resp.source_uris.iter().any(|u| u == &lint_uri),
        "expected pre-warm to include {lint_uri}, got {:?}",
        resp.source_uris,
    );

    let a_uri = url::Url::from_file_path(&a_rs).unwrap().to_string();
    let diags = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri.clone(),
        })
        .await
        .unwrap();

    let keys: Vec<&String> = diags.by_uri.keys().collect();
    assert!(
        diags.by_uri.contains_key(&a_uri),
        "expected cross-URI diag on {a_uri} after pre-warm; keys={keys:?}",
    );
    let a_diags = diags.by_uri.get(&a_uri).unwrap();
    assert!(!a_diags.is_empty(), "no diags published on {a_uri}");
    assert_eq!(a_diags[0].code, "test-pre-warm");
    assert_eq!(a_diags[0].uri.as_deref(), Some(a_uri.as_str()));
}
