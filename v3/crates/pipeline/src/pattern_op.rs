//! PatternOp — sub-trait of [`Op`] for ops whose paren body is an
//! injected sub-grammar (§14.5, §14.6).
//!
//! Post-§14.5m: construction (`compile_from_tree`) lives as an inherent
//! method on each pattern op struct (it returns `Self`, incompatible
//! with object safety). `PatternOp` keeps only the runtime hooks the
//! resolver and LSP hover surface call on a compiled instance.
//!
//! Non-pattern ops implement [`Op`] only. Pattern ops implement both.
//!
//! Two slots beyond the base `Op` surface:
//!
//! 1. [`PatternOp::binds_captures`] — walk the injected tree for
//!    `term_ref` nodes. The resolver reads this at lower time (§19
//!    phase lower) to validate `> $NAME` and xref references. Distinct
//!    from [`Op::bound_captures`] because this walks a raw parse tree
//!    before the op is compiled, while `bound_captures` reports on a
//!    fully-compiled instance.
//! 2. [`PatternOp::hover_match`] — op-owned hover body for match-kind
//!    nodes inside the injected tree. The framework dispatches by
//!    `node.kind()`: `term_ref` → binding graph, `carveout_expr` →
//!    recurse into host, else → this hook.
//!
//! The macro-generated `Op::pipe` delegates to `PatternOp::pattern_pipe`;
//! the default is a passthrough. Pattern ops override it to drive their
//! own identity-apply.

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use effect_runtime::{BoxFuture, RtCtx};
use std::sync::Arc;
use tree_sitter::{Node, Tree};

pub trait PatternOp: Op {
    /// Names of captures this call-site binds (§14.7).
    ///
    /// `tree` is the injected sub-tree; `bytes` is the slot-body source.
    /// Walks the injected tree for `term_ref` nodes and returns their
    /// lexical names. Engines with native capture syntax (regex named
    /// groups, ast-grep metavars) surface those names here too.
    fn binds_captures(&self, tree: &Tree, bytes: &[u8]) -> Vec<Arc<str>>;

    /// Hover body for a match-kind node inside the injected tree.
    ///
    /// Framework-owned kinds (`term_ref`, `carveout_expr`) never reach
    /// this hook (§14.6). Default: no hover.
    fn hover_match(&self, _node: Node<'_>, _cursors: &[Cursor]) -> Option<String> {
        None
    }

    /// Per-cursor work at pipe-step position. The `sprf_pattern_op`
    /// macro's generated `Op::pipe` delegates here. Default: passthrough
    /// (used before the op has been compiled — e.g. a fresh
    /// `Default::default()` instance that never saw a tree).
    fn pattern_pipe<'a>(
        &'a self,
        _ctx: &'a RtCtx,
        c: Cursor,
    ) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move { vec![c] })
    }
}

/// Structural diagnostic emitted during pattern compilation.
///
/// Parse-phase diagnostics (tree-sitter ERROR / MISSING) are produced
/// by `sprefa_parse`; this type covers lowering-phase problems that
/// only surface once the op inspects its own tree (e.g. a glob segment
/// that references an unknown directive, a regex named-group collision).
#[derive(Debug, Clone)]
pub struct PatternDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub byte_range: std::ops::Range<usize>,
}
