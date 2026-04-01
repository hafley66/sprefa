use sprefa_extract::{kind, Extractor};
use sprefa_rules::*;

// ── Schema + deserialization tests ─────────────────────────────────

#[test]
fn schema_is_valid_json() {
    // Just verify generation doesn't panic and produces parseable JSON.
    let s = schema::generate_schema_string();
    let v: serde_json::Value = serde_json::from_str(&s).expect("schema should be valid JSON");
    assert_eq!(v["title"], "RuleSet");
}

#[test]
fn minimal_rule() {
    let json = r#"{
        "rules": [{
            "name": "catch-all",
            "select": [{ "step": "file", "pattern": "**/*.json" }],
            "emit": [{ "capture": "val", "kind": "json_value" }]
        }]
    }"#;
    let ruleset: RuleSet = serde_json::from_str(json).unwrap();
    assert_eq!(ruleset.rules.len(), 1);
    assert_eq!(ruleset.rules[0].name, "catch-all");
    assert_eq!(ruleset.rules[0].create_matches[0].kind, "json_value");
}

#[test]
fn full_rule_with_captures() {
    let json = r#"{
        "$schema": "sprefa-rules.schema.json",
        "rules": [{
            "name": "helm-image-refs",
            "description": "Docker image references in Helm values files",
            "select": [
                { "step": "repo", "pattern": "*/helm-charts" },
                { "step": "rev", "pattern": "main|release/*|v*" },
                { "step": "file", "pattern": "values.yaml|values-*.yaml" },
                { "step": "any" },
                { "step": "key", "name": "image" },
                { "step": "object", "entries": [
                    { "key": "repository", "value": [{ "step": "leaf", "capture": "repo" }] },
                    { "key": "tag", "value": [{ "step": "leaf", "capture": "tag" }] }
                ] }
            ],
            "emit": [
                { "capture": "repo", "kind": "dep_name" },
                { "capture": "tag", "kind": "dep_version", "parent": "repo" }
            ],
            "confidence": 0.9
        }]
    }"#;
    let ruleset: RuleSet = serde_json::from_str(json).unwrap();
    let rule = &ruleset.rules[0];
    assert_eq!(rule.name, "helm-image-refs");
    assert_eq!(rule.confidence, Some(0.9));
    assert_eq!(rule.select.len(), 6);
    assert_eq!(rule.create_matches.len(), 2);
    assert_eq!(rule.create_matches[1].parent.as_deref(), Some("repo"));
}

#[test]
fn reject_missing_name() {
    let json = r#"{
        "rules": [{
            "select": [{ "step": "file", "pattern": "*.json" }],
            "emit": []
        }]
    }"#;
    assert!(serde_json::from_str::<RuleSet>(json).is_err());
}

#[test]
fn accept_arbitrary_kind_string() {
    let json = r#"{
        "rules": [{
            "name": "custom",
            "select": [{ "step": "file", "pattern": "*.json" }],
            "emit": [{ "capture": "x", "kind": "helm_value" }]
        }]
    }"#;
    let ruleset: RuleSet = serde_json::from_str(json).unwrap();
    assert_eq!(ruleset.rules[0].create_matches[0].kind, "helm_value");
}

// ── Walk engine tests ──────────────────────────────────────────────

#[test]
fn walk_key_then_key_match() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "dependencies": {
            "express": { "version": "4.18.2" },
            "lodash": { "version": "4.17.21" }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "dependencies".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
    ];

    let results = walk::walk_select(&source, &steps);
    let mut names: Vec<&str> = results.iter()
        .filter_map(|r| r.captures.get("name").map(|c| c.text.as_str()))
        .collect();
    names.sort();
    insta::assert_yaml_snapshot!("walk_dep_names", names);
}

#[test]
fn walk_key_match_capture_then_leaf_capture() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "dependencies": {
            "express": { "version": "4.18.2" },
            "lodash": { "version": "4.17.21" }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "dependencies".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
        SelectStep::Key { name: "version".into(), capture: None },
        SelectStep::Leaf { capture: Some("version".into()) },
    ];

    let results = walk::walk_select(&source, &steps);
    let mut pairs: Vec<(&str, &str)> = results.iter()
        .filter_map(|r| {
            let name = r.captures.get("name")?.text.as_str();
            let ver = r.captures.get("version")?.text.as_str();
            Some((name, ver))
        })
        .collect();
    pairs.sort();
    insta::assert_yaml_snapshot!("walk_dep_name_version_pairs", pairs);
}

#[test]
fn walk_any_descends_arbitrary_depth() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "image": {
            "repository": "myorg/frontend",
            "tag": "v1.2.3"
        },
        "sidecar": {
            "proxy": {
                "image": {
                    "repository": "envoy/envoy",
                    "tag": "v1.28"
                }
            }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Any,
        SelectStep::Key { name: "image".into(), capture: None },
        SelectStep::Object {
            entries: vec![
                ObjectEntry { key: KeyMatcher::Exact("repository".into()), value: vec![SelectStep::Leaf { capture: Some("repo".into()) }] },
                ObjectEntry { key: KeyMatcher::Exact("tag".into()), value: vec![SelectStep::Leaf { capture: Some("tag".into()) }] },
            ],
        },
    ];

    let results = walk::walk_select(&source, &steps);
    let mut repos: Vec<(&str, &str)> = results.iter()
        .filter_map(|r| {
            let repo = r.captures.get("repo")?.text.as_str();
            let tag = r.captures.get("tag")?.text.as_str();
            Some((repo, tag))
        })
        .collect();
    repos.sort();
    insta::assert_yaml_snapshot!("walk_helm_image_refs", repos);
}

#[test]
fn walk_without_any_only_matches_root_level() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "image": {
            "repository": "myorg/frontend",
            "tag": "v1.2.3"
        },
        "sidecar": {
            "proxy": {
                "image": {
                    "repository": "envoy/envoy",
                    "tag": "v1.28"
                }
            }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "image".into(), capture: None },
        SelectStep::Key { name: "repository".into(), capture: None },
        SelectStep::Leaf { capture: Some("repo".into()) },
    ];

    let results = walk::walk_select(&source, &steps);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].captures.get("repo").unwrap().text, "myorg/frontend");
}

#[test]
fn walk_depth_filter() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "a": {
            "b": {
                "c": "deep"
            }
        },
        "shallow": "top"
    }"#).unwrap();

    let steps = vec![
        SelectStep::Any,
        SelectStep::DepthMin { n: 2 },
        SelectStep::Leaf { capture: Some("val".into()) },
    ];

    let results = walk::walk_select(&source, &steps);
    let mut vals: Vec<&str> = results.iter()
        .filter_map(|r| r.captures.get("val").map(|c| c.text.as_str()))
        .collect();
    vals.sort();
    insta::assert_yaml_snapshot!("walk_depth_filter", vals);
}

#[test]
fn walk_object_step_captures_siblings() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "service": {
            "name": "api-gateway",
            "port": 8080,
            "enabled": true
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "service".into(), capture: None },
        SelectStep::Object {
            entries: vec![
                ObjectEntry { key: KeyMatcher::Exact("name".into()), value: vec![SelectStep::Leaf { capture: Some("svc_name".into()) }] },
                ObjectEntry { key: KeyMatcher::Exact("port".into()), value: vec![SelectStep::Leaf { capture: Some("svc_port".into()) }] },
                ObjectEntry { key: KeyMatcher::Exact("enabled".into()), value: vec![SelectStep::Leaf { capture: Some("svc_enabled".into()) }] },
            ],
        },
    ];

    let results = walk::walk_select(&source, &steps);
    assert_eq!(results.len(), 1);
    let caps = &results[0].captures;
    assert_eq!(caps.get("svc_name").unwrap().text, "api-gateway");
    assert_eq!(caps.get("svc_port").unwrap().text, "8080");
    assert_eq!(caps.get("svc_enabled").unwrap().text, "true");
}

// ── Emit tests (walk + value pattern + action -> RawRef) ───────────

#[test]
fn emit_package_lock_deps() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "dependencies": {
            "express": { "version": "4.18.2" },
            "lodash": { "version": "4.17.21" },
            "react": { "version": "18.2.0" }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "dependencies".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
        SelectStep::Key { name: "version".into(), capture: None },
        SelectStep::Leaf { capture: Some("version".into()) },
    ];

    let emits = vec![
        MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
        MatchDef { capture: "version".into(), kind: kind::DEP_VERSION.into(), parent: Some("name".into()), scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, None, "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("emit_package_lock_deps", refs);
}

#[test]
fn emit_pnpm_lock_deps_with_regex_split() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "packages": {
            "express@4.18.2": { "resolution": {} },
            "lodash@4.17.21": { "resolution": {} }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "packages".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];

    let value_pattern = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>[^@]+)@(?P<version>.+)".into(),
        full_match: true,
    };

    let emits = vec![
            MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
            MatchDef { capture: "version".into(), kind: kind::DEP_VERSION.into(), parent: Some("name".into()), scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, Some(&value_pattern), "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("emit_pnpm_lock_deps", refs);
}

#[test]
fn emit_helm_image_object_capture() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "image": {
            "repository": "myorg/frontend",
            "tag": "v1.2.3"
        },
        "sidecar": {
            "proxy": {
                "image": {
                    "repository": "envoy/envoy",
                    "tag": "v1.28"
                }
            }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Any,
        SelectStep::Key { name: "image".into(), capture: None },
        SelectStep::Object {
            entries: vec![
                ObjectEntry { key: KeyMatcher::Exact("repository".into()), value: vec![SelectStep::Leaf { capture: Some("repo".into()) }] },
                ObjectEntry { key: KeyMatcher::Exact("tag".into()), value: vec![SelectStep::Leaf { capture: Some("tag".into()) }] },
            ],
        },
    ];

    let emits = vec![
            MatchDef { capture: "repo".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
            MatchDef { capture: "tag".into(), kind: kind::DEP_VERSION.into(), parent: Some("repo".into()), scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, None, "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("emit_helm_images", refs);
}

#[test]
fn cross_lockfile_same_deps() {
    let npm_source: serde_json::Value = serde_json::from_str(r#"{
        "dependencies": {
            "express": { "version": "4.18.2" },
            "lodash": { "version": "4.17.21" }
        }
    }"#).unwrap();

    let pnpm_source: serde_json::Value = serde_json::from_str(r#"{
        "packages": {
            "express@4.18.2": { "resolution": {} },
            "lodash@4.17.21": { "resolution": {} }
        }
    }"#).unwrap();

    let npm_steps = vec![
        SelectStep::Key { name: "dependencies".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
    ];
    let npm_emits = vec![MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None }];

    let pnpm_steps = vec![
        SelectStep::Key { name: "packages".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];
    let pnpm_value = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>[^@]+)@(?P<version>.+)".into(),
        full_match: true,
    };
    let pnpm_emits = vec![MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None }];

    let npm_refs: Vec<_> = walk::walk_select(&npm_source, &npm_steps).iter()
        .flat_map(|r| emit::create_refs(r, &npm_emits, None, "test"))
        .collect();
    let pnpm_refs: Vec<_> = walk::walk_select(&pnpm_source, &pnpm_steps).iter()
        .flat_map(|r| emit::create_refs(r, &pnpm_emits, Some(&pnpm_value), "test"))
        .collect();

    let mut npm_names: Vec<&str> = npm_refs.iter().map(|r| r.value.as_str()).collect();
    let mut pnpm_names: Vec<&str> = pnpm_refs.iter().map(|r| r.value.as_str()).collect();
    npm_names.sort();
    pnpm_names.sort();

    assert_eq!(npm_names, pnpm_names);
    insta::assert_yaml_snapshot!("cross_lockfile_dep_names", npm_names);
}

// ── Git matcher tests ──────────────────────────────────────────────

#[test]
fn git_match_all_fields() {
    let compiled = git_match::CompiledGitSelector::from_patterns(
        &["org/*"],
        &["main|release/*"],
    ).unwrap();

    assert!(compiled.matches("org/repo", Some("main"), &[]));
    assert!(compiled.matches("org/repo", Some("release/v3"), &[]));
    assert!(!compiled.matches("other/repo", Some("main"), &[]));
    assert!(!compiled.matches("org/repo", Some("dev"), &[]));
}

#[test]
fn git_match_empty_matches_everything() {
    let compiled = git_match::CompiledGitSelector::from_patterns(&[], &[]).unwrap();
    assert!(compiled.matches("anything", Some("any-branch"), &[]));
    assert!(compiled.matches("anything", None, &[]));
}

// ── File matcher tests ─────────────────────────────────────────────

#[test]
fn file_match_single_glob() {
    let compiled = file_match::CompiledFileSelector::from_patterns(&["**/*.json"]).unwrap();
    assert!(compiled.matches("foo/bar.json"));
    assert!(!compiled.matches("foo/bar.yaml"));
}

#[test]
fn file_match_multiple_globs() {
    let compiled = file_match::CompiledFileSelector::from_patterns(&["*.yaml|*.yml"]).unwrap();
    assert!(compiled.matches("values.yaml"));
    assert!(compiled.matches("config.yml"));
    assert!(!compiled.matches("config.json"));
}

// ── Template expansion test ────────────────────────────────────────

#[test]
fn template_expansion() {
    let mut captures = std::collections::HashMap::new();
    captures.insert("repo".into(), walk::CapturedValue {
        text: "myorg/frontend".into(), span_start: 0, span_end: 0,
    });
    captures.insert("tag".into(), walk::CapturedValue {
        text: "v1.2.3".into(), span_start: 0, span_end: 0,
    });

    let result = emit::expand_template("{repo}:{tag}", &captures);
    assert_eq!(result, "myorg/frontend:v1.2.3");
}

// ── Context step ordering validation ──────────────────────────────

#[test]
fn context_step_after_structural_rejected() {
    let json = r#"{
        "rules": [{
            "name": "bad-order",
            "select": [
                { "step": "key", "name": "foo" },
                { "step": "file", "pattern": "*.json" }
            ],
            "emit": [{ "capture": "x", "kind": "y" }]
        }]
    }"#;
    let ruleset: RuleSet = serde_json::from_str(json).unwrap();
    let err = extractor::RuleExtractor::from_ruleset(&ruleset).unwrap_err();
    assert!(err.to_string().contains("context step"), "{}", err);
}

// ── hafley-tsp shaped integration tests ────────────────────────────

#[test]
fn tsp_workspace_deps() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "name": "@hafley/typespec-asyncapi",
        "dependencies": {
            "@typespec/compiler": "^0.64.0",
            "@typespec/http": "^0.64.0",
            "@hafley/typespec-decorator-def": "workspace:*"
        },
        "devDependencies": {
            "@types/node": "^22.13.10",
            "typescript": "~5.7.3",
            "@hafley/alloy-rs": "workspace:*"
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::KeyMatch { pattern: "dependencies|devDependencies|peerDependencies".into(), capture: Some("dep_type".into()) },
        SelectStep::KeyMatch { pattern: "@hafley/*".into(), capture: Some("name".into()) },
        SelectStep::Leaf { capture: Some("version".into()) },
    ];

    let emits = vec![
            MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
            MatchDef { capture: "version".into(), kind: kind::DEP_VERSION.into(), parent: Some("name".into()), scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, None, "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("tsp_workspace_deps", refs);
}

#[test]
fn tsp_package_json_exports() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "name": "@hafley/typespec-decorator-def",
        "exports": {
            "./factory": {
                "import": "./dist/src/factory/index.js",
                "types": "./dist/src/factory/index.d.ts"
            },
            "./codegen": {
                "import": "./dist/src/codegen/index.js",
                "types": "./dist/src/codegen/index.d.ts"
            }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "exports".into(), capture: None },
        SelectStep::KeyMatch { pattern: "./*".into(), capture: Some("export_path".into()) },
        SelectStep::Any,
        SelectStep::Leaf { capture: Some("file_path".into()) },
    ];

    let emits = vec![
            MatchDef { capture: "export_path".into(), kind: kind::EXPORT_NAME.into(), parent: None, scan: None },
            MatchDef { capture: "file_path".into(), kind: kind::IMPORT_PATH.into(), parent: Some("export_path".into()), scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, None, "test"))
        .collect();
    refs.sort_by(|a, b| (&a.value, &a.parent_key).cmp(&(&b.value, &b.parent_key)));
    insta::assert_yaml_snapshot!("tsp_package_exports", refs);
}

#[test]
fn tsp_tsconfig_jsx_import_source() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "compilerOptions": {
            "target": "ES2022",
            "module": "Node16",
            "jsx": "react-jsx",
            "jsxImportSource": "@alloy-js/core"
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "compilerOptions".into(), capture: None },
        SelectStep::Key { name: "jsxImportSource".into(), capture: None },
        SelectStep::Leaf { capture: Some("pkg".into()) },
    ];

    let emits = vec![
            MatchDef { capture: "pkg".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, None, "test"))
        .collect();

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].value, "@alloy-js/core");
    assert_eq!(refs[0].kind, kind::DEP_NAME);
}

#[test]
fn tsp_cargo_toml_deps() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "dependencies": {
            "serde": { "version": "1.0", "features": ["derive"] },
            "axum": "0.8",
            "tokio": { "version": "1", "features": ["full"] }
        },
        "dev-dependencies": {
            "insta": "1"
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::KeyMatch { pattern: "dependencies|dev-dependencies".into(), capture: Some("dep_type".into()) },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
    ];

    let emits = vec![
            MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, None, "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("tsp_cargo_deps", refs);
}

#[test]
fn tsp_pnpm_lock_scoped_packages() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "packages": {
            "@hafley/typespec-decorator-def@0.1.0": { "resolution": {} },
            "@hafley/typespec-asyncapi@0.1.0": { "resolution": {} },
            "@typespec/compiler@0.64.0": { "resolution": {} },
            "typescript@5.7.3": { "resolution": {} }
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "packages".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];

    let value_pattern = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>@[^@]+)@(?P<version>.+)".into(),
        full_match: true,
    };

    let emits = vec![
            MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
            MatchDef { capture: "version".into(), kind: kind::DEP_VERSION.into(), parent: Some("name".into()), scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, Some(&value_pattern), "test"))
        .collect();
    refs.sort_by(|a, b| (&a.value, &a.parent_key).cmp(&(&b.value, &b.parent_key)));

    // typescript@5.7.3 doesn't match the scoped-only pattern -- only 3 scoped packages.
    let dep_names: Vec<&str> = refs.iter().filter(|r| r.kind == "dep_name").map(|r| r.value.as_str()).collect();
    assert_eq!(dep_names.len(), 3);
    assert!(dep_names.contains(&"@hafley/typespec-decorator-def"));
    assert!(dep_names.contains(&"@hafley/typespec-asyncapi"));
    assert!(dep_names.contains(&"@typespec/compiler"));
    assert!(!dep_names.contains(&"typescript"), "unscoped package should not match");

    let versions: Vec<&str> = refs.iter().filter(|r| r.kind == "dep_version").map(|r| r.value.as_str()).collect();
    assert!(versions.contains(&"0.64.0"));
}

#[test]
fn tsp_pnpm_lock_mixed_scoped_and_unscoped() {
    let source: serde_json::Value = serde_json::from_str(r#"{
        "packages": {
            "@hafley/typespec-decorator-def@0.1.0": {},
            "typescript@5.7.3": {},
            "@typespec/compiler@0.64.0": {}
        }
    }"#).unwrap();

    let steps = vec![
        SelectStep::Key { name: "packages".into(), capture: None },
        SelectStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];

    let value_pattern = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>.+)@(?P<version>[^@]+)$".into(),
        full_match: true,
    };

    let emits = vec![
            MatchDef { capture: "name".into(), kind: kind::DEP_NAME.into(), parent: None, scan: None },
            MatchDef { capture: "version".into(), kind: kind::DEP_VERSION.into(), parent: Some("name".into()), scan: None },
    ];

    let walk_results = walk::walk_select(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::create_refs(r, &emits, Some(&value_pattern), "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("tsp_pnpm_lock_mixed", refs);
}

// ── Session 2: User-defined kind extraction rules ─────────────────

/// Helper: build a RuleExtractor from inline JSON, run against source bytes, return sorted refs.
fn run_rule_raw(rule_json: &str, source: &[u8], path: &str) -> Vec<sprefa_extract::RawRef> {
    let ruleset: RuleSet = serde_json::from_str(rule_json).unwrap();
    let ex = extractor::RuleExtractor::from_ruleset(&ruleset).unwrap();
    let ctx = sprefa_extract::ExtractContext::default();
    let mut refs = ex.extract(source, path, &ctx);
    refs.sort_by(|a, b| (&a.kind, &a.value).cmp(&(&b.kind, &b.value)));
    refs
}

/// Helper: build a RuleExtractor from inline JSON, run against JSON source string, return sorted refs.
fn run_rule(rule_json: &str, source_json: &str, path: &str) -> Vec<sprefa_extract::RawRef> {
    run_rule_raw(rule_json, source_json.as_bytes(), path)
}

#[test]
fn rule_tsconfig_paths() {
    let rule = r#"{
        "rules": [{
            "name": "tsconfig-paths",
            "select": [
                { "step": "file", "pattern": "**/tsconfig.json" },
                { "step": "key", "name": "compilerOptions" },
                { "step": "key", "name": "paths" },
                { "step": "key_match", "pattern": "*", "capture": "alias" },
                { "step": "array_item" },
                { "step": "leaf", "capture": "target" }
            ],
            "emit": [
                { "capture": "alias", "kind": "path_alias" },
                { "capture": "target", "kind": "import_path", "parent": "alias" }
            ]
        }]
    }"#;

    let source = r#"{
        "compilerOptions": {
            "baseUrl": ".",
            "paths": {
                "@utils/*": ["src/utils/*"],
                "@components/*": ["src/components/*", "src/shared/components/*"],
                "@config": ["src/config/index.ts"]
            }
        }
    }"#;

    let refs = run_rule(rule, source, "packages/app/tsconfig.json");
    insta::assert_yaml_snapshot!("rule_tsconfig_paths", refs);
}

#[test]
fn rule_package_json_exports() {
    let rule = r#"{
        "rules": [{
            "name": "package-json-exports",
            "select": [
                { "step": "file", "pattern": "**/package.json" },
                { "step": "key", "name": "exports" },
                { "step": "key_match", "pattern": "*", "capture": "subpath" },
                { "step": "any" },
                { "step": "leaf", "capture": "entry" }
            ],
            "emit": [
                { "capture": "subpath", "kind": "package_entry" },
                { "capture": "entry", "kind": "import_path", "parent": "subpath" }
            ]
        }]
    }"#;

    let source = r#"{
        "name": "@myorg/utils",
        "exports": {
            ".": {
                "import": "./dist/index.js",
                "types": "./dist/index.d.ts"
            },
            "./math": {
                "import": "./dist/math.js",
                "types": "./dist/math.d.ts"
            }
        }
    }"#;

    let refs = run_rule(rule, source, "packages/utils/package.json");
    insta::assert_yaml_snapshot!("rule_package_json_exports", refs);
}

#[test]
fn rule_helm_values() {
    let rule = r#"{
        "rules": [{
            "name": "helm-values",
            "select": [
                { "step": "file", "pattern": "**/values.yaml|**/values-*.yaml" },
                { "step": "any" },
                { "step": "depth_min", "n": 1 },
                { "step": "leaf", "capture": "val" }
            ],
            "emit": [
                { "capture": "val", "kind": "helm_value" }
            ]
        }]
    }"#;

    let source = r#"{
        "replicaCount": 3,
        "image": {
            "repository": "myorg/api",
            "tag": "v2.1.0"
        },
        "service": {
            "type": "ClusterIP",
            "port": 8080
        }
    }"#;

    let refs = run_rule(rule, source, "charts/api/values.yaml");
    insta::assert_yaml_snapshot!("rule_helm_values", refs);
}

#[test]
fn rule_k8s_configmap_envs() {
    let rule = r#"{
        "rules": [{
            "name": "k8s-configmap-envs",
            "select": [
                { "step": "file", "pattern": "**/*configmap*.yaml" },
                { "step": "key", "name": "data" },
                { "step": "key_match", "pattern": "*", "capture": "key" }
            ],
            "emit": [
                { "capture": "key", "kind": "env_var_name" }
            ]
        }]
    }"#;

    let source = r#"{
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": { "name": "app-config" },
        "data": {
            "DATABASE_URL": "postgres://db:5432/app",
            "REDIS_HOST": "redis.svc.cluster.local",
            "LOG_LEVEL": "info",
            "MAX_CONNECTIONS": "100"
        }
    }"#;

    let refs = run_rule(rule, source, "k8s/base/app-configmap.yaml");
    insta::assert_yaml_snapshot!("rule_k8s_configmap_envs", refs);
}

#[test]
fn rule_docker_compose_services() {
    let rule = r#"{
        "rules": [{
            "name": "docker-compose-services",
            "select": [
                { "step": "file", "pattern": "**/docker-compose.yaml|**/docker-compose.yml|**/docker-compose*.yaml|**/docker-compose*.yml" },
                { "step": "key", "name": "services" },
                { "step": "key_match", "pattern": "*", "capture": "name" }
            ],
            "emit": [
                { "capture": "name", "kind": "service_name" }
            ]
        }]
    }"#;

    let source = r#"{
        "version": "3.8",
        "services": {
            "api": { "build": "./api", "ports": ["8080:8080"] },
            "worker": { "build": "./worker" },
            "postgres": { "image": "postgres:16" },
            "redis": { "image": "redis:7-alpine" }
        }
    }"#;

    let refs = run_rule(rule, source, "docker-compose.yaml");
    insta::assert_yaml_snapshot!("rule_docker_compose_services", refs);
}

#[test]
fn rule_openapi_operations() {
    let rule = r#"{
        "rules": [{
            "name": "openapi-operations",
            "select": [
                { "step": "file", "pattern": "**/openapi.yaml|**/openapi.yml|**/openapi*.yaml|**/openapi*.yml|**/openapi.json" },
                { "step": "key", "name": "paths" },
                { "step": "key_match", "pattern": "*", "capture": "path" },
                { "step": "key_match", "pattern": "get|post|put|delete|patch|options|head", "capture": "method" },
                { "step": "key", "name": "operationId" },
                { "step": "leaf", "capture": "op_id" }
            ],
            "emit": [
                { "capture": "op_id", "kind": "operation_id" }
            ]
        }]
    }"#;

    let source = r#"{
        "openapi": "3.0.0",
        "paths": {
            "/users": {
                "get": {
                    "operationId": "listUsers",
                    "summary": "List all users"
                },
                "post": {
                    "operationId": "createUser",
                    "summary": "Create a user"
                }
            },
            "/users/{id}": {
                "get": {
                    "operationId": "getUser",
                    "summary": "Get user by ID"
                },
                "delete": {
                    "operationId": "deleteUser",
                    "summary": "Delete a user"
                }
            }
        }
    }"#;

    let refs = run_rule(rule, source, "api/openapi.yaml");
    insta::assert_yaml_snapshot!("rule_openapi_operations", refs);
}

#[test]
fn rule_cargo_workspace_members() {
    let rule = r#"{
        "rules": [{
            "name": "cargo-workspace-members",
            "select": [
                { "step": "file", "pattern": "**/Cargo.toml" },
                { "step": "key", "name": "workspace" },
                { "step": "key", "name": "members" },
                { "step": "array_item" },
                { "step": "leaf", "capture": "member" }
            ],
            "emit": [
                { "capture": "member", "kind": "workspace_member" }
            ]
        }]
    }"#;

    let source = r#"
[workspace]
members = [
    "crates/core",
    "crates/cli",
    "crates/extract",
    "crates/rules",
    "crates/schema",
]
resolver = "2"
"#;

    let refs = run_rule_raw(rule, source.as_bytes(), "Cargo.toml");
    insta::assert_yaml_snapshot!("rule_cargo_workspace_members", refs);
}

#[test]
fn rule_pnpm_workspace() {
    let rule = r#"{
        "rules": [{
            "name": "pnpm-workspace",
            "select": [
                { "step": "file", "pattern": "**/pnpm-workspace.yaml" },
                { "step": "key", "name": "packages" },
                { "step": "array_item" },
                { "step": "leaf", "capture": "member" }
            ],
            "emit": [
                { "capture": "member", "kind": "workspace_member" }
            ]
        }]
    }"#;

    let source = r#"{
        "packages": [
            "packages/*",
            "apps/*",
            "tools/scripts"
        ]
    }"#;

    let refs = run_rule(rule, source, "pnpm-workspace.yaml");
    insta::assert_yaml_snapshot!("rule_pnpm_workspace", refs);
}
