//! Compiled walk step types and WalkState.
//!
//! Ported from v1 `crates/rules/src/walk.rs`.
//! `WalkCapture` replaces v1's `CapturedValue`; named differently to avoid
//! collision with `Capture` already exported from `_0_types` in this crate.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::_16_pattern::{PatternMatcher, Segment};

/// A value captured during a walk, with its byte position in the source.
///
/// `path` is a jq-style accessor string; left empty here and filled by the
/// walker in task #6 when DataNode byte ranges are available.
#[derive(Debug, Clone)]
pub struct WalkCapture {
    pub text:       Arc<str>,
    pub path:       Arc<str>,  // jq-style, empty string for now (filled by walker in task #6)
    pub byte_start: u32,
    pub byte_end:   u32,
}

/// Pre-compiled walk step. Built once per rule, used per file.
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
    DepthMin { n: u32 },
    DepthMax { n: u32 },
    DepthEq  { n: u32 },
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
    pub key:   CompiledKeyMatcher,
    pub value: Vec<CompiledStep>,
}

/// Pre-compiled key matcher.
#[derive(Debug)]
pub enum CompiledKeyMatcher {
    Exact(String),
    Glob(Vec<PatternMatcher>),
    Capture(String),
    Wildcard,
}

/// Walk state threaded through recursion.
#[derive(Clone)]
pub struct WalkState {
    pub depth:      u32,
    pub parent_key: Option<Arc<str>>,
    pub path:       Vec<Arc<str>>,
    pub captures:   FxHashMap<Arc<str>, WalkCapture>,
}

impl WalkState {
    pub fn new() -> Self {
        WalkState {
            depth:      0,
            parent_key: None,
            path:       vec![],
            captures:   FxHashMap::default(),
        }
    }

    pub fn with_captures(captures: FxHashMap<Arc<str>, WalkCapture>) -> Self {
        WalkState {
            depth:      0,
            parent_key: None,
            path:       vec![],
            captures,
        }
    }

    pub fn descend(&self, key: Option<&str>) -> Self {
        let mut path = self.path.clone();
        if let Some(k) = key {
            path.push(Arc::from(k));
        }
        WalkState {
            depth:      self.depth + 1,
            parent_key: key.map(Arc::from),
            path,
            captures:   self.captures.clone(),
        }
    }
}
