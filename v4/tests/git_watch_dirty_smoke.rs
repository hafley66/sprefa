#![cfg(feature = "ghcache")]

use std::sync::Arc;

use serde_json::json;
use v4::app::{
    build_router, GhcacheChangeReq, InProcessClient, NotifyGhcacheChangeReq, SprfClient, SprfState,
};
use v4::git_watch::{
    dirty_source_uri, git_branch_dirty_key, git_repo_dirty_key, GIT_BRANCH_DOMAIN, GIT_REPO_DOMAIN,
};
use v4::runtime_graph::RUNTIME_DIRTY;

#[tokio::test]
async fn ghcache_branch_change_dispatches_repo_and_branch_dirty_keys() {
    let root = tempfile::tempdir().unwrap();
    let state = Arc::new(SprfState::new(root.path().to_path_buf()));
    let client = InProcessClient::new(build_router(state.clone()));

    let repo_key = git_repo_dirty_key("acme/api");
    let branch_key = git_branch_dirty_key("acme/api", "main");
    let owner = state.runtime_graph.declare_owner(
        "sprf://ast/test/ghcache-branch",
        None,
        "input",
        "source",
        "test",
        1,
    );
    let branch_source = state.runtime_graph.declare_source(
        dirty_source_uri(GIT_BRANCH_DOMAIN, Some(branch_key)).as_str(),
        1,
    );
    state
        .runtime_graph
        .subscribe(&owner, "branch", &branch_source, 1);

    let notices = client
        .notify_ghcache_change(NotifyGhcacheChangeReq {
            change: GhcacheChangeReq {
                id: 9,
                entity_type: "branch".into(),
                entity_id: 7,
                event: "updated".into(),
                repo_slug: Some("acme/api".into()),
                payload: json!({"name": "main", "sha": "abc123"}),
                occurred_at: "2026-05-09T00:00:00Z".into(),
            },
        })
        .await
        .unwrap();

    let pairs: Vec<(&str, Option<String>)> = notices
        .iter()
        .map(|n| (n.domain.as_str(), n.key_hex.clone()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            (GIT_REPO_DOMAIN, Some(hex_key(repo_key))),
            (GIT_BRANCH_DOMAIN, Some(hex_key(branch_key))),
        ],
    );
    assert_eq!(state.queue.depth(), 0);
    assert!(
        state.facts.len(RUNTIME_DIRTY) >= 1,
        "ghcache graph wake should mark subscribed owners dirty",
    );
}

fn hex_key(key: effect_runtime::v2::NextKey) -> String {
    key.0.iter().map(|b| format!("{b:02x}")).collect()
}
