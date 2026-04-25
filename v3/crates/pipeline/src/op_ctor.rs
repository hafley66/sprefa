//! Construction + lowering trait surface for stdlib ops (parse.md §14.5m
//! Pass B). Three sibling traits, each registered through a single
//! `Registry::register::<O>()` family.
//!
//! - [`OpCtor`]: value-arg construction. Most stdlib ops use this.
//!   Implements `from_values(Vec<Value>) -> Result<Self, ...>` and
//!   carries `NAME` / `DOC`.
//! - [`PatternCtor`]: compile-from-injected-tree construction. The
//!   pattern-op door (glob, re). Carries the sub-grammar `Language`
//!   and optional `highlights` query string.
//! - [`OpLowering`]: escape hatch for ops that need raw CST inspection
//!   at lower time (canonical: `tag`). No stdlib consumer yet; landed
//!   so the door exists when the first consumer arrives.
//!
//! `Op::name()` and `OpCtor::NAME` are intentionally distinct: instance
//! `name()` is used at runtime for `PathSeg::Op` tagging; `NAME` is used
//! by the registry at registration time before any instance exists.
//! Both should agree per op.

use std::sync::Arc;

use tree_sitter::{Language, Node, Tree};

use crate::_1_op::Op;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

/// Construction door for value-arg stdlib ops.
pub trait OpCtor: Op + Sized {
    const NAME: &'static str;
    const DOC:  &'static str;
    /// One-line label describing what the paren body looks like.
    /// Surfaced by the LSP hover banner. Override per op; the default
    /// punts the reader at the full doc.
    const BODY_GRAMMAR: &'static str = "(see op doc)";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>>;
}

/// Construction door for pattern ops with a sub-grammar (§14.5).
pub trait PatternCtor: Op + Sized {
    const NAME: &'static str;
    const DOC:  &'static str;
    const BODY_GRAMMAR: &'static str = "(see op doc)";

    fn language() -> Language;
    fn highlights() -> Option<&'static str> { None }

    fn compile_from_tree(
        tree:  &Tree,
        bytes: &[u8],
    ) -> Result<Self, Vec<PatternDiagnostic>>;
}

/// Custom lowering escape hatch for ops that need raw CST access at
/// lower time (e.g. `tag`, which inspects sibling nodes to bind
/// relations). Lower returns a fully-built `Value` so the host lowerer
/// just splices it into the surrounding paren slot. Implementors are
/// responsible for emitting their own diagnostics into the supplied
/// sink.
///
/// No stdlib op uses this yet; the door exists so the first consumer
/// (tag) lands without re-shaping the registry.
pub trait OpLowering: 'static + Send + Sync {
    const NAME: &'static str;
    const DOC:  &'static str;
    const BODY_GRAMMAR: &'static str = "(see op doc)";

    fn language() -> Option<Language> { None }

    fn lower(
        node:        Node<'_>,
        src:         &[u8],
        registry:    &crate::registry::Registry,
        diagnostics: &mut Vec<PatternDiagnostic>,
    ) -> Value;
}

/// Erased custom-lowering signature stored in the registry. Mirrors
/// [`OpLowering::lower`] but as a trait object.
pub(crate) type CustomLowerFn = dyn Fn(
        Node<'_>,
        &[u8],
        &crate::registry::Registry,
        &mut Vec<PatternDiagnostic>,
    ) -> Value
    + Send
    + Sync;

/// Bridge: convert an [`OpCtor`] impl into the erased `OpFactory`
/// closure stored by the registry.
pub(crate) fn op_factory<O: OpCtor>() -> Arc<crate::registry::OpFactory> {
    Arc::new(|_, values| {
        O::from_values(values).map(|op| Box::new(op) as Box<dyn Op>)
    })
}
