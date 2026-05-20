//! MVP-6a: `lsp_wake_fs` drives the existing reactive arc.
//!
//! The Watchman subscription producer is out of scope here (Phase 6
//! integration / Phase 6b). This test invokes `lsp_wake_fs` directly
//! to verify the daemon's wake handler:
//!   1. bumps `SourceClock::for_file(path)`,
//!   2. dispatches `SourceWake::dirty(file_dirty_source_uri(path), gen)`,
//!   3. drains the dispatcher,
//!   4. re-ingests every open `.sprf` doc,
//!   5. returns the `.sprf` URIs the caller should re-poll.
//!
//! Together with Phase 2's cross-URI publish, this is the user-visible
//! "edit a .rs file outside the IDE; lint diag updates" path.

use v4::app::{
    build_in_process, LspBufferCloseReq, LspBufferOpenReq, LspDiagsByUriReq, LspOpenReq,
    LspWakeFsReq, SprfClient,
};

fn lint_src() -> &'static str {
    r#"
fs > glob`**/*.rs`
   > ast_yaml(:rs)`
       rule:
         kind: function_item
     `
   > lsp_error(:wake-target)`fn at ${FS}`;
"#
}

#[tokio::test]
async fn lsp_wake_fs_picks_up_disk_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let target_rs = tmp.path().join("target.rs");
    std::fs::write(&target_rs, "fn alpha() {}\n").unwrap();

    let lint_sprf = tmp.path().join("lint.sprf");
    std::fs::write(&lint_sprf, lint_src()).unwrap();

    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let lint_uri = url::Url::from_file_path(&lint_sprf).unwrap().to_string();

    // Initial ingest of the lint. Target has one fn => one diag.
    client
        .lsp_open(LspOpenReq {
            uri: lint_uri.clone(),
            text: lint_src().into(),
            version: 1,
        })
        .await
        .unwrap();

    let first = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri.clone(),
        })
        .await
        .unwrap();
    let target_key = first
        .by_uri
        .keys()
        .find(|k| k.ends_with("target.rs"))
        .cloned()
        .unwrap_or_else(|| panic!("expected target.rs key in {:?}", first.by_uri.keys()));
    assert_eq!(
        first.by_uri.get(&target_key).unwrap().len(),
        1,
        "initial: 1 fn on disk => 1 diag",
    );

    // Edit target.rs on DISK (no LSP did_change). Without lsp_wake_fs
    // the daemon does not learn about this edit; the next refresh of
    // the .sprf doc would still see the cached identity. lsp_wake_fs
    // bumps the source clock and dispatches the wake so the existing
    // subscribe-edge arc re-renders the rule.
    std::fs::write(&target_rs, "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n").unwrap();

    let wake = client
        .lsp_wake_fs(LspWakeFsReq {
            paths: vec![target_rs.clone()],
        })
        .await
        .unwrap();
    assert_eq!(wake.paths_seen, 1);
    assert!(
        wake.woken_sprf_uris.iter().any(|u| u == &lint_uri),
        "expected the open lint URI to be returned in woken_sprf_uris; got {:?}",
        wake.woken_sprf_uris,
    );

    // After wake the diag set reflects the new disk content.
    let second = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri.clone(),
        })
        .await
        .unwrap();
    let target_key2 = second
        .by_uri
        .keys()
        .find(|k| k.ends_with("target.rs"))
        .cloned()
        .unwrap_or_else(|| panic!("expected target.rs key in {:?}", second.by_uri.keys()));
    assert_eq!(
        second.by_uri.get(&target_key2).unwrap().len(),
        3,
        "after wake: 3 fns on disk => 3 diags",
    );
}

#[tokio::test]
async fn lsp_wake_fs_does_not_clobber_overlay() {
    // When a buffer overlays the path Watchman just reported as
    // changed, the next ingest still sees the overlay (Phase 4 wired
    // VfsOverlay into SourceReader). The disk write is suppressed by
    // the read path, NOT by a special-case in lsp_wake_fs.
    let tmp = tempfile::tempdir().unwrap();
    let target_rs = tmp.path().join("target.rs");
    std::fs::write(&target_rs, "fn only() {}\n").unwrap();

    let lint_sprf = tmp.path().join("lint.sprf");
    std::fs::write(&lint_sprf, lint_src()).unwrap();

    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let target_uri = url::Url::from_file_path(&target_rs).unwrap().to_string();
    let lint_uri = url::Url::from_file_path(&lint_sprf).unwrap().to_string();

    // Open the lint, then overlay target.rs with 5 fns. The overlay
    // text wins on the next expand.
    client
        .lsp_open(LspOpenReq {
            uri: lint_uri.clone(),
            text: lint_src().into(),
            version: 1,
        })
        .await
        .unwrap();
    client
        .lsp_buffer_open(LspBufferOpenReq {
            uri: target_uri.clone(),
            text: "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\nfn e() {}\n".into(),
        })
        .await
        .unwrap();

    // Re-run the lint so the overlay's 5-fn text takes effect first.
    client
        .lsp_open(LspOpenReq {
            uri: lint_uri.clone(),
            text: lint_src().into(),
            version: 2,
        })
        .await
        .unwrap();

    // Now simulate a disk edit + wake (e.g., Watchman fires).
    std::fs::write(&target_rs, "fn only() {}\n").unwrap();
    client
        .lsp_wake_fs(LspWakeFsReq {
            paths: vec![target_rs.clone()],
        })
        .await
        .unwrap();

    let resp = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri.clone(),
        })
        .await
        .unwrap();
    let target_key = resp
        .by_uri
        .keys()
        .find(|k| k.ends_with("target.rs"))
        .cloned()
        .unwrap_or_else(|| panic!("expected target.rs key in {:?}", resp.by_uri.keys()));
    assert_eq!(
        resp.by_uri.get(&target_key).unwrap().len(),
        5,
        "buffer overlay (5 fns) must win over disk wake (1 fn)",
    );

    // Close the buffer; subsequent wake reflects disk truth.
    client
        .lsp_buffer_close(LspBufferCloseReq {
            uri: target_uri.clone(),
        })
        .await
        .unwrap();
    client
        .lsp_wake_fs(LspWakeFsReq {
            paths: vec![target_rs.clone()],
        })
        .await
        .unwrap();
    let resp2 = client
        .lsp_diags_by_uri(LspDiagsByUriReq {
            source_uri: lint_uri,
        })
        .await
        .unwrap();
    let target_key2 = resp2
        .by_uri
        .keys()
        .find(|k| k.ends_with("target.rs"))
        .cloned()
        .unwrap_or_else(|| panic!("expected target.rs key in {:?}", resp2.by_uri.keys()));
    assert_eq!(
        resp2.by_uri.get(&target_key2).unwrap().len(),
        1,
        "after buffer close + wake: disk has 1 fn",
    );
}

#[tokio::test]
async fn lsp_wake_fs_with_no_open_sprfs_is_a_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let target_rs = tmp.path().join("target.rs");
    std::fs::write(&target_rs, "fn x() {}\n").unwrap();

    let (_state, client) = build_in_process(tmp.path().to_path_buf());
    let resp = client
        .lsp_wake_fs(LspWakeFsReq {
            paths: vec![target_rs],
        })
        .await
        .unwrap();
    assert_eq!(resp.paths_seen, 1);
    assert!(
        resp.woken_sprf_uris.is_empty(),
        "no open .sprf docs => empty woken list, got {:?}",
        resp.woken_sprf_uris,
    );
}

#[tokio::test]
async fn lsp_wake_fs_empty_paths_is_a_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let (_state, client) = build_in_process(tmp.path().to_path_buf());
    let resp = client
        .lsp_wake_fs(LspWakeFsReq { paths: vec![] })
        .await
        .unwrap();
    assert_eq!(resp.paths_seen, 0);
    assert!(resp.woken_sprf_uris.is_empty());
}
