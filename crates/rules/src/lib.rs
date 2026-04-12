pub mod ast;
pub mod emit;
pub mod extractor;
pub mod file_match;
pub mod git_match;
pub mod graph;
pub mod marker;
pub mod md;
pub mod pattern;
pub mod schema;
pub mod span_fallback;
pub mod types;
pub mod walk;

pub use types::*;
pub use walk::{enhance_with_spans, CapturedValue, MatchResult};
pub use span_fallback::{SourceSpan, find_path_span};
