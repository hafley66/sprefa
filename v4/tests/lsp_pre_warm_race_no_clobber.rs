//! Race regression for `lsp_pre_warm`: pre-warm must not damage the
//! daemon's `docs` state for an URI an editor has already opened.
//!
//! Before the fix, pre-warm called `lsp_open(version=0)` on every
//! `.sprf` it found, which (at the daemon layer) is harmless because
//! `lsp_open` is idempotent; the editor-visible bug lived in
//! `Backend::refresh` writing `pending[uri]=0` and dropping the
//! editor's higher-version publish. This RPC-level test guards the
//! daemon-side invariant: re-opening at version=0 must not regress the
//! diags published for an editor-opened version=5.

use v4::app::{
    build_in_process, LspDiagsByUriReq, LspOpenReq, LspPreWarmReq, SprfClient,
};

#[tokio::test]
async fn pre_warm_does_not_clobber_editor_opened_doc() {
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
   > lsp_error(:test-no-clobber)`fn at ${FS}`;
"#;
    std::fs::write(&lint_sprf, lint_src).unwrap();

    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let lint_uri = url::Url::from_file_path(&lint_sprf).unwrap().to_string();
    let a_uri = url::Url::from_file_path(&a_rs).unwrap().to_string();

    // Editor opens at v5 first.
    client
        .lsp_open(LspOpenReq {
            uri: lint_uri.clone(),
            text: lint_src.into(),
            version: 5,
        })
        .await
        .unwrap();

    // Pre-warm runs after the editor and re-opens at version=0.
    let resp = client
        .lsp_pre_warm(LspPreWarmReq {
            root: tmp.path().to_path_buf(),
            scm_branch: "main".into(),
        })
        .await
        .unwrap();
    assert!(resp.source_uris.iter().any(|u| u == &lint_uri));

    // After pre-warm the daemon must still produce the v5 cross-URI
    // diagnostics for the editor's source URI.
    let diags = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri.clone(),
        })
        .await
        .unwrap();

    let keys: Vec<&String> = diags.by_uri.keys().collect();
    assert!(
        diags.by_uri.contains_key(&a_uri),
        "pre-warm damaged docs state; expected diags on {a_uri}, got keys={keys:?}",
    );
    let a_diags = diags.by_uri.get(&a_uri).unwrap();
    assert!(!a_diags.is_empty(), "no diags on {a_uri} after pre-warm");
    assert_eq!(a_diags[0].code, "test-no-clobber");
}
