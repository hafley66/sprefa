//! Data walker: compiled step types and select/walk machinery.
//!
//! Pattern matching types (`PatternMatcher`, `Segment`, `compile_patterns`,
//! etc.) now live at the crate root (`pattern`) so top-level ops
//! (`repo`, `rev`, `fs`) share the same matcher the walker uses. Re-exports
//! below preserve backwards compatibility for existing `walk::` imports.

pub mod compiled;
pub mod compile;
pub mod walker;
pub mod brace_parse;

pub use crate::pattern::{
    Segment, PatternMatcher,
    parse_segment_pattern, match_segments_with_bindings,
    compile_pattern, compile_patterns,
};
pub use compiled::{
    WalkCapture, CompiledStep, CompiledObjectEntry, CompiledKeyMatcher, WalkState,
};
pub use compile::{
    KeyMatcher, ObjectEntry, SelectStep,
    compile_steps, compile_one_step, compile_object_entry, compile_key_matcher,
    compiled_key_matches,
};
pub use walker::{walk, walk_with_captures, MatchResult, WalkOutcome, Captures};
