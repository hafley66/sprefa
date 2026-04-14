//! Data walker: pattern matching and compiled step types.
//!
//! `walk_inner` (task #6) is not yet ported; this module provides the
//! compilation and pattern infrastructure only.

pub mod _0_pattern;
pub mod _1_compiled;
pub mod _2_compile;
pub mod _3_walker;
pub mod _4_brace_parse;

pub use _0_pattern::{
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
