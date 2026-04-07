//! End-to-end test for demand scanning via repo()/rev() capture annotations.
//!
//! Tests verify:
//!   1. repo()/rev() inline annotations in json patterns set scan annotations on per-rule tables
//!   2. store.unscanned_rev_pairs() finds (repo, rev) pairs from per-rule table columns
//!   3. scan_rev() indexes the tagged blob content
//!   4. exclude_revs prevents scanning excluded patterns
//!   5. Multi-round diamond-shaped discovery chains reach fixed point

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use git2::{Repository, Signature};
use sqlx::SqlitePool;

use sprefa_cache::{SqliteStore, Store};
use sprefa_config::RepoConfig;
use sprefa_rules::extractor::RuleExtractor;
use sprefa_scan::{Extractor, Scanner};

// ── git helpers ───────────────────────────────────────────────────────────

fn init_repo(dir: &Path) -> Repository {
    let repo = Repository::init(dir).unwrap();
    {
        let sig = Signature::now("test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
    }
    repo
}

fn commit_file(repo: &Repository, path: &str, content: &[u8]) {
    let root = repo.workdir().unwrap();
    let abs = root.join(path);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&abs, content).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = Signature::now("test", "test@test.com").unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, &format!("add {path}"), &tree, &[&parent]).unwrap();
}

fn tag_head(repo: &Repository, tag_name: &str) {
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.tag_lightweight(tag_name, head.as_object(), false).unwrap();
}

// ── test scaffolding ──────────────────────────────────────────────────────

async fn make_db() -> SqlitePool {
    sprefa_schema::init_db(":memory:").await.unwrap()
}

async fn make_scanner(db: SqlitePool, sprf_source: &str) -> Scanner<SqliteStore> {
    let (ruleset, _dep_edges) = sprefa_sprf::parse_sprf(sprf_source).unwrap();
    let rule_ext = RuleExtractor::from_ruleset(&ruleset).unwrap();
    let store = SqliteStore::new(db);

    let specs: Vec<sprefa_cache::RuleTableSpec> = ruleset
        .rules
        .iter()
        .map(|r| sprefa_cache::RuleTableSpec {
            rule_name: r.name.clone(),
            columns: r
                .create_matches
                .iter()
                .map(|m| (m.kind.to_lowercase(), m.scan.clone()))
                .collect(),
        })
        .collect();
    store.create_rule_tables(&specs, None).await.unwrap();

    let scan_pairs: Vec<sprefa_schema::rule_tables::ScanPair> = specs
        .iter()
        .filter_map(|spec| {
            sprefa_schema::rule_tables::RuleTableDef::from_matches(
                &spec.rule_name,
                &spec.columns.iter().map(|(n, s)| (n.clone(), s.clone())).collect::<Vec<_>>(),
            )
            .scan_pair()
        })
        .collect();

    Scanner {
        extractors: Arc::new(vec![Box::new(rule_ext) as Box<dyn Extractor>]),
        store,
        normalize_config: None,
        global_filter: None,
        scan_pair_levels: vec![scan_pairs],
    }
}

fn repo_config(name: &str, path: &Path) -> RepoConfig {
    RepoConfig {
        name: name.to_string(),
        path: path.to_str().unwrap().to_string(),
        revs: None,
        filter: None,
        branch_overrides: None,
        exclude_revs: None,
    }
}

/// Helper: run discovery loop using per-rule table scan pairs.
async fn discover_and_scan(
    scanner: &Scanner<SqliteStore>,
    repo_cfgs: &HashMap<&str, RepoConfig>,
    max_iterations: usize,
) -> Vec<Vec<(String, String)>> {
    let mut round_scans = Vec::new();

    for _iteration in 1..=max_iterations {
        let mut new_targets: Vec<(String, String, RepoConfig)> = Vec::new();

        for pair in scanner.scan_pair_levels.iter().flatten() {
            let pairs = scanner
                .store
                .unscanned_rev_pairs(&pair.table, &pair.repo_column, &pair.rev_column)
                .await
                .unwrap();
            for (repo_name, rev) in pairs {
                let Some(repo_cfg) = repo_cfgs.get(repo_name.as_str()) else {
                    continue;
                };
                if repo_cfg.rev_excluded(&rev) {
                    continue;
                }
                new_targets.push((repo_name, rev, repo_cfg.clone()));
            }
        }

        if new_targets.is_empty() {
            break;
        }

        let mut this_round = Vec::new();
        for (repo_name, rev, cfg) in &new_targets {
            match scanner.scan_rev(cfg, rev).await {
                Ok(_) => this_round.push((repo_name.clone(), rev.clone())),
                Err(e) => eprintln!("scan {repo_name}@{rev} failed: {e}"),
            }
        }
        if !this_round.is_empty() {
            round_scans.push(this_round);
        }
    }

    round_scans
}

// ── tests ─────────────────────────────────────────────────────────────────

/// Full discovery pipeline: inline repo()/rev() annotations -> per-rule tables -> discover -> scan_rev -> verify blob content.
#[tokio::test]
async fn discovery_indexes_tag_content_not_head() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // -- service-api repo: v1.0.0 has name "service-api-v1", HEAD has "service-api-v2" --
    let svc_path = root.join("service-api");
    let svc_repo = init_repo(&svc_path);
    commit_file(&svc_repo, "package.json", br#"{ "name": "service-api-v1", "version": "1.0.0" }"#);
    tag_head(&svc_repo, "v1.0.0");
    commit_file(&svc_repo, "package.json", br#"{ "name": "service-api-v2", "version": "2.0.0" }"#);

    // -- infra repo: references service-api at v1.0.0 --
    let infra_path = root.join("infra");
    let infra_repo = init_repo(&infra_path);
    commit_file(&infra_repo, "deploy/values.yaml", br#"
image:
  repository: service-api
  tag: v1.0.0
"#);

    let sprf = r#"
        rule(image_refs) {
            fs(**/values.yaml) > json({ image: { repository: repo($REPO), tag: rev($TAG) } })
        };
        rule(pkg_name) {
            fs(**/package.json) > json({ name: $NAME })
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf).await;

    // Tier 1: scan both repos at HEAD.
    let infra_cfg = repo_config("infra", &infra_path);
    let svc_cfg = repo_config("service-api", &svc_path);
    scanner.scan_repo(&infra_cfg, "main").await.unwrap();
    scanner.scan_repo(&svc_cfg, "main").await.unwrap();

    // Verify per-rule table has image_refs data with scan annotations.
    let image_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM \"image_refs_data\"",
    ).fetch_one(&db).await.unwrap();
    assert!(image_rows >= 1, "expected image_refs data, got {}", image_rows);

    // Verify HEAD scan got the v2 name via per-rule table.
    let head_names: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM \"pkg_name_data\" t
         JOIN strings s ON t.name_str = s.id
         JOIN files f ON t.file_id = f.id
         JOIN repos rp ON f.repo_id = rp.id
         WHERE rp.name = 'service-api'",
    ).fetch_all(&db).await.unwrap();
    let names: Vec<&str> = head_names.iter().map(|r| r.0.as_str()).collect();
    assert!(names.contains(&"service-api-v2"), "HEAD should have v2 name, got: {:?}", names);

    // Discover targets from per-rule tables.
    let targets = scanner.store.unscanned_rev_pairs("image_refs", "repo", "tag").await.unwrap();
    let svc_targets: Vec<_> = targets.iter()
        .filter(|(r, _)| r == "service-api")
        .collect();
    assert_eq!(svc_targets.len(), 1, "expected 1 discovery target for service-api, got: {:?}", svc_targets);
    assert_eq!(svc_targets[0].1, "v1.0.0");

    // Scan the discovered rev.
    let rev_result = scanner.scan_rev(&svc_cfg, "v1.0.0").await.unwrap();
    assert!(rev_result.files_scanned > 0, "scan_rev should find files");
    assert!(rev_result.refs_inserted > 0, "scan_rev should insert refs");

    // Verify the v1.0.0 content was indexed via per-rule table.
    let v1_names: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM \"pkg_name_data\" t
         JOIN strings s ON t.name_str = s.id
         JOIN files f ON t.file_id = f.id
         JOIN repos rp ON f.repo_id = rp.id
         WHERE rp.name = 'service-api' AND t.rev = 'v1.0.0'",
    ).fetch_all(&db).await.unwrap();
    let v1_name_vals: Vec<&str> = v1_names.iter().map(|r| r.0.as_str()).collect();
    assert!(v1_name_vals.contains(&"service-api-v1"),
        "v1.0.0 should have 'service-api-v1', got: {:?}", v1_name_vals);

    // After scan_rev, unscanned_rev_pairs should return empty (v1.0.0 is now scanned).
    let targets2 = scanner.store.unscanned_rev_pairs("image_refs", "repo", "tag").await.unwrap();
    let svc_targets2: Vec<_> = targets2.iter()
        .filter(|(r, _)| r == "service-api")
        .collect();
    assert!(svc_targets2.is_empty(), "service-api@v1.0.0 should no longer be unscanned");

    // Re-scanning same rev is idempotent -- no ref duplication.
    let refs_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refs r JOIN files f ON r.file_id = f.id
         JOIN repos rp ON f.repo_id = rp.id WHERE rp.name = 'service-api'",
    ).fetch_one(&db).await.unwrap();

    scanner.scan_rev(&svc_cfg, "v1.0.0").await.unwrap();

    let refs_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refs r JOIN files f ON r.file_id = f.id
         JOIN repos rp ON f.repo_id = rp.id WHERE rp.name = 'service-api'",
    ).fetch_one(&db).await.unwrap();
    assert_eq!(refs_before, refs_after, "re-scanning same rev should not duplicate refs");
}

/// exclude_revs prevents discovery from scanning matching revs.
#[tokio::test]
async fn exclude_revs_blocks_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let svc_path = root.join("service-api");
    let svc_repo = init_repo(&svc_path);
    commit_file(&svc_repo, "package.json", br#"{ "name": "svc-v1", "version": "1.0.0" }"#);
    tag_head(&svc_repo, "v1.0.0");
    commit_file(&svc_repo, "package.json", br#"{ "name": "svc-v2", "version": "2.0.0" }"#);

    let infra_path = root.join("infra");
    let infra_repo = init_repo(&infra_path);
    commit_file(&infra_repo, "deploy/values.yaml", br#"
image:
  repository: service-api
  tag: v1.0.0
"#);

    let sprf = r#"
        rule(image_refs) {
            fs(**/values.yaml) > json({ image: { repository: repo($REPO), tag: rev($TAG) } })
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf).await;

    let infra_cfg = repo_config("infra", &infra_path);
    scanner.scan_repo(&infra_cfg, "main").await.unwrap();

    let svc_cfg = RepoConfig {
        name: "service-api".to_string(),
        path: svc_path.to_str().unwrap().to_string(),
        revs: None,
        filter: None,
        branch_overrides: None,
        exclude_revs: Some(vec!["v1.*".to_string()]),
    };

    // Discovery finds the target via per-rule table...
    let targets = scanner.store.unscanned_rev_pairs("image_refs", "repo", "tag").await.unwrap();
    assert!(!targets.is_empty(), "should discover at least one target");

    // ...but rev_excluded blocks it.
    assert!(svc_cfg.rev_excluded("v1.0.0"), "v1.0.0 should be excluded by 'v1.*' pattern");
    assert!(!svc_cfg.rev_excluded("v2.0.0"), "v2.0.0 should NOT be excluded");
}

/// Static rev scanning with revs config (unified branches + tags).
#[tokio::test]
async fn static_revs_indexes_tag_content() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let svc_path = root.join("myservice");
    let svc_repo = init_repo(&svc_path);
    commit_file(&svc_repo, "package.json", br#"{ "name": "myservice-v1" }"#);
    tag_head(&svc_repo, "v1.0.0");
    commit_file(&svc_repo, "package.json", br#"{ "name": "myservice-v2" }"#);
    tag_head(&svc_repo, "v2.0.0");

    let sprf = r#"
        rule(pkg_name) {
            fs(**/package.json) > json({ name: $NAME })
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf).await;

    let cfg = RepoConfig {
        name: "myservice".to_string(),
        path: svc_path.to_str().unwrap().to_string(),
        revs: Some(vec!["v1.*".to_string()]),
        filter: None,
        branch_overrides: None,
        exclude_revs: None,
    };

    // Scan HEAD first.
    scanner.scan_repo(&cfg, "main").await.unwrap();

    // Scan matching revs (v1.0.0 matches "v1.*", v2.0.0 does not).
    let all_revs = sprefa_index::read_git_revs(&svc_path).unwrap();
    let rev_globs: Vec<globset::GlobMatcher> = cfg.revs.as_ref().unwrap().iter()
        .filter_map(|p| globset::Glob::new(p).ok().map(|g| g.compile_matcher()))
        .collect();

    for rev in &all_revs {
        if rev_globs.iter().any(|g| g.is_match(&rev.name)) {
            scanner.scan_rev(&cfg, &rev.name).await.unwrap();
        }
    }

    // v1.0.0 should be indexed with "myservice-v1" via per-rule table.
    let v1_names: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM \"pkg_name_data\" t
         JOIN strings s ON t.name_str = s.id
         WHERE t.rev = 'v1.0.0'",
    ).fetch_all(&db).await.unwrap();
    let names: Vec<&str> = v1_names.iter().map(|r| r.0.as_str()).collect();
    assert!(names.contains(&"myservice-v1"), "v1.0.0 should have myservice-v1, got: {:?}", names);

    // v2.0.0 should NOT be indexed (doesn't match "v1.*").
    let v2_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rev_files WHERE rev = 'v2.0.0'",
    ).fetch_one(&db).await.unwrap();
    assert_eq!(v2_count, 0, "v2.0.0 should not be indexed");
}

/// Per-rule tables correctly pair repo+rev from the same extraction row.
/// With the old file_id join, multiple services in one file produced a cartesian product.
/// Per-rule tables store repo and rev in the same row, so pairs are exact.
#[tokio::test]
async fn multi_service_file_discovery_exact_pairs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    let svc_a = root.join("svc-a");
    let repo_a = init_repo(&svc_a);
    commit_file(&repo_a, "package.json", br#"{ "name": "svc-a" }"#);
    tag_head(&repo_a, "v1.0.0");

    let svc_b = root.join("svc-b");
    let repo_b = init_repo(&svc_b);
    commit_file(&repo_b, "package.json", br#"{ "name": "svc-b" }"#);
    tag_head(&repo_b, "v2.0.0");

    let infra_path = root.join("infra");
    let infra_repo = init_repo(&infra_path);
    commit_file(&infra_repo, "deploy/values.yaml", br#"
svc_a:
  image:
    repository: svc-a
    tag: v1.0.0
svc_b:
  image:
    repository: svc-b
    tag: v2.0.0
"#);

    let sprf = r#"
        rule(image_refs) {
            fs(**/values.yaml) > json({ **: { image: { repository: repo($REPO), tag: rev($TAG) } } })
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf).await;

    let infra_cfg = repo_config("infra", &infra_path);
    scanner.scan_repo(&infra_cfg, "main").await.unwrap();

    let targets = scanner.store.unscanned_rev_pairs("image_refs", "repo", "tag").await.unwrap();

    let pairs: HashSet<(String, String)> = targets.into_iter().collect();

    // Per-rule tables store repo+rev in the same row, so pairs are exact.
    assert!(pairs.contains(&("svc-a".into(), "v1.0.0".into())),
        "should find svc-a@v1.0.0, got: {:?}", pairs);
    assert!(pairs.contains(&("svc-b".into(), "v2.0.0".into())),
        "should find svc-b@v2.0.0, got: {:?}", pairs);

    // No cartesian ghost pairs.
    assert_eq!(pairs.len(), 2, "should have exactly 2 pairs, got: {:?}", pairs);
}

/// Diamond-shaped 4-round discovery chain:
///
///   deploy --> app --> lib-core --> lib-utils@v3.1.0
///                \--> lib-utils@v3.0.0
///
/// Round 1: scan deploy -> discovers app@v1.0.0
/// Round 2: scan app@v1.0.0 -> discovers lib-core@v2.0.0, lib-utils@v3.0.0
/// Round 3: scan lib-core@v2.0.0 -> discovers lib-utils@v3.1.0
/// Round 4: scan lib-utils@v3.1.0 -> no new targets, stable
#[tokio::test]
async fn diamond_chain_four_round_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // -- lib-utils: leaf repo, two tagged versions --
    let utils_path = root.join("lib-utils");
    let utils_repo = init_repo(&utils_path);
    commit_file(&utils_repo, "package.json", br#"{ "name": "lib-utils", "version": "3.0.0" }"#);
    tag_head(&utils_repo, "v3.0.0");
    commit_file(&utils_repo, "package.json", br#"{ "name": "lib-utils", "version": "3.1.0" }"#);
    tag_head(&utils_repo, "v3.1.0");

    // -- lib-core: depends on lib-utils@v3.1.0 --
    let core_path = root.join("lib-core");
    let core_repo = init_repo(&core_path);
    commit_file(&core_repo, "package.json", br#"{ "name": "lib-core", "version": "2.0.0" }"#);
    commit_file(&core_repo, "deps/lib-utils.yaml", br#"
dependency:
  name: lib-utils
  version: v3.1.0
"#);
    tag_head(&core_repo, "v2.0.0");

    // -- app: depends on lib-core@v2.0.0 AND lib-utils@v3.0.0 (diamond) --
    let app_path = root.join("app-service");
    let app_repo = init_repo(&app_path);
    commit_file(&app_repo, "package.json", br#"{ "name": "app-service", "version": "1.0.0" }"#);
    commit_file(&app_repo, "deps/lib-core.yaml", br#"
dependency:
  name: lib-core
  version: v2.0.0
"#);
    commit_file(&app_repo, "deps/lib-utils.yaml", br#"
dependency:
  name: lib-utils
  version: v3.0.0
"#);
    tag_head(&app_repo, "v1.0.0");

    // -- deploy: references app-service@v1.0.0 --
    let deploy_path = root.join("deploy");
    let deploy_repo = init_repo(&deploy_path);
    commit_file(&deploy_repo, "deploy/values.yaml", br#"
image:
  repository: app-service
  tag: v1.0.0
"#);

    let sprf = r#"
        rule(deploy_ref) {
            fs(**/values.yaml) > json({ image: { repository: repo($REPO), tag: rev($TAG) } })
        };
        rule(lib_dep) {
            fs(**/deps/*.yaml) > json({ dependency: { name: repo($LIB), version: rev($VER) } })
        };
        rule(pkg_name) {
            fs(**/package.json) > json({ name: $NAME })
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf).await;

    let repo_cfgs: HashMap<&str, RepoConfig> = [
        ("deploy", deploy_path.as_path()),
        ("app-service", app_path.as_path()),
        ("lib-core", core_path.as_path()),
        ("lib-utils", utils_path.as_path()),
    ]
    .into_iter()
    .map(|(name, path)| (name, repo_config(name, path)))
    .collect();

    // Initial scan: only deploy (the entry point).
    scanner.scan_repo(&repo_cfgs["deploy"], "main").await.unwrap();

    // Run the discovery loop.
    let round_scans = discover_and_scan(&scanner, &repo_cfgs, 10).await;

    // Verify we got exactly 3 rounds of actual scanning.
    assert_eq!(round_scans.len(), 3,
        "expected 3 discovery rounds, got {}: {:?}", round_scans.len(), round_scans);

    // Round 1: deploy discovered app-service@v1.0.0
    let r1: HashSet<_> = round_scans[0].iter().collect();
    assert!(r1.contains(&("app-service".into(), "v1.0.0".into())),
        "round 1 should discover app-service@v1.0.0, got: {:?}", round_scans[0]);

    // Round 2: app discovered lib-core@v2.0.0 and lib-utils@v3.0.0
    let r2: HashSet<_> = round_scans[1].iter().collect();
    assert!(r2.contains(&("lib-core".into(), "v2.0.0".into())),
        "round 2 should discover lib-core@v2.0.0, got: {:?}", round_scans[1]);
    assert!(r2.contains(&("lib-utils".into(), "v3.0.0".into())),
        "round 2 should discover lib-utils@v3.0.0, got: {:?}", round_scans[1]);

    // Round 3: lib-core discovered lib-utils@v3.1.0
    let r3: HashSet<_> = round_scans[2].iter().collect();
    assert!(r3.contains(&("lib-utils".into(), "v3.1.0".into())),
        "round 3 should discover lib-utils@v3.1.0, got: {:?}", round_scans[2]);

    // Verify all repos have indexed content.
    let all_scanned = sprefa_cache::discovery::scanned_revs(&db).await.unwrap();
    for (repo, rev) in &[
        ("app-service", "v1.0.0"),
        ("lib-core", "v2.0.0"),
        ("lib-utils", "v3.0.0"),
        ("lib-utils", "v3.1.0"),
    ] {
        assert!(all_scanned.contains(&(repo.to_string(), rev.to_string())),
            "{}@{} should be scanned", repo, rev);
    }

    // Verify package names were extracted at correct versions via per-rule table.
    let pkg_names: Vec<(String, String)> = sqlx::query_as(
        "SELECT s.value, t.rev FROM \"pkg_name_data\" t
         JOIN strings s ON t.name_str = s.id",
    ).fetch_all(&db).await.unwrap();
    let name_at: HashSet<_> = pkg_names.iter()
        .map(|(name, rev)| (name.as_str(), rev.as_str()))
        .collect();

    assert!(name_at.contains(&("app-service", "v1.0.0")),
        "app-service should be indexed at v1.0.0, got: {:?}", pkg_names);
    assert!(name_at.contains(&("lib-core", "v2.0.0")),
        "lib-core should be indexed at v2.0.0, got: {:?}", pkg_names);
    assert!(name_at.contains(&("lib-utils", "v3.0.0")),
        "lib-utils should be indexed at v3.0.0, got: {:?}", pkg_names);
    assert!(name_at.contains(&("lib-utils", "v3.1.0")),
        "lib-utils should be indexed at v3.1.0, got: {:?}", pkg_names);
}
