use std::collections::HashMap;

use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde_json::Value;

use crate::types::SelectStep;

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

/// Walk a parsed JSON/YAML/TOML value tree, applying a structural selector chain.
/// Returns all matches (querySelectorAll semantics).
///
/// Byte spans are not tracked by serde_json::Value, so span_start/span_end
/// in CapturedValue are set to 0. The caller resolves spans by searching
/// the raw source bytes for captured strings.
pub fn walk(node: &Value, steps: &[SelectStep]) -> Vec<MatchResult> {
    let state = WalkState {
        depth: 0,
        parent_key: None,
        path: vec![],
        captures: HashMap::new(),
    };
    walk_inner(node, steps, &state)
}

fn walk_inner(node: &Value, steps: &[SelectStep], state: &WalkState) -> Vec<MatchResult> {
    if steps.is_empty() {
        return vec![state.clone().into_result()];
    }

    // Note: state is &WalkState. We clone when we need to mutate (captures)
    // or pass ownership (into_result). The descend/with_capture methods
    // on WalkState already return owned copies.

    let step = &steps[0];
    let rest = &steps[1..];

    match step {
        SelectStep::Any => {
            let mut results = vec![];
            // Fork 1: stop consuming Any, advance to next step at this node
            results.extend(walk_inner(node, rest, state));
            // Fork 2: consume this level, keep Any active on children
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

        SelectStep::Key { name, capture } => match node {
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

        SelectStep::KeyMatch { pattern, capture } => match node {
            Value::Object(map) => {
                let mut results = vec![];
                for (k, v) in map {
                    if pipe_glob_matches(pattern, k) {
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

        SelectStep::DepthMin { n } => {
            if state.depth >= *n {
                walk_inner(node, rest, state)
            } else {
                vec![]
            }
        }

        SelectStep::DepthMax { n } => {
            if state.depth <= *n {
                walk_inner(node, rest, state)
            } else {
                vec![]
            }
        }

        SelectStep::DepthEq { n } => {
            if state.depth == *n {
                walk_inner(node, rest, state)
            } else {
                vec![]
            }
        }

        SelectStep::ParentKey { pattern } => match &state.parent_key {
            Some(pk) if pipe_glob_matches(pattern, pk) => walk_inner(node, rest, state),
            _ => vec![],
        },

        SelectStep::ArrayItem => match node {
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

        SelectStep::Leaf { capture } => match node {
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

        SelectStep::Object { captures } => match node {
            Value::Object(map) => {
                let mut next_state = state.clone();
                for (json_key, cap_name) in captures {
                    match map.get(json_key.as_str()) {
                        Some(Value::String(s)) => {
                            next_state.captures.insert(
                                cap_name.clone(),
                                CapturedValue {
                                    text: s.clone(),
                                    span_start: 0,
                                    span_end: 0,
                                },
                            );
                        }
                        Some(Value::Number(n)) => {
                            next_state.captures.insert(
                                cap_name.clone(),
                                CapturedValue {
                                    text: n.to_string(),
                                    span_start: 0,
                                    span_end: 0,
                                },
                            );
                        }
                        Some(Value::Bool(b)) => {
                            next_state.captures.insert(
                                cap_name.clone(),
                                CapturedValue {
                                    text: b.to_string(),
                                    span_start: 0,
                                    span_end: 0,
                                },
                            );
                        }
                        _ => {
                            // Key missing or not a leaf value -- skip this capture.
                            // The match still proceeds; missing captures just won't
                            // produce refs in the emit phase.
                        }
                    }
                }
                walk_inner(node, rest, &next_state)
            }
            _ => vec![],
        },

        // Context steps (Repo/Branch/Tag/Folder/File) should never reach the
        // walk engine -- they are partitioned out during compilation.
        // If one slips through, skip it and advance to the next step.
        SelectStep::Repo { .. }
        | SelectStep::Branch { .. }
        | SelectStep::Tag { .. }
        | SelectStep::Folder { .. }
        | SelectStep::File { .. } => walk_inner(node, rest, state),
    }
}

/// Match a string against a pipe-delimited pattern.
/// Patterns prefixed with `re:` are treated as regex, otherwise as glob.
/// `"*image*|*repository*"` matches if either glob matches.
/// `"re:^dep(endencies|.*)"` matches via regex.
fn pipe_glob_matches(pattern: &str, value: &str) -> bool {
    if let Some(re_pattern) = pattern.strip_prefix("re:") {
        Regex::new(re_pattern)
            .ok()
            .map(|re| re.is_match(value))
            .unwrap_or(false)
    } else {
        pattern.split('|').any(|p| {
            Glob::new(p.trim())
                .ok()
                .map(|g| g.compile_matcher().is_match(value))
                .unwrap_or(false)
        })
    }
}

/// Compiled pattern matcher: glob (pipe-delimited) or regex (re: prefix).
pub enum CompiledPipeGlob {
    Globs(Vec<GlobMatcher>),
    Regex(Regex),
}

impl CompiledPipeGlob {
    pub fn compile(pattern: &str) -> anyhow::Result<Self> {
        if let Some(re_pattern) = pattern.strip_prefix("re:") {
            let re = Regex::new(re_pattern)?;
            Ok(Self::Regex(re))
        } else {
            let matchers = pattern
                .split('|')
                .map(|p| Glob::new(p.trim()).map(|g| g.compile_matcher()))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Self::Globs(matchers))
        }
    }

    pub fn is_match(&self, value: &str) -> bool {
        match self {
            Self::Globs(matchers) => matchers.iter().any(|m| m.is_match(value)),
            Self::Regex(re) => re.is_match(value),
        }
    }
}
