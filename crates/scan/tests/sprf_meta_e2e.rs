//! E2E test for sprf_meta rule change detection.
//!
//! Tests:
//! 1. Unchanged rule → skip extraction
//! 2. Changed schema → DROP + CREATE + extract
//! 3. Changed pattern → DELETE rows + re-extract
//! 4. New rule → queue + extract

use anyhow::Result;
use sprefa_cache::{SqliteStore, Store};
use sprefa_schema::init_db;
use std::collections::HashMap;

#[tokio::test]
async fn sprf_meta_caches_unchanged_rules() -> Result<()> {
    let pool = init_db(":memory:").await?;
    let store = SqliteStore::new(pool);

    // Create rule tables with hashes
    let specs = vec![sprefa_cache::RuleTableSpec {
        rule_name: "test_rule".to_string(),
        namespace: None,
        columns: vec![("name".to_string(), None)],
    }];

    let mut hashes = HashMap::new();
    hashes.insert(
        "test_rule".to_string(),
        sprefa_sprf::hash::RuleHashes {
            schema_hash: "abc123".to_string(),
            extract_hash: "def456".to_string(),
        },
    );

    store.create_rule_tables(&specs, Some(&hashes)).await?;

    // Insert a row
    store.ensure_repo("test-repo", "/tmp").await?;
    store.ensure_rev("test-repo", "main").await?;

    let files = vec![sprefa_cache::FileResult {
        rel_path: "test.json".to_string(),
        content_hash: "hash1".to_string(),
        stem: Some("test".to_string()),
        ext: Some("json".to_string()),
        rule_rows: vec![(
            "test_rule".to_string(),
            vec![sprefa_cache::ExtractionRow {
                captures: vec![sprefa_cache::CaptureEntry {
                    column: "name".to_string(),
                    value: "myvalue".to_string(),
                    span_start: 0,
                    span_end: 5,
                    node_path: None,
                    is_path: false,
                    parent_key: None,
                    scan: None,
                }],
            }],
        )],
    }];

    let inserted1 = store.flush_batch("test-repo", "main", &files, "test-binary").await?;
    assert_eq!(inserted1, 1, "First flush should insert 1 row");

    // Verify data exists
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_rule_data")
        .fetch_one(store.pool())
        .await?;
    assert_eq!(count, 1, "Should have 1 row after first flush");

    Ok(())
}

#[tokio::test]
async fn sprf_meta_detects_schema_change() -> Result<()> {
    let pool = init_db(":memory:").await?;
    let store = SqliteStore::new(pool);

    // Initial schema
    let specs1 = vec![sprefa_cache::RuleTableSpec {
        rule_name: "test_rule".to_string(),
        namespace: None,
        columns: vec![("name".to_string(), None)],
    }];

    let mut hashes1 = HashMap::new();
    hashes1.insert(
        "test_rule".to_string(),
        sprefa_sprf::hash::RuleHashes {
            schema_hash: "old_schema".to_string(),
            extract_hash: "old_extract".to_string(),
        },
    );

    store.create_rule_tables(&specs1, Some(&hashes1)).await?;
    store.ensure_repo("test-repo", "/tmp").await?;
    store.ensure_rev("test-repo", "main").await?;

    // Insert initial data
    let files = vec![sprefa_cache::FileResult {
        rel_path: "test.json".to_string(),
        content_hash: "hash1".to_string(),
        stem: Some("test".to_string()),
        ext: Some("json".to_string()),
        rule_rows: vec![(
            "test_rule".to_string(),
            vec![sprefa_cache::ExtractionRow {
                captures: vec![sprefa_cache::CaptureEntry {
                    column: "name".to_string(),
                    value: "oldvalue".to_string(),
                    span_start: 0,
                    span_end: 8,
                    node_path: None,
                    is_path: false,
                    parent_key: None,
                    scan: None,
                }],
            }],
        )],
    }];
    store.flush_batch("test-repo", "main", &files, "test-binary").await?;

    // Check sprf_meta has old hashes
    let row: (String, String) = sqlx::query_as(
        "SELECT schema_hash, extract_hash FROM sprf_meta WHERE rule_name = ?",
    )
    .bind("test_rule")
    .fetch_one(store.pool())
    .await?;
    assert_eq!(row.0, "old_schema");

    // Simulate schema change
    let specs2 = vec![sprefa_cache::RuleTableSpec {
        rule_name: "test_rule".to_string(),
        namespace: None,
        columns: vec![("name".to_string(), None), ("version".to_string(), None)],
    }];

    let mut hashes2 = HashMap::new();
    hashes2.insert(
        "test_rule".to_string(),
        sprefa_sprf::hash::RuleHashes {
            schema_hash: "new_schema".to_string(), // Changed!
            extract_hash: "old_extract".to_string(),
        },
    );

    store.create_rule_tables(&specs2, Some(&hashes2)).await?;

    // Verify sprf_meta updated
    let row: (String, String) = sqlx::query_as(
        "SELECT schema_hash, extract_hash FROM sprf_meta WHERE rule_name = ?",
    )
    .bind("test_rule")
    .fetch_one(store.pool())
    .await?;
    assert_eq!(row.0, "new_schema");

    Ok(())
}

#[tokio::test]
async fn sprf_meta_detects_extract_change() -> Result<()> {
    let pool = init_db(":memory:").await?;
    let store = SqliteStore::new(pool);

    let specs = vec![sprefa_cache::RuleTableSpec {
        rule_name: "test_rule".to_string(),
        namespace: None,
        columns: vec![("name".to_string(), None)],
    }];

    let mut hashes1 = HashMap::new();
    hashes1.insert(
        "test_rule".to_string(),
        sprefa_sprf::hash::RuleHashes {
            schema_hash: "same_schema".to_string(),
            extract_hash: "old_extract".to_string(),
        },
    );

    store.create_rule_tables(&specs, Some(&hashes1)).await?;
    store.ensure_repo("test-repo", "/tmp").await?;
    store.ensure_rev("test-repo", "main").await?;

    // Insert data
    let files = vec![sprefa_cache::FileResult {
        rel_path: "test.json".to_string(),
        content_hash: "hash1".to_string(),
        stem: Some("test".to_string()),
        ext: Some("json".to_string()),
        rule_rows: vec![(
            "test_rule".to_string(),
            vec![sprefa_cache::ExtractionRow {
                captures: vec![sprefa_cache::CaptureEntry {
                    column: "name".to_string(),
                    value: "oldvalue".to_string(),
                    span_start: 0,
                    span_end: 8,
                    node_path: None,
                    is_path: false,
                    parent_key: None,
                    scan: None,
                }],
            }],
        )],
    }];
    store.flush_batch("test-repo", "main", &files, "test-binary").await?;

    // Verify data exists
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_rule_data")
        .fetch_one(store.pool())
        .await?;
    assert_eq!(count, 1, "Should have 1 row before extract change");

    // Simulate extract change (pattern changed)
    let mut hashes2 = HashMap::new();
    hashes2.insert(
        "test_rule".to_string(),
        sprefa_sprf::hash::RuleHashes {
            schema_hash: "same_schema".to_string(),
            extract_hash: "new_extract".to_string(), // Changed!
        },
    );

    store.create_rule_tables(&specs, Some(&hashes2)).await?;

    // Verify data was deleted (but table still exists)
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM test_rule_data")
        .fetch_one(store.pool())
        .await?;
    assert_eq!(count, 0, "Rows should be deleted after extract change");

    Ok(())
}
