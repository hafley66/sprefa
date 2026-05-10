#![cfg(feature = "ghcache")]

use std::sync::Arc;

use effect_runtime::v2::{
    attach_dirty_to_queue, expand, EventBus, ExpandOpts, FactStore, MemFactStore, MemQueue,
    PipeInstance, QueueBackend,
};
use rusqlite::Connection;

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::config::{DaemonConfig, SprfConfig};
use v4::git_watch::{ghcache_dirty_notices, git_pr_dirty_key, GIT_PR_DOMAIN, GIT_REPO_DOMAIN};
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

#[test]
fn gh_prs_queries_open_pr_rows_from_ghcache_db() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gh.db");
    seed_ghcache_db(&db);

    let src = r#"
        rule(:seen_prs, REPO?, NUMBER?, TITLE?, STATE?, HEAD?, BASE?, UPDATED_AT?);

        gh.prs(REPO: REPO?, NUMBER: NUMBER?, TITLE: TITLE?, STATE: `open`, HEAD: HEAD?, BASE: BASE?, UPDATED_AT: UPDATED_AT?)
          > seen_prs(REPO, NUMBER, TITLE, STATE, HEAD, BASE, UPDATED_AT);
    "#;

    let store = run_pipes_with_ghcache(src, db);
    let rows = store.rows_of("seen_prs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("REPO"), Some("acme/api"));
    assert_eq!(rows[0].get("NUMBER"), Some("17"));
    assert_eq!(rows[0].get("TITLE"), Some("Add billing sync"));
    assert_eq!(rows[0].get("STATE"), Some("open"));
    assert_eq!(rows[0].get("HEAD"), Some("feature/billing"));
    assert_eq!(rows[0].get("BASE"), Some("main"));
    assert_eq!(rows[0].get("UPDATED_AT"), Some("2026-05-10T10:00:00Z"));
}

#[test]
fn gh_prs_rerun_retracts_prior_supported_rule_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gh.db");
    seed_ghcache_db(&db);

    let src = r#"
        rule(:seen_prs, REPO?, NUMBER?, TITLE?, STATE?, HEAD?, BASE?, UPDATED_AT?);

        gh.prs(REPO: REPO?, NUMBER: NUMBER?, TITLE: TITLE?, STATE: `open`, HEAD: HEAD?, BASE: BASE?, UPDATED_AT: UPDATED_AT?)
          > seen_prs(REPO, NUMBER, TITLE, STATE, HEAD, BASE, UPDATED_AT);
    "#;

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    run_pipes_into_store_with_ghcache(src, db.clone(), store.clone());
    update_pr_title(&db, "Add billing sync v2", "2026-05-10T11:00:00Z");
    run_pipes_into_store_with_ghcache(src, db, store.clone());

    let rows = store.rows_of("seen_prs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("TITLE"), Some("Add billing sync v2"));
    assert_eq!(rows[0].get("UPDATED_AT"), Some("2026-05-10T11:00:00Z"));
}

#[test]
fn gh_prs_parked_query_wakes_from_pr_dirty_domain() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("gh.db");
    seed_ghcache_db(&db);

    let src = r#"
        rule(:seen_prs, REPO?, NUMBER?, TITLE?, STATE?, HEAD?, BASE?, UPDATED_AT?);

        gh.prs(REPO: REPO?, NUMBER: NUMBER?, TITLE: TITLE?, STATE: `open`, HEAD: HEAD?, BASE: BASE?, UPDATED_AT: UPDATED_AT?)
          > seen_prs(REPO, NUMBER, TITLE, STATE, HEAD, BASE, UPDATED_AT);
    "#;

    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    let bus = Arc::new(EventBus::new());
    attach_dirty_to_queue(&bus, queue.clone());
    let inst = lower_single_pipe(src, db.clone(), store.clone());

    expand(
        &inst,
        queue.clone(),
        vec![Arc::new(Cursor::default())],
        ExpandOpts::default().with_bus(bus.clone()),
    );
    assert_eq!(queue.depth(), 1);

    update_pr_title(&db, "Add billing sync v3", "2026-05-10T12:00:00Z");
    bus.dispatch_dirty(GIT_PR_DOMAIN, None);
    expand(
        &inst,
        queue,
        Vec::new(),
        ExpandOpts::default().with_bus(bus),
    );

    let rows = store.rows_of("seen_prs");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("TITLE"), Some("Add billing sync v3"));
    assert_eq!(rows[0].get("UPDATED_AT"), Some("2026-05-10T12:00:00Z"));
}

#[test]
fn ghcache_pull_request_change_maps_to_repo_and_pr_dirty_keys() {
    let change = v4::app::GhcacheChangeReq {
        id: 4,
        entity_type: "pull_request".into(),
        entity_id: 99,
        event: "updated".into(),
        repo_slug: Some("acme/api".into()),
        payload: serde_json::json!({"number": 17}),
        occurred_at: "2026-05-10T10:05:00Z".into(),
    };

    assert_eq!(
        ghcache_dirty_notices(&change),
        vec![
            (
                GIT_REPO_DOMAIN.to_string(),
                Some(v4::git_watch::git_repo_dirty_key("acme/api"))
            ),
            (GIT_PR_DOMAIN.to_string(), None),
            (
                GIT_PR_DOMAIN.to_string(),
                Some(git_pr_dirty_key("acme/api", 17))
            ),
        ],
    );
}

fn run_pipes_with_ghcache(src: &str, ghcache_db: std::path::PathBuf) -> Arc<dyn FactStore<Cursor>> {
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    run_pipes_into_store_with_ghcache(src, ghcache_db, store.clone());
    store
}

fn run_pipes_into_store_with_ghcache(
    src: &str,
    ghcache_db: std::path::PathBuf,
    store: Arc<dyn FactStore<Cursor>>,
) {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let reg = default_registry();
    let cfg = SprfConfig {
        daemon: DaemonConfig {
            ghcache_db: Some(ghcache_db),
            ..DaemonConfig::default()
        },
        ..SprfConfig::empty()
    };
    let mut ctx = LowerCtx::new(store.clone(), std::env::temp_dir()).with_config(Arc::new(cfg));
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");

    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for pipe in pipes {
        expand(
            &pipe.into_instance(),
            queue.clone(),
            vec![Arc::new(Cursor::default())],
            ExpandOpts::default(),
        );
    }
}

fn lower_single_pipe(
    src: &str,
    ghcache_db: std::path::PathBuf,
    store: Arc<dyn FactStore<Cursor>>,
) -> PipeInstance<Cursor> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse diags: {parse_diags:?}");

    let reg = default_registry();
    let cfg = SprfConfig {
        daemon: DaemonConfig {
            ghcache_db: Some(ghcache_db),
            ..DaemonConfig::default()
        },
        ..SprfConfig::empty()
    };
    let mut ctx = LowerCtx::new(store, std::env::temp_dir()).with_config(Arc::new(cfg));
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk diags: {walk_diags:?}");
    pipes.into_iter().last().unwrap().into_instance()
}

fn seed_ghcache_db(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE repo (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            owner TEXT NOT NULL,
            name TEXT NOT NULL,
            default_branch TEXT NOT NULL DEFAULT 'main'
        );
        CREATE TABLE pull_request (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            repo_id INTEGER NOT NULL REFERENCES repo(id),
            number INTEGER NOT NULL,
            state TEXT NOT NULL,
            title TEXT NOT NULL,
            author TEXT,
            head_ref TEXT,
            head_sha TEXT,
            base_ref TEXT,
            mergeable TEXT,
            draft INTEGER NOT NULL DEFAULT 0,
            additions INTEGER,
            deletions INTEGER,
            changed_files INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            merged_at TEXT,
            closed_at TEXT,
            body TEXT,
            raw_json TEXT,
            UNIQUE(repo_id, number)
        );
        INSERT INTO repo (id, owner, name, default_branch)
        VALUES (1, 'acme', 'api', 'main');
        INSERT INTO pull_request
            (repo_id, number, state, title, author, head_ref, head_sha, base_ref, draft, created_at, updated_at)
        VALUES
            (1, 17, 'open', 'Add billing sync', 'chris', 'feature/billing', 'abc123', 'main', 0, '2026-05-09T10:00:00Z', '2026-05-10T10:00:00Z'),
            (1, 18, 'closed', 'Old change', 'chris', 'feature/old', 'def456', 'main', 0, '2026-05-08T10:00:00Z', '2026-05-09T10:00:00Z');
        "#,
    ).unwrap();
}

fn update_pr_title(path: &std::path::Path, title: &str, updated_at: &str) {
    let conn = Connection::open(path).unwrap();
    conn.execute(
        "UPDATE pull_request SET title = ?1, updated_at = ?2 WHERE number = 17",
        (title, updated_at),
    )
    .unwrap();
}
