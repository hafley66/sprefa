//! `ParseSite` — "where did this come from in the source".
//!
//! One struct with a file path, a tree-walk (`ParseSeg` breadcrumbs), and
//! a byte range. Parser stamps one on every AST node. Later the runtime
//! clones it into diagnostics, `PathSeg` entries in `SprfPath`, hover
//! lookups, and `OpEvidence` records.
//!
//! Who reaches for this:
//!   - `parse.rs` — produces them while lexing/parsing.
//!   - `ast.rs`   — every `OpInvocation` holds an `Arc<ParseSite>`.
//!   - `sprefa::diagnostic` — `Diagnostic::primary()` returns one.
//!   - `sprefa::types::Cursor.evidence` / `SprfPath` — both embed them.
//!   - `sprefa::server` (LSP) — hover/goto resolve cursor → site → range.
//!
//! Design constraint: must stay `Clone + Hash + Eq` so the runtime can
//! intern by it and diff re-parses by site equality.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSite {
    pub file: Arc<Path>,
    pub path: Arc<[ParseSeg]>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParseSeg {
    Top { index: u16 },
    BraceChild { index: u16 },
    ParenChild { index: u16 },
    PatternLeaf { key: Arc<str> },
}
