//! Host-parser AST — what `parse.rs` hands back.
//!
//! `ParsedSource` owns the tree-sitter `Tree` (Arc-shared so every
//! `OpInvocation` can resolve back to its CST `Node`). `Pipe` is a
//! sequence of pipe steps; `OpInvocation` is one op call. Slot bodies
//! are NOT pre-sliced — ops walk the CST node directly via
//! `OpInvocation::node(...)`.
//!
//! Why no `ParenSlot` / `BraceSlot` / `BracketSlot` wrappers: the
//! tree-sitter grammar exposes named fields (`bracket`, `paren`, `brace`)
//! on `op_invocation` nodes. Ops use `node.children_by_field_name(...)` /
//! `node.child_by_field_name(...)` and walk the bodies they care about.
//! Re-parsing slot bytes is a v2 leftover.

use std::sync::Arc;

use tree_sitter::{Node, Tree};

use crate::site::{ParseSeg, ParseSite};

/// One parse of a .sprf source. Owns the underlying CST so every
/// `OpInvocation` in `pipes` can resolve its `parse_site.byte_range`
/// back to a tree-sitter `Node` for further structured walking.
#[derive(Debug, Clone)]
pub struct ParsedSource {
    pub tree:  Arc<Tree>,
    pub pipes: Vec<Pipe>,
}

/// One Rx-style pipe of ops chained with `>`. Lowers to `Pipeline::Seq`.
#[derive(Debug, Clone)]
pub struct Pipe {
    pub ops: Vec<OpInvocation>,
}

#[derive(Debug, Clone)]
pub struct OpInvocation {
    /// Op-invocation kind. `op_invocation` for normal `name(...)` calls,
    /// `cursor_ref`, `xref`, `capture_write`, `ans_ref`, or
    /// `deprecated_scan_sigil` for the sugar steps.
    pub kind:       PipeStepKind,
    /// Op name. Empty for non-`op_invocation` step kinds (caller inspects
    /// `kind` first).
    pub name:       Arc<str>,
    pub parse_site: Arc<ParseSite>,
    /// Shared with every other invocation from the same parse. Used to
    /// resolve `parse_site.byte_range` back to a CST `Node` via
    /// `OpInvocation::node`.
    pub tree:       Arc<Tree>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipeStepKind {
    OpInvocation,
    CursorRef,
    Xref,
    CaptureWrite,
    AnsRef,
    DeprecatedScanSigil,
}

impl OpInvocation {
    /// Resolve back to the CST node this invocation came from. Walks the
    /// `parse_site.path` from the tree root via named-child indices so
    /// the result is unambiguous (byte-range lookup picks the smallest
    /// descendant, which collapses when an op_invocation wraps a single
    /// identifier).
    pub fn node(&self) -> Node<'_> {
        let mut n = self.tree.root_node();
        for seg in self.parse_site.path.iter() {
            let ParseSeg::Child { index } = seg;
            let mut cur = n.walk();
            n = n
                .named_children(&mut cur)
                .nth(*index as usize)
                .expect("parse path step must point at an existing named child");
        }
        n
    }
}
