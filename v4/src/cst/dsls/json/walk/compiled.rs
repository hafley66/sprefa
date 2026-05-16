//! Compiled walk-step types. Ported from v2/src/walk/_1_compiled.rs.
//! v3 drops the `path` field on captures — v3 `Capture` carries only
//! `name + byte_range`, so the walker tracks `text + byte_range` and
//! emits to the framework via byte_range alone.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::pattern::{PatternMatcher, Segment};

#[derive(Debug, Clone)]
pub struct WalkCapture {
    pub text: Arc<str>,
    pub byte_start: u32,
    pub byte_end: u32,
}

pub type Captures = FxHashMap<Arc<str>, WalkCapture>;

#[derive(Debug)]
pub enum CompiledStep {
    Any,
    /// Recursive descent that binds the dot-joined traversed key path
    /// into `capture`. Compiled form of the json `$$${PATH?}` carveout.
    AnyCapture {
        capture: String,
    },
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
    /// Matches any data value (scalar/object/array) and captures its span.
    /// Used for bare `$VAR` in value position.
    CaptureAny {
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

#[derive(Debug)]
pub struct CompiledObjectEntry {
    pub key: CompiledKeyMatcher,
    pub value: Vec<CompiledStep>,
}

#[derive(Debug)]
pub enum CompiledKeyMatcher {
    Exact(String),
    Glob(Vec<PatternMatcher>),
    Capture(String),
    Wildcard,
}
