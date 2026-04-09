use std::collections::HashMap;

use serde_json::Value;

use crate::pattern::{compile_pattern, parse_segment_pattern, PatternMatcher, Segment};
use crate::types::{KeyMatcher, ObjectEntry, SelectStep};

/// A value captured during a walk, with its position in the source.
#[derive(Debug, Clone)]
pub struct CapturedValue {
    pub text: String,
    pub span_start: u32,
    pub span_end: u32,
}

/// Result of a successful walk: all named captures accumulated along the path.
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub captures: HashMap<String, CapturedValue>,
    pub path: Vec<String>,
}

/// Pre-compiled walk step. Built once during compile_rule, used per-file.
/// Mirrors SelectStep but holds compiled PatternMatchers instead of strings.
#[derive(Debug)]
pub enum CompiledStep {
    Any,
    Key {
        name: String,
        capture: Option<String>,
    },
    KeyMatch {
        matchers: Vec<PatternMatcher>,
        capture: Option<String>,
    },
    DepthMin {
        n: u32,
    },
    DepthMax {
        n: u32,
    },
    DepthEq {
        n: u32,
    },
    ParentKey {
        matchers: Vec<PatternMatcher>,
    },
    ArrayItem,
    Leaf {
        capture: Option<String>,
    },
    LeafPattern {
        segments: Vec<Segment>,
    },
    Object {
        entries: Vec<CompiledObjectEntry>,
    },
    Array {
        item: Vec<CompiledStep>,
    },
}

/// Pre-compiled object entry for Object step destructuring.
#[derive(Debug)]
pub struct CompiledObjectEntry {
    pub key: CompiledKeyMatcher,
    pub value: Vec<CompiledStep>,
}

/// Pre-compiled key matcher. Glob variant holds compiled matchers.
#[derive(Debug)]
pub enum CompiledKeyMatcher {
    Exact(String),
    Glob(Vec<PatternMatcher>),
    Capture(String),
    Wildcard,
}

/// Compile structural SelectSteps into CompiledSteps with pre-built pattern matchers.
pub fn compile_steps(steps: &[SelectStep]) -> anyhow::Result<Vec<CompiledStep>> {
    steps.iter().map(compile_one_step).collect()
}

fn compile_one_step(step: &SelectStep) -> anyhow::Result<CompiledStep> {
    Ok(match step {
        SelectStep::Any => CompiledStep::Any,
        SelectStep::Key { name, capture } => CompiledStep::Key {
            name: name.clone(),
            capture: capture.clone(),
        },
        SelectStep::KeyMatch { pattern, capture } => CompiledStep::KeyMatch {
            matchers: compile_pattern(pattern)?,
            capture: capture.clone(),
        },
        SelectStep::DepthMin { n } => CompiledStep::DepthMin { n: *n },
        SelectStep::DepthMax { n } => CompiledStep::DepthMax { n: *n },
        SelectStep::DepthEq { n } => CompiledStep::DepthEq { n: *n },
        SelectStep::ParentKey { pattern } => CompiledStep::ParentKey {
            matchers: compile_pattern(pattern)?,
        },
        SelectStep::ArrayItem => CompiledStep::ArrayItem,
        SelectStep::Leaf { capture } => CompiledStep::Leaf {
            capture: capture.clone(),
        },
        SelectStep::LeafPattern { pattern } => CompiledStep::LeafPattern {
            segments: parse_segment_pattern(pattern),
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
        // Context steps should never reach here, but handle gracefully
        SelectStep::Repo { .. }
        | SelectStep::Rev { .. }
        | SelectStep::Folder { .. }
        | SelectStep::File { .. } => CompiledStep::Any, // skip
    })
}

fn compile_object_entry(entry: &ObjectEntry) -> anyhow::Result<CompiledObjectEntry> {
    Ok(CompiledObjectEntry {
        key: compile_key_matcher(&entry.key)?,
        value: compile_steps(&entry.value)?,
    })
}

fn compile_key_matcher(km: &KeyMatcher) -> anyhow::Result<CompiledKeyMatcher> {
    Ok(match km {
        KeyMatcher::Exact(s) => CompiledKeyMatcher::Exact(s.clone()),
        KeyMatcher::Glob(pattern) => CompiledKeyMatcher::Glob(compile_pattern(pattern)?),
        KeyMatcher::Capture(name) => CompiledKeyMatcher::Capture(name.clone()),
        KeyMatcher::Wildcard => CompiledKeyMatcher::Wildcard,
    })
}

// ── Walk engine ──────────────────────────────────

/// Walk state threaded through recursion.
#[derive(Clone)]
struct WalkState {
    depth: u32,
    parent_key: Option<String>,
    path: Vec<String>,
    captures: HashMap<String, CapturedValue>,
}

impl WalkState {
    fn descend(&self, key: Option<&str>) -> Self {
        let mut path = self.path.clone();
        if let Some(k) = key {
            path.push(k.to_string());
        }
        WalkState {
            depth: self.depth + 1,
            parent_key: key.map(String::from),
            path,
            captures: self.captures.clone(),
        }
    }

    fn into_result(self) -> MatchResult {
        MatchResult {
            captures: self.captures,
            path: self.path,
        }
    }
}

/// Compile and walk in one shot. Convenience for tests and ad-hoc callers.
pub fn walk_select(node: &Value, steps: &[SelectStep]) -> Vec<MatchResult> {
    match compile_steps(steps) {
        Ok(compiled) => walk(node, &compiled),
        Err(_) => vec![],
    }
}

/// Walk a parsed JSON/YAML/TOML value tree using pre-compiled steps.
pub fn walk(node: &Value, steps: &[CompiledStep]) -> Vec<MatchResult> {
    let state = WalkState {
        depth: 0,
        parent_key: None,
        path: vec![],
        captures: HashMap::new(),
    };
    walk_inner(node, steps, &state)
}

/// Walk with pre-seeded captures (e.g. $current* constants).
/// Pre-seeded captures act as constraints during pattern matching:
/// if a pattern references $currentRepo, it must match the pre-seeded value.
pub fn walk_with_captures(
    node: &Value,
    steps: &[CompiledStep],
    seed_captures: HashMap<String, CapturedValue>,
) -> Vec<MatchResult> {
    let state = WalkState {
        depth: 0,
        parent_key: None,
        path: vec![],
        captures: seed_captures,
    };
    walk_inner(node, steps, &state)
}

fn walk_inner(node: &Value, steps: &[CompiledStep], state: &WalkState) -> Vec<MatchResult> {
    if steps.is_empty() {
        return vec![state.clone().into_result()];
    }

    let step = &steps[0];
    let rest = &steps[1..];

    match step {
        CompiledStep::Any => {
            let mut results = vec![];
            results.extend(walk_inner(node, rest, state));
            match node {
                Value::Object(map) => {
                    for (k, v) in map {
                        let child_state = state.descend(Some(k));
                        results.extend(walk_inner(v, steps, &child_state));
                    }
                }
                Value::Array(arr) => {
                    for (i, v) in arr.iter().enumerate() {
                        let child_state = state.descend(Some(&i.to_string()));
                        results.extend(walk_inner(v, steps, &child_state));
                    }
                }
                _ => {}
            }
            results
        }

        CompiledStep::Key { name, capture } => match node {
            Value::Object(map) => match map.get(name.as_str()) {
                Some(child) => {
                    let mut child_state = state.descend(Some(name));
                    if let Some(cap_name) = capture {
                        child_state.captures.insert(
                            cap_name.clone(),
                            CapturedValue {
                                text: name.clone(),
                                span_start: 0,
                                span_end: 0,
                            },
                        );
                    }
                    walk_inner(child, rest, &child_state)
                }
                None => vec![],
            },
            _ => vec![],
        },

        CompiledStep::KeyMatch { matchers, capture } => match node {
            Value::Object(map) => {
                let mut results = vec![];
                for (k, v) in map {
                    if matchers.iter().any(|m| m.is_match(k)) {
                        let mut child_state = state.descend(Some(k));
                        if let Some(cap_name) = capture {
                            child_state.captures.insert(
                                cap_name.clone(),
                                CapturedValue {
                                    text: k.clone(),
                                    span_start: 0,
                                    span_end: 0,
                                },
                            );
                        }
                        results.extend(walk_inner(v, rest, &child_state));
                    }
                }
                results
            }
            _ => vec![],
        },

        CompiledStep::DepthMin { n } => {
            if state.depth >= *n {
                walk_inner(node, rest, state)
            } else {
                vec![]
            }
        }

        CompiledStep::DepthMax { n } => {
            if state.depth <= *n {
                walk_inner(node, rest, state)
            } else {
                vec![]
            }
        }

        CompiledStep::DepthEq { n } => {
            if state.depth == *n {
                walk_inner(node, rest, state)
            } else {
                vec![]
            }
        }

        CompiledStep::ParentKey { matchers } => match &state.parent_key {
            Some(pk) if matchers.iter().any(|m| m.is_match(pk)) => walk_inner(node, rest, state),
            _ => vec![],
        },

        CompiledStep::ArrayItem => match node {
            Value::Array(arr) => {
                let mut results = vec![];
                for (i, v) in arr.iter().enumerate() {
                    let child_state = state.descend(Some(&i.to_string()));
                    results.extend(walk_inner(v, rest, &child_state));
                }
                results
            }
            _ => vec![],
        },

        CompiledStep::Leaf { capture } => match node {
            Value::String(s) => {
                let mut next_state = state.clone();
                if let Some(cap_name) = capture {
                    next_state.captures.insert(
                        cap_name.clone(),
                        CapturedValue {
                            text: s.clone(),
                            span_start: 0,
                            span_end: 0,
                        },
                    );
                }
                walk_inner(node, rest, &next_state)
            }
            Value::Number(n) => {
                let mut next_state = state.clone();
                if let Some(cap_name) = capture {
                    next_state.captures.insert(
                        cap_name.clone(),
                        CapturedValue {
                            text: n.to_string(),
                            span_start: 0,
                            span_end: 0,
                        },
                    );
                }
                walk_inner(node, rest, &next_state)
            }
            Value::Bool(b) => {
                let mut next_state = state.clone();
                if let Some(cap_name) = capture {
                    next_state.captures.insert(
                        cap_name.clone(),
                        CapturedValue {
                            text: b.to_string(),
                            span_start: 0,
                            span_end: 0,
                        },
                    );
                }
                walk_inner(node, rest, &next_state)
            }
            _ => vec![],
        },

        CompiledStep::LeafPattern { segments } => {
            let text = match node {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => return vec![],
            };
            // Pass pre-seeded captures so $current* names act as constraints.
            let pre_bound: HashMap<String, String> = state
                .captures
                .iter()
                .map(|(k, cv)| (k.clone(), cv.text.clone()))
                .collect();
            match crate::pattern::match_segments_with_bindings(segments, &text, pre_bound) {
                Some(caps) => {
                    let mut next_state = state.clone();
                    for (name, value) in caps {
                        next_state.captures.insert(
                            name,
                            CapturedValue {
                                text: value,
                                span_start: 0,
                                span_end: 0,
                            },
                        );
                    }
                    walk_inner(node, rest, &next_state)
                }
                None => vec![],
            }
        }

        CompiledStep::Object { entries } => match node {
            Value::Object(map) => {
                let mut product: Vec<HashMap<String, CapturedValue>> = vec![state.captures.clone()];

                for entry in entries {
                    let matching_keys = compiled_key_matches(&entry.key, map);
                    let mut next_product = vec![];

                    for caps_so_far in &product {
                        for (key_name, child_value) in &matching_keys {
                            let mut child_state = state.descend(Some(key_name));
                            child_state.captures = caps_so_far.clone();

                            match &entry.key {
                                CompiledKeyMatcher::Capture(cap) => {
                                    child_state.captures.insert(
                                        cap.clone(),
                                        CapturedValue {
                                            text: key_name.clone(),
                                            span_start: 0,
                                            span_end: 0,
                                        },
                                    );
                                }
                                CompiledKeyMatcher::Glob(matchers) => {
                                    // Extract segment captures from pattern keys
                                    for m in matchers {
                                        if let Some(caps) = m.captures(key_name) {
                                            for (name, value) in caps {
                                                child_state.captures.insert(
                                                    name,
                                                    CapturedValue {
                                                        text: value,
                                                        span_start: 0,
                                                        span_end: 0,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }

                            let sub_results = walk_inner(child_value, &entry.value, &child_state);
                            for r in sub_results {
                                next_product.push(r.captures);
                            }
                        }
                    }

                    if next_product.is_empty() {
                        return vec![];
                    }
                    product = next_product;
                }

                let mut results = vec![];
                for merged in product {
                    let mut next_state = state.clone();
                    next_state.captures = merged;
                    results.extend(walk_inner(node, rest, &next_state));
                }
                results
            }
            _ => vec![],
        },

        CompiledStep::Array { item } => match node {
            Value::Array(arr) => {
                let mut results = vec![];
                for (i, v) in arr.iter().enumerate() {
                    let child_state = state.descend(Some(&i.to_string()));
                    let sub_results = walk_inner(v, item, &child_state);
                    for r in sub_results {
                        let mut next_state = state.clone();
                        next_state.captures = r.captures;
                        results.extend(walk_inner(node, rest, &next_state));
                    }
                }
                results
            }
            _ => vec![],
        },
    }
}

/// Find all keys in a JSON object that match a compiled key matcher.
fn compiled_key_matches<'a>(
    matcher: &CompiledKeyMatcher,
    map: &'a serde_json::Map<String, Value>,
) -> Vec<(String, &'a Value)> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn leaf_pattern_captures() {
        let node = json!({ "image": "nginx:1.25" });
        let steps = vec![
            SelectStep::Object {
                entries: vec![ObjectEntry {
                    key: KeyMatcher::Exact("image".into()),
                    value: vec![SelectStep::LeafPattern {
                        pattern: "$REPO:$TAG".into(),
                    }],
                }],
            },
        ];
        let results = walk_select(&node, &steps);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].captures["REPO"].text, "nginx");
        assert_eq!(results[0].captures["TAG"].text, "1.25");
    }

    #[test]
    fn leaf_pattern_no_match() {
        let node = json!({ "image": "nocolon" });
        let steps = vec![
            SelectStep::Object {
                entries: vec![ObjectEntry {
                    key: KeyMatcher::Exact("image".into()),
                    value: vec![SelectStep::LeafPattern {
                        pattern: "$REPO:$TAG".into(),
                    }],
                }],
            },
        ];
        let results = walk_select(&node, &steps);
        assert!(results.is_empty());
    }

    #[test]
    fn key_segment_captures() {
        let node = json!({ "@myorg/mylib": "^1.0.0" });
        let steps = vec![
            SelectStep::Object {
                entries: vec![ObjectEntry {
                    key: KeyMatcher::Glob("@$SCOPE/$NAME".into()),
                    value: vec![SelectStep::Leaf {
                        capture: Some("VERSION".into()),
                    }],
                }],
            },
        ];
        let results = walk_select(&node, &steps);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].captures["SCOPE"].text, "myorg");
        assert_eq!(results[0].captures["NAME"].text, "mylib");
        assert_eq!(results[0].captures["VERSION"].text, "^1.0.0");
    }

    #[test]
    fn scoped_npm_deps() {
        let node = json!({
            "dependencies": {
                "@angular/core": "^17.0.0",
                "@angular/forms": "^17.0.0",
                "lodash": "^4.17.0"
            }
        });
        let steps = vec![
            SelectStep::Object {
                entries: vec![ObjectEntry {
                    key: KeyMatcher::Exact("dependencies".into()),
                    value: vec![SelectStep::Object {
                        entries: vec![ObjectEntry {
                            key: KeyMatcher::Glob("@$SCOPE/$NAME".into()),
                            value: vec![SelectStep::Leaf {
                                capture: Some("VERSION".into()),
                            }],
                        }],
                    }],
                }],
            },
        ];
        let results = walk_select(&node, &steps);
        assert_eq!(results.len(), 2);
        let scopes: Vec<&str> = results.iter().map(|r| r.captures["SCOPE"].text.as_str()).collect();
        assert!(scopes.contains(&"angular"));
        let names: Vec<&str> = results.iter().map(|r| r.captures["NAME"].text.as_str()).collect();
        assert!(names.contains(&"core"));
        assert!(names.contains(&"forms"));
    }

    #[test]
    fn walk_with_current_repo_constraint() {
        let data = json!({ "name": "myrepo" });
        let steps = vec![SelectStep::Object {
            entries: vec![ObjectEntry {
                key: KeyMatcher::Exact("name".into()),
                value: vec![SelectStep::LeafPattern {
                    pattern: "$currentRepo".into(),
                }],
            }],
        }];
        let compiled = compile_steps(&steps).unwrap();

        // Seed currentRepo = "myrepo" -> should match
        let mut seed = HashMap::new();
        seed.insert("currentRepo".to_string(), CapturedValue {
            text: "myrepo".to_string(),
            span_start: 0,
            span_end: 0,
        });
        let results = walk_with_captures(&data, &compiled, seed);
        assert_eq!(results.len(), 1);

        // Seed currentRepo = "other" -> should NOT match
        let mut seed2 = HashMap::new();
        seed2.insert("currentRepo".to_string(), CapturedValue {
            text: "other".to_string(),
            span_start: 0,
            span_end: 0,
        });
        let results2 = walk_with_captures(&data, &compiled, seed2);
        assert_eq!(results2.len(), 0);
    }
}
