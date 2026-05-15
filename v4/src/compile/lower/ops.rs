//! Lang-layer wrappers around `pipeline.rs` Components.
//!
//! Each `OperatorDef` here:
//!   • declares its slot spec (`paren_args`, `dsl_body`, `brace_block`, `flow`)
//!   • optionally parses its own DSL grammar (`parse_dsl`)
//!   • in `lower()`: substitutes `${X}` from `LowerCtx::bindings`,
//!     constructs the lowered Component, returns `Pipe<Cursor>`.
//!
//! No grammar/CST imports. No tree-sitter. The CST walker calls
//! `Registry::lower(name, …)` and the dispatch lands here.

use std::sync::Arc;

use ast_grep_config::{DeserializeEnv, SerializableRuleCore};
use effect_runtime::v2::Pipe;

use crate::chan::{Next, NextQ};
use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{
    ArgKind, ArgSig, BlockShape, DslBinder, DslBody, DslShape, OperatorDef,
};
use crate::compile::lower::value::{run_once_const, Value};
use crate::fact::{FactRead, FactWrite};
use crate::pipeline::{GlobComponent, StrConstComponent, StrTemplateComponent};
use crate::rule::Rule;
use crate::term::Term;
use crate::v2_ops::{
    AstNmComponent, AstYamlComponent, CollectComponent, CollectMode, CommentComponent, FsComponent,
    JsonComponent, PathComponent, PrintComponent, ReComponent, ReadComponent,
    RenderMarkdownComponent, RepoComponent, RevComponent, ShBangComponent, ShComponent,
    SplitComponent, VoidComponent, WriteCursorComponent, WriteFileComponent, WriteFilePath,
    WriteMode,
};
use crate::Cursor;
use effect_runtime::v2::ByteRange;

// ─── str ──────────────────────────────────────────────────────────────────

pub struct StrDef;

impl OperatorDef for StrDef {
    fn name(&self) -> &'static str {
        "str"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl.expect("str: dsl body present (validate)");
        if body.interps.is_empty() {
            return Ok(Pipe::new().step(Arc::new(StrConstComponent {
                literal: body.raw.clone(),
            })));
        }
        // Runtime template: ${X} resolves per-cursor against c.terms at
        // render time. Unbound terms emit `term/unbound-at-interp` and
        // splice empty. Compile-time `LowerCtx::bindings` is no longer
        // consulted; binding flows through cursor.terms (the runtime).
        let mut interps = body.interps.clone();
        interps.sort_by_key(|i| i.range.lo);
        Ok(Pipe::new().step(Arc::new(StrTemplateComponent {
            raw: body.raw.clone(),
            interps: Arc::new(interps),
        })))
    }
}

// ─── path ─────────────────────────────────────────────────────────────────

pub struct PathDef;

impl OperatorDef for PathDef {
    fn name(&self) -> &'static str {
        "path"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        false
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &["FS"]
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let mut comp = PathComponent::new(ctx.root.clone());
        if let Some(body) = dsl {
            let mut interps = body.interps.clone();
            interps.sort_by_key(|i| i.range.lo);
            comp = comp.with_template(body.raw.clone(), interps);
        }
        if let Some(s) = &ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// ─── rule ─────────────────────────────────────────────────────────────────

pub struct RuleDef;

const RULE_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom,
        name: "name",
        doc: "rule + sink table name",
        required: true,
    },
    ArgSig {
        // Variadic(Any) so `rule(:name, COL_A, COL_B?)` accepts the two
        // bareword-desugar shapes:
        //   COL  → Value::Pipe(term(:COL))      (declares output column;
        //                                        runtime read on each row)
        //   COL? → Value::Pipe(term_bind(:COL)) (declares + introduces the
        //                                        column at run time)
        // Plain `Value::Atom` strings are still accepted for backward shape.
        kind: ArgKind::Variadic(&ArgKind::Any),
        name: "cols",
        doc: "sink columns",
        required: false,
    },
];

impl OperatorDef for RuleDef {
    fn name(&self) -> &'static str {
        "rule"
    }
    fn paren_args(&self) -> &[ArgSig] {
        RULE_SPEC
    }
    fn brace_block(&self) -> Option<BlockShape> {
        Some(BlockShape::Pipe)
    }
    fn brace_block_required(&self) -> bool {
        false
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        flow: Option<Value>,
        args: &[Value],
        block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // Default chain_pos is 0 (head-of-pipe) for tests / any caller
        // that doesn't pass position info. The walker uses
        // `lower_with_chain` to thread real chain_pos.
        self.lower_with_chain(ctx, flow, args, block, dsl, 0)
    }

    /// Three-state dispatch on chain position × block presence:
    ///
    ///   chain_pos == 0 && block.is_none()  → pure decl. `store.declare`
    ///       for its side-effect, return an empty `Pipe` (zero steps).
    ///       Cursor seed flows through nothing → zero rows; the table
    ///       is registered with its declared columns.
    ///
    ///   chain_pos >= 1 && block.is_none()  → sink position. Splice a
    ///       `FactWrite` ONLY. No `store.declare` — the rule must have
    ///       been declared earlier by a head-of-pipe `rule(:name, …);`.
    ///       Re-declaring with mismatched cols would panic; the
    ///       discipline is "cols live on the decl; sink form references
    ///       by name only".
    ///
    ///   block.is_some()  → bodied form. `store.declare` + body steps +
    ///       `FactWrite` (the existing behavior).
    fn lower_with_chain(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
        chain_pos: usize,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let name = match &args[0] {
            Value::Atom(s) => s.clone(),
            _ => unreachable!("validate ensured Atom"),
        };

        // Sink position, no body: pure FactWrite, no declare. Discards
        // any extra positional args (cols don't apply at the sink site).
        if chain_pos >= 1 && block.is_none() {
            return Ok(Pipe::new().step(Arc::new(FactWrite::new(ctx.store.clone(), name))));
        }

        // Head-of-pipe forms (decl-only or bodied) collect col args and
        // declare. Plain atoms become declared sink columns; pipe args
        // (the ALL_CAPS bareword desugar — `NAME` / `NAME?`) walk
        // through `PipeIntrospect` to recover the term names declared
        // by `Term::bind` / `Term::read` steps inside the sub-pipe.
        use crate::sprf_introspect::PipeIntrospect;
        let mut col_strings: Vec<String> = Vec::with_capacity(args.len().saturating_sub(1));
        for a in &args[1..] {
            match a {
                Value::Atom(s) => col_strings.push(s.to_string()),
                Value::Pipe(p) => {
                    for n in p.binds_terms() {
                        col_strings.push(n.to_string());
                    }
                    for n in p.reads_terms() {
                        col_strings.push(n.to_string());
                    }
                }
            }
        }
        let cols: Vec<&str> = col_strings.iter().map(|s| s.as_str()).collect();

        // Pure decl: declare for side-effect, return empty Pipe so the
        // cursor seed has nothing to drain into.
        if block.is_none() {
            ctx.store.declare(&name, &cols);
            return Ok(Pipe::new());
        }

        // Bodied form: declare + register the rule. The body executes
        // as part of declaration only when its column list is wholly
        // binders (TERM?) — those are the rows the body itself
        // produces. With any grounded `:atom` column the body expects
        // caller-supplied values, so it stays dormant until an apply.
        let body = block.unwrap();
        let rule = Rule::new(name.clone(), ctx.store.clone(), name, &cols, body);
        ctx.register_rule(rule.clone());

        // A column came from a `:name` atom argument → its `Value::Atom`
        // was stringified into `col_strings`. A column came from a
        // `NAME?` bareword desugar → its `Value::Pipe` produced a
        // `term_bind` step (binds_terms non-empty). We re-walk the
        // original args to recover which shape supplied each col.
        let auto_run = args[1..].iter().all(|a| match a {
            Value::Atom(_) => false,
            Value::Pipe(p) => !p.binds_terms().is_empty(),
        });
        if auto_run {
            Ok(rule.into_pipe())
        } else {
            Ok(Pipe::new())
        }
    }
}

// ─── fact ─────────────────────────────────────────────────────────────────
// One op that dispatches by argument binding-mode at lower time:
//   fact(:t, ${A}, ${B})    all bound  → INSERT (FactWrite)
//   fact(:t, ${A?}, ${B?})  all unbound → SELECT * (FactRead, drain+subscribe)
//   fact(:t, ${A}, ${B?})   mixed       → SELECT B WHERE col0=A
// Args carry binding mode through the bareword desugar in walk.rs:
//   COL  → Value::Pipe with `term(:COL)` step  (Read)
//   COL? → Value::Pipe with `term_bind(:COL)` step (Bind)
// Plain `Value::Atom` is treated as a literal bound value.

pub struct FactDef;

const FACT_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom,
        name: "table",
        doc: "fact table name",
        required: true,
    },
    ArgSig {
        kind: ArgKind::Variadic(&ArgKind::Any),
        name: "cols",
        doc: "column args; bound (X / :a) = filter or insert value, unbound (X?) = select target",
        required: false,
    },
];

impl OperatorDef for FactDef {
    fn name(&self) -> &'static str {
        "fact"
    }
    fn paren_args(&self) -> &[ArgSig] {
        FACT_SPEC
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        use crate::sprf_introspect::PipeIntrospect;

        let table = match &args[0] {
            Value::Atom(s) => s.clone(),
            _ => {
                return Err(LowerError::Unknown(
                    "fact: first arg must be a :table atom".into(),
                ))
            }
        };
        let col_args = &args[1..];

        // Classify each col arg as Bound (Read/Atom) or Unbound (Bind).
        #[derive(Debug)]
        enum ColMode {
            BoundLiteral(Arc<str>),
            BoundRead(Arc<str>),
            Unbound(Arc<str>),
        }
        let modes: Vec<ColMode> = col_args
            .iter()
            .map(|v| -> Result<ColMode, LowerError> {
                match v {
                    Value::Atom(s) => Ok(ColMode::BoundLiteral(s.clone())),
                    Value::Pipe(p) => {
                        let binds = p.binds_terms();
                        let reads = p.reads_terms();
                        if let Some(name) = binds.first() {
                            Ok(ColMode::Unbound(name.clone()))
                        } else if let Some(name) = reads.first() {
                            Ok(ColMode::BoundRead(name.clone()))
                        } else {
                            // No term in the pipe (e.g. a backtick literal).
                            // Treat as a bound literal value with a synthetic
                            // anonymous name. Today FactRead/FactWrite ignore
                            // literal-value cols at runtime; surface a stable
                            // placeholder so dispatch still works.
                            Ok(ColMode::BoundLiteral(Arc::<str>::from("$lit")))
                        }
                    }
                }
            })
            .collect::<Result<_, _>>()?;

        let any_unbound = modes.iter().any(|m| matches!(m, ColMode::Unbound(_)));

        if !any_unbound {
            // Pure write — table + (no col surface yet on FactWrite).
            return Ok(Pipe::new().step(Arc::new(FactWrite::new(ctx.store.clone(), table))));
        }

        // Read shape: pick first bound col as key_term (if any), project the
        // unbound names. With no bound cols, key_term is "" (drain+subscribe
        // semantics — but FactRead today requires a key. Surface the gap as
        // a clear error rather than silently mismatching.)
        let key_term: Arc<str> = modes
            .iter()
            .find_map(|m| match m {
                ColMode::BoundRead(n) | ColMode::BoundLiteral(n) => Some(n.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                LowerError::Unknown(
                    "fact: SELECT * (all-unbound args) not yet wired; \
             at least one bound col is required as the join key"
                        .into(),
                )
            })?;
        let project: Vec<String> = modes
            .iter()
            .filter_map(|m| match m {
                ColMode::Unbound(n) => Some(n.to_string()),
                _ => None,
            })
            .collect();
        let project_refs: Vec<&str> = project.iter().map(|s| s.as_str()).collect();
        Ok(Pipe::new().step(Arc::new(FactRead::new(
            ctx.store.clone(),
            table,
            key_term,
            &project_refs,
        ))))
    }
}

// ─── fact_read (legacy) ───────────────────────────────────────────────────
// Kept registered for now; the unified `fact` op is the preferred surface.

pub struct FactReadDef;

const FACT_READ_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom,
        name: "table",
        doc: "fact table name",
        required: true,
    },
    ArgSig {
        kind: ArgKind::Atom,
        name: "key_term",
        doc: "cursor term used as join key",
        required: true,
    },
    ArgSig {
        kind: ArgKind::Variadic(&ArgKind::Atom),
        name: "project",
        doc: "projected col names",
        required: false,
    },
];

impl OperatorDef for FactReadDef {
    fn name(&self) -> &'static str {
        "fact_read"
    }
    fn paren_args(&self) -> &[ArgSig] {
        FACT_READ_SPEC
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let atom = |i: usize| -> Arc<str> {
            match &args[i] {
                Value::Atom(s) => s.clone(),
                _ => unreachable!("validate ensured Atom"),
            }
        };
        let table = atom(0);
        let key_term = atom(1);
        let mut project_cols: Vec<String> = Vec::with_capacity(args.len().saturating_sub(2));
        for a in &args[2..] {
            match a {
                Value::Atom(s) => project_cols.push(s.to_string()),
                _ => unreachable!("validate ensured Atom"),
            }
        }
        let project_refs: Vec<&str> = project_cols.iter().map(|s| s.as_str()).collect();
        Ok(Pipe::new().step(Arc::new(FactRead::new(
            ctx.store.clone(),
            table,
            key_term,
            &project_refs,
        ))))
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────

fn atom_arg(args: &[Value], i: usize) -> Arc<str> {
    match &args[i] {
        Value::Atom(s) => s.clone(),
        _ => unreachable!("validate ensured Atom"),
    }
}

// ─── fs ───────────────────────────────────────────────────────────────────

pub struct FsDef;

/// `fs` is a pure path query. Bare form emits one cursor per file under
/// `ctx.root`, `cursor.value = path string`. `fs(glob`...`)` is the
/// source-side filter form: only static single-step glob filters are
/// accepted here, so rejected paths never enter the runtime queue.
const FS_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Pipe,
    name: "filter",
    doc: "optional static source path filter, currently glob`...`",
    required: false,
}];

impl OperatorDef for FsDef {
    fn name(&self) -> &'static str {
        "fs"
    }
    fn paren_args(&self) -> &[ArgSig] {
        FS_SPEC
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &["FS"]
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // TODO: LowerCtx batch-size knob; hardcoded for now.
        let mut comp = FsComponent::new(ctx.root.clone(), 1024);
        if let Some(filter) = args.first() {
            comp = comp.with_source_filter(source_path_filter(filter)?);
        }
        if let Some(s) = &ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        if let Some(c) = &ctx.config {
            comp = comp.with_config(c.clone());
        }
        if let Some(t) = &ctx.telemetry {
            comp = comp.with_telemetry(t.fs.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

fn source_path_filter(
    value: &Value,
) -> Result<Arc<dyn Fn(&str) -> bool + Send + Sync>, LowerError> {
    let Value::Pipe(pipe) = value else {
        return Err(LowerError::Unknown(
            "fs: source filter must be a pipe, e.g. fs(glob`**/*.rs`)".into(),
        ));
    };
    if pipe.steps.len() != 1 {
        return Err(LowerError::Unknown(
            "fs: source filter must be one static step, e.g. fs(glob`**/*.rs`)".into(),
        ));
    }
    let step = pipe.steps[0].clone();
    let Some(_) = step
        .describe()
        .and_then(|d| d.downcast_ref::<GlobComponent>())
    else {
        return Err(LowerError::Unknown(
            "fs: source filter currently supports only static glob`...`".into(),
        ));
    };
    Ok(Arc::new(move |path: &str| {
        step.describe()
            .and_then(|d| d.downcast_ref::<GlobComponent>())
            .map(|glob| glob.matches_value(path))
            .unwrap_or(false)
    }))
}

// ─── glob ─────────────────────────────────────────────────────────────────

pub struct GlobDef;

/// `glob` is a pure text matcher over `cursor.value`. Backtick body
/// only — the paren-atom form was removed during the substrate
/// cleanup. Use `glob`**/*.rs`` etc.
const GLOB_SPEC: &[ArgSig] = &[];

/// Scan a regex body for `(?P<NAME>...)` named groups. Returns each
/// captured name with the byte range of the full sigil.
fn scan_re_named_groups(raw: &str) -> Vec<DslBinder> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 4 < bytes.len() {
        // look for `(?P<` or `(?<`  (Rust regex accepts both shapes)
        let lo = i;
        let prefix_len = if bytes[i] == b'('
            && bytes[i + 1] == b'?'
            && bytes[i + 2] == b'P'
            && bytes[i + 3] == b'<'
        {
            4
        } else if bytes[i] == b'(' && bytes[i + 1] == b'?' && bytes[i + 2] == b'<' {
            3
        } else {
            i += 1;
            continue;
        };
        let name_lo = i + prefix_len;
        let mut j = name_lo;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        if j > name_lo && j < bytes.len() && bytes[j] == b'>' {
            let name = &raw[name_lo..j];
            out.push(DslBinder {
                name: Arc::<str>::from(name),
                range: ByteRange {
                    lo: lo as u32,
                    hi: (j + 1) as u32,
                },
            });
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

impl OperatorDef for GlobDef {
    fn name(&self) -> &'static str {
        "glob"
    }
    fn paren_args(&self) -> &[ArgSig] {
        GLOB_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        true
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl.ok_or_else(|| {
            LowerError::Unknown(
                "glob: pattern required — use backtick body, e.g. glob`**/*.rs`".into(),
            )
        })?;
        // SubPipe-bearing bodies route through the dynamic Component.
        // Per-cursor: drain inner pipes, render body, re-translate to
        // regex, match. Term-only bodies stay on the static fast path.
        if crate::template::has_subpipe(&body.interps) {
            let mut interps = body.interps.clone();
            interps.sort_by_key(|i| i.range.lo);
            let mut comp = crate::pipeline::GlobDynamicComponent::new(body.raw.clone(), interps);
            if let Some(s) = &_ctx.sprf_store {
                comp = comp.with_sprf_store(s.clone());
            }
            return Ok(Pipe::new().step(Arc::new(comp)));
        }
        let regex_src = glob_body_to_regex(body)?;
        let re = regex::Regex::new(&regex_src).map_err(|e| {
            LowerError::Unknown(format!(
                "glob: regex compile failure for translated pattern {regex_src:?}: {e}"
            ))
        })?;
        let mut comp = GlobComponent::new(re);
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

/// Translate a glob body (with universal `${X?}` carveouts) to an
/// anchored Rust regex string.
///
/// Glob → regex table:
///   `*`        → `[^/]*`
///   `**`       → `.*`
///   `?`        → `[^/]`
///   `[abc]`    → `[abc]`        (passed through, glob char-class shape)
///   `{a,b}`    → `(?:a|b)`      (alternation)
///   literal    → escaped (regex::escape one char at a time)
///   `${X?}`    → `(?P<X>[^/]*)` (single-segment named capture)
///   `${X}`     → rejected for now (`lower/glob-read-not-supported`)
///   `$$${...}` → rejected: triple-dollar is ast-grep only per universal rule.
///                Use `**/${X?}` for multi-segment capture in glob.
///   `${...}`   → SubPipe form: handled by `GlobDynamicComponent` path
///                in `GlobDef::lower`; never reaches this translator.
///
/// The result is end-anchored via trailing `$` (so the pattern's tail
/// must align with the value's end), but not start-anchored — the regex
/// engine walks for the leftmost match. This mirrors globset's
/// "match the path's basename / trailing segments" semantics: a bare
/// `${STEM?}.${EXT?}` matches the trailing basename of a path-shape
/// value, while `**/*.rs` matches `^.*/[^/]*\.rs$`-style suffix.
/// Interps come pre-extracted by the universal carveout pre-pass on
/// `DslBody.interps`; the literal walk skips their byte ranges (and the
/// `$$$` prefix when present).
fn glob_body_to_regex(body: &DslBody) -> Result<String, LowerError> {
    use crate::compile::lower::op_def::{InterpKind, InterpMode};

    let raw = body.raw.as_ref();
    let bytes = raw.as_bytes();

    // Index interps by their lo offset for O(1) lookup at the literal
    // walk's current position. Each entry remembers whether this interp
    // is preceded by `$$$` (multi-segment greedy).
    let mut by_lo: std::collections::BTreeMap<u32, &crate::compile::lower::op_def::DslInterp> =
        std::collections::BTreeMap::new();
    for it in &body.interps {
        by_lo.insert(it.range.lo, it);
    }

    let mut out = String::with_capacity(raw.len() + 8);

    let mut i: usize = 0;
    while i < bytes.len() {
        // Per universal rule: `$$${...}` is ast-grep ONLY. In glob,
        // surface a fatal diag so the author switches to `**/${X?}` for
        // multi-segment captures. Single-segment is `${X?}` as usual.
        if i + 3 < bytes.len()
            && bytes[i] == b'$'
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{'
        {
            return Err(LowerError::Unknown(
                "glob: `$$${...}` is ast-grep only; use `**/${NAME?}` for \
                 multi-segment captures in glob".into(),
            ));
        }
        if let Some(interp) = by_lo.get(&(i as u32)) {
            match &interp.kind {
                InterpKind::Term { mode, field } => {
                    if field.is_some() {
                        return Err(LowerError::Unknown(format!(
                            "glob: dot-access in body not supported (`${{{}.{}}}`); \
                             use a separate term step",
                            interp.name,
                            field.as_ref().map(|f| f.as_ref()).unwrap_or("")
                        )));
                    }
                    match mode {
                        InterpMode::Bind => {
                            // `${X?}` → single-segment named capture.
                            out.push_str("(?P<");
                            out.push_str(interp.name.as_ref());
                            out.push_str(">[^/]*)");
                        }
                        InterpMode::Read => {
                            return Err(LowerError::Unknown(format!(
                                "lower/glob-read-not-supported: `${{{}}}` Read \
                                 mode in glob body not supported in this slice",
                                interp.name
                            )));
                        }
                    }
                }
                InterpKind::SubPipe { .. } => {
                    // Unreachable in the static path: GlobDef::lower
                    // routes SubPipe-bearing bodies to GlobDynamicComponent
                    // before this translator runs. Defensive bail-out.
                    return Err(LowerError::Unknown(
                        "glob: internal — SubPipe interp reached static \
                         translator (should have routed to dynamic path)"
                            .into(),
                    ));
                }
            }
            i = interp.range.hi as usize;
            continue;
        }

        // Literal walk: glob metachar translation table.
        let b = bytes[i];
        match b {
            b'*' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    out.push_str(".*");
                    i += 2;
                } else {
                    out.push_str("[^/]*");
                    i += 1;
                }
            }
            b'?' => {
                out.push_str("[^/]");
                i += 1;
            }
            b'[' => {
                // Pass through char-class verbatim until matching `]`.
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b']' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(LowerError::Unknown("glob: unterminated `[`".into()));
                }
                out.push_str(&raw[i..=j]);
                i = j + 1;
            }
            b'{' => {
                // `{a,b,c}` → `(?:a|b|c)`. No nested `{...}` recognized.
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != b'}' {
                    j += 1;
                }
                if j >= bytes.len() {
                    return Err(LowerError::Unknown("glob: unterminated `{`".into()));
                }
                out.push_str("(?:");
                let inner = &raw[i + 1..j];
                let parts: Vec<&str> = inner.split(',').collect();
                for (k, part) in parts.iter().enumerate() {
                    if k > 0 {
                        out.push('|');
                    }
                    out.push_str(&regex::escape(part));
                }
                out.push(')');
                i = j + 1;
            }
            _ => {
                // Single-char escape.
                out.push_str(&regex::escape(&raw[i..i + 1]));
                i += 1;
            }
        }
    }
    out.push('$');
    Ok(out)
}

// ─── ast ──────────────────────────────────────────────────────────────────
// `ast(:lang)\`pattern\`` — lang as paren-arg atom, pattern in dsl body.
// Default lang `:rs` if omitted.

pub struct AstDef;

const AST_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "lang",
    doc: "language atom (:rs, :c, :cpp, :ts, :tsx, :js, :py, :go, :java)",
    required: false,
}];

impl OperatorDef for AstDef {
    fn name(&self) -> &'static str {
        "ast"
    }
    fn paren_args(&self) -> &[ArgSig] {
        AST_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &["LO", "HI"]
    }

    /// ast-grep metavars: `$NAME`, `$$$REST`. Both bind at runtime.
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        scan_ast_metavars(raw)
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let lang_atom: Option<&str> = args.first().and_then(|v| match v {
            Value::Atom(s) => Some(s.as_ref()),
            _ => None,
        });
        let lang = parse_lang_atom(lang_atom.unwrap_or("rs")).ok_or_else(|| {
            LowerError::Unknown(format!(
                "ast: unknown lang atom :{} (try :rs, :c, :cpp, :ts, :tsx, :js, :py, :go, :java)",
                lang_atom.unwrap_or("?")
            ))
        })?;
        let body = dsl.ok_or_else(|| {
            LowerError::Unknown(
                "ast: dsl body required (e.g. ast(:rs)`fn ${NAME?}($$ARGS)`)".into(),
            )
        })?;
        // SubPipe-bearing bodies route to AstNmComponent's dyn_body slow
        // path (per-cursor ast-grep Pattern recompile). The static
        // Pattern built here is unused on that path; we stamp a `*`
        // placeholder so construction doesn't fail on a body that's
        // unparseable as an ast-grep pattern in isolation.
        let static_src: String = if crate::template::has_subpipe(&body.interps) {
            // Reuse the body verbatim — most ast-grep patterns parse
            // even with placeholder text; failure here is acceptable
            // since the dynamic path replaces the Pattern per cursor.
            // If `Pattern::new` panics on the stamp, the dynamic path
            // is still mandatory and we fall back via a future bd note.
            body.raw.to_string()
        } else {
            body.raw.to_string()
        };
        let mut comp = AstNmComponent::new(static_src, lang).with_root(_ctx.root.clone());
        if crate::template::has_subpipe(&body.interps) {
            let mut interps = body.interps.clone();
            interps.sort_by_key(|i| i.range.lo);
            comp = comp.with_dyn_body(body.raw.clone(), interps);
        }
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        if let Some(c) = &_ctx.config {
            comp = comp.with_config(c.clone());
        }
        if let Some(t) = &_ctx.telemetry {
            comp = comp.with_telemetry(t.ast.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// `ast_yaml(:lang)`rule: ...`` — ast-grep RuleCore YAML body.
// V4 uses the normal paren atom + backtick body shape instead of V3's
// bracket form `ast_yaml[lang](...)`.

pub struct AstYamlDef;

impl OperatorDef for AstYamlDef {
    fn name(&self) -> &'static str {
        "ast_yaml"
    }
    fn paren_args(&self) -> &[ArgSig] {
        AST_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &["LO", "HI"]
    }

    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        let (caps, positions) = collect_ast_yaml_carveout_names_and_positions(raw);
        let mut out: Vec<DslBinder> = positions
            .into_iter()
            .map(|(name, range)| DslBinder {
                name,
                range: ByteRange {
                    lo: range.start as u32,
                    hi: range.end as u32,
                },
            })
            .collect();
        for binder in scan_ast_metavars(raw) {
            if !out.iter().any(|b| b.name.as_ref() == binder.name.as_ref()) {
                out.push(binder);
            }
        }
        for cap in caps {
            if !out.iter().any(|b| b.name.as_ref() == cap.as_ref()) {
                out.push(DslBinder {
                    name: cap,
                    range: ByteRange { lo: 0, hi: 0 },
                });
            }
        }
        out
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let lang_atom: Option<&str> = args.first().and_then(|v| match v {
            Value::Atom(s) => Some(s.as_ref()),
            _ => None,
        });
        let lang = parse_lang_atom(lang_atom.unwrap_or("rs")).ok_or_else(|| {
            LowerError::Unknown(format!(
                "ast_yaml: unknown lang atom :{} (try :rs, :c, :cpp)",
                lang_atom.unwrap_or("?")
            ))
        })?;
        let body = dsl.ok_or_else(|| {
            LowerError::Unknown(
                "ast_yaml: dsl body required (e.g. ast_yaml(:rs)`pattern: \"fn $N() {}\"`)".into(),
            )
        })?;
        if crate::template::has_subpipe(&body.interps) {
            return Err(LowerError::Unknown(
                "ast_yaml: subpipe interpolation inside YAML is not supported yet".into(),
            ));
        }

        let rewritten = rewrite_ast_yaml_carveouts(body.raw.as_ref());
        let value: serde_yaml::Value = serde_yaml::from_str(&rewritten)
            .map_err(|e| LowerError::Unknown(format!("ast_yaml YAML invalid: {e}")))?;
        let core_value = wrap_ast_yaml_under_rule_if_bare(value);
        let core: SerializableRuleCore = serde_yaml::from_value(core_value).map_err(|e| {
            LowerError::Unknown(format!("ast_yaml RuleCore deserialise failed: {e}"))
        })?;
        let env = DeserializeEnv::new(lang);
        let rule_core = core
            .get_matcher(env)
            .map_err(|e| LowerError::Unknown(format!("ast_yaml rule build failed: {e}")))?;

        let (mut bound_caps, _) = collect_ast_yaml_carveout_names_and_positions(body.raw.as_ref());
        for v in rule_core.defined_vars() {
            let s: Arc<str> = Arc::from(v);
            if !bound_caps.iter().any(|c| c.as_ref() == s.as_ref()) {
                bound_caps.push(s);
            }
        }

        let mut comp =
            AstYamlComponent::new(lang, rule_core, bound_caps).with_root(_ctx.root.clone());
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        if let Some(c) = &_ctx.config {
            comp = comp.with_config(c.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

fn parse_lang_atom(s: &str) -> Option<ast_grep_language::SupportLang> {
    use ast_grep_language::SupportLang as L;
    Some(match s {
        "rs" | "rust" => L::Rust,
        // ast-grep's C grammar is stricter than its C++ one; the C++
        // parser cleanly handles C source too. v4-bench picks Cpp for
        // both. Match that so the kernel walks the same on both paths.
        "c" | "cpp" | "c++" | "cc" => L::Cpp,
        "ts" | "typescript" => L::TypeScript,
        "tsx" => L::Tsx,
        "js" | "javascript" => L::JavaScript,
        "py" | "python" => L::Python,
        "go" | "golang" => L::Go,
        "java" => L::Java,
        _ => return None,
    })
}

/// Scan an ast-grep body for `$NAME` and `$$$REST` metavars. Both are
/// runtime binders.
fn scan_ast_metavars(raw: &str) -> Vec<DslBinder> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let lo = i;
            let mut j = i + 1;
            // skip up to two extra `$` for the $$$REST form
            while j < bytes.len() && bytes[j] == b'$' && j - i < 3 {
                j += 1;
            }
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                let name_lo = j;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                let name = &raw[name_lo..j];
                out.push(DslBinder {
                    name: Arc::<str>::from(name),
                    range: ByteRange {
                        lo: lo as u32,
                        hi: j as u32,
                    },
                });
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn rewrite_ast_yaml_carveouts(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < bytes.len() {
        let multi = bytes[i] == b'$'
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{';
        let single = !multi && bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{';
        if multi || single {
            let inner_start = if multi { i + 4 } else { i + 2 };
            let mut j = inner_start;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j >= bytes.len() {
                out.push(bytes[i] as char);
                i += 1;
                continue;
            }
            let inner = src[inner_start..j].trim();
            let name = inner.strip_suffix('?').unwrap_or(inner);
            if multi {
                out.push_str("$$$");
            } else {
                out.push('$');
            }
            out.push_str(name);
            i = j + 1;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn collect_ast_yaml_carveout_names_and_positions(
    src: &str,
) -> (Vec<Arc<str>>, Vec<(Arc<str>, std::ops::Range<usize>)>) {
    let bytes = src.as_bytes();
    let mut names: Vec<Arc<str>> = Vec::new();
    let mut positions: Vec<(Arc<str>, std::ops::Range<usize>)> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let multi = bytes[i] == b'$'
            && i + 3 < bytes.len()
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{';
        let single = !multi && bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{';
        if multi || single {
            let token_start = i;
            let inner_start = if multi { i + 4 } else { i + 2 };
            let mut j = inner_start;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j >= bytes.len() {
                i += 1;
                continue;
            }
            let inner = src[inner_start..j].trim();
            let name = inner.strip_suffix('?').unwrap_or(inner);
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                let arc: Arc<str> = Arc::from(name);
                if !names.iter().any(|s| s.as_ref() == arc.as_ref()) {
                    names.push(arc.clone());
                }
                positions.push((arc, token_start..(j + 1)));
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    (names, positions)
}

fn wrap_ast_yaml_under_rule_if_bare(value: serde_yaml::Value) -> serde_yaml::Value {
    const ENVELOPE_KEYS: &[&str] = &["rule", "constraints", "utils", "transform", "fix"];
    if let serde_yaml::Value::Mapping(m) = &value {
        let has_envelope = ENVELOPE_KEYS
            .iter()
            .any(|k| m.contains_key(serde_yaml::Value::String((*k).into())));
        if has_envelope {
            return value;
        }
    }
    let mut wrapper = serde_yaml::Mapping::new();
    wrapper.insert(serde_yaml::Value::String("rule".into()), value);
    serde_yaml::Value::Mapping(wrapper)
}

// ─── json ─────────────────────────────────────────────────────────────────
// `json(:fmt)\`{ key: $V }\`` — brace-pattern walk over a parsed
// JSON/YAML/TOML document. fmt = :json (default), :yaml, :toml.

pub struct JsonDef;

const JSON_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "fmt",
    doc: "target format atom (:json, :yaml, :toml). Default :json.",
    required: false,
}];

impl OperatorDef for JsonDef {
    fn name(&self) -> &'static str {
        "json"
    }
    fn paren_args(&self) -> &[ArgSig] {
        JSON_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }

    /// json brace-pattern bindings: `$NAME` only (no braces). The braced
    /// form `${NAME}` is host territory — it's a host-pipe hole that
    /// applies to every dsl body universally. The host pre-scans those
    /// via `default_plain_dsl_parse` and treats them as reads/binds at
    /// the host level. Per-dsl `binders_in_dsl` must NOT see them.
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        scan_bare_dollar_idents(raw)
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        use crate::cst::dsls::json::{JsonDsl, TargetFormat};

        let body = dsl.ok_or_else(|| {
            LowerError::Unknown("json: dsl body required (e.g. json`{ name: $N }`)".into())
        })?;
        let fmt_atom: Option<&str> = args.first().and_then(|v| match v {
            Value::Atom(s) => Some(s.as_ref()),
            _ => None,
        });
        let fmt = match fmt_atom.unwrap_or("json") {
            "json" => TargetFormat::Json,
            "yaml" | "yml" => TargetFormat::Yaml,
            "toml" => TargetFormat::Toml,
            other => {
                return Err(LowerError::Unknown(format!(
                    "json: unknown fmt atom :{} (try :json, :yaml, :toml)",
                    other
                )))
            }
        };
        // SubPipe-bearing bodies route to JsonComponent's dyn_body slow
        // path (per-cursor JsonCompiled rebuild). Build a placeholder
        // static compile when the body has SubPipe present so a future
        // bd note can remove the placeholder once compile_typed accepts
        // a None.
        let has_subpipe = crate::template::has_subpipe(&body.interps);
        let static_src: &[u8] = if has_subpipe {
            // Minimal pattern that JsonDsl can parse — used as the
            // static-path placeholder; the dynamic path replaces it.
            b"{}"
        } else {
            body.raw.as_bytes()
        };
        let compiled = JsonDsl::compile_typed(static_src)
            .map_err(|d| LowerError::Unknown(format!("json: compile failed: {}", d.message)))?;
        let compiled = compiled.with_format(fmt);
        let mut comp = JsonComponent::new(compiled).with_root(_ctx.root.clone());
        if has_subpipe {
            let mut interps = body.interps.clone();
            interps.sort_by_key(|i| i.range.lo);
            comp = comp.with_dyn_body(body.raw.clone(), interps, fmt);
        }
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        if let Some(c) = &_ctx.config {
            comp = comp.with_config(c.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

/// Scan a body for the dsl-internal capture form `$IDENT` (no braces).
/// The braced form `${IDENT}` is a host pipe-hole and is NOT scanned
/// here — the host pre-pass owns it.
fn scan_bare_dollar_idents(raw: &str) -> Vec<DslBinder> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            // skip braced form — host owns `${...}`
            if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                i += 2;
                continue;
            }
            let lo = i;
            let mut j = i + 1;
            if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                let name_lo = j;
                while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                    j += 1;
                }
                let name = &raw[name_lo..j];
                out.push(DslBinder {
                    name: Arc::<str>::from(name),
                    range: ByteRange {
                        lo: lo as u32,
                        hi: j as u32,
                    },
                });
                i = j;
                continue;
            }
        }
        i += 1;
    }
    out
}

// ─── re ───────────────────────────────────────────────────────────────────

pub struct ReDef;

impl OperatorDef for ReDef {
    fn name(&self) -> &'static str {
        "re"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &["LO", "HI", "MATCH"]
    }

    /// Scan the regex body for `(?P<NAME>...)` named groups. Each name
    /// is a runtime binder; the binding graph treats them as bound at
    /// the step of this `re` op.
    fn binders_in_dsl(&self, raw: &str) -> Vec<DslBinder> {
        scan_re_named_groups(raw)
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl.expect("re: dsl body present (validate)");
        // Universal `${X?}` Bind / `${X}` Read carveouts are rewritten
        // into the regex surface here before ReComponent::new sees the
        // body. SubPipe-bearing bodies still route to ReComponent's
        // dyn_body slow path; the static `re` constructed here is a
        // placeholder for that case.
        let static_pat: String = if crate::template::has_subpipe(&body.interps) {
            // Placeholder; dyn_body recompile per cursor is the real
            // pattern. Keep it syntactically valid.
            ".*".into()
        } else {
            re_body_to_regex(body)?
        };
        // Capture names come from BOTH the rewritten regex (which has
        // groups synthesised from `${X?}` Bind interps) and any native
        // `(?P<NAME>)` written by the author. Walk-time `binders_in_dsl`
        // covers binding-graph; this list drives the runtime cursor
        // stamping in `ReComponent`.
        let mut capture_names = scan_re_named_groups(&static_pat);
        for b in scan_re_named_groups(body.raw.as_ref()) {
            if !capture_names.iter().any(|c| c.name.as_ref() == b.name.as_ref()) {
                capture_names.push(b);
            }
        }
        let capture_name_refs: Vec<&str> = capture_names.iter().map(|b| b.name.as_ref()).collect();
        let mut comp = ReComponent::new(&static_pat, &capture_name_refs);
        if crate::template::has_subpipe(&body.interps) {
            let mut interps = body.interps.clone();
            interps.sort_by_key(|i| i.range.lo);
            comp = comp.with_dyn_body(body.raw.clone(), interps);
        }
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

/// Rewrite the `re` body's universal `${...}` carveouts into the regex
/// surface. `${X?}` becomes `(?P<X>.*?)` (the per-DSL default named
/// group); `${X}` Read is rejected at lower time (re does not yet have
/// a per-cursor splice for non-SubPipe Read). Literal text passes through
/// untouched; the regex crate handles its own escapes.
fn re_body_to_regex(body: &DslBody) -> Result<String, LowerError> {
    use crate::compile::lower::op_def::{InterpKind, InterpMode};

    let raw = body.raw.as_ref();
    let bytes = raw.as_bytes();
    let mut interps = body.interps.clone();
    interps.sort_by_key(|i| i.range.lo);

    let mut by_lo: std::collections::BTreeMap<u32, &crate::compile::lower::op_def::DslInterp> =
        std::collections::BTreeMap::new();
    for it in &interps {
        by_lo.insert(it.range.lo, it);
    }

    let mut out = String::with_capacity(raw.len() + 8);
    let mut i: usize = 0;
    while i < bytes.len() {
        // Reject triple-dollar in `re`: it's ast-grep-only per spec.
        if i + 3 < bytes.len()
            && bytes[i] == b'$'
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'$'
            && bytes[i + 3] == b'{'
        {
            return Err(LowerError::Unknown(
                "re: `$$${...}` is ast-grep only; use `${NAME?}` in re bodies".into(),
            ));
        }
        if let Some(interp) = by_lo.get(&(i as u32)) {
            match &interp.kind {
                InterpKind::Term { mode, field } => {
                    if field.is_some() {
                        return Err(LowerError::Unknown(format!(
                            "re: dot-access in body not supported (`${{{}.{}}}`)",
                            interp.name,
                            field.as_ref().map(|f| f.as_ref()).unwrap_or("")
                        )));
                    }
                    match mode {
                        InterpMode::Bind => {
                            out.push_str("(?P<");
                            out.push_str(interp.name.as_ref());
                            out.push_str(">.*?)");
                        }
                        InterpMode::Read => {
                            return Err(LowerError::Unknown(format!(
                                "lower/re-read-not-supported: `${{{}}}` Read mode not \
                                 supported in re bodies (use sub-pipe form `${{ <pipe> }}` \
                                 to splice a value per cursor)",
                                interp.name
                            )));
                        }
                    }
                }
                InterpKind::SubPipe { .. } => {
                    // SubPipe interps are handled by the dyn_body path;
                    // ReDef::lower routes them before this function runs.
                    return Err(LowerError::Unknown(
                        "re: internal — SubPipe interp reached static translator".into(),
                    ));
                }
            }
            i = interp.range.hi as usize;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Ok(out)
}

// ─── term (read) ──────────────────────────────────────────────────────────

const TERM_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "name",
    doc: "term (capture) name",
    required: true,
}];

pub struct TermReadDef;

impl OperatorDef for TermReadDef {
    fn name(&self) -> &'static str {
        "term"
    }
    fn paren_args(&self) -> &[ArgSig] {
        TERM_SPEC
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let name = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(Term::read(name))))
    }
}

// ─── term_bind ────────────────────────────────────────────────────────────

pub struct TermBindDef;

impl OperatorDef for TermBindDef {
    fn name(&self) -> &'static str {
        "term_bind"
    }
    fn paren_args(&self) -> &[ArgSig] {
        TERM_SPEC
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let name = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(Term::bind(name))))
    }
}

// ─── fact_write ───────────────────────────────────────────────────────────

pub struct FactWriteDef;

const FACT_WRITE_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom,
        name: "table",
        doc: "fact table name",
        required: true,
    },
    ArgSig {
        kind: ArgKind::Variadic(&ArgKind::Atom),
        name: "cols",
        doc: "columns the row carries (advisory; FactWrite currently \
              records the whole cursor, columns are validated for arity \
              + LSP only)",
        required: false,
    },
];

impl OperatorDef for FactWriteDef {
    fn name(&self) -> &'static str {
        "fact_write"
    }
    fn paren_args(&self) -> &[ArgSig] {
        FACT_WRITE_SPEC
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // TODO: `FactWrite::new` only takes the table name today. Cols
        // are accepted by the slot spec for arity/LSP but discarded
        // until FactWrite grows column projection.
        let table = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(FactWrite::new(ctx.store.clone(), table))))
    }
}

// ─── Op-classify test wrappers ────────────────────────────────────────────
//
// The fuser consumes per-op classifications (`Op::classify`). The
// surface is one constructor per op type plus `classify()` returning
// `Liftable`. Tests in `liftable_classify_target.rs` construct these
// directly via `new_for_test_*`. The real fuser path threads the same
// classifications from walked OpCalls — once that wiring lands, these
// wrappers also let the fuser stay one constructor away from any
// op without touching the registry.

use super::liftable::{stream_noop, Col, IdKind, Liftable, Op, TermBind};

pub struct FsOp {
    pub glob: Arc<str>,
}

impl FsOp {
    pub fn new_for_test_glob(g: &str) -> Self {
        Self {
            glob: Arc::<str>::from(g),
        }
    }
}

impl Op for FsOp {
    fn classify(&self) -> Liftable {
        Liftable::Stream {
            schema: vec![
                (Col::from("FS"), IdKind::PathId),
            ],
            run: stream_noop,
        }
    }
}

pub struct AstOp {
    pub lang: Arc<str>,
    pub pattern: Arc<str>,
}

impl AstOp {
    pub fn new_for_test(lang: &str, pattern: &str) -> Self {
        Self {
            lang: Arc::<str>::from(lang),
            pattern: Arc::<str>::from(pattern),
        }
    }
}

impl Op for AstOp {
    fn classify(&self) -> Liftable {
        // AST emits a (FILE, LO, HI) tuple plus any captured metavars.
        // The canonical fixed columns are FILE (FileId) and a
        // WhereBytesId for the matched span. Metavar columns are
        // appended by the fuser at compose time from the
        // TermFlowGraph; this skeleton fixes the two mandatory cols
        // the test checks.
        Liftable::Stream {
            schema: vec![
                (Col::from("FILE"), IdKind::FileId),
                (Col::from("LO"), IdKind::WhereBytesId),
                (Col::from("HI"), IdKind::WhereBytesId),
            ],
            run: stream_noop,
        }
    }
}

pub struct ReOp {
    pub pattern: Arc<str>,
}

impl ReOp {
    pub fn new_for_test(pat: &str) -> Self {
        Self {
            pattern: Arc::<str>::from(pat),
        }
    }
}

impl Op for ReOp {
    fn classify(&self) -> Liftable {
        // Lifts to SQL via the `sprf_re_match` UDF over an existing
        // string column. The project field is left empty here because
        // the fuser fills the destination column from the
        // TermFlowGraph; the udfs entry is the actual lifting signal.
        Liftable::Pure {
            project: Vec::new(),
            filter: Some(format!("sprf_re_match({}, _input)", self.pattern)),
            udfs: vec![Arc::<str>::from("sprf_re_match")],
        }
    }
}

pub struct WhereOp {
    pub predicate: Arc<str>,
}

impl WhereOp {
    pub fn new_for_test(pred: &str) -> Self {
        Self {
            predicate: Arc::<str>::from(pred),
        }
    }
}

impl Op for WhereOp {
    fn classify(&self) -> Liftable {
        Liftable::Pure {
            project: Vec::new(),
            filter: Some(self.predicate.to_string()),
            udfs: Vec::new(),
        }
    }
}

/// Rule-call wrapper. Encodes the apply-vs-bare-vs-force distinction
/// so the fuser can decide whether to treat as a join (bare) or skip
/// (apply / force).
pub struct RuleCallOp {
    pub rule: Arc<str>,
    pub apply: bool,
    pub force: bool,
    pub args: Vec<Arc<str>>,
}

impl RuleCallOp {
    pub fn new_for_test_bare(rule: &str, args: &[&str]) -> Self {
        Self {
            rule: Arc::<str>::from(rule),
            apply: false,
            force: false,
            args: args.iter().map(|s| Arc::<str>::from(*s)).collect(),
        }
    }
    pub fn new_for_test_apply(rule: &str, args: &[&str]) -> Self {
        Self {
            rule: Arc::<str>::from(rule),
            apply: true,
            force: false,
            args: args.iter().map(|s| Arc::<str>::from(*s)).collect(),
        }
    }
}

impl Op for RuleCallOp {
    fn classify(&self) -> Liftable {
        if self.apply {
            return Liftable::Opaque;
        }
        let table = Arc::<str>::from(format!("{}_facts", self.rule));
        let binds: Vec<(Col, TermBind)> = self
            .args
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let col = Arc::<str>::from(format!("col{i}"));
                let raw = a.as_ref();
                let bind = if let Some(name) = raw.strip_suffix('?') {
                    TermBind::Bind(Arc::<str>::from(name))
                } else if let Some(stripped) = raw
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                {
                    TermBind::Literal(Arc::<str>::from(stripped))
                } else if let Some(stripped) = raw
                    .strip_prefix(':')
                {
                    TermBind::Atom(Arc::<str>::from(stripped))
                } else {
                    TermBind::Read(Arc::<str>::from(raw))
                };
                (col, bind)
            })
            .collect();
        Liftable::RuleQuery { table, binds }
    }
}

// ─── where ────────────────────────────────────────────────────────────────
// `where\`PREDICATE\`` is the predicate slot for fused-SQL rule
// bodies. The body is a SqlWhereDsl-grammar expression (SQLite
// predicate vocabulary with `${capture}` host holes — see
// v4/src/cst/dsls/sql_where/). The DSL body's raw text is read by the
// fuser for predicate pushdown (P2). Runtime semantics for the
// streamed-Rust fallback are out of scope for this op today — the
// fuser's full-SQL path is the primary consumer.

pub struct WhereDef;

impl OperatorDef for WhereDef {
    fn name(&self) -> &'static str {
        "where"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        // An empty `where\`\`` is permitted at lower time; the fuser
        // is responsible for rejecting empty predicates if they ever
        // reach it.
        false
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // Two consumers read the same `op.dsl.raw`:
        //   - fuser (compile/fuser.rs::classify_op) — predicate
        //     pushdown into JOIN ON for full-SQL bodies.
        //   - this lower path — interpreted filter for streamed-Rust
        //     bodies (any Stream op present).
        // Both pull from the same text so the two regimes can't
        // disagree on what the predicate says.
        let Some(body) = dsl else {
            // Empty `where\`\`` → pass-through (matches the prior
            // VoidComponent semantics; the fuser already rejects empty
            // predicates if they ever reach SQL emission).
            return Ok(Pipe::new().step(Arc::new(VoidComponent)));
        };
        let raw = body.raw.as_ref();
        if raw.trim().is_empty() {
            return Ok(Pipe::new().step(Arc::new(VoidComponent)));
        }
        match crate::compile::lower::where_eval::parse_predicate(raw) {
            Ok(expr) => {
                let comp =
                    crate::compile::lower::where_eval::PredicateComponent::new(expr, body.raw.clone());
                Ok(Pipe::new().step(Arc::new(comp)))
            }
            Err(err) => Err(LowerError::Validate(vec![
                crate::compile::lower::where_eval::parse_diag(raw, &err),
            ])),
        }
    }
}

// ─── tag ──────────────────────────────────────────────────────────────────
// `tag(:rel, X, Y)` writes one row into relation `rel`. The first atom
// names the target table; remaining positional args supply cell values.
// Args desugar through walk's classify_slot:
//   bare `X`  / `${X}`  → term-read (column gets cursor[X])
//   `X?`     / `${X?}` → introducer (only legal at query position;
//                        tag detects and refuses)
//   `"lit"`  / `:atom` → literal cell value
//
// If the table has not been declared, tag auto-declares it with
// synthetic column names (`col0`, `col1`, …) so call sites don't have
// to drag a `rule(:rel, …);` decl ahead of every write. The auto-decl
// is non-destructive: a later explicit decl with the same shape is
// accepted; a conflicting shape panics as today.

pub struct TagDef;

const TAG_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom,
        name: "table",
        doc: "relation name (`:rel`)",
        required: true,
    },
    ArgSig {
        kind: ArgKind::Variadic(&ArgKind::Any),
        name: "cells",
        doc: "positional cell values; bare term-ref, literal, or `${X}`",
        required: false,
    },
];

impl OperatorDef for TagDef {
    fn name(&self) -> &'static str {
        "tag"
    }
    fn paren_args(&self) -> &[ArgSig] {
        TAG_SPEC
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        use crate::fact::FactWrite;

        let table = match args.first() {
            Some(Value::Atom(s)) => s.clone(),
            _ => {
                return Err(LowerError::Unknown(
                    "tag: first arg must be the :table atom".into(),
                ))
            }
        };

        let cell_args = &args[1..];

        // Resolve declared cols. If the table is not declared yet,
        // synthesize `col0..colN` and declare so FactStore impls that
        // require schema (sqlite) accept the insert.
        let declared = ctx.store.declared_cols(&table);
        if declared.map(|c| c.is_empty()).unwrap_or(true) {
            let synth: Vec<String> = (0..cell_args.len())
                .map(|i| format!("col{i}"))
                .collect();
            let synth_refs: Vec<&str> = synth.iter().map(|s| s.as_str()).collect();
            ctx.store.declare(&table, &synth_refs);
        }

        // Tag writes the full cursor so any synthetic terms stamped by
        // upstream apply sites (`_call_seq` on `r!.`) propagate into
        // the row's content_id. That keeps content-dedup correct for
        // intentional duplicates while still distinguishing
        // cache-bypassed invocations.
        Ok(Pipe::new().step(Arc::new(FactWrite::new(ctx.store.clone(), table))))
    }
}

// ─── void ─────────────────────────────────────────────────────────────────

pub struct VoidDef;

impl OperatorDef for VoidDef {
    fn name(&self) -> &'static str {
        "void"
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        Ok(Pipe::new().step(Arc::new(VoidComponent)))
    }
}

// ─── print ────────────────────────────────────────────────────────────────
// `print` / `print(:prefix)` / `` print`hello ${X}` `` / `` print(:tag)`${X}` ``
// Side-tap: cursor flows through unchanged; one line per cursor.

pub struct PrintDef;

const PRINT_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "prefix",
    doc: "label prepended to each printed line",
    required: false,
}];

impl OperatorDef for PrintDef {
    fn name(&self) -> &'static str {
        "print"
    }
    fn paren_args(&self) -> &[ArgSig] {
        PRINT_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        false
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let prefix: Option<String> = args.first().and_then(|v| match v {
            Value::Atom(s) => Some(s.to_string()),
            _ => None,
        });
        let mut comp = match dsl {
            Some(b) => PrintComponent::new(b.raw.as_ref()),
            None => PrintComponent::dump(),
        };
        if let Some(p) = prefix {
            comp = comp.with_prefix(p);
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// ─── sh / sh! ─────────────────────────────────────────────────────────────
// `sh\`cmd\``                           — capture stdout into &.value
// `sh(OUT?)\`cmd\``                     — capture into &.value AND bind OUT
// `sh(:filter)\`cmd\``                  — keep cursor only if exit == 0
// `sh!\`cmd\``                          — fire-and-forget side effect; passes through
// (`sh > split\`\n\`` replaces the previous :lines mode.)

pub struct ShDef;

const SH_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Variadic(&ArgKind::Any),
    name: "modifier",
    doc: "either :lines (emit one cursor per stdout line) or BIND? (capture stdout into BIND)",
    required: false,
}];

impl OperatorDef for ShDef {
    fn name(&self) -> &'static str {
        "sh"
    }
    fn paren_args(&self) -> &[ArgSig] {
        SH_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &[]
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        use crate::sprf_introspect::PipeIntrospect;
        let body = dsl.ok_or_else(|| {
            LowerError::Unknown("sh: dsl body required (e.g. sh`echo ${X}`)".into())
        })?;
        let cmd = body.raw.to_string();

        // Classify modifier: :filter atom, or BIND? capture term.
        let mut bind_term: Option<String> = None;
        let mut filter_only = false;
        for a in args {
            match a {
                Value::Atom(s) if s.as_ref() == "filter" => {
                    filter_only = true;
                }
                Value::Atom(other) => {
                    return Err(LowerError::Unknown(format!(
                        "sh: unknown atom :{} (try :filter or BIND? for capture)",
                        other
                    )));
                }
                Value::Pipe(p) => {
                    if let Some(name) = p.binds_terms().first() {
                        bind_term = Some(name.to_string());
                    } else if let Some(name) = p.reads_terms().first() {
                        bind_term = Some(name.to_string());
                    }
                }
            }
        }

        let mut interps = body.interps.clone();
        interps.sort_by_key(|i| i.range.lo);
        let comp: Arc<dyn effect_runtime::v2::Component<Next = Cursor>> = if filter_only {
            Arc::new(ShComponent::filter(&cmd).with_interps(interps))
        } else if let Some(bind) = bind_term {
            Arc::new(ShComponent::capture(&cmd, &bind).with_interps(interps))
        } else {
            // Default: capture stdout into &.value (no named term).
            Arc::new(ShComponent::capture_value(&cmd).with_interps(interps))
        };
        Ok(Pipe::new().step(comp))
    }
}

pub struct ShBangDef;

impl OperatorDef for ShBangDef {
    fn name(&self) -> &'static str {
        "sh!"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl.ok_or_else(|| LowerError::Unknown("sh!: dsl body required".into()))?;
        let mut interps = body.interps.clone();
        interps.sort_by_key(|i| i.range.lo);
        Ok(Pipe::new().step(Arc::new(
            ShBangComponent::new(body.raw.as_ref()).with_interps(interps),
        )))
    }
}

// ─── repo ─────────────────────────────────────────────────────────────────
// Two shapes:
//   `repo()`                    — bare-form generator. Reads
//                                 `~/.config/sprefa/repos.toml` via
//                                 `LowerCtx::config` and emits one
//                                 cursor per `RepoConfig` entry with
//                                 cursor.value = slug, REPO = slug,
//                                 Coord { repo: intern_repo(slug, ""),
//                                 rest: 0 }. No filesystem walk; the
//                                 cursor is the seed for downstream
//                                 `> rev() > fs()` chains.
//   `repo(:slug, :path)+`       — args form. Walks the filesystem
//                                 under each (slug, path) pair and
//                                 emits one cursor per matching file
//                                 with REPO=slug and FS=path.

pub struct RepoDef;

const REPO_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Variadic(&ArgKind::Atom),
    name: "args",
    doc: "0 args = bare-form (read repos.toml); else alternating :slug :path atoms",
    required: false,
}];

impl OperatorDef for RepoDef {
    fn name(&self) -> &'static str {
        "repo"
    }
    fn paren_args(&self) -> &[ArgSig] {
        REPO_SPEC
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &["REPO", "FS"]
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // Bare form `repo()`: drive the generator from `LowerCtx::config`.
        // One synthetic cursor per `RepoConfig` entry. No filesystem walk.
        if args.is_empty() {
            let mut comp = RepoComponent::from_config();
            if let Some(s) = &_ctx.sprf_store {
                comp = comp.with_sprf_store(s.clone());
            }
            if let Some(c) = &_ctx.config {
                comp = comp.with_config(c.clone());
            }
            return Ok(Pipe::new().step(Arc::new(comp)));
        }
        if args.len() % 2 != 0 {
            return Err(LowerError::Unknown(
                "repo: requires alternating (:slug, :path) atom pairs".into(),
            ));
        }
        let mut roots: Vec<(String, std::path::PathBuf)> = Vec::with_capacity(args.len() / 2);
        let mut i = 0;
        while i < args.len() {
            let slug = match &args[i] {
                Value::Atom(s) => s.to_string(),
                _ => return Err(LowerError::Unknown("repo: slug must be :atom".into())),
            };
            let path = match &args[i + 1] {
                Value::Atom(s) => std::path::PathBuf::from(s.as_ref()),
                _ => {
                    return Err(LowerError::Unknown(
                        "repo: path must be :atom (filesystem path)".into(),
                    ))
                }
            };
            roots.push((slug, path));
            i += 2;
        }
        let mut comp = RepoComponent::new(roots, Vec::new());
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// ─── rev ──────────────────────────────────────────────────────────────────
// `rev()`              — defaults to :HEAD; one cursor per upstream repo.
// `rev(:HEAD)`         — single revspec.
// `rev(:HEAD, :HEAD~1)`— cartesian: one cursor per (upstream × spec).
//
// Reads upstream `Coord.repo` to find `(slug, root)` in `LowerCtx::config`,
// opens the repo via `git2::Repository::open(root)`, resolves each spec
// via `revparse_single` + `peel_to_commit`, interns `(repo, oid, ts) →
// RevId`, emits cursor with `value = oid_hex`, REV term, Coord{repo, rev}.

pub struct RevDef;

const REV_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Variadic(&ArgKind::Any),
    name: "specs",
    doc: "0 args = :HEAD; atom revspecs; or glob`...` to enumerate refs",
    required: false,
}];

impl OperatorDef for RevDef {
    fn name(&self) -> &'static str {
        "rev"
    }
    fn paren_args(&self) -> &[ArgSig] {
        REV_SPEC
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &[
            "REPO",
            "KIND",
            "SPEC",
            "NAME",
            "REF",
            "REV",
            "TARGET",
            "COMMIT_TS",
            "TAG_TS",
        ]
    }

    fn lower(
        &self,
        ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        crate::v2_ops::declare_rev_relation(ctx.store.as_ref());
        let mut comp = if args.len() == 1 {
            if let Value::Pipe(pipe) = &args[0] {
                let glob = pipe
                    .steps
                    .first()
                    .and_then(|step| step.describe())
                    .and_then(|desc| desc.downcast_ref::<GlobComponent>())
                    .ok_or_else(|| {
                        LowerError::Unknown(
                            "rev: pipe arg must be a static glob`...` matcher".into(),
                        )
                    })?;
                RevComponent::glob(glob.regex())
            } else {
                let spec = match &args[0] {
                    Value::Atom(s) => s.to_string(),
                    Value::Pipe(_) => unreachable!(),
                };
                RevComponent::new(vec![spec])
            }
        } else if args.is_empty() {
            RevComponent::new(Vec::new()) // ctor sugars to ["HEAD"]
        } else {
            let mut out = Vec::with_capacity(args.len());
            for v in args {
                match v {
                    Value::Atom(s) => out.push(s.to_string()),
                    _ => {
                        return Err(LowerError::Unknown(
                            "rev: args must be atom revspecs or one static glob`...` matcher"
                                .into(),
                        ))
                    }
                }
            }
            RevComponent::new(out)
        };
        if let Some(s) = &ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        if let Some(c) = &ctx.config {
            comp = comp.with_config(c.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// ─── split ────────────────────────────────────────────────────────────────
// `split\`sep\``                  — split &.value, rebind value per piece
// `split(INTO?)\`sep\``           — also bind INTO term
// `split(FROM, INTO?)\`sep\``     — read FROM term, bind INTO

pub struct SplitDef;
pub struct SplitLineDef;

const SPLIT_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Variadic(&ArgKind::Any),
    name: "terms",
    doc: "0 args = on &.value; 1 arg = INTO?; 2 args = FROM, INTO?",
    required: false,
}];

fn lower_split_with_separator(
    op: &str,
    args: &[Value],
    sep: &str,
) -> Result<Pipe<Cursor>, LowerError> {
    use crate::sprf_introspect::PipeIntrospect;

    let pipe_term = |p: &Pipe<Cursor>| -> Option<String> {
        if let Some(n) = p.binds_terms().first() {
            return Some(n.to_string());
        }
        if let Some(n) = p.reads_terms().first() {
            return Some(n.to_string());
        }
        None
    };

    let comp = match args.len() {
        0 => SplitComponent::on_value(sep),
        1 => match &args[0] {
            Value::Pipe(p) => {
                let into = pipe_term(p).ok_or_else(|| {
                    LowerError::Unknown(format!("{op}: arg must be a term (INTO?)"))
                })?;
                SplitComponent::on_value(sep).into_term(&into)
            }
            Value::Atom(s) => SplitComponent::on_value(sep).into_term(s),
        },
        2 => {
            let from = match &args[0] {
                Value::Pipe(p) => pipe_term(p),
                Value::Atom(s) => Some(s.to_string()),
            };
            let into = match &args[1] {
                Value::Pipe(p) => pipe_term(p),
                Value::Atom(s) => Some(s.to_string()),
            };
            let from = from
                .ok_or_else(|| LowerError::Unknown(format!("{op}: 1st arg must be FROM term")))?;
            let into = into
                .ok_or_else(|| LowerError::Unknown(format!("{op}: 2nd arg must be INTO? term")))?;
            SplitComponent::new(&from, sep, &into)
        }
        n => {
            return Err(LowerError::Unknown(format!(
                "{op}: takes 0, 1, or 2 args (got {n})"
            )))
        }
    };
    Ok(Pipe::new().step(Arc::new(comp)))
}

impl OperatorDef for SplitDef {
    fn name(&self) -> &'static str {
        "split"
    }
    fn paren_args(&self) -> &[ArgSig] {
        SPLIT_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl.ok_or_else(|| {
            LowerError::Unknown("split: dsl body required (separator string)".into())
        })?;
        lower_split_with_separator("split", args, &body.raw)
    }
}

impl OperatorDef for SplitLineDef {
    fn name(&self) -> &'static str {
        "split_line"
    }
    fn paren_args(&self) -> &[ArgSig] {
        SPLIT_SPEC
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        lower_split_with_separator("split_line", args, "\n")
    }
}

// ─── read ─────────────────────────────────────────────────────────────────
// `read` — load file bytes at cursor.FS into cursor.value. Cursors
// without FS pass through; downstream ops can read &.value as the
// file content.

pub struct ReadDef;

impl OperatorDef for ReadDef {
    fn name(&self) -> &'static str {
        "read"
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let mut comp = ReadComponent::new().with_root(_ctx.root.clone());
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        if let Some(c) = &_ctx.config {
            comp = comp.with_config(c.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// ─── comment ──────────────────────────────────────────────────────────────
// `comment(`open_re`)`                 — sequential markers
// `comment(`open_re`, `close_re`)`     — paired markers
//
// Reads cursor.value as the haystack. Stamps LO/HI per region and rebinds
// value to the inner slice.

pub struct CommentDef;

const COMMENT_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Any,
        name: "open",
        doc: "open-pattern pipe, usually a backtick literal",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Any,
        name: "close",
        doc: "optional close-pattern pipe, usually a backtick literal",
        required: false,
    },
];

impl OperatorDef for CommentDef {
    fn name(&self) -> &'static str {
        "comment"
    }
    fn paren_args(&self) -> &[ArgSig] {
        COMMENT_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        false
    }
    fn cursor_binds(&self) -> &'static [&'static str] {
        &["LO", "HI"]
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        if args.len() > 2 {
            return Err(LowerError::Unknown(format!(
                "comment: takes 1 or 2 args (got {})",
                args.len()
            )));
        }
        if dsl.is_some() && !args.is_empty() {
            return Err(LowerError::Unknown(
                "comment: use either pipe args or a legacy backtick body, not both".into(),
            ));
        }
        let arg_pattern = |idx: usize| -> Result<Option<String>, LowerError> {
            match args.get(idx) {
                Some(Value::Pipe(p)) => run_once_const(p, _ctx).map(Some),
                Some(Value::Atom(a)) => Ok(Some(a.to_string())),
                None => Ok(None),
            }
        };
        let open = match arg_pattern(0)? {
            Some(open) => open,
            None => match dsl {
                Some(body) => body.raw.to_string(),
                None => return Err(LowerError::Unknown(
                    "comment: open regex required, e.g. comment(`<!-- start -->`, `<!-- end -->`)"
                        .into(),
                )),
            },
        };
        let close = arg_pattern(1)?;
        let mut comp = match close {
            Some(close) => CommentComponent::paired(&open, &close)
                .map_err(|e| LowerError::Unknown(format!("comment: bad regex: {e}")))?,
            None => CommentComponent::sequential(&open)
                .map_err(|e| LowerError::Unknown(format!("comment: bad regex: {e}")))?,
        };
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// ─── write_cursor ─────────────────────────────────────────────────────────
// `write_cursor`                — :replace at (FS, [LO, HI))
// `write_cursor(:replace)`      — explicit
// `write_cursor(:append)`       — insert at HI
// `write_cursor(:prepend)`      — insert at LO
// `write_cursor(:wrap)`         — surround [LO, HI) with cursor.value on both sides
//
// Layer 3 — capture-internal mode. A second positional atom whose name
// starts with an uppercase letter is treated as the NAME of a coord-
// space capture; the splice byte range comes from `cursor.term(NAME).at`
// instead of the focal `FS`/`LO`/`HI` terms. Mirrors v4's
// existing convention that ALL-CAPS atoms = capture names, lowercase
// atoms = mode/option keywords.
//
// `write_cursor(:replace, :NAME)`  — replace NAME's bytes with &.value
// `write_cursor(:append,  :NAME)`  — insert &.value at NAME.HI
// `write_cursor(:prepend, :NAME)`  — insert &.value at NAME.LO
//
// Aggregates per-FS in render_batch and applies right-to-left so
// multi-cursor splices stay correct.

pub struct WriteCursorDef;

const WRITE_CURSOR_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom,
        name: "mode",
        doc: ":replace (default) | :append | :prepend | :wrap",
        required: false,
    },
    ArgSig {
        kind: ArgKind::Atom,
        name: "capture",
        doc: ":NAME (uppercase) — splice into NAME's coord-space range",
        required: false,
    },
];

impl OperatorDef for WriteCursorDef {
    fn name(&self) -> &'static str {
        "write_cursor"
    }
    fn paren_args(&self) -> &[ArgSig] {
        WRITE_CURSOR_SPEC
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let mode = match args.first() {
            None => WriteMode::Replace,
            Some(Value::Atom(s)) => match s.as_ref() {
                "replace" => WriteMode::Replace,
                "append" => WriteMode::Append,
                "prepend" => WriteMode::Prepend,
                "wrap" => WriteMode::Wrap,
                other => {
                    return Err(LowerError::Unknown(format!(
                        "write_cursor: unknown mode :{} (try :replace, :append, :prepend, :wrap)",
                        other
                    )))
                }
            },
            Some(_) => {
                return Err(LowerError::Unknown(
                    "write_cursor: mode must be :atom".into(),
                ))
            }
        };
        // Layer 3 — second positional atom is the capture name when its
        // first char is uppercase.
        let capture: Option<Arc<str>> = match args.get(1) {
            None => None,
            Some(Value::Atom(s)) => {
                let first = s.chars().next();
                if first.map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                    Some(s.clone())
                } else {
                    return Err(LowerError::Unknown(format!(
                        "write_cursor: second atom must be an uppercase capture name, got :{}",
                        s
                    )));
                }
            }
            Some(_) => {
                return Err(LowerError::Unknown(
                    "write_cursor: second arg must be :atom".into(),
                ))
            }
        };
        let mut comp = WriteCursorComponent::new(mode);
        if let Some(name) = capture {
            comp = comp.on_capture(name);
        }
        if let Some(s) = &_ctx.sprf_store {
            comp = comp.with_sprf_store(s.clone());
        }
        Ok(Pipe::new().step(Arc::new(comp)))
    }
}

// ─── write_file ───────────────────────────────────────────────────────────
// `write_file`                   — write &.value to cursor.FS
// `write_file(:path/to.out)`     — write &.value to literal path

pub struct WriteFileDef;

const WRITE_FILE_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "path",
    doc: "explicit output path; overrides cursor.FS when present",
    required: false,
}];

impl OperatorDef for WriteFileDef {
    fn name(&self) -> &'static str {
        "write_file"
    }
    fn paren_args(&self) -> &[ArgSig] {
        WRITE_FILE_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        false
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        if !args.is_empty() && dsl.is_some() {
            return Err(LowerError::Unknown(
                "write_file: use either a path arg or backtick path body, not both".into(),
            ));
        }
        let path = match args.first() {
            None => dsl.map(|body| {
                let mut interps = body.interps.clone();
                interps.sort_by_key(|i| i.range.lo);
                if interps.is_empty() {
                    WriteFilePath::Static(std::path::PathBuf::from(body.raw.as_ref()))
                } else {
                    WriteFilePath::Template {
                        raw: body.raw.clone(),
                        interps: Arc::new(interps),
                    }
                }
            }),
            Some(Value::Atom(s)) => {
                Some(WriteFilePath::Static(std::path::PathBuf::from(s.as_ref())))
            }
            Some(_) => {
                return Err(LowerError::Unknown(
                    "write_file: path arg must be :atom".into(),
                ))
            }
        };
        Ok(Pipe::new().step(Arc::new(WriteFileComponent::new(path))))
    }
}

// ─── render ──────────────────────────────────────────────────────────────

pub struct RenderDef;
pub struct RenderMarkdownDef;
pub struct RenderDotMarkdownDef;

const RENDER_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "format",
    doc: ":markdown",
    required: true,
}];

impl OperatorDef for RenderDef {
    fn name(&self) -> &'static str {
        "render"
    }
    fn paren_args(&self) -> &[ArgSig] {
        RENDER_SPEC
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        true
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let format = atom_arg(args, 0);
        if format.as_ref() != "markdown" {
            return Err(LowerError::Unknown(format!(
                "render: unknown format :{} (try :markdown)",
                format
            )));
        }
        let body = dsl.ok_or_else(|| {
            LowerError::Unknown("render(:markdown): template body required".into())
        })?;
        lower_render_markdown_body(body)
    }
}

impl OperatorDef for RenderMarkdownDef {
    fn name(&self) -> &'static str {
        "render_markdown"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        true
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl
            .ok_or_else(|| LowerError::Unknown("render_markdown: template body required".into()))?;
        lower_render_markdown_body(body)
    }
}

impl OperatorDef for RenderDotMarkdownDef {
    fn name(&self) -> &'static str {
        "render.markdown"
    }
    fn dsl_body(&self) -> Option<DslShape> {
        Some(DslShape::Plain)
    }
    fn dsl_required(&self) -> bool {
        true
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl
            .ok_or_else(|| LowerError::Unknown("render.markdown: template body required".into()))?;
        lower_render_markdown_body(body)
    }
}

fn lower_render_markdown_body(body: &DslBody) -> Result<Pipe<Cursor>, LowerError> {
    let mut interps = body.interps.clone();
    interps.sort_by_key(|i| i.range.lo);
    Ok(Pipe::new().step(Arc::new(RenderMarkdownComponent::new(
        body.raw.clone(),
        interps,
    ))))
}

// ─── collect / collect_ready ──────────────────────────────────────────────

pub struct CollectDef;

impl OperatorDef for CollectDef {
    fn name(&self) -> &'static str {
        "collect"
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        _args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        Ok(Pipe::new().step(Arc::new(CollectComponent::new(CollectMode::Complete))))
    }
}

pub struct CollectReadyDef;

const COLLECT_READY_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "mode",
    doc: "partial flush mode: :snapshot or :append",
    required: false,
}];

impl OperatorDef for CollectReadyDef {
    fn name(&self) -> &'static str {
        "collect_ready"
    }
    fn paren_args(&self) -> &[ArgSig] {
        COLLECT_READY_SPEC
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let mode = match args.first() {
            None => CollectMode::ReadySnapshot,
            Some(Value::Atom(s)) if s.as_ref() == "snapshot" => CollectMode::ReadySnapshot,
            Some(Value::Atom(s)) if s.as_ref() == "append" => CollectMode::ReadyAppend,
            Some(Value::Atom(s)) => {
                return Err(LowerError::Unknown(format!(
                    "collect_ready: unknown mode :{}",
                    s
                )))
            }
            Some(_) => {
                return Err(LowerError::Unknown(
                    "collect_ready: mode arg must be :snapshot or :append".into(),
                ))
            }
        };
        Ok(Pipe::new().step(Arc::new(CollectComponent::new(mode))))
    }
}

// ─── next / next_q ────────────────────────────────────────────────────────

const CHAN_SPEC: &[ArgSig] = &[ArgSig {
    kind: ArgKind::Atom,
    name: "chan",
    doc: "channel name",
    required: true,
}];

pub struct NextDef;

impl OperatorDef for NextDef {
    fn name(&self) -> &'static str {
        "next"
    }
    fn paren_args(&self) -> &[ArgSig] {
        CHAN_SPEC
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let chan = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(Next::new(chan))))
    }
}

pub struct NextQDef;

impl OperatorDef for NextQDef {
    fn name(&self) -> &'static str {
        "next_q"
    }
    fn paren_args(&self) -> &[ArgSig] {
        CHAN_SPEC
    }

    fn lower(
        &self,
        _ctx: &LowerCtx,
        _flow: Option<Value>,
        args: &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl: Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let chan = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(NextQ::new(chan))))
    }
}
