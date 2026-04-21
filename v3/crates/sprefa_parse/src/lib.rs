//! sprefa_parse — the parse half of sprefa v3.
//!
//! Source bytes → CST + AST. Wraps `tree-sitter-sprefa`.
//!
//! Three modules:
//!   - `site`  — `ParseSite` + `ParseSeg`, the compile-time coordinate
//!                stamped on every `OpInvocation`.
//!   - `ast`   — `ParsedSource`, `Pipe`, `OpInvocation`, `PipeStepKind`.
//!   - `parse` — `host_parse`. One entry point.
//!
//! Ops receive an `OpInvocation` carrying the originating CST node (via
//! `OpInvocation::node()`). All slot-body parsing happens against the
//! tree-sitter tree, not against pre-sliced byte ranges. Re-parsing slot
//! text is a v2 leftover.

pub mod site;
pub mod ast;
pub mod parse;

pub use site::{ParseSite, ParseSeg};
pub use ast::{InjectedTree, OpInvocation, ParsedSource, Pipe, PipeStepKind};
pub use parse::{
    ParseError, ParseErrorKind, ResolveLanguage, host_parse, host_parse_with_injections,
};
