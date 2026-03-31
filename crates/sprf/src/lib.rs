pub mod _0_ast;
pub mod _1_parse;
pub mod _2_pattern;
pub mod _3_lower;

use std::path::Path;

use anyhow::Result;
use sprefa_rules::types::RuleSet;

/// Parse a .sprf file and produce a RuleSet compatible with the JSON rule format.
pub fn parse_sprf(source: &str) -> Result<RuleSet> {
    let program = _1_parse::parse_program(source)?;
    _3_lower::lower_program(&program)
}

/// Load a .sprf file from disk and produce a RuleSet.
pub fn load_sprf(path: &Path) -> Result<RuleSet> {
    let source = std::fs::read_to_string(path)?;
    parse_sprf(&source)
}

#[cfg(test)]
mod integration_tests {
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

    fn run_sprf(sprf_src: &str, data: &[u8], ext: &str) -> Vec<Vec<(String, String)>> {
        let ruleset = parse_sprf(sprf_src).unwrap();
        let json_val = parse_data(data, ext).unwrap();
        let mut all = vec![];
        for rule in &ruleset.rules {
            let structural: Vec<_> = rule.select.iter()
                .filter(|s| !s.is_context_step())
                .cloned()
                .collect();
            let results = walk::walk(&json_val, &structural);
            for m in results {
                let mut caps: Vec<_> = m.captures.iter()
                    .map(|(k, v)| (k.clone(), v.text.clone()))
                    .collect();
                caps.sort();
                all.push(caps);
            }
        }
        all
    }

    #[test]
    fn cargo_package_name() {
        let sprf = "fs(**/Cargo.toml) > json({ package: { name: $NAME } });";
        let toml = br#"
            [package]
            name = "sprefa"
            version = "0.1.0"
        "#;
        let results = run_sprf(sprf, toml, "toml");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0], vec![("NAME".into(), "sprefa".into())]);
    }

    #[test]
    fn cargo_workspace_members() {
        let sprf = "fs(**/Cargo.toml) > json({ workspace: { members: [...$MEMBER] } });";
        let toml = br#"
            [workspace]
            members = ["crates/foo", "crates/bar"]
        "#;
        let results = run_sprf(sprf, toml, "toml");
        assert_eq!(results.len(), 2);
        let members: Vec<_> = results.iter()
            .map(|caps| caps.iter().find(|(k,_)| k == "MEMBER").unwrap().1.clone())
            .collect();
        assert!(members.contains(&"crates/foo".to_string()));
        assert!(members.contains(&"crates/bar".to_string()));
    }

    #[test]
    fn json_nested_deps() {
        let sprf = "fs(**/package.json) > json({ dependencies: { $NAME: $VERSION } });";
        let json = br#"{ "dependencies": { "express": "4.18.2", "lodash": "4.17.21" } }"#;
        let mut results = run_sprf(sprf, json, "json");
        results.sort();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], vec![("NAME".into(), "express".into()), ("VERSION".into(), "4.18.2".into())]);
        assert_eq!(results[1], vec![("NAME".into(), "lodash".into()), ("VERSION".into(), "4.17.21".into())]);
    }

    #[test]
    fn yaml_recursive_descent() {
        let sprf = "fs(**/values.yaml) > json({ **: { image: { repository: $REPO, tag: $TAG } } });";
        let yaml = b"services:\n  frontend:\n    image:\n      repository: myorg/frontend\n      tag: v1.2.3\n";
        let mut results = run_sprf(sprf, yaml, "yaml");
        assert_eq!(results.len(), 1);
        results[0].sort();
        assert_eq!(results[0], vec![
            ("REPO".into(), "myorg/frontend".into()),
            ("TAG".into(), "v1.2.3".into()),
        ]);
    }

    #[test]
    fn multiple_rules() {
        let sprf = r#"
            fs(**/Cargo.toml) > json({ package: { name: $NAME } });
            fs(**/Cargo.toml) > json({ workspace: { members: [...$M] } });
        "#;
        let toml = br#"
            [package]
            name = "hello"
            [workspace]
            members = ["a", "b"]
        "#;
        let results = run_sprf(sprf, toml, "toml");
        // 1 from package name + 2 from workspace members
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn kitchen_sink_sprf_parses() {
        let sprf_src = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../sprefa-rules.sprf")
        ).unwrap();
        let ruleset = parse_sprf(&sprf_src).unwrap();
        assert!(ruleset.rules.len() >= 10, "expected 10+ rules, got {}", ruleset.rules.len());
        assert!(ruleset.link_rules.len() >= 3, "expected 3+ link rules, got {}", ruleset.link_rules.len());

        // Every rule should have at least one create_matches entry
        for rule in &ruleset.rules {
            assert!(
                !rule.create_matches.is_empty(),
                "rule '{}' has no match slots",
                rule.name
            );
        }
    }

    #[test]
    fn kitchen_sink_cargo_deps() {
        let sprf_src = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../sprefa-rules.sprf")
        ).unwrap();
        let ruleset = parse_sprf(&sprf_src).unwrap();

        // Run the cargo-deps rule against the rules crate Cargo.toml
        let cargo_bytes = std::fs::read(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../rules/Cargo.toml")
        ).unwrap();
        let json_val = parse_data(&cargo_bytes, "toml").unwrap();

        // Find the dep_name rule (second rule, index 1)
        let dep_rule = ruleset.rules.iter()
            .find(|r| r.create_matches.iter().any(|m| m.kind == "dep_name"))
            .expect("no dep_name rule found");

        let structural: Vec<_> = dep_rule.select.iter()
            .filter(|s| !s.is_context_step())
            .cloned()
            .collect();
        let results = walk::walk(&json_val, &structural);
        let dep_names: Vec<_> = results.iter()
            .filter_map(|m| m.captures.get("NAME").map(|c| c.text.clone()))
            .collect();
        // $_ matches any value shape, so workspace = { workspace = true } deps are included
        assert!(dep_names.contains(&"serde".to_string()), "expected serde in deps, got {:?}", dep_names);
        assert!(dep_names.contains(&"anyhow".to_string()), "expected anyhow in deps, got {:?}", dep_names);
        assert!(dep_names.contains(&"ast-grep-core".to_string()), "expected ast-grep-core in deps, got {:?}", dep_names);
    }

    #[test]
    fn real_workspace_cargo_toml() {
        let toml_bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.toml")).unwrap();
        let sprf = "fs(**/Cargo.toml) > json({ workspace: { members: [...$MEMBER] } });";
        let results = run_sprf(sprf, &toml_bytes, "toml");
        assert!(results.len() > 5, "expected multiple workspace members, got {}", results.len());
        let members: Vec<_> = results.iter()
            .map(|caps| caps.iter().find(|(k,_)| k == "MEMBER").unwrap().1.clone())
            .collect();
        assert!(members.contains(&"crates/sprf".to_string()));
        assert!(members.contains(&"crates/rules".to_string()));
    }
}
