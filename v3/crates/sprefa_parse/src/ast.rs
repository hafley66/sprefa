//! Host-parser AST — what `parse.rs` hands back.
//!
//! Pure data, zero behavior. Every op invocation in a .sprf file comes
//! out the other end of `host_parse` as one of these `OpInvocation`
//! structs with zero or more slot bodies (`BracketSlot`, `ParenSlot`,
//! `BraceSlot`) carrying raw source bytes. The owning op is responsible
//! for parsing those slot bodies in its own `parse()` hook — this crate
//! only does host-level slicing.
//!
//! Who consumes these:
//!   - `sprefa::ops::*::parse` — each op's per-op lowering reads its
//!     matched `OpInvocation`, dispatches on slot contents, returns a
//!     `Pipeline`.
//!   - `sprefa::registry::lower_rules` — walks invocations, calls each
//!     op's `parse`, stitches the result into the rule DAG.
//!   - `sprefa::op::CrossRefOccurrence` consumers — rule dep-graph
//!     builder (`sprefa::dag`) pulls xref edges from here.
//!   - `sprefa::analysis` (LSP hover/completion) — inspects invocations
//!     at a cursor position.
//!
//! `BraceMode` and `GrammarRef` exist so ops can declare at registration
//! time how their brace body wants to be interpreted; parse uses that
//! signal when slicing.

use std::ops::Range;
use std::sync::Arc;

use crate::site::ParseSite;

#[derive(Debug, Clone)]
pub struct OpInvocation {
    pub name:       Arc<str>,
    pub brackets:   Vec<BracketSlot>,
    pub paren_src:  Option<ParenSlot>,
    pub brace_src:  Option<BraceSlot>,
    pub parse_site: Arc<ParseSite>,
    /// Cross-refs (`${rule.$VAR}`) discovered in paren/bracket tokens at
    /// host-parse time. Feeds the rule DAG build. Walker-embedded cross-refs
    /// inside brace bodies are gathered separately at lower time.
    pub crossrefs:  Vec<CrossRefOccurrence>,
}

/// One `${rule.$VAR}` cross-ref token discovered at host-parse time.
///
/// `rule` and `var` are the parsed components inside the braces.
/// `byte_range` is the absolute byte span of the entire `${...}` token in
/// the original source — used to anchor `xref/empty-join` and friends so
/// the diagnostic underlines the exact ref that caused the problem.
///
/// Populated by `parse::scan_slot_crossrefs` over paren, bracket,
/// and brace slots. Consumed by the runtime crate's lowering.
#[derive(Debug, Clone)]
pub struct CrossRefOccurrence {
    pub rule:       Arc<str>,
    pub var:        Arc<str>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct BracketSlot { pub src: Arc<str>, pub byte_range: Range<usize> }
#[derive(Debug, Clone)]
pub struct ParenSlot   { pub src: Arc<str>, pub byte_range: Range<usize> }
#[derive(Debug, Clone)]
pub struct BraceSlot   { pub src: Arc<str>, pub byte_range: Range<usize> }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BraceMode { DefaultFork, CustomSprf, WalkerPattern }

#[derive(Debug, Clone)]
pub struct GrammarRef(pub Arc<str>);
