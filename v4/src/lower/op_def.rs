//! `OperatorDef` — the four-slot operator surface.
//!
//! Every op exposes up to four call slots:
//!
//!   `[]`  flow  override (a Value, usually a Pipe)
//!   `()`  paren positionals (sprf-typed)
//!   `{}`  brace block (a sub-Pipe<Cursor>, lowered already)
//!   ` `` ` dsl body (raw text + parsed `${X}` interpolations)
//!
//! A def declares which slots it accepts via `flow_arg`, `paren_args`,
//! `brace_block`, `dsl_body`. `validate_call` checks shape and emits
//! diags. `lower` runs after validate clears.

use std::sync::Arc;

use effect_runtime::v2::{ByteRange, Pipe};

use crate::Cursor;

use super::ctx::{LowerCtx, LowerError};
use super::value::Value;

/// Variadic carries a `&'static ArgKind` rather than a Box so the
/// trait's `paren_args()` can return a `const` slice. Build with
/// `ArgKind::Variadic(&ArgKind::Atom)`.
#[derive(Clone, Copy, Debug)]
pub enum ArgKind {
    Atom,
    Pipe,
    Any,
    Variadic(&'static ArgKind),
}

impl ArgKind {
    pub fn label(&self) -> String {
        match self {
            ArgKind::Atom => "atom".into(),
            ArgKind::Pipe => "pipe".into(),
            ArgKind::Any  => "any".into(),
            ArgKind::Variadic(inner) => format!("variadic({})", inner.label()),
        }
    }
    pub fn matches(&self, v: &Value) -> bool {
        match self {
            ArgKind::Atom => matches!(v, Value::Atom(_)),
            ArgKind::Pipe => matches!(v, Value::Pipe(_)),
            ArgKind::Any  => true,
            ArgKind::Variadic(_) => false, // checked at the slot level
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArgSig {
    pub kind:     ArgKind,
    pub name:     &'static str,
    pub doc:      &'static str,
    pub required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockShape {
    /// `{ ... }` body lowers to a `Pipe<Cursor>` and is required.
    Pipe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DslShape {
    /// Raw text with `${X}` interpolation, no sub-grammar.
    Plain,
    // Future: Regex / Glob / Ast(Lang) / Cst(Lang).
}

#[derive(Clone, Debug)]
pub struct DslInterp {
    pub name:  Arc<str>,
    pub range: ByteRange,
}

#[derive(Clone, Debug)]
pub struct DslBody {
    pub raw:     Arc<str>,
    pub interps: Vec<DslInterp>,
}

pub trait OperatorDef: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn flow_arg(&self)    -> Option<ArgSig>      { None }
    fn paren_args(&self)  -> &[ArgSig]           { &[] }
    fn brace_block(&self) -> Option<BlockShape>  { None }
    fn dsl_body(&self)    -> Option<DslShape>    { None }

    fn lower(
        &self,
        ctx:   &LowerCtx,
        flow:  Option<Value>,
        args:  &[Value],
        block: Option<Pipe<Cursor>>,
        dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError>;
}
