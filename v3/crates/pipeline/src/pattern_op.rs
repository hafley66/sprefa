//! PatternOp — sub-trait of [`Op`] for ops whose paren body is an
//! injected sub-grammar (§14.5, §14.6).
//!
//! Non-pattern ops implement [`Op`] only. Pattern ops implement both.
//! `str` is special-cased: it is a plain [`Op`] even though it lives
//! on the injection registry, because its body is raw unparsed bytes
//! (§14.2, §14.5 closing note).
//!
//! Three slots beyond the base Op surface:
//!
//! 1. [`PatternOp::compile`] — parsed sub-tree → executable matcher
//!    (`CompiledPattern`). Emits `pattern/<op>-syntax` diagnostics on
//!    structural problems in the injected tree.
//! 2. [`PatternOp::binds_captures`] — walk injected tree for `term_ref`
//!    nodes, return the capture names bound by this call-site. The
//!    resolver reads this at lower time (§19 phase lower) to validate
//!    `> $NAME` and xref references.
//! 3. [`PatternOp::hover_match`] — op-owned hover body for match-kind
//!    nodes inside the injected tree. The framework dispatches by
//!    `node.kind()`: `term_ref` → binding graph, `carveout_expr` →
//!    recurse into host, else → this hook.

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use effect_runtime::{BoxFuture, RtCtx};
use std::sync::Arc;
use tree_sitter::{Node, Tree};

pub trait PatternOp: Op {
    /// Compile the parsed sub-tree into a runnable matcher.
    ///
    /// `bytes` is the full source file; caller resolves sub-tree
    /// byte ranges against it. Returns [`CompiledPattern`] on success
    /// or a set of structural diagnostics on failure. Syntax-level
    /// errors (ERROR / MISSING nodes) are surfaced at parse phase by
    /// `sprefa_parse::collect_errors` and do not reach `compile`.
    fn compile(
        &self,
        tree: &Tree,
        bytes: &[u8],
    ) -> Result<CompiledPattern, Vec<PatternDiagnostic>>;

    /// Names of captures this call-site binds (§14.7).
    ///
    /// `tree` is the injected sub-tree; `bytes` is the slot-body source
    /// (byte offsets in `tree` are substring-relative, matching §14.5a).
    /// Walks the injected tree for `term_ref` nodes and returns their
    /// lexical names. Engines with native capture syntax (regex named
    /// groups, ast-grep metavars) surface those names here too, so the
    /// resolver treats sugar and native identically.
    fn binds_captures(&self, tree: &Tree, bytes: &[u8]) -> Vec<Arc<str>>;

    /// Hover body for a match-kind node inside the injected tree.
    ///
    /// Framework-owned kinds (`term_ref`, `carveout_expr`) never reach
    /// this hook (§14.6). Default: no hover.
    fn hover_match(
        &self,
        _node: Node<'_>,
        _cursors: &[Cursor],
    ) -> Option<String> {
        None
    }

    /// Per-cursor work for a pattern op; called by `Op::pipe`.
    ///
    /// Pattern ops typically forward `Op::pipe` to this via the
    /// `#[sprf_pattern_op]` macro expansion. Kept separate so the
    /// pattern-specific signature can evolve without touching `Op`.
    fn pattern_pipe<'a>(
        &'a self,
        ctx: &'a RtCtx,
        c: Cursor,
    ) -> BoxFuture<'a, Vec<Cursor>>;
}

/// Output of [`PatternOp::compile`].
///
/// Per-op storage is an `Arc<dyn Any>`: each op owns its compiled
/// representation (regex::Regex for `re`, a Vec<Segment> for `glob`,
/// etc.) and downcasts at match time. Keeps the trait object-safe
/// without forcing a central enum of compiled-pattern variants (§2.1
/// ops-own-everything).
#[derive(Clone)]
pub struct CompiledPattern {
    pub op_name: &'static str,
    pub inner: Arc<dyn std::any::Any + Send + Sync>,
}

impl std::fmt::Debug for CompiledPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledPattern")
            .field("op_name", &self.op_name)
            .field("inner", &"<opaque>")
            .finish()
    }
}

impl CompiledPattern {
    pub fn new<T: std::any::Any + Send + Sync>(op_name: &'static str, value: T) -> Self {
        Self {
            op_name,
            inner: Arc::new(value),
        }
    }

    pub fn downcast<T: std::any::Any + Send + Sync>(&self) -> Option<&T> {
        self.inner.downcast_ref::<T>()
    }
}

/// Structural diagnostic emitted by [`PatternOp::compile`].
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
