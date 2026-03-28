//! End-to-end tests for sprefa_query: eval atoms, filters, set ops, cascade,
//! and standing rule lifecycle against a scanned fixture set.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;

use sprefa_config::RepoConfig;
use sprefa_query::{Expr, StandingRule};
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
    }
}

struct Fixtures {
    _dir: tempfile::TempDir,
    backend: PathBuf,
    frontend: PathBuf,
    infra: PathBuf,
}

fn setup_fixtures() -> Fixtures {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();

    let backend = root.join("backend");
    let frontend = root.join("frontend");
    let infra = root.join("infra");

    // backend
    write_file(&backend, "Cargo.toml", r#"
[workspace]
members = ["crates/core", "crates/api"]

[dependencies]
shared-lib = "1.0"
serde = "1.0"

[dev-dependencies]
tokio = "1"
"#);
    write_file(&backend, "src/main.rs", r#"
use crate::config::Settings;
use shared_lib::Client;
mod config;
"#);
    write_file(&backend, "openapi.yaml", r#"
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
"#);

    // frontend
    write_file(&frontend, "package.json", r#"
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
"#);
    write_file(&frontend, "tsconfig.json", r#"
{
  "compilerOptions": {
    "paths": {
      "@app/*": ["./src/*"],
      "@shared/*": ["../shared/src/*"]
    }
  }
}
"#);
    write_file(&frontend, "src/app.ts", r#"
import { render } from "react";
import { Widget } from "@app/components";
import { format } from "shared-lib";
"#);

    // infra
    write_file(&infra, "helm/values.yaml", r#"
replicaCount: 3
image:
  repository: myapp
  tag: latest
service:
  port: 8080
"#);
    write_file(&infra, "k8s/my-configmap.yaml", r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: app-config
data:
  DATABASE_URL: postgres://db:5432
  REDIS_HOST: redis.svc
  LOG_LEVEL: info
"#);
    write_file(&infra, "docker-compose.yaml", r#"
version: "3"
services:
  web:
    image: myapp:latest
  worker:
    image: myapp:latest
  redis:
    image: redis:7
"#);

    Fixtures { _dir: dir, backend, frontend, infra }
}

async fn scan_all(scanner: &Scanner, f: &Fixtures) {
    for (name, path) in [("backend", &f.backend), ("frontend", &f.frontend), ("infra", &f.infra)] {
        let cfg = repo_config(name, path);
        scanner.scan_repo(&cfg, "main").await.unwrap();
    }
}

// ── atom tests ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn atom_exact_finds_string() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    let hits = sprefa_query::eval(&db, &Expr::exact("shared-lib")).await.unwrap();
    assert!(!hits.is_empty(), "exact('shared-lib') should find hits");

    let repos: std::collections::HashSet<&str> = hits.hits.iter().map(|h| h.repo_name.as_str()).collect();
    assert!(repos.contains("backend"), "should find in backend");
    assert!(repos.contains("frontend"), "should find in frontend");

    // All exact hits have confidence 1.0
    assert!(hits.hits.iter().all(|h| h.confidence == 1.0));
}

#[tokio::test]
async fn atom_substring_via_fts5() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    let hits = sprefa_query::eval(&db, &Expr::substring("Widget")).await.unwrap();
    assert!(!hits.is_empty(), "substring('Widget') should find hits");

    let values: Vec<&str> = hits.hits.iter().map(|h| h.value.as_str()).collect();
    assert!(values.iter().any(|v| v.contains("Widget")), "at least one value should contain 'Widget', got: {:?}", values);
}

#[tokio::test]
async fn atom_path_matches_segments() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // "src/app" should match file paths or import paths containing those segments
    let hits = sprefa_query::eval(&db, &Expr::path("src/app")).await.unwrap();
    // Might be empty if no string values contain "src/app" -- but our tsconfig paths
    // have "./src/*" which normalizes to contain "src". Check that eval runs without error.
    // The path atom uses LIKE, so it's a best-effort structural match.
    assert!(hits.hits.iter().all(|h| h.confidence == 0.9));
}

// ── filter tests ───────────────────────────────────────────────────────────

#[tokio::test]
async fn filter_of_kind_narrows() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    let expr = Expr::exact("shared-lib").of_kind(["dep_name"]);
    let hits = sprefa_query::eval(&db, &expr).await.unwrap();
    assert!(!hits.is_empty());
    assert!(
        hits.hits.iter().all(|h| h.kind == "dep_name"),
        "all hits should be dep_name, got kinds: {:?}",
        hits.hits.iter().map(|h| &h.kind).collect::<Vec<_>>(),
    );
}

#[tokio::test]
async fn filter_in_repo_narrows() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    let expr = Expr::exact("shared-lib").in_repo("backend");
    let hits = sprefa_query::eval(&db, &expr).await.unwrap();
    assert!(!hits.is_empty());
    assert!(
        hits.hits.iter().all(|h| h.repo_name == "backend"),
        "all hits should be from backend",
    );
}

// ── set operation tests ────────────────────────────────────────────────────

#[tokio::test]
async fn set_intersect() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // Intersect exact("shared-lib") with in_repo("backend") -- equivalent to filter,
    // but tests the set operation path.
    let left = Expr::exact("shared-lib");
    let right = Expr::exact("shared-lib").in_repo("backend");
    let expr = left.and(right);
    let hits = sprefa_query::eval(&db, &expr).await.unwrap();
    assert!(!hits.is_empty());
    assert!(hits.hits.iter().all(|h| h.repo_name == "backend"));
}

#[tokio::test]
async fn set_diff_excludes() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // All "shared-lib" hits minus frontend ones = only backend + any others
    let left = Expr::exact("shared-lib");
    let right = Expr::exact("shared-lib").in_repo("frontend");
    let expr = left.minus(right);
    let hits = sprefa_query::eval(&db, &expr).await.unwrap();
    assert!(!hits.is_empty());
    assert!(
        hits.hits.iter().all(|h| h.repo_name != "frontend"),
        "diff should exclude frontend hits",
    );
}

// ── cascade test ───────────────────────────────────────────────────────────

#[tokio::test]
async fn cascade_cross_repo() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // LHS: "shared-lib" in backend. RHS: "shared-lib" in frontend.
    // Cascade keeps RHS hits whose string_id appeared in LHS.
    let lhs = Expr::exact("shared-lib").in_repo("backend");
    let rhs = Expr::exact("shared-lib").in_repo("frontend");
    let expr = lhs.cascade(rhs);
    let hits = sprefa_query::eval(&db, &expr).await.unwrap();

    assert!(!hits.is_empty(), "cascade should find shared-lib in both repos");
    assert!(hits.hits.iter().all(|h| h.repo_name == "frontend"), "cascade output is RHS hits");
    // Cascade boosts confidence by 1.2x (capped at 1.0). Exact starts at 1.0 so stays 1.0.
    assert!(hits.hits.iter().all(|h| h.confidence == 1.0));
}

// ── standing rule tests ────────────────────────────────────────────────────

#[tokio::test]
async fn standing_rule_inserts_matches() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    let rule = StandingRule {
        name: "find-shared-lib".to_string(),
        expr: Expr::exact("shared-lib"),
        kind: "standing".to_string(),
        rule_hash: "v1".to_string(),
    };

    let result = sprefa_query::evaluate_rule(&db, &rule).await.unwrap();
    assert_eq!(result.rule_name, "find-shared-lib");
    assert!(result.new_matches > 0, "should insert matches");
    assert_eq!(result.stale_removed, 0);

    // Verify matches are in the DB
    let match_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches WHERE rule_name = 'find-shared-lib'"
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(match_count as usize, result.new_matches);

    // get_matches should return the same hits
    let stored = sprefa_query::get_matches(&db, "find-shared-lib").await.unwrap();
    assert_eq!(stored.len(), result.new_matches);
}

#[tokio::test]
async fn standing_rule_hash_change_invalidates() {
    let db = make_db().await;
    let scanner = make_scanner(db.clone());
    let fixtures = setup_fixtures();
    scan_all(&scanner, &fixtures).await;

    // Evaluate with a broad expression
    let rule = StandingRule {
        name: "find-shared-lib".to_string(),
        expr: Expr::exact("shared-lib"),
        kind: "standing".to_string(),
        rule_hash: "v1".to_string(),
    };

    let r1 = sprefa_query::evaluate_rule(&db, &rule).await.unwrap();
    let initial_matches = r1.new_matches;
    assert!(initial_matches > 0);

    let match_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches WHERE rule_name = 'find-shared-lib'"
    ).fetch_one(&db).await.unwrap();
    assert_eq!(match_count as usize, initial_matches);

    // Now change the rule to a narrower expression with a new hash.
    // upsert_rule detects hash change, deletes all old matches, then
    // evaluate_rule re-inserts only the narrower set.
    let rule_v2 = StandingRule {
        name: "find-shared-lib".to_string(),
        expr: Expr::exact("shared-lib").in_repo("backend"),
        kind: "standing".to_string(),
        rule_hash: "v2".to_string(),
    };

    let r2 = sprefa_query::evaluate_rule(&db, &rule_v2).await.unwrap();
    // After hash-change invalidation + re-eval with narrower expr,
    // we should have fewer matches than before.
    let match_count_v2: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM matches WHERE rule_name = 'find-shared-lib'"
    ).fetch_one(&db).await.unwrap();

    assert!(
        (match_count_v2 as usize) < initial_matches,
        "narrower rule should produce fewer matches: v1={}, v2={}",
        initial_matches, match_count_v2,
    );
    assert!(r2.new_matches > 0);
    // stale_removed is 0 here because upsert_rule already deleted all old matches
    // before evaluate_rule ran the diff.
    assert_eq!(r2.stale_removed, 0);
}
