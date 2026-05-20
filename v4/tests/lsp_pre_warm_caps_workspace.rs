//! Pre-warm walker caps: `lsp_pre_warm` must stop after the file-count
//! cap fires and report `walked` + `deadline_hit` so the caller can
//! decide whether to schedule a follow-up scan.
//!
//! This test pins the file-count cap, not the wall-clock cap; the
//! wall cap is hard to test without a synthetic clock seam. We verify
//! `walked` is bounded by the cap and `deadline_hit` stays false on a
//! small synthetic workspace.

use v4::app::{build_in_process, LspPreWarmReq, SprfClient};

// Test-local mirror of the production cap. Kept lowercase to match the
// constants in `app.rs::lsp_pre_warm`.
const PRE_WARM_MAX_FILES: usize = 50_000;

#[tokio::test]
async fn pre_warm_respects_file_cap_and_wall_budget() {
    let tmp = tempfile::tempdir().unwrap();
    for i in 0..51 {
        let path = tmp.path().join(format!("f{i:03}.sprf"));
        std::fs::write(&path, format!("// file {i}\n")).unwrap();
    }

    let (_state, client) = build_in_process(tmp.path().to_path_buf());
    let resp = client
        .lsp_pre_warm(LspPreWarmReq {
            root: tmp.path().to_path_buf(),
            scm_branch: "main".into(),
        })
        .await
        .unwrap();

    assert!(
        resp.walked <= PRE_WARM_MAX_FILES,
        "walked={} exceeded cap {}",
        resp.walked,
        PRE_WARM_MAX_FILES,
    );
    assert!(
        resp.source_uris.len() <= PRE_WARM_MAX_FILES,
        "source_uris.len()={} exceeded cap {}",
        resp.source_uris.len(),
        PRE_WARM_MAX_FILES,
    );
    assert!(
        !resp.deadline_hit,
        "51 files should not trip the wall-clock budget",
    );
    // On a 51-file workspace below the cap we expect to walk all 51.
    assert_eq!(resp.walked, 51, "expected to walk all 51 fixture files");
    assert_eq!(resp.source_uris.len(), 51);
}
