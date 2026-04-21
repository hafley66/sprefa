//! `ParseSite` — "where did this come from in the source".
//!
//! File path, a tree-walk (`ParseSeg` breadcrumbs from root), and a byte
//! range. Parser stamps one on every `OpInvocation`. Diagnostics, hover,
//! and cursor paths embed it.
//!
//! The path is flat: one `Child { index }` per step down the named-child
//! axis of the tree-sitter CST. Reconstruction: start at `source_file`,
//! walk `named_child(path[0].index)` → `named_child(path[1].index)` → ...

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParseSite {
    pub file:       Arc<Path>,
    pub path:       Arc<[ParseSeg]>,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseSeg {
    Child { index: u16 },
}
