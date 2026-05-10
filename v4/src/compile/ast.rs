//! Value-typed AST shared by `parse.rs` and `walk.rs`.
//!
//! No tree-sitter handles, no lifetimes, no per-op typing. The walker
//! resolves `SlotText` bodies into `compile::lower::Value` (Atom | Pipe)
//! via per-arg classifiers; `DslText` is handed to the op's `parse_dsl`
//! at lower-time.
//!
//! Spans are byte ranges into the original source string. They flow
//! through to lower-time validate_call (for arity/kind diags) and to
//! ProbeSink at runtime (for "cursors happened here" LSP support).

use std::sync::Arc;

use effect_runtime::v2::ByteRange;

/// One pipe of ops chained with `>`. Top-level program is `Vec<PipeAst>`,
/// one per `;`-separated source pipe.
#[derive(Debug, Clone)]
pub struct PipeAst {
    pub steps: Vec<OpCall>,
    pub span: ByteRange,
}

/// One op invocation in source.
///
/// Slot bodies are kept as raw text + span. The walker classifies each
/// `()`-arg into `Value::Atom` or `Value::Pipe` and parses the dsl body
/// via the op's `OperatorDef::parse_dsl`. The `{...}` block is parsed
/// eagerly into `PipeAst` because it's always a sub-pipe.
#[derive(Debug, Clone)]
pub struct OpCall {
    /// Op name (without the `?` suffix).
    pub name: Arc<str>,
    /// `!` op suffix presence. For declared rules this is force-run /
    /// cache-bypass intent. Builtin `sh!` is preserved by lowering.
    pub force: bool,
    /// `?` op suffix presence. For declared rules this is the relation
    /// query/materialize marker.
    pub predicate: bool,
    /// Immediate dotted apply marker after the name / predicate suffix.
    /// Kept while older examples migrate; bare `rule_name(...)` is the
    /// current rule apply/send form.
    pub apply: bool,
    /// Full call extent, used as the span tag for ProbeSink.
    pub span: ByteRange,
    /// `[flow]` body. Optional source override. Defaults to `&.value`.
    pub flow: Option<SlotText>,
    /// `(args)` body, split by top-level `,`. Each is a slot of raw text.
    /// Walker classifies into Value::Atom | Value::Pipe.
    pub args: Vec<SlotText>,
    /// `` `dsl body` `` raw text + span. Walker hands to `parse_dsl`.
    pub dsl: Option<DslText>,
    /// `{ block }` parsed sub-pipe.
    pub block: Option<PipeAst>,
}

/// Raw bytes of a slot body plus its span. Walker re-parses as needed.
#[derive(Debug, Clone)]
pub struct SlotText {
    pub raw: Arc<str>,
    pub span: ByteRange,
}

/// Raw bytes of a dsl body plus its span. Op's `parse_dsl(raw)` walks
/// it for `${X}` interp holes (or whatever its grammar wants).
#[derive(Debug, Clone)]
pub struct DslText {
    pub raw: Arc<str>,
    pub span: ByteRange,
}
