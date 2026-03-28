use std::path::Path;

use anyhow::Result;
use sprefa_extract::{ExtractContext, Extractor, RawRef};

use crate::{
    emit, walk,
    file_match::CompiledFileSelector,
    git_match::CompiledGitSelector,
    types::{EmitRef, RuleSet, StructStep, ValuePattern},
};

// All data extensions this extractor handles.
// The file selector on each rule does the fine-grained filtering.
const DATA_EXTENSIONS: &[&str] = &["json", "yaml", "yml", "toml"];

pub struct CompiledRule {
    pub name: String,
    pub git: Option<CompiledGitSelector>,
    pub file: CompiledFileSelector,
    pub steps: Vec<StructStep>,
    pub value_pattern: Option<ValuePattern>,
    pub emit: Vec<EmitRef>,
}

pub struct RuleExtractor {
    rules: Vec<CompiledRule>,
}

impl RuleExtractor {
    pub fn from_ruleset(ruleset: &RuleSet) -> Result<Self> {
        let rules = ruleset
            .rules
            .iter()
            .filter(|r| r.select.is_some()) // skip ast-only rules until ast engine exists
            .map(|r| {
                let git = r.git.as_ref().map(CompiledGitSelector::compile).transpose()?;
                let file = CompiledFileSelector::compile(&r.file)?;
                Ok(CompiledRule {
                    name: r.name.clone(),
                    git,
                    file,
                    steps: r.select.clone().unwrap_or_default(),
                    value_pattern: r.value.clone(),
                    emit: r.emit.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { rules })
    }

    pub fn from_json(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let ruleset: RuleSet = serde_json::from_slice(&bytes)?;
        Self::from_ruleset(&ruleset)
    }

    pub fn from_yaml(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let ruleset: RuleSet = serde_yaml::from_slice(&bytes)?;
        Self::from_ruleset(&ruleset)
    }

    /// Filter rules to those whose file selector could match the given path.
    /// Used by the scanner to skip rules early without parsing the file.
    pub fn rules_for_path<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a CompiledRule> + 'a {
        self.rules.iter().filter(move |r| r.file.matches(path))
    }
}

impl Extractor for RuleExtractor {
    fn extensions(&self) -> &[&str] {
        DATA_EXTENSIONS
    }

    fn extract(&self, source: &[u8], path: &str, ctx: &ExtractContext) -> Vec<RawRef> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let value = match parse_data(source, ext) {
            Some(v) => v,
            None => return vec![],
        };

        let mut refs = vec![];
        for rule in self.rules_for_path(path) {
            if let Some(ref git) = rule.git {
                let repo = ctx.repo.unwrap_or("");
                if !git.matches(repo, ctx.branch, ctx.tags) {
                    continue;
                }
            }
            let results = walk::walk(&value, &rule.steps);
            for result in results {
                refs.extend(emit::emit_refs(&result, &rule.emit, rule.value_pattern.as_ref(), &rule.name));
            }
        }
        refs
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    const RULES_JSON: &str = r#"{
        "rules": [
            {
                "name": "npm-deps",
                "file": ["**/package.json", "**/package-lock.json"],
                "select": [
                    { "step": "key", "name": "dependencies" },
                    { "step": "key_match", "pattern": "*", "capture": "name" },
                    { "step": "key", "name": "version" },
                    { "step": "leaf", "capture": "version" }
                ],
                "emit": [
                    { "capture": "name", "kind": "dep_name" },
                    { "capture": "version", "kind": "dep_version", "parent": "name" }
                ]
            },
            {
                "name": "helm-images",
                "file": ["**/values.yaml", "**/values-*.yaml"],
                "select": [
                    { "step": "any" },
                    { "step": "key", "name": "image" },
                    { "step": "object", "captures": { "repository": "repo", "tag": "tag" } }
                ],
                "emit": [
                    { "capture": "repo", "kind": "dep_name" },
                    { "capture": "tag", "kind": "dep_version", "parent": "repo" }
                ]
            }
        ]
    }"#;

    fn make_extractor() -> RuleExtractor {
        let ruleset: RuleSet = serde_json::from_str(RULES_JSON).unwrap();
        RuleExtractor::from_ruleset(&ruleset).unwrap()
    }

    fn run(extractor: &RuleExtractor, src: &[u8], path: &str) -> Vec<RawRef> {
        extractor.extract(src, path, &ExtractContext::default())
    }

    #[test]
    fn extracts_json_package_deps() {
        let ex = make_extractor();
        let src = br#"{
            "dependencies": {
                "express": { "version": "4.18.2" },
                "lodash": { "version": "4.17.21" }
            }
        }"#;
        let mut refs = run(&ex, src, "apps/api/package-lock.json");
        refs.sort_by(|a, b| a.value.cmp(&b.value));
        insta::assert_yaml_snapshot!("extractor_json_deps", refs);
    }

    #[test]
    fn extracts_yaml_helm_values() {
        let ex = make_extractor();
        let src = b"image:\n  repository: myorg/frontend\n  tag: v1.2.3\n";
        let mut refs = run(&ex, src, "charts/values.yaml");
        refs.sort_by(|a, b| a.value.cmp(&b.value));
        insta::assert_yaml_snapshot!("extractor_yaml_helm", refs);
    }

    #[test]
    fn extracts_toml_cargo_deps() {
        let ex = make_extractor();
        // Cargo.toml doesn't match any rule file selector so should return empty
        let src = b"[dependencies]\nserde = \"1\"\n";
        let refs = run(&ex, src, "Cargo.toml");
        assert!(refs.is_empty());
    }

    #[test]
    fn file_selector_filters_unmatched_paths() {
        let ex = make_extractor();
        // A JSON file that doesn't match any rule's file selector
        let src = br#"{ "dependencies": { "foo": { "version": "1.0" } } }"#;
        let refs = run(&ex, src, "config/db-config.json");
        assert!(refs.is_empty());
    }

    #[test]
    fn node_path_is_populated() {
        let ex = make_extractor();
        let src = br#"{
            "dependencies": {
                "express": { "version": "4.18.2" }
            }
        }"#;
        let refs = run(&ex, src, "package-lock.json");
        // All refs should have node_path set
        assert!(refs.iter().all(|r| r.node_path.is_some()));
        // The version ref path should reflect the structural walk
        let version_ref = refs.iter().find(|r| r.value == "4.18.2").unwrap();
        assert_eq!(version_ref.node_path.as_deref(), Some("dependencies/express/version"));
    }

    #[test]
    fn from_ruleset_skips_ast_only_rules() {
        let json = r#"{
            "rules": [
                {
                    "name": "ast-rule",
                    "file": "**/*.rs",
                    "select_ast": { "pattern": "use $PATH" },
                    "emit": [{ "capture": "$PATH", "kind": "rs_use" }]
                }
            ]
        }"#;
        let ruleset: RuleSet = serde_json::from_str(json).unwrap();
        let ex = RuleExtractor::from_ruleset(&ruleset).unwrap();
        // ast-only rule has no `select` so it's filtered out
        assert_eq!(ex.rules.len(), 0);
    }
}
