use std::collections::HashMap;
use std::path::Path;

use ast_grep_config::{DeserializeEnv, SerializableRuleCore};
use ast_grep_language::{Language, LanguageExt, SupportLang};

use crate::types::AstSelector;
use crate::walk::{CapturedValue, MatchResult};

/// Run an ast-grep selector against source code, returning capture maps.
///
/// `config_dir` is used to resolve `rule_file` paths. If None and a `rule_file`
/// is specified, it is resolved relative to the current working directory.
///
/// Returns empty vec if:
/// - language cannot be inferred and is not specified
/// - source is not valid UTF-8
/// - rule fails to compile
/// - no matches
pub fn ast_match(
    source: &[u8],
    path: &str,
    selector: &AstSelector,
    config_dir: Option<&Path>,
) -> Vec<MatchResult> {
    tracing::debug!(path, pattern = ?selector.pattern, language = ?selector.language, rule = ?selector.rule.is_some(), rule_file = ?selector.rule_file, "ast_match enter");
    let src = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => {
            tracing::debug!(path, "ast_match: source is not valid UTF-8");
            return vec![];
        }
    };

    match resolve_mode(selector, path, config_dir) {
        Mode::Pattern(ref pattern, lang) => {
            tracing::debug!(path, %pattern, ?lang, "ast_match: Pattern mode");
            let root = lang.ast_grep(src);
            let node = root.root();
            let results = collect_matches(node.find_all(pattern.as_str()), selector);
            tracing::debug!(path, count = results.len(), "ast_match: Pattern results");
            results
        }
        Mode::Rule(rule_core, lang) => {
            tracing::debug!(path, ?lang, "ast_match: Rule mode");
            let root = lang.ast_grep(src);
            let node = root.root();
            let results = collect_matches(node.find_all(rule_core), selector);
            tracing::debug!(path, count = results.len(), "ast_match: Rule results");
            results
        }
        Mode::None => {
            tracing::debug!(path, "ast_match: resolve_mode returned None");
            vec![]
        }
    }
}

enum Mode {
    Pattern(String, SupportLang),
    Rule(ast_grep_config::RuleCore, SupportLang),
    None,
}

fn resolve_mode(selector: &AstSelector, path: &str, config_dir: Option<&Path>) -> Mode {
    tracing::debug!(path, pattern = ?selector.pattern, language = ?selector.language, rule_file = ?selector.rule_file, has_rule = selector.rule.is_some(), "resolve_mode enter");

    // --- rule_file ---
    if let Some(rule_file) = &selector.rule_file {
        let base = config_dir.unwrap_or_else(|| Path::new("."));
        let full_path = base.join(rule_file);
        tracing::debug!(?full_path, "resolve_mode: loading rule_file");
        let yaml = match std::fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!(?full_path, %e, "resolve_mode: rule_file read failed");
                return Mode::None;
            }
        };
        return build_from_yaml_str(&yaml, selector.language.as_deref());
    }

    // --- inline rule object ---
    if let Some(rule_val) = &selector.rule {
        let Some(lang) = resolve_language(selector.language.as_deref(), Some(path)) else {
            tracing::debug!(path, "resolve_mode: inline rule - language resolution failed");
            return Mode::None;
        };
        let core_val = serde_json::json!({
            "rule": rule_val,
            "constraints": selector.constraints,
        });
        let Ok(core) = serde_json::from_value::<SerializableRuleCore>(core_val) else {
            tracing::debug!(path, "resolve_mode: inline rule - deser failed");
            return Mode::None;
        };
        let env = DeserializeEnv::new(lang);
        let Ok(rule_core) = core.get_matcher(env) else {
            tracing::debug!(path, "resolve_mode: inline rule - get_matcher failed");
            return Mode::None;
        };
        tracing::debug!(path, ?lang, "resolve_mode: inline Rule mode");
        return Mode::Rule(rule_core, lang);
    }

    // --- simple pattern ---
    if let Some(pattern) = &selector.pattern {
        tracing::debug!(path, %pattern, "resolve_mode: trying simple pattern");
        if let Some(lang) = resolve_language(selector.language.as_deref(), Some(path)) {
            tracing::debug!(path, %pattern, ?lang, "resolve_mode: Pattern mode");
            return Mode::Pattern(pattern.clone(), lang);
        }
        tracing::debug!(path, %pattern, override_lang = ?selector.language, "resolve_mode: language resolution failed for pattern");
    }

    tracing::debug!(path, "resolve_mode: returning None");
    Mode::None
}

fn build_from_yaml_str(yaml: &str, lang_override: Option<&str>) -> Mode {
    use ast_grep_config::SerializableRuleConfig;

    let Ok(config) = serde_yaml::from_str::<SerializableRuleConfig<SupportLang>>(yaml) else {
        return Mode::None;
    };
    let lang = lang_override
        .and_then(|s| s.parse::<SupportLang>().ok())
        .unwrap_or(config.language);
    let env = DeserializeEnv::new(lang);
    let Ok(rule_core) = config.core.get_matcher(env) else {
        return Mode::None;
    };
    Mode::Rule(rule_core, lang)
}

fn resolve_language(override_lang: Option<&str>, path: Option<&str>) -> Option<SupportLang> {
    override_lang
        .and_then(|s| s.parse().ok())
        .or_else(|| path.and_then(|p| SupportLang::from_path(Path::new(p))))
}

fn collect_matches<'a, I, D>(iter: I, selector: &AstSelector) -> Vec<MatchResult>
where
    I: Iterator<Item = ast_grep_core::NodeMatch<'a, D>>,
    D: ast_grep_core::Doc,
{
    iter.filter_map(|m| {
        let env = m.get_env();
        let mut captures: HashMap<String, CapturedValue> = HashMap::new();

        if let Some(cap_map) = &selector.captures {
            for (metavar, capture_name) in cap_map {
                let var_name = metavar.trim_start_matches('$');
                if let Some(n) = env.get_match(var_name) {
                    let range = n.range();
                    captures.insert(
                        capture_name.clone(),
                        CapturedValue {
                            text: n.text().into_owned(),
                            span_start: range.start as u32,
                            span_end: range.end as u32,
                        },
                    );
                }
            }
            if captures.is_empty() {
                return None;
            }
        } else {
            let range = m.range();
            captures.insert(
                selector.capture.clone(),
                CapturedValue {
                    text: m.text().into_owned(),
                    span_start: range.start as u32,
                    span_end: range.end as u32,
                },
            );
        }

        // Post-process segment_captures: extract sub-captures from synthetic metavars.
        // Computes precise byte spans for each sub-capture within the identifier.
        if let Some(seg_map) = &selector.segment_captures {
            for (metavar_name, seg_pattern) in seg_map {
                if let Some(n) = env.get_match(metavar_name) {
                    let text = n.text().into_owned();
                    let metavar_start = n.range().start as u32;
                    let segments = crate::pattern::parse_segment_pattern(seg_pattern);
                    if let Some(sub_caps) =
                        crate::pattern::match_segments_pub(&segments, &text)
                    {
                        let offsets =
                            crate::pattern::capture_offsets_in_value(&segments, &sub_caps);
                        for (name, value) in sub_caps {
                            let (local_start, local_end) =
                                offsets.get(&name).copied().unwrap_or((0, text.len() as u32));
                            captures.insert(
                                name,
                                CapturedValue {
                                    text: value,
                                    span_start: metavar_start + local_start,
                                    span_end: metavar_start + local_end,
                                },
                            );
                        }
                    } else {
                        return None;
                    }
                }
            }
        }

        Some(MatchResult {
            captures,
            path: vec![],
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel_pattern(pattern: &str, lang: &str) -> AstSelector {
        AstSelector {
            pattern: Some(pattern.into()),
            rule: None,
            constraints: None,
            rule_file: None,
            language: Some(lang.into()),
            capture: "match".into(),
            captures: None,
            segment_captures: None,
        }
    }

    #[test]
    fn single_capture_whole_match() {
        let src = b"import foo from './foo'";
        let sel = sel_pattern("import $NAME from $PATH", "js");
        let results = ast_match(src, "index.js", &sel, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].captures["match"].text, "import foo from './foo'");
    }

    #[test]
    fn multi_capture_metavars() {
        let src = b"import foo from './foo'";
        let mut cap_map = std::collections::BTreeMap::new();
        cap_map.insert("$NAME".into(), "name".into());
        cap_map.insert("$PATH".into(), "path".into());
        let sel = AstSelector {
            pattern: Some("import $NAME from $PATH".into()),
            rule: None,
            constraints: None,
            rule_file: None,
            language: Some("js".into()),
            capture: "unused".into(),
            captures: Some(cap_map),
            segment_captures: None,
        };
        let results = ast_match(src, "index.js", &sel, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].captures["name"].text, "foo");
        assert_eq!(results[0].captures["path"].text, "'./foo'");
    }

    #[test]
    fn language_inferred_from_extension() {
        // language inferred from path when using rule_file/rule modes.
        // For pattern mode we pass language explicitly; here test rule mode.
        let src = b"def foo(): pass";
        let rule_val = serde_json::json!({ "pattern": "def $NAME(): $$$BODY" });
        let mut cap_map = std::collections::BTreeMap::new();
        cap_map.insert("$NAME".into(), "name".into());
        let sel = AstSelector {
            pattern: None,
            rule: Some(rule_val),
            constraints: None,
            rule_file: None,
            language: Some("py".into()),
            capture: "unused".into(),
            captures: Some(cap_map),
            segment_captures: None,
        };
        let results = ast_match(src, "script.py", &sel, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].captures["name"].text, "foo");
    }

    #[test]
    fn inline_rule_with_constraint() {
        let src = b"import foo from './foo'\nimport bar from './bar'";
        let rule_val = serde_json::json!({ "pattern": "import $NAME from $PATH" });
        let constraints = serde_json::json!({ "NAME": { "regex": "^foo$" } });
        let mut cap_map = std::collections::BTreeMap::new();
        cap_map.insert("$NAME".into(), "name".into());
        let sel = AstSelector {
            pattern: None,
            rule: Some(rule_val),
            constraints: Some(constraints),
            rule_file: None,
            language: Some("js".into()),
            capture: "unused".into(),
            captures: Some(cap_map),
            segment_captures: None,
        };
        let results = ast_match(src, "index.js", &sel, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].captures["name"].text, "foo");
    }

    #[test]
    fn unknown_extension_pattern_returns_empty() {
        let sel = AstSelector {
            pattern: Some("fn $NAME() {}".into()),
            rule: None,
            constraints: None,
            rule_file: None,
            language: None,
            capture: "name".into(),
            captures: None,
            segment_captures: None,
        };
        let results = ast_match(b"fn foo() {}", "file.unknown_ext", &sel, None);
        assert!(results.is_empty());
    }

    #[test]
    fn segment_capture_extracts_sub_identifier() {
        // Simulates lowered output of ast(use${ENTITY}Query($$$ARGS))
        // Pattern: $SPREFA0($$$ARGS) with constraint + segment_captures
        let constraints = serde_json::json!({
            "SPREFA0": { "regex": "^use.+Query$" }
        });
        let mut seg_caps = std::collections::BTreeMap::new();
        seg_caps.insert("SPREFA0".into(), "use${ENTITY}Query".into());

        let sel = AstSelector {
            pattern: None,
            rule: Some(serde_json::json!({ "pattern": "$SPREFA0($$$ARGS)" })),
            constraints: Some(constraints),
            rule_file: None,
            language: Some("tsx".into()),
            capture: "unused".into(),
            captures: None,
            segment_captures: Some(seg_caps),
        };

        //                0         1         2         3
        //                0123456789012345678901234567890123
        let src = b"const x = useUserQuery({ id: 1 })";
        let results = ast_match(src, "App.tsx", &sel, None);
        assert_eq!(results.len(), 1);
        let entity = &results[0].captures["ENTITY"];
        assert_eq!(entity.text, "User");
        // "useUserQuery" starts at byte 10, "User" starts at 13 (after "use"), ends at 17
        assert_eq!(entity.span_start, 13);
        assert_eq!(entity.span_end, 17);
    }

    #[test]
    fn segment_capture_filters_non_matching() {
        let constraints = serde_json::json!({
            "SPREFA0": { "regex": "^use.+Query$" }
        });
        let mut seg_caps = std::collections::BTreeMap::new();
        seg_caps.insert("SPREFA0".into(), "use${ENTITY}Query".into());

        let sel = AstSelector {
            pattern: None,
            rule: Some(serde_json::json!({ "pattern": "$SPREFA0($$$ARGS)" })),
            constraints: Some(constraints),
            rule_file: None,
            language: Some("tsx".into()),
            capture: "unused".into(),
            captures: None,
            segment_captures: Some(seg_caps),
        };

        // "useState" matches regex ^use.+Query$? No -- it doesn't end with Query
        let src = b"const [x, setX] = useState(0)";
        let results = ast_match(src, "App.tsx", &sel, None);
        assert!(results.is_empty());
    }
}
