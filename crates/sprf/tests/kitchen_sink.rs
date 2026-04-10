//! Kitchen sink integration test for the .sprf parser, lowerer, and walk engine.
//!
//! Exercises every parser feature against the fixture at tests/fixtures/kitchen_sink.sprf,
//! then runs selected rules against fixture data files to verify end-to-end extraction.

use sprefa_extract::{ExtractContext, Extractor};
use sprefa_rules::types::{LineMatcher, SelectStep};
use sprefa_sprf::{parse_sprf, parse_sprf_full};

const FIXTURE: &str = include_str!("fixtures/kitchen_sink.sprf");

// ── Helpers ──────────────────────────────────────────────

fn rules() -> Vec<sprefa_rules::types::Rule> {
    let (ruleset, _) = parse_sprf(FIXTURE).unwrap();
    ruleset.rules
}

fn rules_named(name: &str) -> Vec<sprefa_rules::types::Rule> {
    rules().into_iter().filter(|r| r.name == name).collect()
}

fn rule(name: &str) -> sprefa_rules::types::Rule {
    let found = rules_named(name);
    assert_eq!(found.len(), 1, "expected exactly 1 rule named '{}', found {}", name, found.len());
    found.into_iter().next().unwrap()
}

fn edges() -> Vec<sprefa_rules::graph::DepEdge> {
    let (_, edges) = parse_sprf(FIXTURE).unwrap();
    edges
}

fn checks() -> Vec<sprefa_sprf::CheckDecl> {
    let (_, _, checks) = parse_sprf_full(FIXTURE).unwrap();
    checks
}

// ── Parse completeness ──────────────────────────────────

#[test]
fn parses_without_error() {
    let (ruleset, edges) = parse_sprf(FIXTURE).unwrap();
    // 35 rule() blocks, but branched produces 2 and dep_source appears twice
    assert!(
        ruleset.rules.len() >= 35,
        "expected at least 35 lowered rules, got {}",
        ruleset.rules.len()
    );
    // dep edges from: ref_no_block, ref_with_block, level1, level2
    assert!(edges.len() >= 3, "expected at least 3 dep edges, got {}", edges.len());
}

#[test]
fn all_rules_have_create_matches() {
    for r in &rules() {
        // line_regex: (?P<name>...) named groups are lowercase, invisible to SCREAMING_CASE extractor
        // any_object: empty json({ }) has no captures by design
        if r.name == "any_object" {
            continue;
        }
        assert!(
            !r.create_matches.is_empty(),
            "rule '{}' has no create_matches",
            r.name
        );
    }
}

// ── 1. Basic rule: fs + json ────────────────────────────

#[test]
fn package_name_flat_chain() {
    let r = rule("package_name");
    assert!(matches!(&r.select[0], SelectStep::File { pattern, .. } if pattern == "**/Cargo.toml"));
    assert!(matches!(&r.select[1], SelectStep::Object { .. }));
    assert_eq!(r.create_matches.len(), 1);
    assert_eq!(r.create_matches[0].capture, "NAME");
}

// ── 2. Regex key ────────────────────────────────────────

#[test]
fn dep_name_regex_key() {
    let r = rule("dep_name");
    match &r.select[1] {
        SelectStep::Object { entries } => {
            assert_eq!(entries.len(), 1);
            match &entries[0].key {
                sprefa_rules::types::KeyMatcher::Glob(s) => assert!(s.starts_with("re:"), "expected re: prefix, got {}", s),
                other => panic!("expected Glob(re:...), got {:?}", other),
            }
        }
        _ => panic!("expected Object step"),
    }
}

// ── 3. Array iteration ──────────────────────────────────

#[test]
fn workspace_member_array() {
    let r = rule("workspace_member");
    // Array is nested inside Object entries, not at top-level select.
    // Walk the Object tree to find it.
    fn has_array(steps: &[SelectStep]) -> bool {
        steps.iter().any(|s| match s {
            SelectStep::Array { .. } => true,
            SelectStep::Object { entries } => entries.iter().any(|e| has_array(&e.value)),
            _ => false,
        })
    }
    assert!(has_array(&r.select), "expected Array step somewhere in workspace_member");
    assert_eq!(r.create_matches[0].capture, "MEMBER");
}

// ── 4. Recursive descent ────────────────────────────────

#[test]
fn deploy_image_recursive_descent() {
    let r = rule("deploy_image");
    assert!(matches!(&r.select[1], SelectStep::Any), "expected Any step for **");
    assert_eq!(r.create_matches.len(), 2);
    let caps: Vec<&str> = r.create_matches.iter().map(|m| m.capture.as_str()).collect();
    assert!(caps.contains(&"REPO"));
    assert!(caps.contains(&"TAG"));
}

// ── 5. Nested scoped blocks ─────────────────────────────

#[test]
fn scoped_deep_nesting() {
    let r = rule("scoped_deep");
    let step_types: Vec<&str> = r.select.iter().map(|s| match s {
        SelectStep::Repo { .. } => "repo",
        SelectStep::Rev { .. } => "rev",
        SelectStep::Folder { .. } => "folder",
        SelectStep::File { .. } => "file",
        SelectStep::Object { .. } => "object",
        _ => "other",
    }).collect();
    assert_eq!(step_types, vec!["repo", "rev", "folder", "file", "object"]);
}

// ── 6. branch() and tag() aliases ───────────────────────

#[test]
fn branch_and_tag_aliases_lower_to_rev() {
    let b = rule("branch_alias");
    assert!(matches!(&b.select[1], SelectStep::Rev { pattern, .. } if pattern == "main|develop"));

    let t = rule("tag_alias");
    assert!(matches!(&t.select[1], SelectStep::Rev { pattern, .. } if pattern == "v*"));
}

// ── 7. file() tag ───────────────────────────────────────

#[test]
fn file_tag_lowers_to_file_step() {
    let r = rule("file_tag");
    assert!(matches!(&r.select[0], SelectStep::File { pattern, .. } if pattern == "**/README.md"));
}

// ── 8. folder() tag ─────────────────────────────────────

#[test]
fn folder_tag_lowers_to_folder_step() {
    let r = rule("folder_tag");
    assert!(matches!(&r.select[0], SelectStep::Folder { pattern, .. } if pattern == "src/components/*"));
}

// ── 9. Cross-ref no block ───────────────────────────────

#[test]
fn cross_ref_no_block_edges() {
    let e: Vec<_> = edges().into_iter().filter(|e| e.consumer == "ref_no_block").collect();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].producer, "deploy_image");
    assert_eq!(e[0].bindings, vec![
        ("repo".to_string(), "REPO".to_string()),
        ("tag".to_string(), "TAG".to_string()),
    ]);
}

// ── 10. Cross-ref with block ────────────────────────────

#[test]
fn cross_ref_with_block_edges() {
    let e: Vec<_> = edges().into_iter().filter(|e| e.consumer == "ref_with_block").collect();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].producer, "deploy_image");
    let r = rule("ref_with_block");
    // Should have file + object from the block children
    assert!(r.select.iter().any(|s| matches!(s, SelectStep::File { pattern, .. } if pattern == "**/package.json")));
}

// ── 11. Monomorphization ────────────────────────────────

#[test]
fn branched_produces_two_rules() {
    let rs = rules_named("branched");
    assert_eq!(rs.len(), 2, "expected 2 monomorphized rules for 'branched'");

    let has_main = rs.iter().any(|r| r.select.iter().any(|s|
        matches!(s, SelectStep::Rev { pattern, .. } if pattern == "main")
    ));
    let has_staging = rs.iter().any(|r| r.select.iter().any(|s|
        matches!(s, SelectStep::Rev { pattern, .. } if pattern == "staging")
    ));
    assert!(has_main, "one branch should select rev(main)");
    assert!(has_staging, "one branch should select rev(staging)");

    // Both share the full capture set
    for r in &rs {
        let caps: Vec<&str> = r.create_matches.iter().map(|m| m.capture.as_str()).collect();
        assert!(caps.contains(&"REPO"));
        assert!(caps.contains(&"PROD"));
        assert!(caps.contains(&"STAGE"));
    }
}

// ── 12. Rule union ──────────────────────────────────────

#[test]
fn dep_source_union() {
    let rs = rules_named("dep_source");
    assert_eq!(rs.len(), 2, "expected 2 dep_source rules (union)");
    // One matches package.json, the other Cargo.toml
    let patterns: Vec<&str> = rs.iter().map(|r| match &r.select[0] {
        SelectStep::File { pattern, .. } => pattern.as_str(),
        _ => panic!("expected File step"),
    }).collect();
    assert!(patterns.contains(&"**/package.json"));
    assert!(patterns.contains(&"**/Cargo.toml"));
}

// ── 13. ast() simple ────────────────────────────────────

#[test]
fn ast_simple_pattern() {
    let r = rule("env_var_ref");
    let ast = r.select_ast.as_ref().expect("expected ast selector");
    assert_eq!(ast.pattern.as_deref(), Some("process.env.$NAME"));
    assert!(ast.language.is_none(), "language should be inferred, not set");
}

// ── 14. ast() with language override ────────────────────

#[test]
fn ast_language_override() {
    let r = rule("rust_fn");
    let ast = r.select_ast.as_ref().expect("expected ast selector");
    assert_eq!(ast.language.as_deref(), Some("rust"));
    assert!(ast.pattern.as_ref().unwrap().contains("$$$ARGS"));
}

// ── 15. ast() with braced capture ───────────────────────

#[test]
fn ast_braced_capture() {
    let r = rule("react_hook");
    let ast = r.select_ast.as_ref().expect("expected ast selector");
    // Pattern should have synthetic metavar replacing use${ENTITY}Query
    assert!(ast.pattern.as_ref().unwrap().contains("$SPREFA0"));
    assert!(ast.constraints.is_some());
    let seg = ast.segment_captures.as_ref().expect("expected segment_captures");
    assert_eq!(seg["SPREFA0"], "use${ENTITY}Query");
}

// ── 16. line() segment capture ──────────────────────────

#[test]
fn line_segment_capture() {
    let r = rule("dockerfile_from");
    match r.value.as_ref().expect("expected line matcher") {
        LineMatcher::Segments { pattern, .. } => {
            assert_eq!(pattern, "FROM $IMAGE:$TAG");
        }
        other => panic!("expected Segments, got {:?}", other),
    }
}

// ── 17. line() regex mode ───────────────────────────────

#[test]
fn line_regex_mode() {
    let r = rule("line_regex");
    match r.value.as_ref().expect("expected line matcher") {
        LineMatcher::Regex { pattern, .. } => {
            assert!(pattern.contains("(?P<IMAGE>"));
            assert!(pattern.contains("(?P<TAG>"));
        }
        other => panic!("expected Regex, got {:?}", other),
    }
}

// ── 18. Glob pipe alternation in fs() ───────────────────

#[test]
fn fs_glob_pipe() {
    let r = rule("ts_files");
    match &r.select[0] {
        SelectStep::File { pattern, .. } => {
            assert_eq!(pattern, "**/*.ts|**/*.tsx");
        }
        other => panic!("expected File step, got {:?}", other),
    }
}

// ── 19. Glob pipe in rev() ──────────────────────────────

#[test]
fn rev_glob_pipe() {
    let r = rule("release_config");
    let rev = r.select.iter().find(|s| matches!(s, SelectStep::Rev { .. })).expect("expected Rev step");
    match rev {
        SelectStep::Rev { pattern, .. } => assert_eq!(pattern, "main|release/*"),
        _ => unreachable!(),
    }
}

// ── 20. Scan annotations ────────────────────────────────

#[test]
fn inline_scan_annotations() {
    let r = rule("inline_scan");
    let repo_match = r.create_matches.iter().find(|m| m.capture == "REPO").unwrap();
    let tag_match = r.create_matches.iter().find(|m| m.capture == "TAG").unwrap();
    assert_eq!(repo_match.scan.as_deref(), Some("repo"));
    assert_eq!(tag_match.scan.as_deref(), Some("rev"));
}

#[test]
fn norm_scan_annotations_tag_form() {
    let r = rule("inline_scan_norm_tag");
    let repo_match = r.create_matches.iter().find(|m| m.capture == "REPO").unwrap();
    let tag_match = r.create_matches.iter().find(|m| m.capture == "TAG").unwrap();
    assert_eq!(repo_match.scan.as_deref(), Some("repo.norm"));
    assert_eq!(tag_match.scan.as_deref(), Some("rev.norm"));
}

#[test]
fn norm_scan_annotations_inline_form() {
    let r = rule("inline_scan_norm_inline");
    let repo_match = r.create_matches.iter().find(|m| m.capture == "REPO").unwrap();
    let tag_match = r.create_matches.iter().find(|m| m.capture == "TAG").unwrap();
    assert_eq!(repo_match.scan.as_deref(), Some("repo.norm"));
    assert_eq!(tag_match.scan.as_deref(), Some("rev.norm"));
}

// ── 21. Quoted value pattern ────────────────────────────

#[test]
fn quoted_value_leaf_pattern() {
    let r = rule("quoted_val");
    let has_leaf_pattern = r.select.iter().any(|s| match s {
        SelectStep::Object { entries } => entries.iter().any(|e|
            e.value.iter().any(|v| matches!(v, SelectStep::LeafPattern { pattern } if pattern.contains("$PROTO")))
        ),
        _ => false,
    });
    assert!(has_leaf_pattern, "expected LeafPattern with $PROTO");
}

// ── 22. Quoted key pattern ──────────────────────────────

#[test]
fn quoted_key_pattern() {
    let r = rule("scoped_pkg");
    // The "@$SCOPE/$NAME" key is inside dependencies > { ... }, so walk recursively.
    fn find_glob_key(steps: &[SelectStep], needle: &str) -> bool {
        steps.iter().any(|s| match s {
            SelectStep::Object { entries } => entries.iter().any(|e| {
                let key_match = match &e.key {
                    sprefa_rules::types::KeyMatcher::Glob(s) => s.contains(needle),
                    _ => false,
                };
                key_match || find_glob_key(&e.value, needle)
            }),
            _ => false,
        })
    }
    assert!(find_glob_key(&r.select, "@$SCOPE"), "expected Glob key with @$SCOPE pattern");
}

// ── 23. Wildcard key iteration ($KEY: $VAL) ─────────────

#[test]
fn capture_key_iteration() {
    let r = rule("all_keys");
    let has_capture_key = r.select.iter().any(|s| match s {
        SelectStep::Object { entries } => entries.iter().any(|e|
            matches!(&e.key, sprefa_rules::types::KeyMatcher::Capture(s) if s == "KEY")
        ),
        _ => false,
    });
    assert!(has_capture_key, "expected Capture key $KEY");
}

// ── 24. Three-level DAG edges ───────────────────────────

#[test]
fn three_level_dag() {
    let all_edges = edges();
    let l1: Vec<_> = all_edges.iter().filter(|e| e.consumer == "level1").collect();
    let l2: Vec<_> = all_edges.iter().filter(|e| e.consumer == "level2").collect();
    assert_eq!(l1.len(), 1);
    assert_eq!(l1[0].producer, "level0");
    assert_eq!(l2.len(), 1);
    assert_eq!(l2[0].producer, "level1");
}

// ── 25-26. Check blocks ─────────────────────────────────

#[test]
fn check_blocks_parsed() {
    let cks = checks();
    assert_eq!(cks.len(), 2, "expected 2 check blocks");
    let names: Vec<&str> = cks.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"version_drift"));
    assert!(names.contains(&"orphan_dep"));

    let drift = cks.iter().find(|c| c.name == "version_drift").unwrap();
    assert!(drift.sql.contains("SELECT"));
    assert!(drift.sql.contains("deploy_image"));
}

// ── 27. Bare glob ───────────────────────────────────────

#[test]
fn bare_glob_lowers_to_file() {
    let r = rule("bare_glob_rule");
    assert!(matches!(&r.select[0], SelectStep::File { pattern, .. } if pattern == "**/Makefile"));
    assert!(r.value.is_some(), "expected line matcher for bare_glob_rule");
}

// ── 28. Multi-segment capture ───────────────────────────

#[test]
fn multi_segment_capture() {
    let r = rule("multi_seg");
    match r.value.as_ref().expect("expected line matcher") {
        LineMatcher::Segments { pattern, .. } => {
            assert!(pattern.contains("$$$SRC"), "expected $$$SRC in pattern, got {}", pattern);
            assert!(pattern.contains("$DEST"), "expected $DEST in pattern, got {}", pattern);
        }
        other => panic!("expected Segments, got {:?}", other),
    }
}

// ── 29. Empty object ────────────────────────────────────

#[test]
fn empty_object_vacuous() {
    let r = rule("any_object");
    let has_empty_obj = r.select.iter().any(|s| match s {
        SelectStep::Object { entries } => entries.is_empty(),
        _ => false,
    });
    assert!(has_empty_obj, "expected empty Object step");
}

// ═══════════════════════════════════════════════════════════
// Walk integration: run rules against fixture data files
// ═══════════════════════════════════════════════════════════

mod walk_integration {
    use super::*;
    use sprefa_rules::walk;

    fn parse_data(source: &[u8], ext: &str) -> Option<serde_json::Value> {
        match ext {
            "json" => serde_json::from_slice(source).ok(),
            "yaml" | "yml" => serde_yaml::from_slice(source).ok(),
            "toml" => {
                let s = std::str::from_utf8(source).ok()?;
                let tv: toml::Value = toml::from_str(s).ok()?;
                serde_json::to_value(tv).ok()
            }
            _ => None,
        }
    }

    fn walk_rule(r: &sprefa_rules::types::Rule, data: &[u8], ext: &str) -> Vec<std::collections::BTreeMap<String, String>> {
        let json_val = parse_data(data, ext).unwrap();
        let structural: Vec<_> = r.select.iter().filter(|s| !s.is_context_step()).cloned().collect();
        let results = walk::walk_select(&json_val, &structural);
        results.into_iter().map(|m| {
            m.captures.iter().map(|(k, v)| (k.clone(), v.text.clone())).collect()
        }).collect()
    }

    const CARGO_TOML: &[u8] = include_bytes!("fixtures/cargo.toml");
    const PACKAGE_JSON: &[u8] = include_bytes!("fixtures/package.json");
    const VALUES_YAML: &[u8] = include_bytes!("fixtures/values.yaml");
    const CONFIG_JSON: &[u8] = include_bytes!("fixtures/config.json");
    const DATA_JSON: &[u8] = include_bytes!("fixtures/data.json");

    #[test]
    fn walk_package_name() {
        let r = rule("package_name");
        let results = walk_rule(&r, CARGO_TOML, "toml");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["NAME"], "fixture-app");
    }

    #[test]
    fn walk_dep_name_regex_key() {
        let r = rule("dep_name");
        let results = walk_rule(&r, CARGO_TOML, "toml");
        let names: Vec<&str> = results.iter().map(|m| m["NAME"].as_str()).collect();
        assert!(names.contains(&"serde"), "expected serde, got {:?}", names);
        assert!(names.contains(&"anyhow"), "expected anyhow, got {:?}", names);
        assert!(names.contains(&"tokio"), "expected tokio, got {:?}", names);
        // dev-dependencies should also match via regex
        assert!(names.contains(&"insta"), "expected insta from dev-deps, got {:?}", names);
    }

    #[test]
    fn walk_workspace_members() {
        let r = rule("workspace_member");
        let results = walk_rule(&r, CARGO_TOML, "toml");
        assert_eq!(results.len(), 3);
        let members: Vec<&str> = results.iter().map(|m| m["MEMBER"].as_str()).collect();
        assert!(members.contains(&"crates/alpha"));
        assert!(members.contains(&"crates/beta"));
        assert!(members.contains(&"crates/gamma"));
    }

    #[test]
    fn walk_deploy_image_recursive() {
        let r = rule("deploy_image");
        let results = walk_rule(&r, VALUES_YAML, "yaml");
        assert_eq!(results.len(), 2);
        let repos: Vec<&str> = results.iter().map(|m| m["REPO"].as_str()).collect();
        assert!(repos.contains(&"myorg/frontend"));
        assert!(repos.contains(&"myorg/backend"));
    }

    #[test]
    fn walk_all_keys_capture() {
        let r = rule("all_keys");
        let results = walk_rule(&r, DATA_JSON, "json");
        assert_eq!(results.len(), 3);
        let keys: Vec<&str> = results.iter().map(|m| m["KEY"].as_str()).collect();
        assert!(keys.contains(&"alpha"));
        assert!(keys.contains(&"beta"));
        assert!(keys.contains(&"gamma"));
        let vals: Vec<&str> = results.iter().map(|m| m["VAL"].as_str()).collect();
        assert!(vals.contains(&"one"));
        assert!(vals.contains(&"two"));
        assert!(vals.contains(&"three"));
    }

    #[test]
    fn walk_npm_deps() {
        let rs = rules_named("dep_source");
        let json_rule = rs.iter().find(|r| matches!(&r.select[0], SelectStep::File { pattern, .. } if pattern == "**/package.json")).unwrap();
        let results = walk_rule(json_rule, PACKAGE_JSON, "json");
        let deps: Vec<&str> = results.iter().map(|m| m["DEP"].as_str()).collect();
        assert!(deps.contains(&"express"), "expected express, got {:?}", deps);
        assert!(deps.contains(&"lodash"), "expected lodash, got {:?}", deps);
    }

    #[test]
    fn walk_empty_object_matches_any() {
        let r = rule("any_object");
        let results = walk_rule(&r, DATA_JSON, "json");
        // Empty object pattern matches any object, producing no captures
        // but the walk should succeed (return at least one match)
        assert!(!results.is_empty(), "empty object pattern should match any object");
    }

    #[test]
    fn walk_quoted_value_pattern() {
        let r = rule("quoted_val");
        let results = walk_rule(&r, CONFIG_JSON, "json");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["PROTO"], "https");
        assert_eq!(results[0]["HOST"], "api.example.com");
        assert_eq!(results[0]["PORT"], "8443");
    }

    #[test]
    fn walk_scoped_pkg_quoted_key() {
        let r = rule("scoped_pkg");
        let results = walk_rule(&r, PACKAGE_JSON, "json");
        // Should match @myorg/utils from dependencies
        assert_eq!(results.len(), 1, "expected 1 scoped package, got {:?}", results);
        assert_eq!(results[0]["SCOPE"], "myorg");
        assert_eq!(results[0]["NAME"], "utils");
    }

    // ── Lockfile walk tests ─────────────────────────────

    const PACKAGE_LOCK_JSON: &[u8] = include_bytes!("fixtures/package-lock.json");
    const CARGO_LOCK: &[u8] = include_bytes!("fixtures/Cargo.lock");
    const DEPLOY_YAML: &[u8] = include_bytes!("fixtures/deploy.yaml");

    #[test]
    fn walk_npm_lock_resolved() {
        let r = rule("npm_lock_resolved");
        let results = walk_rule(&r, PACKAGE_LOCK_JSON, "json");
        // Two packages with resolved URLs (node_modules/express, node_modules/lodash)
        // The root "" entry has no resolved field so should not match
        assert!(results.len() >= 2, "expected at least 2 npm lock entries, got {:?}", results);
        let versions: Vec<&str> = results.iter().map(|m| m["VERSION"].as_str()).collect();
        assert!(versions.contains(&"4.18.2"), "expected express version");
        assert!(versions.contains(&"4.17.21"), "expected lodash version");
    }

    #[test]
    fn walk_cargo_lock_packages() {
        let r = rule("cargo_lock_pkg");
        let results = walk_rule(&r, CARGO_LOCK, "toml");
        assert!(results.len() >= 4, "expected at least 4 Cargo.lock packages, got {:?}", results);
        let names: Vec<&str> = results.iter().map(|m| m["NAME"].as_str()).collect();
        assert!(names.contains(&"serde"), "expected serde");
        assert!(names.contains(&"anyhow"), "expected anyhow");
        assert!(names.contains(&"tokio"), "expected tokio");
        assert!(names.contains(&"fixture-app"), "expected fixture-app");
    }

    #[test]
    fn walk_deploy_manifest() {
        let r = rule("deploy_manifest");
        let results = walk_rule(&r, DEPLOY_YAML, "yaml");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["REPO"], "myorg/api-service");
        assert_eq!(results[0]["TAG"], "v3.1.0");
    }
}

// ═══════════════════════════════════════════════════════════
// Line extraction integration tests
//
// These test the full line-by-line extraction pipeline for
// non-structured files (Dockerfile, go.mod, requirements.txt).
// ═══════════════════════════════════════════════════════════

mod line_extraction {
    use super::*;
    use sprefa_rules::extractor::RuleExtractor;
    use std::collections::BTreeMap;

    const DOCKERFILE: &[u8] = include_bytes!("fixtures/Dockerfile");
    const GO_MOD: &[u8] = include_bytes!("fixtures/go.mod");
    const REQUIREMENTS_TXT: &[u8] = include_bytes!("fixtures/requirements.txt");

    fn extractor() -> RuleExtractor {
        let (ruleset, _) = parse_sprf(FIXTURE).unwrap();
        RuleExtractor::from_ruleset(&ruleset).unwrap()
    }

    fn ctx() -> ExtractContext<'static> {
        ExtractContext {
            repo: None,
            branch: None,
            tags: &[],
        }
    }

    /// Extract refs from a file and return them as a vec of (rule_name, captures) tuples.
    fn extract_refs(source: &[u8], path: &str) -> Vec<(String, BTreeMap<String, String>)> {
        let ext = extractor();
        let refs = ext.extract(source, path, &ctx());
        // Group refs by group_id and rule_name into capture maps
        let mut grouped: BTreeMap<(String, Option<u32>), BTreeMap<String, String>> = BTreeMap::new();
        for r in &refs {
            let key = (r.rule_name.clone(), r.group);
            grouped
                .entry(key)
                .or_default()
                .insert(r.kind.clone(), r.value.clone());
        }
        grouped
            .into_iter()
            .map(|((rule, _), caps)| (rule, caps))
            .collect()
    }

    // ── Dockerfile ──────────────────────────────────

    #[test]
    fn dockerfile_segment_from() {
        let refs = extract_refs(DOCKERFILE, "app/Dockerfile");
        let docker_from: Vec<_> = refs.iter().filter(|(r, _)| r == "docker_from").collect();
        // Fixture: FROM node:20-alpine AS builder, FROM nginx:1.25-alpine (no AS)
        // docker_from pattern: FROM $IMAGE AS $STAGE -- only matches lines with AS
        assert_eq!(docker_from.len(), 1, "expected 1 docker_from match, got {:?}", docker_from);
        assert_eq!(docker_from[0].1["IMAGE"], "node:20-alpine");
        assert_eq!(docker_from[0].1["STAGE"], "builder");
    }

    #[test]
    fn dockerfile_segment_colon() {
        let refs = extract_refs(DOCKERFILE, "app/Dockerfile");
        let from_colon: Vec<_> = refs.iter().filter(|(r, _)| r == "dockerfile_from").collect();
        // dockerfile_from pattern: FROM $IMAGE:$TAG
        // Single-capture $TAG is greedy to end-of-line (no following literal).
        // FROM node:20-alpine AS builder -> IMAGE=node, TAG=20-alpine AS builder
        // FROM nginx:1.25-alpine -> IMAGE=nginx, TAG=1.25-alpine
        assert_eq!(from_colon.len(), 2, "expected 2 dockerfile_from matches, got {:?}", from_colon);
        let images: Vec<&str> = from_colon.iter().map(|(_, c)| c["IMAGE"].as_str()).collect();
        assert!(images.contains(&"node"), "expected node image, got {:?}", images);
        assert!(images.contains(&"nginx"), "expected nginx image, got {:?}", images);
        let tags: Vec<&str> = from_colon.iter().map(|(_, c)| c["TAG"].as_str()).collect();
        // Greedy: $TAG captures everything after the colon
        assert!(tags.contains(&"20-alpine AS builder"), "got {:?}", tags);
        assert!(tags.contains(&"1.25-alpine"), "got {:?}", tags);
    }

    #[test]
    fn dockerfile_regex_from() {
        let refs = extract_refs(DOCKERFILE, "app/Dockerfile");
        let regex: Vec<_> = refs.iter().filter(|(r, _)| r == "line_regex").collect();
        // line_regex pattern: re:FROM\s+$IMAGE:$TAG
        // Sugar rewrites to: FROM\s+(?P<IMAGE>[^:\s]+):(?P<TAG>\S+)
        // Word-boundary: $TAG stops at whitespace
        assert_eq!(regex.len(), 2, "expected 2 line_regex matches, got {:?}", regex);
        let images: Vec<&str> = regex.iter().map(|(_, c)| c["IMAGE"].as_str()).collect();
        assert!(images.contains(&"node"));
        assert!(images.contains(&"nginx"));
        let tags: Vec<&str> = regex.iter().map(|(_, c)| c["TAG"].as_str()).collect();
        assert!(tags.contains(&"20-alpine"), "got {:?}", tags);
        assert!(tags.contains(&"1.25-alpine"), "got {:?}", tags);
    }

    #[test]
    fn dockerfile_multi_seg_copy() {
        let refs = extract_refs(DOCKERFILE, "app/Dockerfile");
        let copies: Vec<_> = refs.iter().filter(|(r, _)| r == "multi_seg").collect();
        // multi_seg pattern: COPY $$$SRC $DEST
        // Single-capture $DEST won't cross `/` boundaries, so only `COPY . .` matches
        // (COPY package*.json ./ fails: $DEST can't capture "./", COPY --from=... fails: $DEST can't capture /usr/...)
        assert_eq!(copies.len(), 1, "expected 1 COPY match (only `COPY . .`), got {:?}", copies);
        assert_eq!(copies[0].1["SRC"], ".");
        assert_eq!(copies[0].1["DEST"], ".");
    }

    // ── go.mod ──────────────────────────────────────

    #[test]
    fn go_mod_deps() {
        let refs = extract_refs(GO_MOD, "services/api/go.mod");
        let deps: Vec<_> = refs.iter().filter(|(r, _)| r == "go_mod_dep").collect();
        // go.mod require block has tab-indented lines: \t$MODULE $VERSION
        assert_eq!(deps.len(), 3, "expected 3 go.mod deps, got {:?}", deps);
        let modules: Vec<&str> = deps.iter().map(|(_, c)| c["MODULE"].as_str()).collect();
        assert!(modules.contains(&"github.com/gin-gonic/gin"));
        assert!(modules.contains(&"github.com/go-playground/validator/v10"));
        assert!(modules.contains(&"golang.org/x/net"));
        let versions: Vec<&str> = deps.iter().map(|(_, c)| c["VERSION"].as_str()).collect();
        assert!(versions.contains(&"v1.9.1"));
        assert!(versions.contains(&"v10.14.0"));
        assert!(versions.contains(&"v0.17.0"));
    }

    // ── requirements.txt ────────────────────────────

    #[test]
    fn python_requirements() {
        let refs = extract_refs(REQUIREMENTS_TXT, "services/api/requirements.txt");
        let reqs: Vec<_> = refs.iter().filter(|(r, _)| r == "py_requirement").collect();
        // py_requirement pattern: $PACKAGE==$VERSION
        // Matches: flask==3.0.2, sqlalchemy==2.0.28, requests==2.31.0, celery[redis]==5.3.6
        // Does NOT match: pydantic>=2.6.0,<3.0.0 (uses >= not ==)
        assert_eq!(reqs.len(), 4, "expected 4 pinned requirements, got {:?}", reqs);
        let pkgs: Vec<&str> = reqs.iter().map(|(_, c)| c["PACKAGE"].as_str()).collect();
        assert!(pkgs.contains(&"flask"), "got {:?}", pkgs);
        assert!(pkgs.contains(&"sqlalchemy"), "got {:?}", pkgs);
        assert!(pkgs.contains(&"requests"), "got {:?}", pkgs);
        assert!(pkgs.contains(&"celery[redis]"), "got {:?}", pkgs);
    }
}

// ═══════════════════════════════════════════════════════════
// 36-38. marker() parse + lower tests
// ═══════════════════════════════════════════════════════════

#[test]
fn marker_single_arg_lowers_to_marker_scope() {
    let r = rule("section_fns");
    let ms = r.marker_scope.as_ref().expect("expected marker_scope");
    assert_eq!(ms.open, "SECTION:");
    assert!(ms.close.is_none());
    // Should also have a line matcher from the > line(...) child
    assert!(r.value.is_some(), "expected line matcher");
}

#[test]
fn marker_two_arg_lowers_to_paired_scope() {
    let r = rule("region_fns");
    let ms = r.marker_scope.as_ref().expect("expected marker_scope");
    assert_eq!(ms.open, "BEGIN:");
    assert_eq!(ms.close.as_deref(), Some("END:"));
    assert!(r.value.is_some(), "expected line matcher");
}

#[test]
fn marker_block_syntax_lowers() {
    let r = rule("section_block");
    let ms = r.marker_scope.as_ref().expect("expected marker_scope");
    assert_eq!(ms.open, "SECTION:");
    assert!(ms.close.is_none());
    assert!(r.value.is_some(), "expected line matcher from nested block");
}

// ── md() lowering ────────────────────────────────────────

#[test]
fn md_heading_terminal_lowers_to_md_scope() {
    let r = rule("readme_sections");
    assert!(r.md_scope.is_some(), "expected md_scope on heading-only rule");
    assert!(r.md_matcher.is_none(), "heading-only rule should have no md_matcher");
    let scope = r.md_scope.unwrap();
    match scope {
        sprefa_rules::types::MdPattern::Heading { level, text, capture } => {
            assert_eq!(level, 2);
            assert_eq!(text.as_deref(), Some("$SECTION"));
            assert_eq!(capture.as_deref(), Some("SECTION"));
        }
        _ => panic!("expected Heading pattern"),
    }
}

#[test]
fn md_heading_scope_plus_list_matcher() {
    let r = rule("readme_deps");
    assert!(r.md_scope.is_some(), "expected md_scope for heading scoper");
    assert!(r.md_matcher.is_some(), "expected md_matcher for list item");
    match r.md_scope.unwrap() {
        sprefa_rules::types::MdPattern::Heading { level, text, .. } => {
            assert_eq!(level, 2);
            assert_eq!(text.as_deref(), Some("Dependencies"));
        }
        _ => panic!("expected Heading scope"),
    }
    match r.md_matcher.unwrap() {
        sprefa_rules::types::MdPattern::ListItem { capture } => {
            assert_eq!(capture.as_deref(), Some("ITEM"));
        }
        _ => panic!("expected ListItem matcher"),
    }
}

#[test]
fn md_link_lowers_to_matcher() {
    let r = rule("readme_links");
    assert!(r.md_scope.is_none());
    assert!(r.md_matcher.is_some());
    match r.md_matcher.unwrap() {
        sprefa_rules::types::MdPattern::Link { text_capture, url_capture } => {
            assert_eq!(text_capture.as_deref(), Some("TEXT"));
            assert_eq!(url_capture.as_deref(), Some("URL"));
        }
        _ => panic!("expected Link matcher"),
    }
}

#[test]
fn md_code_block_lowering() {
    let r = rule("readme_code_langs");
    assert!(r.md_scope.is_none(), "code block pattern should not be md_scope");
    assert!(r.md_matcher.is_some(), "code block should be md_matcher");
    match r.md_matcher.unwrap() {
        sprefa_rules::types::MdPattern::CodeBlock { lang_capture, body_capture } => {
            assert_eq!(lang_capture.as_deref(), Some("LANG"));
            assert!(body_capture.is_none());
        }
        _ => panic!("expected CodeBlock matcher"),
    }
}

#[test]
fn md_heading_scope_with_line_matcher() {
    let r = rule("install_cmds");
    assert!(r.md_scope.is_some(), "expected heading scope");
    assert!(r.md_matcher.is_none(), "line() should be value, not md_matcher");
    assert!(r.value.is_some(), "expected line matcher from chain");
    match r.md_scope.unwrap() {
        sprefa_rules::types::MdPattern::Heading { level, text, .. } => {
            assert_eq!(level, 2);
            assert_eq!(text.as_deref(), Some("Installation"));
        }
        _ => panic!("expected Heading scope"),
    }
}

// ── marker() extraction integration ─────────────────────

mod marker_extraction {
    use super::*;
    use sprefa_rules::extractor::RuleExtractor;
    use std::collections::BTreeMap;

    fn extractor() -> RuleExtractor {
        let (ruleset, _) = parse_sprf(FIXTURE).unwrap();
        RuleExtractor::from_ruleset(&ruleset).unwrap()
    }

    fn ctx() -> ExtractContext<'static> {
        ExtractContext {
            repo: None,
            branch: None,
            tags: &[],
        }
    }

    fn extract_raw(source: &[u8], path: &str) -> Vec<(String, BTreeMap<String, String>)> {
        let ext = extractor();
        let refs = ext.extract(source, path, &ctx());
        let mut grouped: BTreeMap<(String, Option<u32>), BTreeMap<String, String>> = BTreeMap::new();
        for r in &refs {
            let key = (r.rule_name.clone(), r.group);
            grouped
                .entry(key)
                .or_default()
                .insert(r.kind.clone(), r.value.clone());
        }
        grouped
            .into_iter()
            .map(|((rule, _), caps)| (rule, caps))
            .collect()
    }

    #[test]
    fn marker_scopes_line_matching_to_regions() {
        let src = b"// SECTION: imports\nfn parse() {}\nfn lex() {}\n// SECTION: eval\nfn evaluate() {}\n";
        let refs = extract_raw(src, "lib.rs");
        let section_fns: Vec<_> = refs.iter().filter(|(r, _)| r == "section_fns").collect();
        let names: Vec<&str> = section_fns.iter().map(|(_, c)| c["NAME"].as_str()).collect();
        // All three fns should be found across both regions
        assert!(names.contains(&"parse"), "got {:?}", names);
        assert!(names.contains(&"lex"), "got {:?}", names);
        assert!(names.contains(&"evaluate"), "got {:?}", names);
    }

    #[test]
    fn marker_paired_scopes_exclude_outside() {
        let src = b"fn outside() {}\n// BEGIN: auth\nfn login() {}\nfn logout() {}\n// END: auth\nfn also_outside() {}\n";
        let refs = extract_raw(src, "lib.rs");
        let region_fns: Vec<_> = refs.iter().filter(|(r, _)| r == "region_fns").collect();
        let names: Vec<&str> = region_fns.iter().map(|(_, c)| c["NAME"].as_str()).collect();
        // Only fns inside the BEGIN/END region
        assert!(names.contains(&"login"), "got {:?}", names);
        assert!(names.contains(&"logout"), "got {:?}", names);
        assert!(!names.contains(&"outside"), "should not contain outside, got {:?}", names);
        assert!(!names.contains(&"also_outside"), "should not contain also_outside, got {:?}", names);
    }
}

// ── md() extraction integration ───────────────────────────

mod md_extraction {
    use super::*;
    use sprefa_rules::extractor::RuleExtractor;
    use std::collections::BTreeMap;

    fn extractor() -> RuleExtractor {
        let (ruleset, _) = parse_sprf(FIXTURE).unwrap();
        RuleExtractor::from_ruleset(&ruleset).unwrap()
    }

    fn ctx() -> ExtractContext<'static> {
        ExtractContext {
            repo: None,
            branch: None,
            tags: &[],
        }
    }

    fn extract_raw(source: &[u8], path: &str) -> Vec<(String, BTreeMap<String, String>)> {
        let ext = extractor();
        let refs = ext.extract(source, path, &ctx());
        let mut grouped: BTreeMap<(String, Option<u32>), BTreeMap<String, String>> = BTreeMap::new();
        for r in &refs {
            let key = (r.rule_name.clone(), r.group);
            grouped
                .entry(key)
                .or_default()
                .insert(r.kind.clone(), r.value.clone());
        }
        grouped
            .into_iter()
            .map(|((rule, _), caps)| (rule, caps))
            .collect()
    }

    #[test]
    fn md_heading_captures_section_names() {
        let src = b"# Title\n## Installation\ncontent\n## Usage\nmore\n## API\nstuff\n";
        let refs = extract_raw(src, "README.md");
        let sections: Vec<_> = refs.iter().filter(|(r, _)| r == "readme_sections").collect();
        let names: Vec<&str> = sections.iter().map(|(_, c)| c["SECTION"].as_str()).collect();
        assert!(names.contains(&"Installation"), "got {:?}", names);
        assert!(names.contains(&"Usage"), "got {:?}", names);
        assert!(names.contains(&"API"), "got {:?}", names);
    }

    #[test]
    fn md_heading_scope_narrows_list_items() {
        let src = b"## Dependencies\n- express\n- lodash\n## Other\n- unrelated\n";
        let refs = extract_raw(src, "README.md");
        let deps: Vec<_> = refs.iter().filter(|(r, _)| r == "readme_deps").collect();
        let items: Vec<&str> = deps.iter().map(|(_, c)| c["ITEM"].as_str()).collect();
        assert!(items.contains(&"express"), "got {:?}", items);
        assert!(items.contains(&"lodash"), "got {:?}", items);
        assert!(!items.contains(&"unrelated"), "should not contain items outside Dependencies, got {:?}", items);
    }

    #[test]
    fn md_link_extraction() {
        let src = b"Check [docs](https://example.com) and [source](https://github.com/x)\n";
        let refs = extract_raw(src, "README.md");
        let links: Vec<_> = refs.iter().filter(|(r, _)| r == "readme_links").collect();
        assert_eq!(links.len(), 2, "expected 2 links, got {:?}", links);
        let texts: Vec<&str> = links.iter().map(|(_, c)| c["TEXT"].as_str()).collect();
        assert!(texts.contains(&"docs"));
        assert!(texts.contains(&"source"));
    }

    #[test]
    fn md_code_block_lang_extraction() {
        let src = b"text\n```rust\nfn main() {}\n```\nmore\n```bash\necho hi\n```\n";
        let refs = extract_raw(src, "README.md");
        let blocks: Vec<_> = refs.iter().filter(|(r, _)| r == "readme_code_langs").collect();
        let langs: Vec<&str> = blocks.iter().map(|(_, c)| c["LANG"].as_str()).collect();
        assert!(langs.contains(&"rust"), "got {:?}", langs);
        assert!(langs.contains(&"bash"), "got {:?}", langs);
    }

    #[test]
    fn md_heading_scope_with_line_matcher() {
        let src = b"## Installation\nnpm install express\nnpm install lodash\n## Usage\nnpm install chalk\n";
        let refs = extract_raw(src, "README.md");
        let cmds: Vec<_> = refs.iter().filter(|(r, _)| r == "install_cmds").collect();
        let pkgs: Vec<&str> = cmds.iter().map(|(_, c)| c["PKG"].as_str()).collect();
        assert!(pkgs.contains(&"express"), "got {:?}", pkgs);
        assert!(pkgs.contains(&"lodash"), "got {:?}", pkgs);
        // "chalk" is under Usage, not Installation, so should be excluded
        assert!(!pkgs.contains(&"chalk"), "should not contain chalk from Usage section, got {:?}", pkgs);
    }
}
