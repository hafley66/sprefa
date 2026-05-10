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
use super::value::{CallArg, Value};

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
            ArgKind::Any => "any".into(),
            ArgKind::Variadic(inner) => format!("variadic({})", inner.label()),
        }
    }
    pub fn matches(&self, v: &Value) -> bool {
        match self {
            ArgKind::Atom => matches!(v, Value::Atom(_)),
            ArgKind::Pipe => matches!(v, Value::Pipe(_)),
            ArgKind::Any => true,
            ArgKind::Variadic(_) => false, // checked at the slot level
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArgSig {
    pub kind: ArgKind,
    pub name: &'static str,
    pub doc: &'static str,
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

/// Kind of carveout body. Term-shape is the term-ref/dot-access surface
/// from Layer 0c.3. SubPipe-shape is task #10 — the body parses as a full
/// sprf pipe expression and drains per outer cursor at render time.
#[derive(Clone)]
pub enum InterpKind {
    /// `${X}` / `${X?}` / `${X.field}` / `${&.field}`.
    /// `mode` and `field` are populated by the scanner; `name` is the
    /// stem (IDENT or `&`). `subpipe_src` is unused.
    Term {
        mode: InterpMode,
        field: Option<Arc<str>>,
    },
    /// `${ <op-shape body> }` — the inner text is a sprf pipe fragment.
    /// `subpipe_src` is the trimmed inner body (no surrounding `${`/`}`),
    /// to be re-parsed and walked by the walker. `lowered` is set after
    /// the walker recurses; render-time drains it per outer cursor and
    /// uses the drained cursor's focal `value` as the slot value.
    SubPipe {
        src: Arc<str>,
        /// Set by the walker after `walk_pipe` lowers `src`. None at
        /// parse-time (parse_dsl can't see LowerCtx/Registry), Some after
        /// walk_op runs. Render-time relies on this being populated.
        ///
        /// `Pipe<Cursor>` doesn't implement `Debug`; manual impl below
        /// hides the contents.
        lowered: Option<Pipe<Cursor>>,
    },
}

impl std::fmt::Debug for InterpKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpKind::Term { mode, field } => f
                .debug_struct("Term")
                .field("mode", mode)
                .field("field", field)
                .finish(),
            InterpKind::SubPipe { src, lowered } => f
                .debug_struct("SubPipe")
                .field("src", src)
                .field("lowered_steps", &lowered.as_ref().map(|p| p.len()))
                .finish(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DslInterp {
    /// Stem name. IDENT (e.g. `NAME`) or the literal `&` for the focal-
    /// cursor self-op. For SubPipe interps this is a synthetic
    /// `$pipe` placeholder.
    pub name: Arc<str>,
    /// Byte span of the full `${...}` form in the dsl body.
    pub range: ByteRange,
    /// Term vs SubPipe shape (task #10). The legacy `mode`/`field` fields
    /// are preserved for back-compat reads on Term-shape interps;
    /// SubPipe interps default `mode = Read` / `field = None`.
    pub kind: InterpKind,
    /// Back-compat alias for `kind = Term { mode, .. }`. Reads return
    /// `Read` for SubPipe interps (the carveout itself doesn't bind).
    pub mode: InterpMode,
    /// Back-compat alias for `kind = Term { field, .. }`. Reads return
    /// `None` for SubPipe interps.
    pub field: Option<Arc<str>>,
}

/// Names this op's dsl body binds at runtime (e.g. `re`'s
/// `(?P<NAME>…)`, `glob`'s `<NAME>` capture sigil). Populated per-op
/// by `OperatorDef::binders_in_dsl`. Used by `binding_graph` so the
/// compile-time analyzer doesn't emit false-positive
/// `lang/use-before-bind` diags for names introduced by DSL syntax.
#[derive(Clone, Debug)]
pub struct DslBinder {
    pub name: Arc<str>,
    pub range: ByteRange,
}

#[derive(Clone, Debug)]
pub struct DslBody {
    pub raw: Arc<str>,
    pub interps: Vec<DslInterp>,
}

pub trait OperatorDef: Send + Sync + 'static {
    fn name(&self) -> &'static str;

    fn flow_arg(&self) -> Option<ArgSig> {
        None
    }
    fn paren_args(&self) -> &[ArgSig] {
        &[]
    }
    fn brace_block(&self) -> Option<BlockShape> {
        None
    }
    fn dsl_body(&self) -> Option<DslShape> {
        None
    }

    /// When `dsl_body()` is `Some`, is the body required at every call?
    /// Default true. Ops that accept either paren-arg OR dsl form
    /// (e.g. `glob(:pattern)` or `` glob`pat` ``) override to false.
    fn dsl_required(&self) -> bool {
        true
    }

    /// When `brace_block()` is `Some`, is the block required at every call?
    /// Default true. Ops that accept declaration-only form (e.g.
    /// empty-body `rule(:x, A?, B?);`) override to false.
    fn brace_block_required(&self) -> bool {
        true
    }

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
    fn binders_in_dsl(&self, _raw: &str) -> Vec<DslBinder> {
        Vec::new()
    }

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
    fn cursor_binds(&self) -> &'static [&'static str] {
        &[]
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError>;

    /// Chain-position-aware lower. `chain_pos = 0` ⇒ head-of-pipe (no
    /// upstream cursors); `>= 1` ⇒ sink position downstream of `>` chaining.
    /// Default delegates to `lower(...)`; override only when the op's
    /// shape differs by position (e.g. `rule` distinguishes standalone-decl
    /// from sink form).
    #[allow(clippy::too_many_arguments)]
    fn lower_with_chain(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
        _chain_pos: usize,
    ) -> Result<Pipe<Cursor>, LowerError> {
        self.lower(ctx, flow, args, block, dsl)
    }

    fn lower_call(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[CallArg],
        block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let values: Vec<Value> = args.iter().map(|arg| arg.value.clone()).collect();
        self.lower(ctx, flow, &values, block, dsl)
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_call_with_chain(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[CallArg],
        block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
        chain_pos: usize,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let values: Vec<Value> = args.iter().map(|arg| arg.value.clone()).collect();
        self.lower_with_chain(ctx, flow, &values, block, dsl, chain_pos)
    }
}

/// Default host pipe-hole scanner. Recognized forms:
///   `${IDENT}`         — Read X (term value)
///   `${IDENT?}`        — Bind X = focal value
///   `${IDENT.field}`   — Read X's `.field` (field requires bound mode)
///   `${&.field}`       — Read focal cursor's `.field` (always bound)
///
/// IDENT = ASCII letter or `_` lead, then alnum/`_`. Field = same shape.
/// Returns `DslInterp { name, range, mode, field }` per hole. `range`
/// covers the full `${...}` span.
///
/// Illegal forms are skipped (treated as literal text) for now; future
/// surface-level passes may turn these into parse-time diagnostics:
///   `${&}`         — bare focal stem, no field
///   `${&?...}`     — focal stem with bind mark
///   `${X?.field}`  — bind mark + field access
///
/// This is the host carveout — every dsl body is scanned for these
/// regardless of the dsl's own grammar. Per-dsl `binders_in_dsl` is
/// orthogonal and handles the dsl-internal capture form (e.g. `$X`).
pub fn default_plain_dsl_parse(raw: &str) -> Vec<DslInterp> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if !(bytes[i] == b'$' && bytes[i + 1] == b'{') {
            i += 1;
            continue;
        }
        let lo = i;

        let body_lo = i + 2;
        let closed_at = scan_interp_close(bytes, body_lo);
        let Some(close_idx) = closed_at else {
            i += 1;
            continue;
        };
        let hi = close_idx + 1;
        let inner = &raw[body_lo..close_idx];

        // First: try to match the term-shape grammar:
        //   `&.field`  |  `IDENT`  |  `IDENT?`  |  `IDENT.field`
        // (no leading/trailing whitespace, no other characters).
        if let Some((mode, field, name, is_focal)) = parse_term_shape(inner) {
            // Reject illegal combinations (mirrors the original guard).
            let illegal = (is_focal && field.is_none())
                || (is_focal && matches!(mode, InterpMode::Bind))
                || (matches!(mode, InterpMode::Bind) && field.is_some());
            if illegal {
                i += 1;
                continue;
            }
            out.push(DslInterp {
                name,
                range: ByteRange {
                    lo: lo as u32,
                    hi: hi as u32,
                },
                kind: InterpKind::Term {
                    mode,
                    field: field.clone(),
                },
                mode,
                field,
            });
            i = hi;
            continue;
        }

        // Op-shape (task #10): the inner text is a sprf pipe fragment.
        // Trim outer whitespace; empty bodies remain illegal (skip).
        let trimmed = inner.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }
        out.push(DslInterp {
            name: Arc::<str>::from("$pipe"),
            range: ByteRange {
                lo: lo as u32,
                hi: hi as u32,
            },
            kind: InterpKind::SubPipe {
                src: Arc::<str>::from(trimmed),
                lowered: None,
            },
            mode: InterpMode::Read,
            field: None,
        });
        i = hi;
    }
    out
}

fn scan_interp_close(bytes: &[u8], body_lo: usize) -> Option<usize> {
    let mut j = body_lo;
    let mut depth = 1usize;
    while j < bytes.len() {
        if starts_interp(bytes, j) {
            depth += 1;
            j += 2;
            continue;
        }
        match bytes[j] {
            b'`' => j = scan_dsl_body(bytes, j + 1)?,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(j);
                }
                j += 1;
            }
            _ => j += 1,
        }
    }
    None
}

fn scan_dsl_body(bytes: &[u8], body_lo: usize) -> Option<usize> {
    let mut j = body_lo;
    while j < bytes.len() {
        if starts_interp(bytes, j) {
            j = scan_interp_close(bytes, j + 2)? + 1;
            continue;
        }
        if bytes[j] == b'`' {
            return Some(j + 1);
        }
        j += 1;
    }
    None
}

fn starts_interp(bytes: &[u8], i: usize) -> bool {
    i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{'
}

/// Try to parse `inner` as the term-shape grammar:
///   `&.field`         — focal + field
///   `IDENT`           — read
///   `IDENT?`          — bind
///   `IDENT.field`     — read + dot
///   `IDENT?.field`    — bind + dot (rejected as illegal upstream)
///
/// Returns `Some((mode, field, name, is_focal))` on full match, `None` if
/// the inner text contains anything else (e.g. backticks, parens, spaces
/// other than around the stem alone). `None` triggers the SubPipe arm.
fn parse_term_shape(inner: &str) -> Option<(InterpMode, Option<Arc<str>>, Arc<str>, bool)> {
    // No surrounding whitespace allowed in term-shape (`${ X }` is op-shape
    // by contract — author writes `${X}` for term-shape).
    if inner.is_empty() {
        return None;
    }
    let bytes = inner.as_bytes();
    let mut j: usize;

    let (stem_str, is_focal): (Arc<str>, bool) = if bytes[0] == b'&' {
        j = 1;
        (Arc::<str>::from("&"), true)
    } else if bytes[0].is_ascii_alphabetic() || bytes[0] == b'_' {
        j = 1;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        (Arc::<str>::from(&inner[..j]), false)
    } else {
        return None;
    };

    let mode = if j < bytes.len() && bytes[j] == b'?' {
        j += 1;
        InterpMode::Bind
    } else {
        InterpMode::Read
    };

    let mut field: Option<Arc<str>> = None;
    if j < bytes.len() && bytes[j] == b'.' {
        j += 1;
        if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
            let f_lo = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            field = Some(Arc::<str>::from(&inner[f_lo..j]));
        } else {
            return None;
        }
    }

    if j != bytes.len() {
        return None;
    }
    Some((mode, field, stem_str, is_focal))
}
