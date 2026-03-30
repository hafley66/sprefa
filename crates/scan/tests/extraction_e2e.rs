//! End-to-end extraction tests: scan multi-repo fixtures, assert DB state via SQL.
//!
//! Validates the full pipeline: filesystem -> extract -> flush -> matches table.
//! Session 3 of the schema migration plan.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;

use sprefa_config::RepoConfig;
use sprefa_rules::{extractor::RuleExtractor, RuleSet};
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

fn load_ruleset() -> RuleSet {
    let bytes = std::fs::read(rules_path()).unwrap();
    serde_json::from_slice(&bytes).unwrap()
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

fn make_scanner_with_links(db: SqlitePool) -> Scanner {
    let ruleset = load_ruleset();
    let rule_ext = RuleExtractor::from_ruleset(&ruleset).unwrap();
    Scanner {
        extractors: Arc::new(vec![
            Box::new(rule_ext) as Box<dyn Extractor>,
            Box::new(sprefa_js::JsExtractor),
            Box::new(sprefa_rs::RsExtractor),
        ]),
        db,
        normalize_config: None,
        global_filter: None,
        link_rules: ruleset.link_rules,
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
[package]
name = "backend"
version = "0.1.0"

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
    write_file(
        root,
        "src/config.ts",
        r#"
export const DB_URL = process.env.DATABASE_URL;
export const LOG = process.env.LOG_LEVEL;
"#,
    );
}

fn write_shared_lib_fixtures(root: &Path) {
    write_file(
        root,
        "package.json",
        r#"
{
  "name": "shared-lib",
  "version": "2.0.0"
}
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
    shared_lib: PathBuf,
}

fn setup_fixtures() -> FixtureSet {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();

    let backend = root.join("backend");
    let frontend = root.join("frontend");
    let infra = root.join("infra");
    let shared_lib = root.join("shared-lib");

    write_backend_fixtures(&backend);
    write_frontend_fixtures(&frontend);
    write_infra_fixtures(&infra);
    write_shared_lib_fixtures(&shared_lib);

    FixtureSet { _dir: dir, backend, frontend, infra, shared_lib }
}

async fn scan_all(scanner: &Scanner, fixtures: &FixtureSet) {
    // Providers scanned before consumers so cross-repo link rules fire on first pass:
    // shared-lib (package_name) and infra (env_var_name) before backend/frontend.
    // NOTE: this ordering dependency is a known architectural gap -- in production,
    // a second link-rule pass after all repos are indexed would make order irrelevant.
    for (name, path) in [
        ("shared-lib", &fixtures.shared_lib),
        ("infra", &fixtures.infra),
        ("backend", &fixtures.backend),
        ("frontend", &fixtures.frontend),
    ] {
        let cfg = repo_config(name, path);
        scanner.scan_repo(&cfg, "main").await.unwrap();
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

/// Full extraction pipeline: one scan, all content assertions in one pass.
#[tokio::test]
async fn extraction_e2e() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // --- match counts by kind ---
    // cargo-deps: shared-lib, serde, tokio=3  +  package-json-deps: shared-lib, react=2
    assert_eq!(count_kind(&db, "dep_name").await, 5);
    // package-json-deps: ^2.0, ^18.0
    assert_eq!(count_kind(&db, "dep_version").await, 2);
    // cargo-package-name: backend=1  +  package-json-name: frontend, shared-lib=2
    assert_eq!(count_kind(&db, "package_name").await, 3);
    assert_eq!(count_kind(&db, "workspace_member").await, 2);   // crates/core, crates/api
    assert_eq!(count_kind(&db, "operation_id").await, 3);       // listWidgets, createWidget, listUsers
    assert_eq!(count_kind(&db, "path_alias").await, 2);         // @app/*, @shared/*
    assert_eq!(count_kind(&db, "package_entry").await, 2);      // ., ./utils
    assert_eq!(count_kind(&db, "helm_value").await, 4);         // 3, myapp, latest, 8080
    assert_eq!(count_kind(&db, "env_var_name").await, 3);       // DATABASE_URL, REDIS_HOST, LOG_LEVEL
    assert_eq!(count_kind(&db, "env_var_ref").await, 2);        // DATABASE_URL, LOG_LEVEL (from config.ts)
    assert_eq!(count_kind(&db, "service_name").await, 3);       // web, worker, redis
    assert!(count_kind(&db, "import_path").await >= 3);
    assert!(count_kind(&db, "rs_use").await >= 2);

    // --- all expected rule_names fired ---
    let rule_names: Vec<(String,)> =
        sqlx::query_as("SELECT DISTINCT rule_name FROM matches ORDER BY rule_name")
            .fetch_all(&db).await.unwrap();
    let names: Vec<&str> = rule_names.iter().map(|r| r.0.as_str()).collect();
    for expected in [
        "cargo-deps", "cargo-package-name", "cargo-workspace-members",
        "docker-compose-services", "helm-values", "js-env-var-refs",
        "k8s-configmap-envs", "openapi-operations", "package-json-deps",
        "package-json-exports", "package-json-name", "tsconfig-paths",
        "js", "rs",
    ] {
        assert!(names.contains(&expected), "missing rule_name '{}', got: {:?}", expected, names);
    }

    // --- cross-repo dedup: "shared-lib" appears in backend + frontend ---
    let cross_repo: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM strings s
         JOIN refs r ON r.string_id = s.id
         JOIN files f ON r.file_id = f.id
         GROUP BY s.value HAVING COUNT(DISTINCT f.repo_id) > 1",
    ).fetch_all(&db).await.unwrap();
    let cross: Vec<&str> = cross_repo.iter().map(|r| r.0.as_str()).collect();
    assert!(cross.contains(&"shared-lib"), "expected 'shared-lib' cross-repo, got: {:?}", cross);

    // --- FTS5 trigram search ---
    let hits = sprefa_schema::search_refs(&db, "Widget", None).await.unwrap();
    assert!(hits.iter().any(|h| h.value.contains("Widget")), "FTS5 should find Widget* matches");

    // --- parent key chains ---
    let dep_version_parents: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT s2.value FROM refs r
         JOIN matches m ON m.ref_id = r.id
         JOIN strings s2 ON r.parent_key_string_id = s2.id
         WHERE m.kind = 'dep_version' AND m.rule_name = 'package-json-deps'",
    ).fetch_all(&db).await.unwrap();
    let pkeys: Vec<&str> = dep_version_parents.iter().map(|r| r.0.as_str()).collect();
    assert!(pkeys.contains(&"shared-lib") && pkeys.contains(&"react"), "dep_version parent keys: {:?}", pkeys);

    let alias_parents: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT s2.value FROM refs r
         JOIN matches m ON m.ref_id = r.id
         JOIN strings s2 ON r.parent_key_string_id = s2.id
         WHERE m.kind = 'import_path' AND m.rule_name = 'tsconfig-paths'",
    ).fetch_all(&db).await.unwrap();
    let aliases: Vec<&str> = alias_parents.iter().map(|r| r.0.as_str()).collect();
    assert!(aliases.contains(&"@app/*") && aliases.contains(&"@shared/*"), "tsconfig alias parents: {:?}", aliases);
}

/// Link rule pipeline: one scan with links enabled, verify cross-repo edges.
#[tokio::test]
async fn link_rules_e2e() {
    let db = make_db().await;
    let scanner = make_scanner_with_links(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // dep_to_package: dep_name "shared-lib" (backend + frontend) → package_name "shared-lib"
    let dep_links: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM match_links WHERE link_kind = 'dep_to_package'",
    ).fetch_one(&db).await.unwrap();
    assert!(dep_links >= 1, "expected dep_to_package links, got {}", dep_links);

    let kinds: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT src_m.kind, tgt_m.kind
         FROM match_links ml
         JOIN matches src_m ON ml.source_match_id = src_m.id
         JOIN matches tgt_m ON ml.target_match_id = tgt_m.id
         WHERE ml.link_kind = 'dep_to_package'",
    ).fetch_all(&db).await.unwrap();
    assert!(
        kinds.iter().all(|(src, tgt)| src == "dep_name" && tgt == "package_name"),
        "unexpected kinds in dep_to_package: {:?}", kinds,
    );

    // env_var_binding: DATABASE_URL + LOG_LEVEL from config.ts → configmap
    let mut env_values: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM match_links ml
         JOIN matches src_m ON ml.source_match_id = src_m.id
         JOIN refs r ON src_m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         WHERE ml.link_kind = 'env_var_binding'
         ORDER BY s.value",
    ).fetch_all(&db).await.unwrap();
    env_values.sort();
    let env: Vec<&str> = env_values.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(env, ["DATABASE_URL", "LOG_LEVEL"], "env_var_binding linked values: {:?}", env);
}
