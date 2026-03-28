use sprefa_extract::kind;
use sprefa_rules::*;

// ── Schema + deserialization tests ─────────────────────────────────

#[test]
fn schema_snapshot() {
    let schema = schema::generate_schema_string();
    insta::assert_snapshot!("rule_schema", schema);
}

#[test]
fn minimal_rule() {
    let json = r#"{
        "rules": [{
            "name": "catch-all",
            "file": "**/*.json",
            "action": {
                "emit": [{ "capture": "val", "kind": "json_value" }]
            }
        }]
    }"#;
    let ruleset: RuleSet = serde_json::from_str(json).unwrap();
    insta::assert_yaml_snapshot!("minimal_rule", ruleset);
}

#[test]
fn full_rule_with_captures() {
    let json = r#"{
        "$schema": "sprefa-rules.schema.json",
        "rules": [{
            "name": "helm-image-refs",
            "description": "Docker image references in Helm values files",
            "git": {
                "repo": "*/helm-charts",
                "branch": "main|release/*",
                "tag": "v*"
            },
            "file": ["values.yaml", "values-*.yaml"],
            "select": [
                { "step": "any" },
                { "step": "key", "name": "image" },
                { "step": "object", "captures": { "repository": "repo", "tag": "tag" } }
            ],
            "action": {
                "emit": [
                    { "capture": "repo", "kind": "dep_name" },
                    { "capture": "tag", "kind": "dep_version", "parent": "repo" }
                ],
                "target_repo": "{repo}",
                "confidence": 0.9
            }
        }]
    }"#;
    let ruleset: RuleSet = serde_json::from_str(json).unwrap();
    insta::assert_yaml_snapshot!("full_rule_with_captures", ruleset);
}

#[test]
fn action_kind_to_kind_str() {
    let pairs = [
        (ActionKind::StringLiteral, "string_literal"),
        (ActionKind::JsonKey, "json_key"),
        (ActionKind::JsonValue, "json_value"),
        (ActionKind::YamlKey, "yaml_key"),
        (ActionKind::YamlValue, "yaml_value"),
        (ActionKind::TomlKey, "toml_key"),
        (ActionKind::TomlValue, "toml_value"),
        (ActionKind::ImportPath, kind::IMPORT_PATH),
        (ActionKind::ImportName, kind::IMPORT_NAME),
        (ActionKind::ExportName, kind::EXPORT_NAME),
        (ActionKind::DepName, kind::DEP_NAME),
        (ActionKind::DepVersion, kind::DEP_VERSION),
        (ActionKind::RsUse, kind::RS_USE),
        (ActionKind::RsDeclare, kind::RS_DECLARE),
        (ActionKind::RsMod, kind::RS_MOD),
    ];
    for (action_kind, expected) in pairs {
        assert_eq!(action_kind.to_kind_str(), expected);
    }
}

#[test]
fn reject_missing_name() {
    let json = r#"{
        "rules": [{
            "file": "*.json",
            "action": { "emit": [] }
        }]
    }"#;
    assert!(serde_json::from_str::<RuleSet>(json).is_err());
}

#[test]
fn reject_invalid_kind() {
    let json = r#"{
        "rules": [{
            "name": "bad",
            "file": "*.json",
            "action": { "emit": [{ "capture": "x", "kind": "not_real" }] }
        }]
    }"#;
    assert!(serde_json::from_str::<RuleSet>(json).is_err());
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
        StructStep::Key { name: "dependencies".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
    ];

    let results = walk::walk(&source, &steps);
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
        StructStep::Key { name: "dependencies".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
        StructStep::Key { name: "version".into(), capture: None },
        StructStep::Leaf { capture: Some("version".into()) },
    ];

    let results = walk::walk(&source, &steps);
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
        StructStep::Any,
        StructStep::Key { name: "image".into(), capture: None },
        StructStep::Object {
            captures: [
                ("repository".into(), "repo".into()),
                ("tag".into(), "tag".into()),
            ].into_iter().collect(),
        },
    ];

    let results = walk::walk(&source, &steps);
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

    // No Any step -- only matches the root-level "image"
    let steps = vec![
        StructStep::Key { name: "image".into(), capture: None },
        StructStep::Key { name: "repository".into(), capture: None },
        StructStep::Leaf { capture: Some("repo".into()) },
    ];

    let results = walk::walk(&source, &steps);
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
        StructStep::Any,
        StructStep::DepthMin { n: 2 },
        StructStep::Leaf { capture: Some("val".into()) },
    ];

    let results = walk::walk(&source, &steps);
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
        StructStep::Key { name: "service".into(), capture: None },
        StructStep::Object {
            captures: [
                ("name".into(), "svc_name".into()),
                ("port".into(), "svc_port".into()),
                ("enabled".into(), "svc_enabled".into()),
            ].into_iter().collect(),
        },
    ];

    let results = walk::walk(&source, &steps);
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
        StructStep::Key { name: "dependencies".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
        StructStep::Key { name: "version".into(), capture: None },
        StructStep::Leaf { capture: Some("version".into()) },
    ];

    let action = Action {
        emit: vec![
            EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None },
            EmitRef { capture: "version".into(), kind: ActionKind::DepVersion, parent: Some("name".into()) },
        ],
        target_repo: None,
        target_path: None,
        confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, None, "test"))
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
        StructStep::Key { name: "packages".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];

    let value_pattern = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>[^@]+)@(?P<version>.+)".into(),
        full_match: true,
    };

    let action = Action {
        emit: vec![
            EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None },
            EmitRef { capture: "version".into(), kind: ActionKind::DepVersion, parent: Some("name".into()) },
        ],
        target_repo: None,
        target_path: None,
        confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, Some(&value_pattern), "test"))
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
        StructStep::Any,
        StructStep::Key { name: "image".into(), capture: None },
        StructStep::Object {
            captures: [
                ("repository".into(), "repo".into()),
                ("tag".into(), "tag".into()),
            ].into_iter().collect(),
        },
    ];

    let action = Action {
        emit: vec![
            EmitRef { capture: "repo".into(), kind: ActionKind::DepName, parent: None },
            EmitRef { capture: "tag".into(), kind: ActionKind::DepVersion, parent: Some("repo".into()) },
        ],
        target_repo: Some("{repo}".into()),
        target_path: None,
        confidence: Some(0.9),
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, None, "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("emit_helm_images", refs);
}

#[test]
fn cross_lockfile_same_deps() {
    // package-lock.json format
    let npm_source: serde_json::Value = serde_json::from_str(r#"{
        "dependencies": {
            "express": { "version": "4.18.2" },
            "lodash": { "version": "4.17.21" }
        }
    }"#).unwrap();

    // pnpm-lock.yaml format (as JSON for test simplicity)
    let pnpm_source: serde_json::Value = serde_json::from_str(r#"{
        "packages": {
            "express@4.18.2": { "resolution": {} },
            "lodash@4.17.21": { "resolution": {} }
        }
    }"#).unwrap();

    // npm rule
    let npm_steps = vec![
        StructStep::Key { name: "dependencies".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
    ];
    let npm_action = Action {
        emit: vec![EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None }],
        target_repo: None, target_path: None, confidence: None,
    };

    // pnpm rule
    let pnpm_steps = vec![
        StructStep::Key { name: "packages".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];
    let pnpm_value = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>[^@]+)@(?P<version>.+)".into(),
        full_match: true,
    };
    let pnpm_action = Action {
        emit: vec![EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None }],
        target_repo: None, target_path: None, confidence: None,
    };

    let npm_refs: Vec<_> = walk::walk(&npm_source, &npm_steps).iter()
        .flat_map(|r| emit::emit_refs(r, &npm_action, None, "test"))
        .collect();
    let pnpm_refs: Vec<_> = walk::walk(&pnpm_source, &pnpm_steps).iter()
        .flat_map(|r| emit::emit_refs(r, &pnpm_action, Some(&pnpm_value), "test"))
        .collect();

    // Both formats produce the same dep names
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
    let sel = GitSelector {
        repo: Some("org/*".into()),
        branch: Some("main|release/*".into()),
        tag: Some("v*".into()),
    };
    let compiled = git_match::CompiledGitSelector::compile(&sel).unwrap();

    assert!(compiled.matches("org/repo", Some("main"), &["v1.0"]));
    assert!(!compiled.matches("other/repo", Some("main"), &["v1.0"]));
    assert!(!compiled.matches("org/repo", Some("dev"), &["v1.0"]));
    assert!(!compiled.matches("org/repo", Some("main"), &["latest"]));
}

#[test]
fn git_match_empty_matches_everything() {
    let sel = GitSelector { repo: None, branch: None, tag: None };
    let compiled = git_match::CompiledGitSelector::compile(&sel).unwrap();
    assert!(compiled.matches("anything", Some("any-branch"), &[]));
    assert!(compiled.matches("anything", None, &[]));
}

// ── File matcher tests ─────────────────────────────────────────────

#[test]
fn file_match_single_glob() {
    let sel = FileSelector::Single("**/*.json".into());
    let compiled = file_match::CompiledFileSelector::compile(&sel).unwrap();
    assert!(compiled.matches("foo/bar.json"));
    assert!(!compiled.matches("foo/bar.yaml"));
}

#[test]
fn file_match_multiple_globs() {
    let sel = FileSelector::Multiple(vec!["*.yaml".into(), "*.yml".into()]);
    let compiled = file_match::CompiledFileSelector::compile(&sel).unwrap();
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

// ── hafley-tsp shaped integration tests ────────────────────────────

#[test]
fn tsp_workspace_deps() {
    // asyncapi/package.json depends on decorator-def via workspace:*
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
        StructStep::KeyMatch { pattern: "dependencies|devDependencies|peerDependencies".into(), capture: Some("dep_type".into()) },
        StructStep::KeyMatch { pattern: "@hafley/*".into(), capture: Some("name".into()) },
        StructStep::Leaf { capture: Some("version".into()) },
    ];

    let action = Action {
        emit: vec![
            EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None },
            EmitRef { capture: "version".into(), kind: ActionKind::DepVersion, parent: Some("name".into()) },
        ],
        target_repo: None, target_path: None, confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, None, "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("tsp_workspace_deps", refs);
}

#[test]
fn tsp_package_json_exports() {
    // decorator-def exports ./factory and ./codegen subpaths
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
        StructStep::Key { name: "exports".into(), capture: None },
        StructStep::KeyMatch { pattern: "./*".into(), capture: Some("export_path".into()) },
        StructStep::Any,
        StructStep::Leaf { capture: Some("file_path".into()) },
    ];

    let action = Action {
        emit: vec![
            EmitRef { capture: "export_path".into(), kind: ActionKind::ExportName, parent: None },
            EmitRef { capture: "file_path".into(), kind: ActionKind::ImportPath, parent: Some("export_path".into()) },
        ],
        target_repo: None, target_path: None, confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, None, "test"))
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
        StructStep::Key { name: "compilerOptions".into(), capture: None },
        StructStep::Key { name: "jsxImportSource".into(), capture: None },
        StructStep::Leaf { capture: Some("pkg".into()) },
    ];

    let action = Action {
        emit: vec![
            EmitRef { capture: "pkg".into(), kind: ActionKind::DepName, parent: None },
        ],
        target_repo: None, target_path: None, confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, None, "test"))
        .collect();

    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].value, "@alloy-js/core");
    assert_eq!(refs[0].kind, kind::DEP_NAME);
}

#[test]
fn tsp_cargo_toml_deps() {
    // Test-output Cargo.toml with mixed dep formats
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
        StructStep::KeyMatch { pattern: "dependencies|dev-dependencies".into(), capture: Some("dep_type".into()) },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("name".into()) },
    ];

    let action = Action {
        emit: vec![
            EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None },
        ],
        target_repo: None, target_path: None, confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, None, "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("tsp_cargo_deps", refs);
}

#[test]
fn tsp_pnpm_lock_scoped_packages() {
    // pnpm-lock with scoped @hafley/ packages
    let source: serde_json::Value = serde_json::from_str(r#"{
        "packages": {
            "@hafley/typespec-decorator-def@0.1.0": { "resolution": {} },
            "@hafley/typespec-asyncapi@0.1.0": { "resolution": {} },
            "@typespec/compiler@0.64.0": { "resolution": {} },
            "typescript@5.7.3": { "resolution": {} }
        }
    }"#).unwrap();

    let steps = vec![
        StructStep::Key { name: "packages".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];

    // Scoped packages: @scope/name@version -- need to handle the double @
    let value_pattern = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>@[^@]+)@(?P<version>.+)".into(),
        full_match: true,
    };

    let action = Action {
        emit: vec![
            EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None },
            EmitRef { capture: "version".into(), kind: ActionKind::DepVersion, parent: Some("name".into()) },
        ],
        target_repo: None, target_path: None, confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, Some(&value_pattern), "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("tsp_pnpm_lock_scoped", refs);
}

#[test]
fn tsp_pnpm_lock_mixed_scoped_and_unscoped() {
    // Mix of scoped (@hafley/foo@1.0) and unscoped (typescript@5.7)
    // Need a regex that handles both
    let source: serde_json::Value = serde_json::from_str(r#"{
        "packages": {
            "@hafley/typespec-decorator-def@0.1.0": {},
            "typescript@5.7.3": {},
            "@typespec/compiler@0.64.0": {}
        }
    }"#).unwrap();

    let steps = vec![
        StructStep::Key { name: "packages".into(), capture: None },
        StructStep::KeyMatch { pattern: "*".into(), capture: Some("raw".into()) },
    ];

    // This regex handles both: greedy up to the LAST @ for scoped, works for unscoped too
    let value_pattern = ValuePattern {
        source: "raw".into(),
        pattern: r"(?P<name>.+)@(?P<version>[^@]+)$".into(),
        full_match: true,
    };

    let action = Action {
        emit: vec![
            EmitRef { capture: "name".into(), kind: ActionKind::DepName, parent: None },
            EmitRef { capture: "version".into(), kind: ActionKind::DepVersion, parent: Some("name".into()) },
        ],
        target_repo: None, target_path: None, confidence: None,
    };

    let walk_results = walk::walk(&source, &steps);
    let mut refs: Vec<_> = walk_results.iter()
        .flat_map(|r| emit::emit_refs(r, &action, Some(&value_pattern), "test"))
        .collect();
    refs.sort_by(|a, b| a.value.cmp(&b.value));
    insta::assert_yaml_snapshot!("tsp_pnpm_lock_mixed", refs);
}
