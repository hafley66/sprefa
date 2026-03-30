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
    let src = match std::str::from_utf8(source) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    match resolve_mode(selector, path, config_dir) {
        Mode::Pattern(pattern, lang) => {
            let root = lang.ast_grep(src);
            let node = root.root();
            collect_matches(node.find_all(pattern.as_str()), selector)
        }
        Mode::Rule(rule_core, lang) => {
            let root = lang.ast_grep(src);
            let node = root.root();
            collect_matches(node.find_all(rule_core), selector)
        }
        Mode::None => vec![],
    }
}

enum Mode {
    Pattern(String, SupportLang),
    Rule(ast_grep_config::RuleCore, SupportLang),
    None,
}

fn resolve_mode(selector: &AstSelector, path: &str, config_dir: Option<&Path>) -> Mode {
    // --- rule_file ---
    if let Some(rule_file) = &selector.rule_file {
        let base = config_dir.unwrap_or_else(|| Path::new("."));
        let full_path = base.join(rule_file);
        let yaml = match std::fs::read_to_string(&full_path) {
            Ok(s) => s,
            Err(_) => return Mode::None,
        };
        return build_from_yaml_str(&yaml, selector.language.as_deref());
    }

    // --- inline rule object ---
    if let Some(rule_val) = &selector.rule {
        let Some(lang) = resolve_language(selector.language.as_deref(), Some(path)) else {
            return Mode::None;
        };
        // Build a SerializableRuleCore from the inline objects.
        // Round-trip through serde_json::Value to avoid direct type coupling.
        let core_val = serde_json::json!({
            "rule": rule_val,
            "constraints": selector.constraints,
        });
        let Ok(core) = serde_json::from_value::<SerializableRuleCore>(core_val) else {
            return Mode::None;
        };
        let env = DeserializeEnv::new(lang);
        let Ok(rule_core) = core.get_matcher(env) else {
            return Mode::None;
        };
        return Mode::Rule(rule_core, lang);
    }

    // --- simple pattern ---
    if let Some(pattern) = &selector.pattern {
        if let Some(lang) = resolve_language(selector.language.as_deref(), Some(path)) {
            return Mode::Pattern(pattern.clone(), lang);
        }
    }

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

fn collect_matches<'a, I, D>(
    iter: I,
    selector: &AstSelector,
) -> Vec<MatchResult>
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
                    captures.insert(capture_name.clone(), CapturedValue {
                        text: n.text().into_owned(),
                        span_start: range.start as u32,
                        span_end: range.end as u32,
                    });
                }
            }
            if captures.is_empty() {
                return None;
            }
        } else {
            let range = m.range();
            captures.insert(selector.capture.clone(), CapturedValue {
                text: m.text().into_owned(),
                span_start: range.start as u32,
                span_end: range.end as u32,
            });
        }

        Some(MatchResult { captures, path: vec![] })
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
        };
        let results = ast_match(b"fn foo() {}", "file.unknown_ext", &sel, None);
        assert!(results.is_empty());
    }
}
