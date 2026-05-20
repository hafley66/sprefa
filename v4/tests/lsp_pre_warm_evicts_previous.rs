//! A second `lsp_pre_warm` round must evict URIs from the previous
//! round that no editor `lsp_open` claimed in the meantime. Without
//! eviction, `SprfState.docs` grows monotonically across sessions
//! that share a daemon.

use v4::app::{
    build_in_process, GetDiagsReq, LspPreWarmReq, SprfClient, SprfError,
};

#[tokio::test]
async fn second_pre_warm_evicts_orphaned_uris() {
    let tmp = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for i in 0..5 {
        let p = tmp.path().join(format!("lint_{i}.sprf"));
        std::fs::write(&p, format!("// lint {i}\n")).unwrap();
        paths.push(p);
    }

    let (_state, client) = build_in_process(tmp.path().to_path_buf());

    let first = client
        .lsp_pre_warm(LspPreWarmReq {
            root: tmp.path().to_path_buf(),
            scm_branch: "main".into(),
        })
        .await
        .unwrap();
    assert_eq!(first.source_uris.len(), 5, "expected 5 sprfs warmed");

    let removed = paths.remove(2);
    let removed_uri = url::Url::from_file_path(&removed).unwrap().to_string();
    std::fs::remove_file(&removed).unwrap();

    let second = client
        .lsp_pre_warm(LspPreWarmReq {
            root: tmp.path().to_path_buf(),
            scm_branch: "main".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        second.source_uris.len(),
        4,
        "expected 4 sprfs after one was deleted; got {:?}",
        second.source_uris,
    );

    // The deleted file's URI must no longer be a known doc.
    let probe = client
        .get_diags(GetDiagsReq {
            uri: removed_uri.clone(),
        })
        .await;
    match probe {
        Err(SprfError::UnknownDoc(_)) => {}
        other => panic!(
            "expected UnknownDoc for evicted URI {removed_uri}, got {other:?}",
        ),
    }

    // Surviving URIs are still present.
    for p in &paths {
        let uri = url::Url::from_file_path(p).unwrap().to_string();
        client
            .get_diags(GetDiagsReq { uri })
            .await
            .expect("surviving pre-warmed uri still queryable");
    }
}
