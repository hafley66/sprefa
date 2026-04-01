use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Result};
use sprefa_extract::{ExtractContext, Extractor, RawRef};

use crate::{
    ast, emit, walk,
    file_match::CompiledFileSelector,
    git_match::CompiledGitSelector,
    types::{AstSelector, MatchDef, RuleSet, SelectStep, ValuePattern},
    walk::{CapturedValue, CompiledStep},
};


#[derive(Debug)]
pub struct CompiledRule {
    pub name: String,
    pub git: CompiledGitSelector,
    pub file: CompiledFileSelector,
    /// (capture_name, value) pairs seeded from context step captures.
    pub context_captures: Vec<(String, ContextCaptureSource)>,
    pub steps: Vec<CompiledStep>,
    pub ast: Option<AstSelector>,
    pub value_pattern: Option<ValuePattern>,
    pub create_matches: Vec<MatchDef>,
}

/// Where a context capture gets its value from at match time.
#[derive(Debug)]
pub enum ContextCaptureSource {
    Repo,
    Rev,
    /// Capture the filename (basename without extension).
    FileName,
    /// Capture the directory portion of the path.
    FolderPath,
}

#[derive(Debug)]
pub struct RuleExtractor {
    rules: Vec<CompiledRule>,
    /// Directory of the rules file, used to resolve `rule_file` paths.
    config_dir: Option<std::path::PathBuf>,
}

impl RuleExtractor {
    pub fn from_ruleset(ruleset: &RuleSet) -> Result<Self> {
        Self::from_ruleset_with_dir(ruleset, None)
    }

    pub fn from_ruleset_with_dir(ruleset: &RuleSet, config_dir: Option<&Path>) -> Result<Self> {
        let rules = ruleset
            .rules
            .iter()
            .filter(|r| {
                // Rule is valid if it has structural steps OR an ast selector.
                r.select.iter().any(|s| !s.is_context_step()) || r.select_ast.is_some()
            })
            .map(compile_rule)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { rules, config_dir: config_dir.map(|p| p.to_path_buf()) })
    }

    pub fn from_json(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let ruleset: RuleSet = serde_json::from_slice(&bytes)?;
        Self::from_ruleset_with_dir(&ruleset, path.parent())
    }

    pub fn from_yaml(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let ruleset: RuleSet = serde_yaml::from_slice(&bytes)?;
        Self::from_ruleset_with_dir(&ruleset, path.parent())
    }

    /// Filter rules to those whose file selector could match the given path.
    pub fn rules_for_path<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a CompiledRule> + 'a {
        self.rules.iter().filter(move |r| {
            r.file.is_empty() || r.file.matches(path)
        })
    }

    /// Run rules and return raw MatchResults (captures) without going through emit.
    /// Used by `sprefa eval` when no match() slots are present.
    pub fn eval_raw(&self, source: &[u8], path: &str, ctx: &ExtractContext) -> Vec<walk::MatchResult> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let mut all = vec![];
        for rule in self.rules_for_path(path) {
            let repo = ctx.repo.unwrap_or("");
            let git_caps = match rule.git.matches_with_captures(repo, ctx.branch, ctx.tags) {
                Some(c) => c,
                None => continue,
            };

            let file_caps = match rule.file.matches_with_captures(path) {
                Some(c) => c,
                None => continue,
            };

            let context_caps = resolve_context_captures(
                &rule.context_captures, ctx, path, &git_caps, &file_caps,
            );

            let results = if let Some(ast_sel) = &rule.ast {
                ast::ast_match(source, path, ast_sel, self.config_dir.as_deref())
            } else {
                let value = match parse_data(source, ext) {
                    Some(v) => v,
                    None => continue,
                };
                walk::walk(&value, &rule.steps)
            };

            for result in results {
                if context_caps.is_empty() {
                    all.push(result);
                } else {
                    let mut merged = result;
                    for (name, cv) in &context_caps {
                        merged.captures.insert(name.clone(), cv.clone());
                    }
                    all.push(merged);
                }
            }
        }
        all
    }
}

impl Extractor for RuleExtractor {
    fn extensions(&self) -> &[&str] {
        // Claim both structured-data and source-code extensions.
        // Per-rule file selectors do the precise filtering within extract().
        // The run-all dispatch in crates/index means claiming an ext already
        // claimed by JsExtractor/RsExtractor is fine -- refs are merged.
        &[
            "json", "yaml", "yml", "toml",
            "js", "jsx", "cjs", "mjs", "ts", "tsx", "cts", "mts",
            "rs",
            "py", "py3", "pyi",
            "go",
            "kt", "kts",
            "sh", "bash", "zsh",
        ]
    }

    fn extract(&self, source: &[u8], path: &str, ctx: &ExtractContext) -> Vec<RawRef> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let mut refs = vec![];
        let mut group_counter: u32 = 0;
        for rule in self.rules_for_path(path) {
            let repo = ctx.repo.unwrap_or("");
            let git_caps = match rule.git.matches_with_captures(repo, ctx.branch, ctx.tags) {
                Some(c) => c,
                None => continue,
            };

            let file_caps = match rule.file.matches_with_captures(path) {
                Some(c) => c,
                None => continue,
            };

            let context_caps = resolve_context_captures(
                &rule.context_captures, ctx, path, &git_caps, &file_caps,
            );

            let results = if let Some(ast_sel) = &rule.ast {
                ast::ast_match(source, path, ast_sel, self.config_dir.as_deref())
            } else {
                let value = match parse_data(source, ext) {
                    Some(v) => v,
                    None => continue,
                };
                walk::walk(&value, &rule.steps)
            };

            let has_matches = !rule.create_matches.is_empty();
            for result in results {
                let merged = if context_caps.is_empty() {
                    result.clone()
                } else {
                    let mut merged = result.clone();
                    for (name, cv) in &context_caps {
                        merged.captures.insert(name.clone(), cv.clone());
                    }
                    merged
                };
                let group = if has_matches {
                    let g = group_counter;
                    group_counter += 1;
                    Some(g)
                } else {
                    None
                };
                refs.extend(emit::create_refs(&merged, &rule.create_matches, rule.value_pattern.as_ref(), &rule.name, group));
            }
        }
        refs
    }
}

/// Resolve context captures to concrete values for this file/context.
///
/// Slot-level captures (e.g. `repo[$REPO](...)`) grab the whole value.
/// Pattern-level captures from segment/regex patterns (e.g. `repo($ORG/$REPO)`)
/// are passed in via `git_caps` and `file_caps` and merged in.
fn resolve_context_captures(
    captures: &[(String, ContextCaptureSource)],
    ctx: &ExtractContext,
    path: &str,
    git_caps: &HashMap<String, String>,
    file_caps: &HashMap<String, String>,
) -> Vec<(String, CapturedValue)> {
    let mut result: Vec<(String, CapturedValue)> = captures
        .iter()
        .filter_map(|(name, source)| {
            let text = match source {
                ContextCaptureSource::Repo => ctx.repo?.to_string(),
                ContextCaptureSource::Rev => {
                    // Prefer branch, fall back to first tag
                    ctx.branch.map(|b| b.to_string())
                        .or_else(|| ctx.tags.first().map(|t| t.to_string()))?
                }
                ContextCaptureSource::FileName => {
                    Path::new(path).file_stem()?.to_str()?.to_string()
                }
                ContextCaptureSource::FolderPath => {
                    Path::new(path).parent()?.to_str()?.to_string()
                }
            };
            Some((
                name.clone(),
                CapturedValue {
                    text,
                    span_start: 0,
                    span_end: 0,
                },
            ))
        })
        .collect();

    // Merge pattern-level captures from segment/regex patterns
    for (name, text) in git_caps.iter().chain(file_caps.iter()) {
        result.push((
            name.clone(),
            CapturedValue {
                text: text.clone(),
                span_start: 0,
                span_end: 0,
            },
        ));
    }

    result
}

/// Compile a Rule into a CompiledRule, partitioning context and structural steps.
fn compile_rule(r: &crate::types::Rule) -> Result<CompiledRule> {
    let mut repo_patterns: Vec<&str> = vec![];
    let mut rev_patterns: Vec<&str> = vec![];
    let mut file_patterns: Vec<&str> = vec![];
    let mut context_captures: Vec<(String, ContextCaptureSource)> = vec![];
    let mut structural_steps: Vec<SelectStep> = vec![];
    let mut seen_structural = false;

    for step in &r.select {
        if step.is_context_step() {
            if seen_structural {
                bail!(
                    "rule '{}': context step ({:?}) after structural step -- \
                     all context steps (repo/branch/tag/folder/file) must precede structural steps",
                    r.name,
                    step_kind_label(step),
                );
            }
            match step {
                SelectStep::Repo { pattern, capture } => {
                    repo_patterns.push(pattern);
                    if let Some(c) = capture {
                        context_captures.push((c.clone(), ContextCaptureSource::Repo));
                    }
                }
                SelectStep::Rev { pattern, capture } => {
                    rev_patterns.push(pattern);
                    if let Some(c) = capture {
                        context_captures.push((c.clone(), ContextCaptureSource::Rev));
                    }
                }
                SelectStep::Folder { pattern, capture } => {
                    // Folder pattern matches against directory portion of path.
                    // We prepend **/ if the pattern doesn't start with ** to allow
                    // matching at any depth, then append /** to match any files within.
                    let dir_glob = if pattern.contains('/') || pattern.starts_with("**") {
                        format!("{}/**", pattern)
                    } else {
                        format!("**/{}/**", pattern)
                    };
                    file_patterns.push(Box::leak(dir_glob.into_boxed_str()));
                    if let Some(c) = capture {
                        context_captures.push((c.clone(), ContextCaptureSource::FolderPath));
                    }
                }
                SelectStep::File { pattern, capture } => {
                    file_patterns.push(pattern);
                    if let Some(c) = capture {
                        context_captures.push((c.clone(), ContextCaptureSource::FileName));
                    }
                }
                _ => unreachable!(),
            }
        } else {
            seen_structural = true;
            structural_steps.push(step.clone());
        }
    }

    let git = CompiledGitSelector::from_patterns(&repo_patterns, &rev_patterns)?;
    let file = CompiledFileSelector::from_patterns(&file_patterns)?;
    let compiled_steps = walk::compile_steps(&structural_steps)?;

    Ok(CompiledRule {
        name: r.name.clone(),
        git,
        file,
        context_captures,
        steps: compiled_steps,
        ast: r.select_ast.clone(),
        value_pattern: r.value.clone(),
        create_matches: r.create_matches.clone(),
    })
}

fn step_kind_label(step: &SelectStep) -> &'static str {
    match step {
        SelectStep::Repo { .. } => "repo",
        SelectStep::Rev { .. } => "rev",
        SelectStep::Folder { .. } => "folder",
        SelectStep::File { .. } => "file",
        _ => "structural",
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
                "select": [
                    { "step": "file", "pattern": "**/package.json|**/package-lock.json" },
                    { "step": "key", "name": "dependencies" },
                    { "step": "key_match", "pattern": "*", "capture": "name" },
                    { "step": "key", "name": "version" },
                    { "step": "leaf", "capture": "version" }
                ],
                "create_matches": [
                    { "capture": "name", "kind": "dep_name" },
                    { "capture": "version", "kind": "dep_version", "parent": "name" }
                ]
            },
            {
                "name": "helm-images",
                "select": [
                    { "step": "file", "pattern": "**/values.yaml|**/values-*.yaml" },
                    { "step": "any" },
                    { "step": "key", "name": "image" },
                    { "step": "object", "entries": [
                        { "key": "repository", "value": [{ "step": "leaf", "capture": "repo" }] },
                        { "key": "tag", "value": [{ "step": "leaf", "capture": "tag" }] }
                    ] }
                ],
                "create_matches": [
                    { "capture": "repo", "kind": "image_repo" },
                    { "capture": "tag", "kind": "image_tag", "parent": "repo" }
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
        assert!(refs.iter().all(|r| r.node_path.is_some()));
        let version_ref = refs.iter().find(|r| r.value == "4.18.2").unwrap();
        assert_eq!(version_ref.node_path.as_deref(), Some("dependencies/express/version"));
    }

    #[test]
    fn ast_only_rules_are_compiled() {
        let json = r#"{
            "rules": [
                {
                    "name": "ast-rule",
                    "select": [],
                    "select_ast": { "pattern": "use $PATH" },
                    "create_matches": [{ "capture": "$PATH", "kind": "rs_use" }]
                }
            ]
        }"#;
        let ruleset: RuleSet = serde_json::from_str(json).unwrap();
        let ex = RuleExtractor::from_ruleset(&ruleset).unwrap();
        assert_eq!(ex.rules.len(), 1);
        assert!(ex.rules[0].ast.is_some());
    }

    #[test]
    fn context_step_after_structural_rejected() {
        let json = r#"{
            "rules": [{
                "name": "bad-order",
                "select": [
                    { "step": "key", "name": "foo" },
                    { "step": "file", "pattern": "*.json" }
                ],
                "create_matches": [{ "capture": "x", "kind": "y" }]
            }]
        }"#;
        let ruleset: RuleSet = serde_json::from_str(json).unwrap();
        let err = RuleExtractor::from_ruleset(&ruleset).unwrap_err();
        assert!(err.to_string().contains("context step"), "{}", err);
    }
}
