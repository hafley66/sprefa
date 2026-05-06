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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpMode {
    /// `${X}` — read X from cursor.terms; sub-pipe = `term(:X)`.
    Read,
    /// `${X?}` — bind X = cursor.value; sub-pipe = `term?(:X)`.
    Bind,
}

#[derive(Clone, Debug)]
pub struct DslInterp {
    pub name:  Arc<str>,
    pub range: ByteRange,
    pub mode:  InterpMode,
}

/// Names this op's dsl body binds at runtime (e.g. `re`'s
/// `(?P<NAME>…)`, `glob`'s `<NAME>` capture sigil). Populated per-op
/// by `OperatorDef::binders_in_dsl`. Used by `binding_graph` so the
/// compile-time analyzer doesn't emit false-positive
/// `lang/use-before-bind` diags for names introduced by DSL syntax.
#[derive(Clone, Debug)]
pub struct DslBinder {
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

    /// When `dsl_body()` is `Some`, is the body required at every call?
    /// Default true. Ops that accept either paren-arg OR dsl form
    /// (e.g. `glob(:pattern)` or `` glob`pat` ``) override to false.
    fn dsl_required(&self) -> bool { true }

    /// Walk-time DSL parser. Default: scan for `${IDENT}` holes and emit
    /// one `DslInterp` per hole with byte ranges relative to `raw`. Ops
    /// with sub-grammars (regex, glob, ast) override.
    fn parse_dsl(&self, raw: &str) -> Result<Vec<DslInterp>, LowerError> {
        Ok(default_plain_dsl_parse(raw))
    }

    /// Names this op's dsl body BINDS at runtime. Used by the
    /// compile-time binding-graph analyzer to suppress
    /// `lang/use-before-bind` diags for captures introduced by
    /// DSL-internal syntax.
    ///   re   — `(?P<NAME>…)` named groups
    ///   glob — `<NAME>` directory capture
    ///   ast  — `$NAME` / `$$$REST` metavars
    /// Default: empty. Op-specific scanners override.
    fn binders_in_dsl(&self, _raw: &str) -> Vec<DslBinder> { Vec::new() }

    /// Cursor-term keys this op sets imperatively at runtime regardless
    /// of dsl content. Used by the binding-graph analyzer so downstream
    /// ops can read these without `use-before-bind` false positives.
    /// Examples:
    ///   fs        → ["FS"]
    ///   glob      → ["FS"]
    ///   ast/re    → ["LO", "HI"]
    ///   re        → ["LO", "HI", "MATCH"]
    /// Default: empty. Op authors declare here so consumers don't have
    /// to know runtime details.
    fn cursor_binds(&self) -> &'static [&'static str] { &[] }

    fn lower(
        &self,
        ctx:   &LowerCtx,
        flow:  Option<Value>,
        args:  &[Value],
        block: Option<Pipe<Cursor>>,
        dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError>;
}

/// Default `${IDENT}` / `${IDENT?}` host pipe-hole scanner. IDENT =
/// ASCII letters / digits / underscore, first char non-digit. Returns
/// `DslInterp { name, range, mode }` per hole. `range` covers the full
/// `${...}` span; `mode` distinguishes `${X}` (Read) from `${X?}` (Bind).
/// This is the host carveout — every dsl body is scanned for these
/// regardless of the dsl's own grammar. Per-dsl `binders_in_dsl` is
/// orthogonal and handles the dsl-internal capture form (e.g. `$X`).
pub fn default_plain_dsl_parse(raw: &str) -> Vec<DslInterp> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'{' {
            let lo = i;
            let mut j = i + 2;
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                let name_lo = j;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_')
                {
                    j += 1;
                }
                let name_hi = j;
                let mode = if j < bytes.len() && bytes[j] == b'?' {
                    j += 1;
                    InterpMode::Bind
                } else {
                    InterpMode::Read
                };
                if j < bytes.len() && bytes[j] == b'}' {
                    let hi = j + 1;
                    let name = &raw[name_lo..name_hi];
                    out.push(DslInterp {
                        name:  Arc::<str>::from(name),
                        range: ByteRange { lo: lo as u32, hi: hi as u32 },
                        mode,
                    });
                    i = hi;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}
