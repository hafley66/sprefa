//! Compilation of `SelectStep` (parsed brace pattern) into `CompiledStep`
//! (walker input). Ported from v2/src/walk/_2_compile.rs. Context-step
//! variants (Repo/Rev/Folder/File) are dropped — v3 has separate ops for
//! those concerns.

use super::compiled::{CompiledKeyMatcher, CompiledObjectEntry, CompiledStep};
use super::pattern::{compile_pattern, parse_segment_pattern};

#[derive(Debug, Clone)]
pub enum KeyMatcher {
    Exact(String),
    Glob(String),
    Capture(String),
    Wildcard,
}

#[derive(Debug, Clone)]
pub struct ObjectEntry {
    pub key: KeyMatcher,
    pub value: Vec<SelectStep>,
}

#[derive(Debug, Clone)]
pub enum SelectStep {
    Any,
    Key {
        name: String,
        capture: Option<String>,
    },
    KeyMatch {
        pattern: String,
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
        pattern: String,
    },
    ArrayItem,
    Leaf {
        capture: Option<String>,
    },
    CaptureAny {
        capture: Option<String>,
    },
    LeafPattern {
        pattern: String,
    },
    Object {
        entries: Vec<ObjectEntry>,
    },
    Array {
        item: Vec<SelectStep>,
    },
}

pub fn compile_steps(steps: &[SelectStep]) -> Result<Vec<CompiledStep>, String> {
    steps.iter().map(compile_one_step).collect()
}

fn compile_one_step(step: &SelectStep) -> Result<CompiledStep, String> {
    Ok(match step {
        SelectStep::Any => CompiledStep::Any,
        SelectStep::Key { name, capture } => CompiledStep::Key {
            name: name.clone(),
            capture: capture.clone(),
        },
        SelectStep::KeyMatch { pattern, capture } => CompiledStep::KeyMatch {
            matchers: compile_pattern(pattern).map_err(|e| e.to_string())?,
            capture: capture.clone(),
        },
        SelectStep::DepthMin { n } => CompiledStep::DepthMin { n: *n },
        SelectStep::DepthMax { n } => CompiledStep::DepthMax { n: *n },
        SelectStep::DepthEq { n } => CompiledStep::DepthEq { n: *n },
        SelectStep::ParentKey { pattern } => CompiledStep::ParentKey {
            matchers: compile_pattern(pattern).map_err(|e| e.to_string())?,
        },
        SelectStep::ArrayItem => CompiledStep::ArrayItem,
        SelectStep::Leaf { capture } => CompiledStep::Leaf {
            capture: capture.clone(),
        },
        SelectStep::CaptureAny { capture } => CompiledStep::CaptureAny {
            capture: capture.clone(),
        },
        SelectStep::LeafPattern { pattern } => CompiledStep::LeafPattern {
            segments: parse_segment_pattern(pattern),
        },
        SelectStep::Object { entries } => CompiledStep::Object {
            entries: entries
                .iter()
                .map(compile_object_entry)
                .collect::<Result<_, _>>()?,
        },
        SelectStep::Array { item } => CompiledStep::Array {
            item: compile_steps(item)?,
        },
    })
}

fn compile_object_entry(entry: &ObjectEntry) -> Result<CompiledObjectEntry, String> {
    Ok(CompiledObjectEntry {
        key: compile_key_matcher(&entry.key)?,
        value: compile_steps(&entry.value)?,
    })
}

fn compile_key_matcher(km: &KeyMatcher) -> Result<CompiledKeyMatcher, String> {
    Ok(match km {
        KeyMatcher::Exact(s) => CompiledKeyMatcher::Exact(s.clone()),
        KeyMatcher::Glob(pattern) => {
            CompiledKeyMatcher::Glob(compile_pattern(pattern).map_err(|e| e.to_string())?)
        }
        KeyMatcher::Capture(name) => CompiledKeyMatcher::Capture(name.clone()),
        KeyMatcher::Wildcard => CompiledKeyMatcher::Wildcard,
    })
}
