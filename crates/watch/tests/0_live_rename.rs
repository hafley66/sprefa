//! Integration test: full watcher pipeline with real filesystem mutations.
//!
//! Creates a tempdir with JS/TS source files, scans them into an in-memory DB,
//! starts the watcher, modifies files via filesystem writes, and asserts that
//! the watcher detects renames and applies edits to downstream consumers.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use sprefa_config::RepoConfig;
use sprefa_extract::Extractor;
use sprefa_schema::init_db;
use sprefa_watch::{plan, rewrite, watcher::WatchConfig};

fn js_extractor() -> Arc<Vec<Box<dyn Extractor>>> {
    Arc::new(vec![Box::new(sprefa_js::JsExtractor)])
}

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
        branches: Some(vec!["main".to_string()]),
        filter: None,
        branch_overrides: None,
    }
}

/// Write a file and wait briefly for notify to pick it up.
fn write_file(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, content).unwrap();
}

/// Read a file to string.
fn read_file(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

// ─── JS/TS re-export chain ─────────────────────────────────────────────

/// Rename in utils.ts propagates through barrel.ts re-export to consumer.ts.
#[tokio::test]
async fn js_rename_propagates_through_reexport_chain() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Lay down fixture files
    write_file(root, "src/utils.ts",
        "export function computeScore(): number { return 42; }\nexport function formatOutput(val: number): string { return `${val}`; }\n");
    write_file(root, "src/barrel.ts",
        "export { computeScore, formatOutput } from './utils';\n");
    write_file(root, "src/consumer.ts",
        "import { computeScore, formatOutput } from './barrel';\nconsole.log(formatOutput(computeScore()));\n");

    // Set up DB + scan
    // Canonicalize to resolve symlinks (macOS /tmp -> /private/tmp)
    let root = root.canonicalize().unwrap();
    let root = root.as_path();
    let db = init_db(":memory:").await.unwrap();
    let extractors = js_extractor();
    let config = repo_config("test", root.to_str().unwrap());

    let scanner = sprefa_scan::Scanner {
        extractors: extractors.clone(),
        db: db.clone(),
        normalize_config: None,
        global_filter: None,
    };
    let result = scanner.scan_repo(&config, "main").await.unwrap();
    assert!(result.refs_inserted > 0, "scan should insert refs");

    // Start watcher
    let repo_id: i64 = sqlx::query_scalar("SELECT id FROM repos WHERE name = 'test'")
        .fetch_one(&db)
        .await
        .unwrap();

    let watch_config = WatchConfig {
        root_path: root.to_path_buf(),
        repo_id,
        debounce: Duration::from_millis(200),
        wt_branch: None,
    };
    let mut rx = sprefa_watch::watcher::watch(watch_config, db.clone(), extractors.clone())
        .await
        .unwrap();

    // Give the watcher a moment to start
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Trigger rename via direct overwrite.
    // On macOS, atomic_rewrite (rename over) produces Create+Remove events that
    // need same-path detection. Direct write produces Modify events which are
    // simpler for the watcher to handle.
    let content = read_file(root, "src/utils.ts").replace("computeScore", "calculateScore");
    std::fs::write(root.join("src/utils.ts"), &content).unwrap();

    // Collect changes from the watcher (with timeout)
    let mut all_edits = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        tokio::select! {
            Some(changes) = rx.recv() => {
                let rewriters: Vec<Box<dyn sprefa_watch::plan::PathRewriter>> = Vec::new();
                match plan::plan_rewrites(&db, &changes, &rewriters).await {
                    Ok(edits) if !edits.is_empty() => {
                        rewrite::apply(&edits);
                        all_edits.extend(edits);
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => panic!("plan_rewrites failed: {e}"),
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("timed out waiting for watcher to produce edits");
            }
        }
    }

    // Verify edits were planned
    let edited_files: Vec<String> = all_edits.iter()
        .map(|e| {
            Path::new(&e.file_path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
        .collect();
    assert!(edited_files.contains(&"barrel.ts".to_string()),
        "barrel.ts should be edited, got: {:?}", edited_files);
    assert!(edited_files.contains(&"consumer.ts".to_string()),
        "consumer.ts should be edited, got: {:?}", edited_files);

    // Verify file contents on disk
    let barrel = read_file(root, "src/barrel.ts");
    assert!(barrel.contains("calculateScore"),
        "barrel.ts should contain calculateScore, got: {barrel}");
    assert!(!barrel.contains("computeScore"),
        "barrel.ts should not contain computeScore, got: {barrel}");

    let consumer = read_file(root, "src/consumer.ts");
    assert!(consumer.contains("import { calculateScore"),
        "consumer.ts import should be rewritten, got: {consumer}");
}

// ─── Rust use statement rename ──────────────────────────────────────────

/// Rename in a Rust file propagates to use statements in consumers.
#[tokio::test]
async fn rs_rename_propagates_to_use_statements() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Minimal Rust crate layout
    write_file(root, "Cargo.toml", r#"
[package]
name = "test-crate"
version = "0.1.0"
edition = "2021"
"#);
    write_file(root, "src/lib.rs", "pub mod utils;\npub mod consumer;\n");
    write_file(root, "src/utils.rs", "pub fn compute_score() -> i32 { 42 }\npub fn format_output(val: i32) -> String { format!(\"{val}\") }\n");
    write_file(root, "src/consumer.rs", "use crate::utils::compute_score;\n\npub fn run() -> i32 { compute_score() }\n");

    let root = root.canonicalize().unwrap();
    let root = root.as_path();
    let db = init_db(":memory:").await.unwrap();
    let extractors = all_extractors();
    let config = repo_config("test-rs", root.to_str().unwrap());

    let scanner = sprefa_scan::Scanner {
        extractors: extractors.clone(),
        db: db.clone(),
        normalize_config: None,
        global_filter: None,
    };
    let result = scanner.scan_repo(&config, "main").await.unwrap();
    assert!(result.refs_inserted > 0, "scan should insert Rust refs");

    let repo_id: i64 = sqlx::query_scalar("SELECT id FROM repos WHERE name = 'test-rs'")
        .fetch_one(&db)
        .await
        .unwrap();

    let watch_config = WatchConfig {
        root_path: root.to_path_buf(),
        repo_id,
        debounce: Duration::from_millis(200),
        wt_branch: None,
    };
    let mut rx = sprefa_watch::watcher::watch(watch_config, db.clone(), extractors.clone())
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Rename via direct write (not atomic) for reliable Modify events
    let content = read_file(root, "src/utils.rs").replace("compute_score", "calculate_score");
    std::fs::write(root.join("src/utils.rs"), &content).unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        tokio::select! {
            Some(changes) = rx.recv() => {
                let rewriters: Vec<Box<dyn sprefa_watch::plan::PathRewriter>> = Vec::new();
                match plan::plan_rewrites(&db, &changes, &rewriters).await {
                    Ok(edits) if !edits.is_empty() => {
                        rewrite::apply(&edits);
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => panic!("plan_rewrites failed: {e}"),
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("timed out waiting for Rust rename edits");
            }
        }
    }

    let consumer = read_file(root, "src/consumer.rs");
    assert!(consumer.contains("calculate_score"),
        "consumer.rs should contain calculate_score, got: {consumer}");
}
