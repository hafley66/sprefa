//! Smoke tests for the Phase 2 SqliteStore: open, migrate, register a schema,
//! verify UDFs, flush + query round-trip, and the mutations effect cache.

use std::sync::Arc;

use v2::store::{CaptureColumn, ExprTableSpec, SqliteStore, Store};

fn spec(name: &str, captures: &[&str]) -> ExprTableSpec {
    let caps: Vec<CaptureColumn> = captures
        .iter()
        .map(|n| CaptureColumn { name: Arc::from(*n), scan_pointer: None })
        .collect();
    let mut s = ExprTableSpec {
        expr_name:    Arc::from(name),
        namespace:    None,
        captures:     caps,
        schema_hash:  Arc::from(""),
        extract_hash: Arc::from(""),
    };
    s.schema_hash  = v2::store::_3_ddl::schema_hash_of(&s);
    s.extract_hash = Arc::from(format!("ext-{name}"));
    s
}

#[tokio::test]
async fn open_memory_and_migrate() {
    let store = SqliteStore::open_memory().await.unwrap();
    let tables: Vec<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&store.pool)
            .await
            .unwrap();
    let names: std::collections::HashSet<_> = tables.iter().map(|(n,)| n.as_str()).collect();
    for t in ["repos","files","strings","refs","rev_files","repo_revs","sprf_meta","mutations","discovery_log"] {
        assert!(names.contains(t), "missing table: {t}");
    }
}

#[tokio::test]
async fn udfs_registered() {
    let store = SqliteStore::open_memory().await.unwrap();
    let (norm,): (String,) = sqlx::query_as("SELECT sprf_norm('Auth-Service')")
        .fetch_one(&store.pool).await.unwrap();
    assert_eq!(norm, "authservice");
    let (score,): (f64,) = sqlx::query_as("SELECT fzy_score('hello world', 'hw')")
        .fetch_one(&store.pool).await.unwrap();
    assert!(score > 0.0);
}

#[tokio::test]
async fn register_expr_schema_new_creates_table_and_view() {
    let store = SqliteStore::open_memory().await.unwrap();
    let s = spec("hooks", &["H"]);
    store.register_expr_schema(s).await.unwrap();

    let (_table,): (String,) =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE name = 'hooks_data'")
            .fetch_one(&store.pool).await.unwrap();
    let (_view,): (String,) =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE name = 'hooks' AND type = 'view'")
            .fetch_one(&store.pool).await.unwrap();
}

#[tokio::test]
async fn register_expr_schema_extract_changed_clears_rows() {
    let store = SqliteStore::open_memory().await.unwrap();

    let s1 = spec("r", &["X"]);
    store.register_expr_schema(s1).await.unwrap();
    sqlx::query("INSERT INTO r_data DEFAULT VALUES")
        .execute(&store.pool).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM r_data").fetch_one(&store.pool).await.unwrap();
    assert_eq!(n, 1);

    let mut s2 = spec("r", &["X"]);
    s2.extract_hash = Arc::from("different");
    store.register_expr_schema(s2).await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM r_data").fetch_one(&store.pool).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn register_expr_schema_schema_changed_drops_table() {
    let store = SqliteStore::open_memory().await.unwrap();

    store.register_expr_schema(spec("s", &["A"])).await.unwrap();
    store.register_expr_schema(spec("s", &["A", "B"])).await.unwrap();

    let (count,): (i64,) = sqlx::query_as(
        "SELECT count(*) FROM pragma_table_info('s_data') WHERE name LIKE '%_str'",
    ).fetch_one(&store.pool).await.unwrap();
    assert_eq!(count, 2, "expected two _str columns after schema change");
}
