//! Compilation of SelectStep/KeyMatcher/ObjectEntry into pre-built compiled types.
//!
//! Ported from v1 `crates/rules/src/walk.rs`.
//! Tests that exercise `walk_inner` (leaf_pattern_captures, leaf_pattern_no_match,
//! key_segment_captures, scoped_npm_deps, walk_with_current_repo_constraint) require
//! DataNode and are deferred to task #6.

use super::_0_pattern::compile_pattern;
use super::_1_compiled::{
    CompiledKeyMatcher, CompiledObjectEntry, CompiledStep,
};

// ── v1 SelectStep/KeyMatcher/ObjectEntry re-defined locally ──────────────────
//
// v2 doesn't have a v1-equivalent `_0_types` with SelectStep yet. These local
// copies mirror the v1 shapes exactly so this module compiles standalone.
// TODO: revisit — replace with shared v2 AST types once they land.

/// How to match an object key in a destructuring pattern.
#[derive(Debug, Clone)]
pub enum KeyMatcher {
    Exact(String),
    Glob(String),
    Capture(String),
    Wildcard,
}

/// One branch in a destructuring Object pattern.
#[derive(Debug, Clone)]
pub struct ObjectEntry {
    pub key:   KeyMatcher,
    pub value: Vec<SelectStep>,
}

/// One step in a selector chain (structural steps only; context steps mapped to Any).
#[derive(Debug, Clone)]
pub enum SelectStep {
    Any,
    Key { name: String, capture: Option<String> },
    KeyMatch { pattern: String, capture: Option<String> },
    DepthMin { n: u32 },
    DepthMax { n: u32 },
    DepthEq  { n: u32 },
    ParentKey { pattern: String },
    ArrayItem,
    Leaf   { capture: Option<String> },
    LeafPattern { pattern: String },
    Object { entries: Vec<ObjectEntry> },
    Array  { item: Vec<SelectStep> },
    // Context steps: Repo / Rev / Folder / File
    Repo   { pattern: String, capture: Option<String> },
    Rev    { pattern: String, capture: Option<String> },
    Folder { pattern: String, capture: Option<String> },
    File   { pattern: String, capture: Option<String> },
}

// ── Compilation ───────────────────────────────────────────────────────────────

/// Compile a slice of SelectSteps into pre-built CompiledSteps.
pub fn compile_steps(steps: &[SelectStep]) -> anyhow::Result<Vec<CompiledStep>> {
    steps.iter().map(compile_one_step).collect()
}

pub fn compile_one_step(step: &SelectStep) -> anyhow::Result<CompiledStep> {
    Ok(match step {
        SelectStep::Any => CompiledStep::Any,
        SelectStep::Key { name, capture } => CompiledStep::Key {
            name:    name.clone(),
            capture: capture.clone(),
        },
        SelectStep::KeyMatch { pattern, capture } => CompiledStep::KeyMatch {
            matchers: compile_pattern(pattern)?,
            capture:  capture.clone(),
        },
        SelectStep::DepthMin { n } => CompiledStep::DepthMin { n: *n },
        SelectStep::DepthMax { n } => CompiledStep::DepthMax { n: *n },
        SelectStep::DepthEq  { n } => CompiledStep::DepthEq  { n: *n },
        SelectStep::ParentKey { pattern } => CompiledStep::ParentKey {
            matchers: compile_pattern(pattern)?,
        },
        SelectStep::ArrayItem => CompiledStep::ArrayItem,
        SelectStep::Leaf { capture } => CompiledStep::Leaf {
            capture: capture.clone(),
        },
        SelectStep::LeafPattern { pattern } => CompiledStep::LeafPattern {
            segments: super::_0_pattern::parse_segment_pattern(pattern),
        },
        SelectStep::Object { entries } => CompiledStep::Object {
            entries: entries
                .iter()
                .map(compile_object_entry)
                .collect::<anyhow::Result<_>>()?,
        },
        SelectStep::Array { item } => CompiledStep::Array {
            item: compile_steps(item)?,
        },
        // Context steps should never reach the structural compiler, but handle gracefully.
        SelectStep::Repo { .. }
        | SelectStep::Rev  { .. }
        | SelectStep::Folder { .. }
        | SelectStep::File { .. } => CompiledStep::Any,
    })
}

pub fn compile_object_entry(entry: &ObjectEntry) -> anyhow::Result<CompiledObjectEntry> {
    Ok(CompiledObjectEntry {
        key:   compile_key_matcher(&entry.key)?,
        value: compile_steps(&entry.value)?,
    })
}

pub fn compile_key_matcher(km: &KeyMatcher) -> anyhow::Result<CompiledKeyMatcher> {
    Ok(match km {
        KeyMatcher::Exact(s)      => CompiledKeyMatcher::Exact(s.clone()),
        KeyMatcher::Glob(pattern) => CompiledKeyMatcher::Glob(compile_pattern(pattern)?),
        KeyMatcher::Capture(name) => CompiledKeyMatcher::Capture(name.clone()),
        KeyMatcher::Wildcard      => CompiledKeyMatcher::Wildcard,
    })
}

/// Find all keys in a serde_json object that match a compiled key matcher.
/// Returns owned key strings paired with references to child values.
pub fn compiled_key_matches<'a>(
    matcher:  &CompiledKeyMatcher,
    map:      &'a serde_json::Map<String, serde_json::Value>,
) -> Vec<(String, &'a serde_json::Value)> {
    match matcher {
        CompiledKeyMatcher::Exact(name) => map
            .get(name)
            .map(|v| vec![(name.clone(), v)])
            .unwrap_or_default(),
        CompiledKeyMatcher::Glob(matchers) => map
            .iter()
            .filter(|(k, _)| matchers.iter().any(|m| m.is_match(k)))
            .map(|(k, v)| (k.clone(), v))
            .collect(),
        CompiledKeyMatcher::Capture(_) | CompiledKeyMatcher::Wildcard => {
            map.iter().map(|(k, v)| (k.clone(), v)).collect()
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Tests that exercise walk_inner (leaf_pattern_captures, leaf_pattern_no_match,
// key_segment_captures, scoped_npm_deps, walk_with_current_repo_constraint)
// are deferred to task #6, which ports walk_inner with DataNode semantics.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_key_exact() {
        let km = KeyMatcher::Exact("dependencies".to_string());
        let compiled = compile_key_matcher(&km).unwrap();
        assert!(matches!(compiled, CompiledKeyMatcher::Exact(ref s) if s == "dependencies"));
    }

    #[test]
    fn compile_key_glob() {
        let km = KeyMatcher::Glob("@$SCOPE/$NAME".to_string());
        let compiled = compile_key_matcher(&km).unwrap();
        assert!(matches!(compiled, CompiledKeyMatcher::Glob(_)));
    }

    #[test]
    fn compile_key_capture() {
        let km = KeyMatcher::Capture("KEY".to_string());
        let compiled = compile_key_matcher(&km).unwrap();
        assert!(matches!(compiled, CompiledKeyMatcher::Capture(ref s) if s == "KEY"));
    }

    #[test]
    fn compile_key_wildcard() {
        let km = KeyMatcher::Wildcard;
        let compiled = compile_key_matcher(&km).unwrap();
        assert!(matches!(compiled, CompiledKeyMatcher::Wildcard));
    }

    #[test]
    fn compile_steps_leaf_pattern() {
        let steps = vec![SelectStep::LeafPattern {
            pattern: "$REPO:$TAG".to_string(),
        }];
        let compiled = compile_steps(&steps).unwrap();
        assert_eq!(compiled.len(), 1);
        assert!(matches!(&compiled[0], CompiledStep::LeafPattern { segments } if !segments.is_empty()));
    }

    #[test]
    fn compile_steps_context_becomes_any() {
        let steps = vec![SelectStep::Repo {
            pattern: "myrepo".to_string(),
            capture: None,
        }];
        let compiled = compile_steps(&steps).unwrap();
        assert!(matches!(compiled[0], CompiledStep::Any));
    }

    #[test]
    fn compiled_key_matches_exact() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"foo": 1, "bar": 2}"#,
        )
        .unwrap();
        let matcher = CompiledKeyMatcher::Exact("foo".to_string());
        let hits = compiled_key_matches(&matcher, &map);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "foo");
    }

    #[test]
    fn compiled_key_matches_wildcard_all() {
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"a": 1, "b": 2}"#,
        )
        .unwrap();
        let matcher = CompiledKeyMatcher::Wildcard;
        let hits = compiled_key_matches(&matcher, &map);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn compiled_key_matches_glob_pattern() {
        use super::super::_0_pattern::compile_pattern;
        let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"@angular/core": "1", "@angular/forms": "2", "lodash": "3"}"#,
        )
        .unwrap();
        let matchers = compile_pattern("@$SCOPE/$NAME").unwrap();
        let matcher = CompiledKeyMatcher::Glob(matchers);
        let hits = compiled_key_matches(&matcher, &map);
        assert_eq!(hits.len(), 2);
        let keys: Vec<&str> = hits.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"@angular/core"));
        assert!(keys.contains(&"@angular/forms"));
    }
}
