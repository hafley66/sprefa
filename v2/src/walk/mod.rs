//! Data walker: compiled step types and select/walk machinery.
//!
//! Pattern matching types (`PatternMatcher`, `Segment`, `compile_patterns`,
//! etc.) now live at the crate root (`_16_pattern`) so top-level ops
//! (`repo`, `rev`, `fs`) share the same matcher the walker uses. Re-exports
//! below preserve backwards compatibility for existing `walk::` imports.

pub mod _1_compiled;
pub mod _2_compile;
pub mod _3_walker;
pub mod _4_brace_parse;

pub use crate::_16_pattern::{
    Segment, PatternMatcher,
    parse_segment_pattern, match_segments_with_bindings,
    compile_pattern, compile_patterns,
};
pub use _1_compiled::{
    WalkCapture, CompiledStep, CompiledObjectEntry, CompiledKeyMatcher, WalkState,
};
pub use _2_compile::{
    KeyMatcher, ObjectEntry, SelectStep,
    compile_steps, compile_one_step, compile_object_entry, compile_key_matcher,
    compiled_key_matches,
};
pub use _3_walker::{walk, walk_with_captures, MatchResult, WalkOutcome, Captures};
