//! Phase 4 narrow scope: when an `.rs` file is overlaid with a buffer
//! whose content differs from disk, the lint must run against the
//! buffer text. Also covers the post-close path: removing the overlay
//! resumes disk reads on the next expand.
//!
//! Drives the in-process axum router via `build_in_process`. Writes
//! `target.rs` with one `fn`, overlays a buffer carrying three `fn`s,
//! runs the lint, asserts 3 diagnostics. Then closes the buffer and
//! re-runs the lint, asserts 1 diagnostic (the on-disk count).

use v4::app::{
    build_in_process, LspBufferCloseReq, LspBufferOpenReq, LspDiagsByUriReq, LspOpenReq, SprfClient,
};

#[tokio::test]
async fn buffer_overlay_supersedes_disk_for_lint_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let target_rs = tmp.path().join("target.rs");
    std::fs::write(&target_rs, "fn alpha() {}\n").unwrap();

    let lint_sprf = tmp.path().join("lint.sprf");
    let lint_src = r#"
fs > glob`**/*.rs`
   > ast_yaml(:rs)`
       rule:
         kind: function_item
     `
   > lsp_error(:test-overlay)`fn at ${FS}`;
"#;
    std::fs::write(&lint_sprf, lint_src).unwrap();

    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let target_uri = url::Url::from_file_path(&target_rs).unwrap().to_string();
    let lint_uri = url::Url::from_file_path(&lint_sprf).unwrap().to_string();

    // Overlay target.rs with THREE fns (buffer-only; disk has one).
    let buffer_text = "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n";
    client
        .lsp_buffer_open(LspBufferOpenReq {
            uri: target_uri.clone(),
            text: buffer_text.into(),
        })
        .await
        .unwrap();

    // Run the lint.
    client
        .lsp_open(LspOpenReq {
            uri: lint_uri.clone(),
            text: lint_src.into(),
            version: 1,
        })
        .await
        .unwrap();

    let resp = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri.clone(),
        })
        .await
        .unwrap();

    // The lint emits the target's URI from the cursor's FS term, which
    // is the literal path the walker passed to fs (the tempdir-relative
    // /var/... form on macOS, not the canonicalized /private/var/...).
    // Compare by file stem instead so the test is platform-stable.
    let keys: Vec<String> = resp.by_uri.keys().cloned().collect();
    let target_key = keys
        .iter()
        .find(|k| k.ends_with("target.rs"))
        .unwrap_or_else(|| panic!("expected a target.rs key in {keys:?}"));
    let diags = resp.by_uri.get(target_key).unwrap();
    assert_eq!(
        diags.len(),
        3,
        "buffer has 3 fns; expected 3 diags, got {}: {diags:?}",
        diags.len()
    );

    // Close the buffer and re-run; should reflect disk's 1 fn.
    client
        .lsp_buffer_close(LspBufferCloseReq {
            uri: target_uri.clone(),
        })
        .await
        .unwrap();
    client
        .lsp_open(LspOpenReq {
            uri: lint_uri.clone(),
            text: lint_src.into(),
            version: 2,
        })
        .await
        .unwrap();
    let resp2 = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri,
        })
        .await
        .unwrap();
    let keys2: Vec<String> = resp2.by_uri.keys().cloned().collect();
    let target_key2 = keys2
        .iter()
        .find(|k| k.ends_with("target.rs"))
        .unwrap_or_else(|| panic!("expected a target.rs key in {keys2:?}"));
    let diags2 = resp2.by_uri.get(target_key2).unwrap();
    assert_eq!(diags2.len(), 1, "after close, disk has 1 fn");
}
