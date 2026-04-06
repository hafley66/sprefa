//! End-to-end test for demand scanning via repo()/rev() capture annotations.
//!
//! Tests verify:
//!   1. repo()/rev() annotations produce match_labels rows
//!   2. discover_scan_targets() finds (repo, rev) pairs
//!   3. scan_rev() indexes the tagged blob content
//!   4. exclude_revs prevents scanning excluded patterns
//!   5. Multi-round diamond-shaped discovery chains reach fixed point

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use git2::{Repository, Signature};
use sqlx::SqlitePool;

use sprefa_cache::SqliteStore;
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

fn make_scanner(db: SqlitePool, sprf_source: &str) -> Scanner<SqliteStore> {
    let ruleset = sprefa_sprf::parse_sprf(sprf_source).unwrap();
    eprintln!("DEBUG: parsed {} rules", ruleset.rules.len());
    for r in &ruleset.rules {
        eprintln!("DEBUG: rule '{}' has {} select steps", r.name, r.select.len());
    }
    let rule_ext = RuleExtractor::from_ruleset(&ruleset).unwrap();
    Scanner {
        extractors: Arc::new(vec![Box::new(rule_ext) as Box<dyn Extractor>]),
        store: SqliteStore::new(db),
        normalize_config: None,
        global_filter: None,
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

// ── test ──────────────────────────────────────────────────────────────────

/// Full discovery pipeline: repo()/rev() -> match_labels -> discover -> scan_rev -> verify blob content.
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
            fs(**/values.yaml) > json({ image: { repository: $REPO, tag: $TAG } })
            scan(repo: $REPO, rev: $TAG)
        };
        rule(pkg_name) {
            fs(**/package.json) > json({ name: $NAME })
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf);

    // Tier 1: scan both repos at HEAD.
    let infra_cfg = repo_config("infra", &infra_path);
    let svc_cfg = repo_config("service-api", &svc_path);
    scanner.scan_repo(&infra_cfg, "main").await.unwrap();
    scanner.scan_repo(&svc_cfg, "main").await.unwrap();

    // Debug: Check what's in the database
    let total_matches: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM matches")
        .fetch_one(&db).await.unwrap();
    let total_refs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refs")
        .fetch_one(&db).await.unwrap();
    eprintln!("DEBUG: total_matches={}, total_refs={}", total_matches, total_refs);
    
    // Check what files were scanned and their content
    let files: Vec<(String,)> = sqlx::query_as("SELECT path FROM files")
        .fetch_all(&db).await.unwrap();
    eprintln!("DEBUG: files={:?}", files.iter().map(|f| &f.0).collect::<Vec<_>>());

    // Verify match_labels were populated with scan annotations.
    let scan_labels: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM match_labels WHERE key = 'scan'",
    ).fetch_one(&db).await.unwrap();
    eprintln!("DEBUG: scan_labels={}", scan_labels);
    assert!(scan_labels >= 2, "expected at least 2 scan labels (repo+rev), got {}", scan_labels);

    // Verify HEAD scan got the v2 name.
    let head_names: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         JOIN files f ON r.file_id = f.id
         JOIN repos rp ON f.repo_id = rp.id
         WHERE m.kind = 'NAME' AND rp.name = 'service-api'",
    ).fetch_all(&db).await.unwrap();
    let names: Vec<&str> = head_names.iter().map(|r| r.0.as_str()).collect();
    assert!(names.contains(&"service-api-v2"), "HEAD should have v2 name, got: {:?}", names);
    assert!(!names.contains(&"service-api-v1"), "HEAD should NOT have v1 name, got: {:?}", names);

    // Discover targets from match_labels.
    let targets = sprefa_cache::discovery::discover_scan_targets(&db).await.unwrap();
    let svc_targets: Vec<_> = targets.iter()
        .filter(|t| t.repo_name == "service-api")
        .collect();
    assert_eq!(svc_targets.len(), 1, "expected 1 discovery target for service-api, got: {:?}",
        svc_targets.iter().map(|t| format!("{}@{}", t.repo_name, t.rev)).collect::<Vec<_>>());
    assert_eq!(svc_targets[0].rev, "v1.0.0");

    // Scan the discovered rev.
    let rev_result = scanner.scan_rev(&svc_cfg, "v1.0.0").await.unwrap();
    assert!(rev_result.files_scanned > 0, "scan_rev should find files");
    assert!(rev_result.refs_inserted > 0, "scan_rev should insert refs");

    // Verify the v1.0.0 content was indexed (not HEAD content).
    let all_pkg_names: Vec<(String, String)> = sqlx::query_as(
        "SELECT s.value, rv.rev FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         JOIN files f ON r.file_id = f.id
         JOIN repos rp ON f.repo_id = rp.id
         JOIN rev_files bf ON bf.file_id = f.id AND bf.repo_id = rp.id
         JOIN repo_revs rv ON rv.repo_id = rp.id AND rv.rev = bf.rev
         WHERE m.kind = 'NAME' AND rp.name = 'service-api'",
    ).fetch_all(&db).await.unwrap();

    let v1_names: Vec<&str> = all_pkg_names.iter()
        .filter(|(_, branch)| branch == "v1.0.0")
        .map(|(name, _)| name.as_str())
        .collect();
    assert!(v1_names.contains(&"service-api-v1"),
        "v1.0.0 branch should have 'service-api-v1', got: {:?}", v1_names);

    // -- Second loop pass: verify dedup and idempotency --

    // After scan_rev, scanned_revs should include (service-api, v1.0.0).
    let scanned = sprefa_cache::discovery::scanned_revs(&db).await.unwrap();
    assert!(
        scanned.contains(&("service-api".into(), "v1.0.0".into())),
        "v1.0.0 should be marked as scanned in repo_revs",
    );
    assert!(
        !scanned.contains(&("service-api".into(), "v9.9.9".into())),
        "v9.9.9 was never scanned",
    );

    // Discovery query still returns targets (match_labels persist),
    // but the loop logic skips them via scanned_revs.
    let targets2 = sprefa_cache::discovery::discover_scan_targets(&db).await.unwrap();
    let svc_targets2: Vec<_> = targets2.iter()
        .filter(|t| t.repo_name == "service-api" && t.rev == "v1.0.0")
        .collect();
    assert!(!svc_targets2.is_empty(), "discovery query still returns the target");

    // All discovered targets are already scanned.
    for t in &svc_targets2 {
        assert!(
            scanned.contains(&(t.repo_name.clone(), t.rev.clone())),
            "loop would skip {}@{} because already scanned", t.repo_name, t.rev,
        );
    }

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
            fs(**/values.yaml) > json({ image: { repository: $REPO, tag: $TAG } })
            scan(repo: $REPO, rev: $TAG)
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf);

    let infra_cfg = repo_config("infra", &infra_path);
    scanner.scan_repo(&infra_cfg, "main").await.unwrap();

    // Build a config with exclude_revs that blocks v1.*
    let svc_cfg = RepoConfig {
        name: "service-api".to_string(),
        path: svc_path.to_str().unwrap().to_string(),
        revs: None,
        filter: None,
        branch_overrides: None,
        exclude_revs: Some(vec!["v1.*".to_string()]),
    };

    // Discovery finds the target...
    let targets = sprefa_cache::discovery::discover_scan_targets(&db).await.unwrap();
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
    let scanner = make_scanner(db.clone(), sprf);

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

    // v1.0.0 should be indexed with "myservice-v1".
    let v1_names: Vec<(String,)> = sqlx::query_as(
        "SELECT s.value FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         JOIN files f ON r.file_id = f.id
         JOIN rev_files bf ON bf.file_id = f.id AND bf.repo_id = f.repo_id
         WHERE m.kind = 'NAME' AND bf.rev = 'v1.0.0'",
    ).fetch_all(&db).await.unwrap();
    let names: Vec<&str> = v1_names.iter().map(|r| r.0.as_str()).collect();
    assert!(names.contains(&"myservice-v1"), "v1.0.0 should have myservice-v1, got: {:?}", names);

    // v2.0.0 should NOT be indexed (doesn't match "v1.*").
    let v2_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM rev_files WHERE rev = 'v2.0.0'",
    ).fetch_one(&db).await.unwrap();
    assert_eq!(v2_count, 0, "v2.0.0 should not be indexed");
}

/// Multiple (repo, rev) pairs in one file: discovery should produce distinct pairs,
/// but the current file_id join creates a cartesian product. This test documents
/// the behavior so we know when/if we fix it.
#[tokio::test]
async fn multi_service_file_discovery_cartesian() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();

    // Two service repos.
    let svc_a = root.join("svc-a");
    let repo_a = init_repo(&svc_a);
    commit_file(&repo_a, "package.json", br#"{ "name": "svc-a" }"#);
    tag_head(&repo_a, "v1.0.0");

    let svc_b = root.join("svc-b");
    let repo_b = init_repo(&svc_b);
    commit_file(&repo_b, "package.json", br#"{ "name": "svc-b" }"#);
    tag_head(&repo_b, "v2.0.0");

    // Infra repo references both services in one file.
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
            fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } })
            scan(repo: $REPO, rev: $TAG)
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf);

    let infra_cfg = repo_config("infra", &infra_path);
    scanner.scan_repo(&infra_cfg, "main").await.unwrap();

    let targets = sprefa_cache::discovery::discover_scan_targets(&db).await.unwrap();

    // With file_id-only join, we get a cartesian product:
    // (svc-a, v1.0.0), (svc-a, v2.0.0), (svc-b, v1.0.0), (svc-b, v2.0.0)
    // The correct pairs would be (svc-a, v1.0.0) and (svc-b, v2.0.0).
    // Document the current behavior -- the extra pairs are harmless (scan_rev
    // on a nonexistent tag in a repo just fails gracefully) but wasteful.
    let pairs: Vec<(String, String)> = targets.iter()
        .map(|t| (t.repo_name.clone(), t.rev.clone()))
        .collect();

    // At minimum, the correct pairs must be present.
    assert!(pairs.contains(&("svc-a".into(), "v1.0.0".into())),
        "should find svc-a@v1.0.0, got: {:?}", pairs);
    assert!(pairs.contains(&("svc-b".into(), "v2.0.0".into())),
        "should find svc-b@v2.0.0, got: {:?}", pairs);

    // Document the cartesian product -- currently produces 4 pairs, not 2.
    // When span proximity is added, this should narrow to 2.
    let count = pairs.len();
    assert!(count >= 2, "at least 2 targets, got {}", count);
    if count > 2 {
        eprintln!(
            "NOTE: cartesian product produced {} targets instead of 2 (expected until span proximity is added): {:?}",
            count, pairs,
        );
    }
}

/// Diamond-shaped 4-round discovery chain:
///
///   deploy ──> app ──> lib-core ──> lib-utils@v3.1.0
///                \──> lib-utils@v3.0.0
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
    // Each dep in its own file to avoid cartesian product from file_id join.
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
            fs(**/values.yaml) > json({ image: { repository: $REPO, tag: $TAG } })
            scan(repo: $REPO, rev: $TAG)
        };
        rule(lib_dep) {
            fs(**/deps/*.yaml) > json({ dependency: { name: $LIB, version: $VER } })
            scan(repo: $LIB, rev: $VER)
        };
        rule(pkg_name) {
            fs(**/package.json) > json({ name: $NAME })
        };
    "#;

    let db = make_db().await;
    let scanner = make_scanner(db.clone(), sprf);

    // Build repo configs.
    let repo_cfgs: HashMap<&str, (RepoConfig, &Path)> = [
        ("deploy", deploy_path.as_path()),
        ("app-service", app_path.as_path()),
        ("lib-core", core_path.as_path()),
        ("lib-utils", utils_path.as_path()),
    ]
    .into_iter()
    .map(|(name, path)| (name, (repo_config(name, path), path)))
    .collect();

    // Initial scan: only deploy (the entry point).
    scanner.scan_repo(&repo_cfgs["deploy"].0, "main").await.unwrap();

    // Run the discovery loop, tracking which rounds scan what.
    let mut round_scans: Vec<Vec<(String, String)>> = Vec::new();
    let max_iterations = 10;

    for _iteration in 1..=max_iterations {
        let targets = sprefa_cache::discovery::discover_scan_targets(&db).await.unwrap();
        let scanned = sprefa_cache::discovery::scanned_revs(&db).await.unwrap();
        let mut new_targets = Vec::new();

        for target in &targets {
            if scanned.contains(&(target.repo_name.clone(), target.rev.clone())) {
                continue;
            }
            let Some((cfg, _)) = repo_cfgs.get(target.repo_name.as_str()) else {
                continue;
            };
            new_targets.push((target.clone(), cfg.clone()));
        }

        if new_targets.is_empty() {
            break;
        }

        let mut this_round = Vec::new();
        for (target, cfg) in &new_targets {
            match scanner.scan_rev(cfg, &target.rev).await {
                Ok(_) => this_round.push((target.repo_name.clone(), target.rev.clone())),
                Err(e) => eprintln!("scan {}@{} failed (expected for cartesian ghosts): {}",
                    target.repo_name, target.rev, e),
            }
        }
        if !this_round.is_empty() {
            round_scans.push(this_round);
        }
    }

    // Verify we got exactly 3 rounds of actual scanning (4th finds nothing).
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

    // Verify package names were extracted at correct versions.
    let pkg_names: Vec<(String, String)> = sqlx::query_as(
        "SELECT s.value, rv.rev FROM matches m
         JOIN refs r ON m.ref_id = r.id
         JOIN strings s ON r.string_id = s.id
         JOIN files f ON r.file_id = f.id
         JOIN repos rp ON f.repo_id = rp.id
         JOIN rev_files bf ON bf.file_id = f.id AND bf.repo_id = rp.id
         JOIN repo_revs rv ON rv.repo_id = rp.id AND rv.rev = bf.rev
         WHERE m.kind = 'NAME'",
    ).fetch_all(&db).await.unwrap();
    let name_at: HashSet<_> = pkg_names.iter()
        .map(|(name, rev)| (name.as_str(), rev.as_str()))
        .collect();

    assert!(name_at.contains(&("app-service", "v1.0.0")),
        "app-service should be indexed at v1.0.0, got: {:?}", pkg_names);
    assert!(name_at.contains(&("lib-core", "v2.0.0")),
        "lib-core should be indexed at v2.0.0, got: {:?}", pkg_names);
    // lib-utils has two versions indexed
    assert!(name_at.contains(&("lib-utils", "v3.0.0")),
        "lib-utils should be indexed at v3.0.0, got: {:?}", pkg_names);
    assert!(name_at.contains(&("lib-utils", "v3.1.0")),
        "lib-utils should be indexed at v3.1.0, got: {:?}", pkg_names);

    // Total: 4 repos registered or discovered, 5 rev scans total
    // (deploy@main + app@v1.0.0 + lib-core@v2.0.0 + lib-utils@v3.0.0 + lib-utils@v3.1.0)
    let total_revs = all_scanned.len();
    assert!(total_revs >= 4, "at least 4 rev entries, got {}", total_revs);
}

// NOTE: Query/check tests removed - query system was cut in favor of scoped block syntax
