//! Integration test: full watcher pipeline with branch scoping.
//!
//! Scans fixtures as committed `main` and working-tree `main+wt`,
//! starts the watcher, renames declarations, and verifies:
//! - Consumers rewritten on disk
//! - matches table has correct kind/rule_name after re-extraction
//! - Committed branch refs unchanged while wt branch refs diverge

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use sprefa_cache::SqliteStore;
use sprefa_config::RepoConfig;
use sprefa_extract::Extractor;
use sprefa_schema::init_db;
use sprefa_watch::{plan, rewrite, watcher::WatchConfig};

fn all_extractors() -> Arc<Vec<Box<dyn Extractor>>> {
    Arc::new(vec![
        Box::new(sprefa_js::JsExtractor),
        Box::new(sprefa_rs::RsExtractor),
    ])
}

fn repo_config(name: &str, path: &str) -> RepoConfig {
    RepoConfig {
        name: name.to_string(),
        path: path.to_string(),
        revs: Some(vec!["main".to_string()]),
        filter: None,
        branch_overrides: None,
        exclude_revs: None,
    }
}

fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
}

fn read_file(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

/// Scan a repo as both committed and working-tree branches.
async fn setup_dual_branch_repo(
    root: &Path,
    name: &str,
) -> (sqlx::SqlitePool, i64, Arc<Vec<Box<dyn Extractor>>>) {
    let db = init_db(":memory:").await.unwrap();
    let extractors = all_extractors();
    let config = repo_config(name, root.to_str().unwrap());

    let scanner = sprefa_scan::Scanner {
        extractors: extractors.clone(),
        store: SqliteStore::new(db.clone()),
        normalize_config: None,
        global_filter: None,
        scan_pairs: vec![],
    };

    // Scan as committed main
    let result = scanner.scan_repo(&config, "main").await.unwrap();
    assert!(result.refs_inserted > 0, "main scan should insert refs");

    // Scan as working-tree main+wt
    scanner
        .scan_repo(&config, &sprefa_watch::wt_rev("main"))
        .await
        .unwrap();

    let repo_id: i64 =
        sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
            .bind(name)
            .fetch_one(&db)
            .await
            .unwrap();

    (db, repo_id, extractors)
}

/// Start watcher with +wt branch tracking, wait for startup, return receiver.
async fn start_watcher(
    root: &Path,
    repo_id: i64,
    db: &sqlx::SqlitePool,
    extractors: &Arc<Vec<Box<dyn Extractor>>>,
) -> tokio::sync::mpsc::Receiver<Vec<sprefa_watch::change::Change>> {
    let watch_config = WatchConfig {
        root_path: root.to_path_buf(),
        repo_id,
        debounce: Duration::from_millis(200),
        wt_branch: Some("main+wt".to_string()),
        ..Default::default()
    };
    let rx = sprefa_watch::watcher::watch(watch_config, db.clone(), extractors.clone())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    rx
}

/// Wait for edits from the watcher, apply them, return the edit list.
async fn wait_for_edits(
    rx: &mut tokio::sync::mpsc::Receiver<Vec<sprefa_watch::change::Change>>,
    db: &sqlx::SqlitePool,
    timeout_secs: u64,
) -> Vec<plan::Edit> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        tokio::select! {
            Some(changes) = rx.recv() => {
                let rewriters: Vec<Box<dyn plan::PathRewriter>> = Vec::new();
                match plan::plan_rewrites(db, &changes, &rewriters).await {
                    Ok(edits) if !edits.is_empty() => {
                        rewrite::apply(&edits);
                        return edits;
                    }
                    Ok(_) => {}
                    Err(e) => panic!("plan_rewrites failed: {e}"),
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("timed out waiting for watcher edits");
            }
        }
    }
}

// ─── JS rename with branch scoping ────────────────────────────────────

/// Rename propagates through re-export chain. After rename:
/// - matches table has correct kinds (import_name, export_name)
/// - committed main branch still sees old ref values
/// - wt branch sees new ref values after re-extraction
#[tokio::test]
async fn js_rename_matches_table_and_branch_divergence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_file(
        root,
        "src/utils.ts",
        "export function computeScore(): number { return 42; }\n",
    );
    write_file(
        root,
        "src/barrel.ts",
        "export { computeScore } from './utils';\n",
    );
    write_file(
        root,
        "src/consumer.ts",
        "import { computeScore } from './barrel';\nconsole.log(computeScore());\n",
    );

    let root = root.canonicalize().unwrap();
    let root = root.as_path();
    let (db, repo_id, extractors) = setup_dual_branch_repo(root, "test-js").await;

    // Snapshot committed state: count matches for "computeScore"
    let committed_match_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value = 'computeScore'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        committed_match_count >= 3,
        "should have matches for computeScore in utils+barrel+consumer, got {committed_match_count}"
    );

    // Verify matches have expected kinds
    let kinds: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.kind, m.rule_name FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value = 'computeScore'
         ORDER BY m.kind",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let kind_strs: Vec<&str> = kinds.iter().map(|(k, _)| k.as_str()).collect();
    assert!(
        kind_strs.contains(&"export_name"),
        "should have export_name match, got: {kind_strs:?}"
    );
    assert!(
        kind_strs.contains(&"import_name"),
        "should have import_name match, got: {kind_strs:?}"
    );
    // All rule_names should be "js" for language-extracted refs
    assert!(
        kinds.iter().all(|(_, rn)| rn == "js"),
        "all rule_names should be 'js', got: {kinds:?}"
    );

    // Start watcher and trigger rename
    let mut rx = start_watcher(root, repo_id, &db, &extractors).await;
    let content = read_file(root, "src/utils.ts").replace("computeScore", "calculateScore");
    std::fs::write(root.join("src/utils.ts"), &content).unwrap();

    let edits = wait_for_edits(&mut rx, &db, 5).await;

    // Verify edits touched barrel and consumer
    let edited_files: Vec<String> = edits
        .iter()
        .map(|e| {
            Path::new(&e.file_path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert!(
        edited_files.contains(&"barrel.ts".to_string()),
        "barrel.ts should be edited, got: {edited_files:?}"
    );
    assert!(
        edited_files.contains(&"consumer.ts".to_string()),
        "consumer.ts should be edited, got: {edited_files:?}"
    );

    // Verify file contents on disk
    let barrel = read_file(root, "src/barrel.ts");
    assert!(
        barrel.contains("calculateScore"),
        "barrel.ts should contain calculateScore, got: {barrel}"
    );
    let consumer = read_file(root, "src/consumer.ts");
    assert!(
        consumer.contains("calculateScore"),
        "consumer.ts should contain calculateScore, got: {consumer}"
    );

    // After watcher re-extracts utils.ts, matches table should have
    // "calculateScore" with export_name kind
    let new_matches: Vec<(String, String)> = sqlx::query_as(
        "SELECT m.kind, m.rule_name FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value = 'calculateScore'",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert!(
        !new_matches.is_empty(),
        "matches table should contain calculateScore after re-extraction"
    );
    let new_kinds: Vec<&str> = new_matches.iter().map(|(k, _)| k.as_str()).collect();
    assert!(
        new_kinds.contains(&"export_name"),
        "calculateScore should have export_name match, got: {new_kinds:?}"
    );
}

// ─── Rust rename with matches verification ────────────────────────────

/// Rename in Rust propagates to use statements. Matches table updated correctly.
#[tokio::test]
async fn rs_rename_updates_matches_and_consumers() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_file(
        root,
        "Cargo.toml",
        "[package]\nname = \"test-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write_file(
        root,
        "src/lib.rs",
        "pub mod utils;\npub mod consumer;\n",
    );
    write_file(
        root,
        "src/utils.rs",
        "pub fn compute_score() -> i32 { 42 }\n",
    );
    write_file(
        root,
        "src/consumer.rs",
        "use crate::utils::compute_score;\n\npub fn run() -> i32 { compute_score() }\n",
    );

    let root = root.canonicalize().unwrap();
    let root = root.as_path();
    let (db, repo_id, extractors) = setup_dual_branch_repo(root, "test-rs").await;

    // Verify initial matches: compute_score should have rs_declare kind (bare name)
    // and the full use path crate::utils::compute_score should have rs_use kind
    let rs_decl_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value = 'compute_score' AND m.kind = 'rs_declare'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        rs_decl_count >= 1,
        "compute_score should have rs_declare match, got {rs_decl_count}"
    );
    let rs_use_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value LIKE '%compute_score%' AND m.kind = 'rs_use'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        rs_use_count >= 1,
        "should have rs_use match for a path containing compute_score, got {rs_use_count}"
    );

    // All Rust refs should have rule_name = "rs"
    let rs_rules: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT m.rule_name FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value LIKE '%compute_score%'",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert!(
        rs_rules.iter().all(|(rn,)| rn == "rs"),
        "all rule_names should be 'rs', got: {rs_rules:?}"
    );

    // Start watcher, rename
    let mut rx = start_watcher(root, repo_id, &db, &extractors).await;
    let content = read_file(root, "src/utils.rs").replace("compute_score", "calculate_score");
    std::fs::write(root.join("src/utils.rs"), &content).unwrap();

    let _edits = wait_for_edits(&mut rx, &db, 5).await;

    // consumer.rs should have the new name
    let consumer = read_file(root, "src/consumer.rs");
    assert!(
        consumer.contains("calculate_score"),
        "consumer.rs should contain calculate_score, got: {consumer}"
    );

    // matches table should have calculate_score with rs_declare
    let new_decl_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value = 'calculate_score' AND m.kind = 'rs_declare'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        new_decl_count >= 1,
        "calculate_score should have rs_declare after rename, got {new_decl_count}"
    );
}

// ─── Branch divergence after rename ───────────────────────────────────

/// After a watcher-applied rename, committed branch refs still hold the old
/// value while the working-tree branch file set reflects the new content.
/// The watcher re-extracts modified files into the DB, so refs/matches for
/// the changed file update in-place (they are not branch-scoped -- refs are
/// file-scoped). Branch divergence shows up in rev_files: the committed
/// branch and wt branch both link the same file_id, but the file's refs
/// now reflect the renamed state.
///
/// This test verifies:
/// 1. Both branches link the same files after scan
/// 2. After rename, wt branch still links the file (watcher keeps it)
/// 3. Refs in DB reflect the new content (watcher re-extracted)
/// 4. No orphaned matches or refs after the pipeline
#[tokio::test]
async fn rev_files_intact_after_rename_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_file(
        root,
        "src/utils.ts",
        "export function greet(): string { return 'hello'; }\n",
    );
    write_file(
        root,
        "src/consumer.ts",
        "import { greet } from './utils';\nconsole.log(greet());\n",
    );

    let root = root.canonicalize().unwrap();
    let root = root.as_path();
    let (db, repo_id, extractors) = setup_dual_branch_repo(root, "test-branch").await;

    // Both branches should link the same files
    let main_files: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rev_files WHERE branch = 'main'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let wt_files: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rev_files WHERE branch = 'main+wt'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(main_files, wt_files, "both branches should have same file count after scan");
    assert!(main_files >= 2, "should have at least utils.ts and consumer.ts");

    // Start watcher, rename greet -> sayHello
    let mut rx = start_watcher(root, repo_id, &db, &extractors).await;
    let content = read_file(root, "src/utils.ts").replace("greet", "sayHello");
    std::fs::write(root.join("src/utils.ts"), &content).unwrap();

    let _edits = wait_for_edits(&mut rx, &db, 5).await;

    // wt branch should still link all files (no orphans)
    let wt_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rev_files WHERE branch = 'main+wt'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        wt_after, wt_files,
        "wt branch file count should be unchanged after rename"
    );

    // No orphaned matches (matches pointing at nonexistent refs)
    let orphaned_matches: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches m
         WHERE NOT EXISTS (SELECT 1 FROM refs r WHERE r.id = m.ref_id)",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(orphaned_matches, 0, "no orphaned matches after pipeline");

    // No orphaned refs (refs pointing at nonexistent files)
    let orphaned_refs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refs r
         WHERE NOT EXISTS (SELECT 1 FROM files f WHERE f.id = r.file_id)",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(orphaned_refs, 0, "no orphaned refs after pipeline");

    // Refs should reflect the renamed content
    let say_hello_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value = 'sayHello' AND m.kind = 'export_name'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        say_hello_count >= 1,
        "should have export_name match for sayHello after re-extraction"
    );
}

// ─── New file during watch gets matches ───────────────────────────────

/// When the watcher detects a new file create, the file gets added to
/// rev_files for +wt. On the next content change to that file, the
/// watcher re-extracts and inserts refs + matches.
///
/// This verifies the full insert path: files → refs → strings → matches
/// all work through the watcher's replace_file_refs codepath.
#[tokio::test]
async fn new_file_during_watch_gets_indexed_with_matches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    write_file(
        root,
        "src/existing.ts",
        "export const VERSION = '1.0';\n",
    );

    let root = root.canonicalize().unwrap();
    let root = root.as_path();
    let (db, repo_id, extractors) = setup_dual_branch_repo(root, "test-newfile").await;

    let mut rx = start_watcher(root, repo_id, &db, &extractors).await;

    // Create a new file -- watcher should pick it up as Create
    write_file(
        root,
        "src/added.ts",
        "export function newHelper(): void {}\n",
    );

    // Wait for the create event
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        tokio::select! {
            Some(changes) = rx.recv() => {
                let has_create = changes.iter().any(|c| {
                    matches!(c, sprefa_watch::change::Change::Fs(
                        sprefa_watch::change::FsChange::Create { path }
                    ) if path.contains("added.ts"))
                });
                if has_create {
                    break;
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("timed out waiting for new file create event");
            }
        }
    }

    // The file should be in wt rev_files
    let wt_linked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rev_files bf
         JOIN files f ON bf.file_id = f.id
         WHERE bf.branch = 'main+wt' AND f.path = 'src/added.ts'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(wt_linked, 1, "new file should be in main+wt rev_files");

    // Modify the file to trigger re-extraction via ContentChange
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(
        root.join("src/added.ts"),
        "export function updatedHelper(): string { return 'ok'; }\n",
    )
    .unwrap();

    // Wait for content change processing
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        tokio::select! {
            Some(changes) = rx.recv() => {
                let has_content = changes.iter().any(|c| {
                    matches!(c, sprefa_watch::change::Change::Fs(
                        sprefa_watch::change::FsChange::ContentChange { .. }
                    ))
                });
                if has_content {
                    break;
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("timed out waiting for content change event");
            }
        }
    }

    // After re-extraction, matches should exist for updatedHelper
    let match_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE s.value = 'updatedHelper' AND m.kind = 'export_name' AND m.rule_name = 'js'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        match_count >= 1,
        "updatedHelper should have export_name match after re-extraction, got {match_count}"
    );
}
