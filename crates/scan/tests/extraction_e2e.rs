//! End-to-end extraction tests: scan multi-repo fixtures, assert DB state via SQL.
//!
//! Validates the full pipeline: filesystem -> extract -> flush -> matches table.
//! Session 3 of the schema migration plan.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;

use sprefa_config::RepoConfig;
use sprefa_rules::extractor::RuleExtractor;
use sprefa_scan::{Extractor, Scanner};

// ── helpers ────────────────────────────────────────────────────────────────

async fn make_db() -> SqlitePool {
    sprefa_schema::init_db(":memory:").await.unwrap()
}

fn write_file(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
}

fn repo_config(name: &str, path: &Path) -> RepoConfig {
    RepoConfig {
        name: name.to_string(),
        path: path.to_str().unwrap().to_string(),
        branches: Some(vec!["main".to_string()]),
        filter: None,
        branch_overrides: None,
    }
}

fn rules_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("sprefa-rules.json")
}

fn make_scanner(db: SqlitePool) -> Scanner {
    let rule_ext = RuleExtractor::from_json(&rules_path()).unwrap();
    Scanner {
        extractors: Arc::new(vec![
            Box::new(rule_ext) as Box<dyn Extractor>,
            Box::new(sprefa_js::JsExtractor),
            Box::new(sprefa_rs::RsExtractor),
        ]),
        db,
        normalize_config: None,
        global_filter: None,
        link_rules: vec![],
    }
}

async fn count_kind(db: &SqlitePool, kind: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM matches WHERE kind = ?")
        .bind(kind)
        .fetch_one(db)
        .await
        .unwrap()
}

// ── fixture data ───────────────────────────────────────────────────────────

fn write_backend_fixtures(root: &Path) {
    write_file(
        root,
        "Cargo.toml",
        r#"
[workspace]
members = ["crates/core", "crates/api"]

[dependencies]
shared-lib = "1.0"
serde = "1.0"

[dev-dependencies]
tokio = "1"
"#,
    );
    write_file(
        root,
        "src/main.rs",
        r#"
use crate::config::Settings;
use shared_lib::Client;
mod config;
"#,
    );
    write_file(
        root,
        "openapi.yaml",
        r#"
openapi: "3.0"
paths:
  /v1/widgets:
    get:
      operationId: listWidgets
    post:
      operationId: createWidget
  /v1/users:
    get:
      operationId: listUsers
"#,
    );
}

fn write_frontend_fixtures(root: &Path) {
    write_file(
        root,
        "package.json",
        r#"
{
  "name": "frontend",
  "dependencies": {
    "shared-lib": "^2.0",
    "react": "^18.0"
  },
  "exports": {
    ".": { "import": "./dist/index.mjs" },
    "./utils": { "import": "./dist/utils.mjs" }
  }
}
"#,
    );
    write_file(
        root,
        "tsconfig.json",
        r#"
{
  "compilerOptions": {
    "paths": {
      "@app/*": ["./src/*"],
      "@shared/*": ["../shared/src/*"]
    }
  }
}
"#,
    );
    write_file(
        root,
        "src/app.ts",
        r#"
import { render } from "react";
import { Widget } from "@app/components";
import { format } from "shared-lib";
"#,
    );
}

fn write_infra_fixtures(root: &Path) {
    write_file(
        root,
        "helm/values.yaml",
        r#"
replicaCount: 3
image:
  repository: myapp
  tag: latest
service:
  port: 8080
"#,
    );
    write_file(
        root,
        "k8s/my-configmap.yaml",
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: app-config
data:
  DATABASE_URL: postgres://db:5432
  REDIS_HOST: redis.svc
  LOG_LEVEL: info
"#,
    );
    write_file(
        root,
        "docker-compose.yaml",
        r#"
version: "3"
services:
  web:
    image: myapp:latest
  worker:
    image: myapp:latest
  redis:
    image: redis:7
"#,
    );
}

struct FixtureSet {
    _dir: tempfile::TempDir,
    backend: PathBuf,
    frontend: PathBuf,
    infra: PathBuf,
}

fn setup_fixtures() -> FixtureSet {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();

    let backend = root.join("backend");
    let frontend = root.join("frontend");
    let infra = root.join("infra");

    write_backend_fixtures(&backend);
    write_frontend_fixtures(&frontend);
    write_infra_fixtures(&infra);

    FixtureSet { _dir: dir, backend, frontend, infra }
}

async fn scan_all(scanner: &Scanner, fixtures: &FixtureSet) {
    for (name, path) in [
        ("backend", &fixtures.backend),
        ("frontend", &fixtures.frontend),
        ("infra", &fixtures.infra),
    ] {
        let cfg = repo_config(name, path);
        scanner.scan_repo(&cfg, "main").await.unwrap();
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scan_all_fixtures_inserts_expected_matches() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // cargo-deps: shared-lib, serde, tokio = 3
    // package-json-deps emits dep_name for: shared-lib, react = 2
    assert_eq!(count_kind(&db, "dep_name").await, 5);

    // package-json-deps emits dep_version for each dep: ^2.0, ^18.0 = 2
    assert_eq!(count_kind(&db, "dep_version").await, 2);

    // cargo-workspace-members: crates/core, crates/api = 2
    assert_eq!(count_kind(&db, "workspace_member").await, 2);

    // openapi-operations: listWidgets, createWidget, listUsers = 3
    assert_eq!(count_kind(&db, "operation_id").await, 3);

    // tsconfig-paths: @app/*, @shared/* = 2
    assert_eq!(count_kind(&db, "path_alias").await, 2);

    // package-json-exports: ., ./utils = 2
    assert_eq!(count_kind(&db, "package_entry").await, 2);

    // helm-values: 3, myapp, latest, 8080 = 4 leaf values
    assert_eq!(count_kind(&db, "helm_value").await, 4);

    // k8s-configmap-envs: DATABASE_URL, REDIS_HOST, LOG_LEVEL = 3
    assert_eq!(count_kind(&db, "env_var_name").await, 3);

    // docker-compose-services: web, worker, redis = 3
    assert_eq!(count_kind(&db, "service_name").await, 3);

    // JS imports: react, @app/components, shared-lib = 3 import_path
    // RS uses: crate::config::Settings, shared_lib::Client = 2 rs_use
    assert!(count_kind(&db, "import_path").await >= 3);
    assert!(count_kind(&db, "rs_use").await >= 2);
}

#[tokio::test]
async fn cross_repo_string_appears_in_multiple_repos() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // "shared-lib" appears as dep_name in both backend (Cargo.toml) and frontend (package.json)
    let cross_repo: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM strings s \
         JOIN refs r ON r.string_id = s.id \
         JOIN files f ON r.file_id = f.id \
         GROUP BY s.value \
         HAVING COUNT(DISTINCT f.repo_id) > 1",
    )
    .fetch_all(&db)
    .await
    .unwrap();

    let values: Vec<&str> = cross_repo.iter().map(|r| r.0.as_str()).collect();
    assert!(
        values.contains(&"shared-lib"),
        "expected 'shared-lib' in cross-repo strings, got: {:?}",
        values
    );
}

#[tokio::test]
async fn fts5_trigram_finds_partial_match() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // "widget" should match "listWidgets", "createWidget" via FTS5 trigram
    let hits = sprefa_schema::search_refs(&db, "Widget", None).await.unwrap();
    assert!(
        !hits.is_empty(),
        "FTS5 should find results for 'Widget'",
    );
    let all_values: Vec<&str> = hits.iter().map(|h| h.value.as_str()).collect();
    assert!(
        all_values.iter().any(|v| v.contains("Widget")),
        "expected at least one value containing 'Widget', got: {:?}",
        all_values,
    );
}

#[tokio::test]
async fn parent_key_chains_intact() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // package-json-deps emits dep_version with parent: "name", linking version refs
    // back to their dep_name string. The parent_key_string_id on dep_version refs
    // should point to string values "shared-lib" and "react".
    let parent_keys: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT s2.value \
         FROM refs r \
         JOIN matches m ON m.ref_id = r.id \
         JOIN strings s2 ON r.parent_key_string_id = s2.id \
         WHERE m.kind = 'dep_version' AND m.rule_name = 'package-json-deps'",
    )
    .fetch_all(&db)
    .await
    .unwrap();

    let keys: Vec<&str> = parent_keys.iter().map(|r| r.0.as_str()).collect();
    assert!(keys.contains(&"shared-lib"), "expected 'shared-lib' parent key, got: {:?}", keys);
    assert!(keys.contains(&"react"), "expected 'react' parent key, got: {:?}", keys);

    // Also verify tsconfig-paths links import_path targets back to alias keys
    let alias_parents: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT s2.value \
         FROM refs r \
         JOIN matches m ON m.ref_id = r.id \
         JOIN strings s2 ON r.parent_key_string_id = s2.id \
         WHERE m.kind = 'import_path' AND m.rule_name = 'tsconfig-paths'",
    )
    .fetch_all(&db)
    .await
    .unwrap();

    let aliases: Vec<&str> = alias_parents.iter().map(|r| r.0.as_str()).collect();
    assert!(aliases.contains(&"@app/*"), "expected '@app/*' alias parent, got: {:?}", aliases);
    assert!(aliases.contains(&"@shared/*"), "expected '@shared/*' alias parent, got: {:?}", aliases);
}

#[tokio::test]
async fn rule_name_populated_on_matches() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    let rule_names: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT rule_name FROM matches ORDER BY rule_name")
            .fetch_all(&db)
            .await
            .unwrap();

    let names: Vec<&str> = rule_names.iter().map(|r| r.0.as_str()).collect();

    // All 10 rules from sprefa-rules.json should have produced at least one match,
    // plus built-in extractors (js, rs)
    let expected_rule_names = [
        "cargo-deps",
        "cargo-workspace-members",
        "docker-compose-services",
        "helm-values",
        "k8s-configmap-envs",
        "openapi-operations",
        "package-json-deps",
        "package-json-exports",
        "tsconfig-paths",
        // pnpm-workspace has no fixture, so it won't appear
    ];

    for expected in expected_rule_names {
        assert!(
            names.contains(&expected),
            "missing rule_name '{}' in matches, got: {:?}",
            expected,
            names,
        );
    }

    // Built-in extractors should also have entries
    assert!(names.contains(&"js"), "missing 'js' rule_name, got: {:?}", names);
    assert!(names.contains(&"rs"), "missing 'rs' rule_name, got: {:?}", names);
}
