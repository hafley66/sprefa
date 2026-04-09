//! Kitchen sink integration test for the .sprf parser, lowerer, and walk engine.
//!
//! Exercises every parser feature against the fixture at tests/fixtures/kitchen_sink.sprf,
//! then runs selected rules against fixture data files to verify end-to-end extraction.

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
        if r.name == "line_regex" || r.name == "any_object" {
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
            assert_eq!(pattern, r"FROM\s+$IMAGE:$TAG");
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
