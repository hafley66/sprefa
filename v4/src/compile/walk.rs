//! `walk.rs` — `Vec<PipeAst>` → `(Vec<Pipe<Cursor>>, Vec<Diag>)`.
//!
//! Lane C of the compile pipeline. Sits between the parser (which hands
//! us `Vec<PipeAst>`) and the registry (which lowers per-op). Walk
//! handles: slot text classification into `Value::Atom | Value::Pipe`,
//! recursive sub-pipe lowering for `{ block }`, dsl parse via the op's
//! `parse_dsl`, and diag aggregation across the program.
//!
//! Diag posture: never bail on first failure. The LSP needs every
//! call-site error in one pass.
//!
//! Inline-pipe args (`foo > bar`-style positional values) are handled
//! by re-entering `host_parse` on the slot body. If the fragment parses
//! as exactly one `PipeAst`, walk it recursively into a `Value::Pipe`.
//! Zero or more-than-one top-level pipes from a single arg slot is
//! malformed and emits `compile/inline-pipe-malformed`.

use std::sync::Arc;

use effect_runtime::v2::{ByteRange, Diag, Pipe};

use crate::Cursor;

use super::ast::{OpCall, PipeAst, SlotText};
use super::lower::ctx::LowerCtx;
use super::lower::op_def::{DslBody, DslInterp, InterpKind};
use super::lower::registry::Registry;
use super::lower::value::{CallArg, Value};

/// Walk a whole program. Each `PipeAst` becomes one `FusedRule`. The
/// fuser at the top of the per-pipe path classifies rule bodies and
/// emits full-SQL / streamed-Rust / unfused regimes; non-rule pipes
/// fall through as `Regime::Unfused` carrying the lowered Pipe.
///
/// Self-driving forms (rule decls with binder-only cols, dotted-apply
/// call sites) are expanded inline against a `Cursor::default()` seed
/// so the rule body executes and its writes hit the store. Callers
/// that need the underlying pipe pull it from the returned FusedRule
/// via `pipe()` / `into_pipe()`.
pub fn walk_program(
    program: &[PipeAst],
    reg: &Registry,
    ctx: &mut LowerCtx,
) -> (Vec<super::FusedRule>, Vec<Diag>) {
    let mut pipes = Vec::with_capacity(program.len());
    let mut diags = Vec::new();

    // ── pass 1: compile-time binding-graph analysis ────────────────
    diags.extend(super::binding_graph::analyze_program(program, reg));

    // ── pass 1b: full ProgramFlowGraph for the fuser ───────────────
    let (graph, _gdiag) = super::binding_graph::build_graph(program, reg, ctx);
    let rule_decls = collect_rule_decl_cols(program);

    // ── pass 2: lower per-pipe; route rule bodies through the fuser ─
    for p in program {
        if let Some(pipe) = walk_pipe(p, reg, ctx, &mut diags) {
            maybe_auto_expand(&pipe, p, ctx);
            let head = p.steps.first();
            let fused = match head {
                Some(op) if op.name.as_ref() == "rule" => {
                    let block = op.block.as_ref();
                    let rule_name = op
                        .args
                        .first()
                        .and_then(|a| a.raw.trim().strip_prefix(':'))
                        .map(Arc::<str>::from)
                        .unwrap_or_else(|| Arc::<str>::from("<unnamed>"));
                    if let Some(block) = block {
                        super::fuser::try_fuse(
                            &rule_name,
                            block,
                            pipe,
                            &graph,
                            &rule_decls,
                            &mut diags,
                        )
                        .unwrap_or_else(|| {
                            super::fuser::passthrough(rule_name, Pipe::new())
                        })
                    } else {
                        super::fuser::passthrough(rule_name, pipe)
                    }
                }
                Some(op) => {
                    super::fuser::passthrough(op.name.clone(), pipe)
                }
                None => super::fuser::passthrough(Arc::<str>::from("<empty>"), pipe),
            };
            pipes.push(fused);
        }
    }
    (pipes, diags)
}

fn collect_rule_decl_cols(
    program: &[PipeAst],
) -> std::collections::HashMap<Arc<str>, Vec<Arc<str>>> {
    let mut out = std::collections::HashMap::new();
    for pipe in program {
        for op in &pipe.steps {
            if op.name.as_ref() != "rule" {
                continue;
            }
            let Some(first) = op.args.first() else {
                continue;
            };
            let Some(name) = first.raw.trim().strip_prefix(':') else {
                continue;
            };
            let name = Arc::<str>::from(name);
            let mut cols = Vec::new();
            for arg in op.args.iter().skip(1) {
                let raw = arg.raw.trim();
                let raw = raw.strip_prefix(':').unwrap_or(raw);
                let raw = raw
                    .strip_suffix('?')
                    .or_else(|| raw.strip_suffix('!'))
                    .unwrap_or(raw);
                if !raw.is_empty() {
                    cols.push(Arc::<str>::from(raw));
                }
            }
            out.insert(name, cols);
        }
    }
    out
}

/// Drive a pipe through a fresh expander when it's a self-driving form
/// (rule declaration with a body, dotted-apply, or rule sink-write
/// position). Other shapes (fs > ast > …) need an external seed and
/// remain caller-driven.
fn maybe_auto_expand(pipe: &Pipe<Cursor>, ast: &PipeAst, _ctx: &LowerCtx) {
    if !is_self_driving(ast) {
        return;
    }
    use effect_runtime::v2::{expand, ExpandOpts, MemQueue, QueueBackend};
    let inst = pipe.clone().into_instance();
    let queue: Arc<dyn QueueBackend<Cursor>> =
        Arc::new(MemQueue::new());
    expand(
        &inst,
        queue,
        vec![Arc::new(Cursor::default())],
        ExpandOpts::default(),
    );
}

/// A pipe is "self-driving" if its first step is a rule apply, a rule
/// sink-write, or a top-level rule declaration with a body. These
/// shapes don't need an external seed because the call site itself is
/// the trigger; bare queries and external generators still rely on the
/// caller to seed an expand.
fn is_self_driving(ast: &PipeAst) -> bool {
    let Some(head) = ast.steps.first() else {
        return false;
    };
    if head.name.as_ref() == "rule" {
        // `rule(:r, …)` decl with a body OR rule sink-write at chain
        // head (rare; handled by the body-attached FactWrite).
        return head.block.is_some();
    }
    // Apply form `r.(…)` / `r!.(…)`: self-driving regardless of body.
    head.apply
}

/// Walk one pipe. Concat per-op `Pipe<Cursor>` via `Pipe::extend`. If
/// any op fails to lower its diags are appended and that op is skipped
/// (the rest of the chain still walks; the resulting pipe may be
/// shorter than the source). Returns `None` only if every op failed.
pub fn walk_pipe(
    p: &PipeAst,
    reg: &Registry,
    ctx: &mut LowerCtx,
    diags: &mut Vec<Diag>,
) -> Option<Pipe<Cursor>> {
    let mut acc: Pipe<Cursor> = Pipe::new();
    let mut any = false;
    for (idx, op) in p.steps.iter().enumerate() {
        match walk_op(op, reg, ctx, diags, idx) {
            Some(piece) => {
                acc = acc.extend(piece);
                any = true;
            }
            None => {} // diag already pushed
        }
    }
    if any {
        Some(acc)
    } else {
        None
    }
}

/// Walk one op: classify slots, recurse on block, parse dsl, dispatch
/// through the registry. On any classification or lower failure, push a
/// diag and return `None` so the caller can skip past it.
pub fn walk_op(
    op: &OpCall,
    reg: &Registry,
    ctx: &mut LowerCtx,
    diags: &mut Vec<Diag>,
    chain_pos: usize,
) -> Option<Pipe<Cursor>> {
    if op.flow.is_none()
        && op.args.is_empty()
        && op.dsl.is_none()
        && op.block.is_none()
        && !op.force
        && !op.apply
        && is_caps_ident(op.name.as_ref())
    {
        let term = if op.predicate {
            crate::term::Term::bind(op.name.clone())
        } else {
            crate::term::Term::read(op.name.clone())
        };
        return Some(Pipe::new().step(Arc::new(term)));
    }

    let lower_name: Arc<str> = if op.force && op.name.as_ref() == "sh" {
        Arc::<str>::from("sh!")
    } else {
        op.name.clone()
    };

    // Resolve flow slot.
    let flow: Option<(Value, ByteRange)> = match &op.flow {
        Some(slot) => match classify_slot(slot, reg, ctx, diags) {
            Some(v) => Some((v, slot.span)),
            None => return None,
        },
        None => None,
    };

    // Resolve paren args.
    let mut args: Vec<(CallArg, ByteRange)> = Vec::with_capacity(op.args.len());
    let mut had_arg_err = false;
    for slot in &op.args {
        match classify_call_arg(slot, reg, ctx, diags) {
            Some(v) => args.push((v, slot.span)),
            None => {
                had_arg_err = true;
            }
        }
    }
    if had_arg_err {
        return None;
    }

    // Resolve dsl body via the op's parse_dsl. Unknown op → registry
    // will emit `lower/unknown-op` below; here we just skip dsl parse.
    let dsl: Option<(DslBody, ByteRange)> = match &op.dsl {
        Some(dsl_text) => {
            let mut interps: Vec<DslInterp> = match reg.get(&lower_name) {
                Some(def) => match def.parse_dsl(&dsl_text.raw) {
                    Ok(v) => v,
                    Err(e) => {
                        diags.push(
                            Diag::error(
                                "compile/dsl-parse",
                                format!("op `{lower_name}` dsl parse: {e}"),
                            )
                            .with_span(dsl_text.span.lo, dsl_text.span.hi),
                        );
                        return None;
                    }
                },
                None => Vec::new(),
            };
            // Task #10 — for any SubPipe-shape carveout, walk the inner
            // pipe source into a `Pipe<Cursor>` and stamp it onto the
            // interp's `lowered` slot. Diags from the inner walk are
            // rebased into outer source coords using the dsl body's
            // origin (dsl_text.span.lo) plus the interp's relative
            // offset within the body.
            for interp in interps.iter_mut() {
                if let InterpKind::SubPipe { src, lowered } = &mut interp.kind {
                    if lowered.is_none() {
                        // Synthesize `;` so host_parse sees a complete stmt.
                        let mut frag = String::with_capacity(src.len() + 1);
                        frag.push_str(src);
                        frag.push(';');
                        let (sub_pipes, sub_diags) = crate::compile::parse::host_parse(&frag);
                        // Rebase diag spans into outer source coordinates.
                        // Interp.range.lo is relative to dsl_text.raw; the
                        // SubPipe body sits at +2 (skip `${`). Errors are
                        // best-effort here: the inner_diags from walk_pipe
                        // get the same shift.
                        let body_offset = dsl_text.span.lo.saturating_add(interp.range.lo + 2);
                        for mut d in sub_diags {
                            if let Some(r) = d.span.as_mut() {
                                r.lo = r.lo.saturating_add(body_offset);
                                r.hi = r.hi.saturating_add(body_offset);
                            }
                            diags.push(d);
                        }
                        if sub_pipes.len() != 1 {
                            diags.push(
                                Diag::error(
                                    "compile/sub-pipe-malformed",
                                    format!(
                                        "${{ … }} sub-pipe parsed to {} pipes (expected 1)",
                                        sub_pipes.len()
                                    ),
                                )
                                .with_span(
                                    dsl_text.span.lo + interp.range.lo,
                                    dsl_text.span.lo + interp.range.hi,
                                ),
                            );
                            continue;
                        }
                        let mut inner_diags: Vec<Diag> = Vec::new();
                        let pipe = walk_pipe(&sub_pipes[0], reg, ctx, &mut inner_diags);
                        for mut d in inner_diags {
                            if let Some(r) = d.span.as_mut() {
                                r.lo = r.lo.saturating_add(body_offset);
                                r.hi = r.hi.saturating_add(body_offset);
                            }
                            diags.push(d);
                        }
                        if let Some(p) = pipe {
                            *lowered = Some(p);
                        }
                    }
                }
            }
            Some((
                DslBody {
                    raw: dsl_text.raw.clone(),
                    interps,
                },
                dsl_text.span,
            ))
        }
        None => None,
    };

    // Recurse on block.
    let block: Option<(Pipe<Cursor>, ByteRange)> = match &op.block {
        Some(sub_ast) => match walk_pipe(sub_ast, reg, ctx, diags) {
            Some(pipe) => Some((pipe, sub_ast.span)),
            None => return None,
        },
        None => None,
    };

    if op.name.as_ref() == "rule" && op.force {
        diags.push(
            Diag::error(
                "lower/force-unsupported",
                "rule!: force applies to dotted rule apply, not rule declarations or writes",
            )
            .with_span(op.span.lo, op.span.hi),
        );
        return None;
    }

    if op.name.as_ref() == "rule" && chain_pos >= 1 && block.is_none() {
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        match crate::sql::rule_write_pipe(ctx, &arg_vals) {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rule-write", e.to_string())
                        .with_span(op.span.lo, op.span.hi),
                );
                return None;
            }
        }
    }

    let declared_rule_call =
        reg.get(&lower_name).is_none() && ctx.store.declared_cols(&op.name).is_some();

    // V4 locked rule semantics (CLAUDE.md):
    //   r.(X, Y)   → apply form: run the body with grounded args
    //   r!.(X, Y)  → apply + bypass apply-cache
    //   r(X?, Y?)  → bare call: QUERY r_facts; never run the body
    //   r.(X?, Y)  → diagnostic `lower/apply-with-hole`
    if op.apply && declared_rule_call {
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        if first_apply_hole(&arg_vals).is_some() {
            diags.push(
                Diag::error(
                    "lower/apply-with-hole",
                    format!(
                        "{}.(...): apply form requires grounded args; \
                         capture-binders (TERM?) are only valid in bare query calls",
                        op.name,
                    ),
                )
                .with_span(op.span.lo, op.span.hi),
            );
            return None;
        }
        // Apply form: prefer bodied invocation (RuleInvokeComponent). For
        // pass-through rules (no body) fall back to the apply-write path
        // so :sink-style declarations still write.
        let result = if ctx.get_rule(&op.name).is_some() {
            crate::sql::rule_body_call_pipe(ctx, &op.name, op.force, &arg_vals)
        } else {
            crate::sql::rule_apply_write_pipe(ctx, &op.name, &arg_vals)
        };
        match result {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rule-apply", e.to_string())
                        .with_span(op.span.lo, op.span.hi),
                );
                return None;
            }
        }
    }

    if op.force && declared_rule_call {
        // `r!(...)` without dotted apply is reserved. Force only applies
        // to the dotted apply form `r!.(...)`.
        diags.push(
            Diag::error("lower/rule-force-reserved",
                format!("{}!(...): force is only valid on the apply form {}!.(...)", op.name, op.name))
                .with_span(op.span.lo, op.span.hi)
        );
        return None;
    }

    // Bare or predicate rule call against the materialized facts table.
    // V4 locked: `r(...)` and `r?(...)` are both QUERY paths against
    // `<r>_facts`. The body is NEVER run from a bare call site.
    if declared_rule_call {
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        match crate::sql::rule_table_call_pipe(ctx, &op.name, &arg_vals) {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rule-call", e.to_string()).with_span(op.span.lo, op.span.hi),
                );
                return None;
            }
        }
    }

    if op.predicate && lower_name.as_ref() == "rev" {
        crate::v2_ops::declare_rev_relation(ctx.store.as_ref());
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        match crate::sql::rule_table_call_pipe(ctx, "rev", &arg_vals) {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rev-query", e.to_string()).with_span(op.span.lo, op.span.hi),
                );
                return None;
            }
        }
    }

    // Dispatch.
    match reg.lower_call_at(ctx, &lower_name, flow, args, block, dsl, op.span, chain_pos) {
        Ok(pipe) => {
            // If a probe sink is configured on the LowerCtx, wrap each
            // step of the lowered pipe with `SpannedComponent` so every
            // emit is tagged with this op's source byte range. With
            // `probe = None` the lowered pipe is returned unchanged.
            let pipe = match &ctx.probe {
                Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                None => pipe,
            };
            Some(pipe)
        }
        Err(ds) => {
            diags.extend(ds);
            None
        }
    }
}

/// Detect a TERM? capture-binder in an apply form's arg list. Returns
/// the byte span of the offending arg's enclosing op when found. Used
/// for the `lower/apply-with-hole` diagnostic — apply cannot accept
/// unbound captures; the bare query form is the only place holes are
/// legal.
fn first_apply_hole(args: &[CallArg]) -> Option<(u32, u32)> {
    use crate::sprf_introspect::PipeIntrospect;
    for a in args {
        if let Value::Pipe(p) = &a.value {
            if !p.binds_terms().is_empty() {
                // Span info would require threading slot ranges; the
                // caller falls back to the op span. Returning a sentinel
                // (0,0) tells the caller "yes, found one".
                return Some((0, 0));
            }
        }
    }
    None
}

fn classify_call_arg(
    slot: &SlotText,
    reg: &Registry,
    ctx: &mut LowerCtx,
    diags: &mut Vec<Diag>,
) -> Option<CallArg> {
    if let Some((keyword, value_slot)) = split_keyword_arg(slot) {
        classify_slot(&value_slot, reg, ctx, diags).map(|value| CallArg::keyword(keyword, value))
    } else {
        classify_slot(slot, reg, ctx, diags).map(CallArg::positional)
    }
}

fn split_keyword_arg(slot: &SlotText) -> Option<(Arc<str>, SlotText)> {
    let raw = slot.raw.as_ref();
    let trimmed = raw.trim();
    let leading = raw.len().saturating_sub(raw.trim_start().len());
    let colon = find_top_level_keyword_colon(trimmed)?;
    let key = trimmed[..colon].trim();
    if !is_ident(key) {
        return None;
    }
    let value = trimmed[colon + 1..].trim();
    if value.is_empty() {
        return None;
    }
    let value_start_in_trimmed = colon
        + 1
        + trimmed[colon + 1..]
            .len()
            .saturating_sub(trimmed[colon + 1..].trim_start().len());
    let value_end_in_trimmed = value_start_in_trimmed + value.len();
    let lo = slot
        .span
        .lo
        .saturating_add((leading + value_start_in_trimmed) as u32);
    let hi = slot
        .span
        .lo
        .saturating_add((leading + value_end_in_trimmed) as u32);
    Some((
        Arc::<str>::from(key),
        SlotText {
            raw: Arc::<str>::from(value),
            span: ByteRange { lo, hi },
        },
    ))
}

fn find_top_level_keyword_colon(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'`' | b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b':' if depth == 0 && i > 0 => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Classify a `SlotText` into `Value::Atom` or `Value::Pipe`.
///
/// Literal classifiers fire first: `:foo` → Atom("foo"), bare ident →
/// Atom, quoted string → Atom (outer quotes stripped). If none match,
/// re-enter `host_parse` on the slot body and treat the result as an
/// inline pipe expression. Span-shift strategy: per-diag at the seam.
/// Inner walk_pipe writes into a fresh local `Vec<Diag>` so its byte
/// ranges (which are relative to `slot.raw`) can be rebased into outer
/// source coords by adding `slot.span.lo`, then merged into `diags`.
pub fn classify_slot(
    slot: &SlotText,
    reg: &Registry,
    ctx: &mut LowerCtx,
    diags: &mut Vec<Diag>,
) -> Option<Value> {
    let raw = slot.raw.as_ref().trim();
    if raw.is_empty() {
        diags.push(
            Diag::error("compile/empty-slot", "empty slot body".to_string())
                .with_span(slot.span.lo, slot.span.hi),
        );
        return None;
    }
    // `${X}` / `${X?}` in arg-slot position desugar to term reads /
    // binders. Per CLAUDE.md "Pinned semantics", `${X}` is the read
    // mode and `${X?}` is the unbound (introducer) mode — both valid
    // at slot position so call sites like `tag(:r, ${A}, ${B})` work
    // without falling back to inline-pipe parsing. Anything more
    // complex than a single `${IDENT}` or `${IDENT?}` falls through to
    // the inline-pipe path below and surfaces a parse-time diag if
    // malformed.
    if let Some(name) = simple_host_term_read(raw) {
        let term = crate::term::Term::read(Arc::<str>::from(name));
        return Some(Value::Pipe(Pipe::new().step(Arc::new(term))));
    }
    if let Some(name) = simple_host_term_bind(raw) {
        let term = crate::term::Term::bind(Arc::<str>::from(name));
        return Some(Value::Pipe(Pipe::new().step(Arc::new(term))));
    }
    // `:foo` colon-prefixed atom.
    if let Some(rest) = raw.strip_prefix(':') {
        if !rest.is_empty() {
            return Some(Value::Atom(Arc::<str>::from(rest)));
        }
    }
    if raw == "&.value" {
        return Some(Value::Atom(Arc::<str>::from("&.value")));
    }
    // Plain backtick literal in host-arg position. This lets rule calls
    // accept direct literal kwargs, e.g. `some_rule(NAME: `Cursor`)`,
    // without compiling and executing a const sub-pipe at lower time.
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        if bytes[0] == b'`' && bytes[bytes.len() - 1] == b'`' {
            let inner = &raw[1..raw.len() - 1];
            return Some(Value::Atom(Arc::<str>::from(inner)));
        }
    }
    // ALL_CAPS bareword → term-ref desugar.
    //   `NAME`  → sub-pipe `term(:NAME)`      (read existing capture)
    //   `NAME?` → sub-pipe `term_bind(:NAME)` (introduce/bind capture)
    // CAPS convention: `[A-Z][A-Z0-9_]*`. Mixed-case / lowercase barewords
    // remain `Value::Atom` for ops that take atom-shaped args by name.
    if let Some(stripped) = raw.strip_suffix('?') {
        if is_caps_ident(stripped) {
            let term = crate::term::Term::bind(Arc::<str>::from(stripped));
            let pipe = Pipe::new().step(Arc::new(term));
            return Some(Value::Pipe(pipe));
        }
    }
    if let Some(stripped) = raw.strip_suffix('!') {
        if is_caps_ident(stripped) {
            let term = crate::term::Term::read(Arc::<str>::from(stripped));
            let pipe = Pipe::new().step(Arc::new(term));
            return Some(Value::Pipe(pipe));
        }
    }
    if is_caps_ident(raw) {
        let term = crate::term::Term::read(Arc::<str>::from(raw));
        let pipe = Pipe::new().step(Arc::new(term));
        return Some(Value::Pipe(pipe));
    }
    // Bare identifier (ASCII letter/underscore start, alnum/underscore body).
    if is_ident(raw) {
        return Some(Value::Atom(Arc::<str>::from(raw)));
    }
    // Double- or single-quoted string literal: strip outer quotes, no
    // escape handling yet (escape semantics belong to the parser lane).
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        let q = bytes[0];
        if (q == b'"' || q == b'\'') && bytes[bytes.len() - 1] == q {
            let inner = &raw[1..raw.len() - 1];
            return Some(Value::Atom(Arc::<str>::from(inner)));
        }
    }
    // Predicate-shape escape hatch: when the slot looks like a SQL
    // predicate (carries `${X}` interps plus a relational operator),
    // skip the inline-pipe fallback and return the raw text as a
    // Value::Atom. The fuser pulls the raw slot text directly for
    // where/predicate ops; re-parsing as a sub-pipe would produce
    // spurious `unknown-op LO` diagnostics here.
    if looks_like_predicate(raw) {
        return Some(Value::Atom(Arc::<str>::from(raw)));
    }

    // Inline-pipe fallback: re-parse the slot body as a sprf fragment.
    // Top-level stmts now require a trailing `;`; slot fragments don't
    // carry one, so synthesize it. The appended `;` sits at byte
    // slot.raw.len(); any diag pointing there gets shifted to slot.span.hi
    // when we rebase below.
    let mut frag = String::with_capacity(slot.raw.len() + 1);
    frag.push_str(slot.raw.as_ref());
    frag.push(';');
    let (sub_pipes, sub_diags) = crate::compile::parse::host_parse(&frag);
    let base = slot.span.lo;
    for mut d in sub_diags {
        if let Some(r) = d.span.as_mut() {
            r.lo = r.lo.saturating_add(base);
            r.hi = r.hi.saturating_add(base);
        }
        diags.push(d);
    }
    if sub_pipes.len() != 1 {
        diags.push(
            Diag::error(
                "compile/inline-pipe-malformed",
                format!(
                    "inline pipe arg parsed to {} pipes (expected 1)",
                    sub_pipes.len()
                ),
            )
            .with_span(slot.span.lo, slot.span.hi),
        );
        return None;
    }
    // Recurse into walk_pipe with a fresh diag buffer so we can rebase
    // its ranges (which are relative to slot.raw) into outer source
    // coords before merging.
    let mut inner_diags: Vec<Diag> = Vec::new();
    let pipe = walk_pipe(&sub_pipes[0], reg, ctx, &mut inner_diags);
    for mut d in inner_diags {
        if let Some(r) = d.span.as_mut() {
            r.lo = r.lo.saturating_add(base);
            r.hi = r.hi.saturating_add(base);
        }
        diags.push(d);
    }
    pipe.map(Value::Pipe)
}

/// Heuristic: does this slot text look like a SQL-style predicate?
/// Two signals, both required:
///   1. carries at least one `${IDENT}` interp;
///   2. carries a relational / comparison token (`!=`, `<>`, `<`, `>`,
///      `=`, ` like `, ` is `, ` between `).
/// This is used as a fast escape hatch out of the inline-pipe parser
/// path inside `classify_slot`. False positives don't break parsing
/// (the slot text is preserved as an atom and the consuming op's
/// classifier still gets the raw bytes) but they DO skip the
/// sub-pipe walk, which the where/predicate fuser path doesn't need.
fn looks_like_predicate(raw: &str) -> bool {
    if !raw.contains("${") {
        return false;
    }
    let lower = raw.to_ascii_lowercase();
    let has_op = raw.contains("!=")
        || raw.contains("<>")
        || raw.contains(">=")
        || raw.contains("<=")
        || raw.contains(" = ")
        || raw.contains(" < ")
        || raw.contains(" > ")
        || lower.contains(" like ")
        || lower.contains(" is ")
        || lower.contains(" between ");
    has_op
}

/// Recognize `${IDENT}` as a host-position term read. Returns the
/// captured name without the wrapping `${ }`. Returns None if the slot
/// has anything else after the closing brace (then we fall through to
/// inline-pipe parsing).
fn simple_host_term_read(raw: &str) -> Option<&str> {
    let rest = raw.strip_prefix("${")?;
    let inner = rest.strip_suffix('}')?;
    if inner.ends_with('?') {
        return None;
    }
    if is_caps_ident(inner) {
        Some(inner)
    } else {
        None
    }
}

/// Recognize `${IDENT?}` as a host-position term binder.
fn simple_host_term_bind(raw: &str) -> Option<&str> {
    let rest = raw.strip_prefix("${")?;
    let inner = rest.strip_suffix('}')?;
    let name = inner.strip_suffix('?')?;
    if is_caps_ident(name) {
        Some(name)
    } else {
        None
    }
}

#[allow(dead_code)]
fn find_host_hole_outside_quotes(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'`' | b'\'' | b'"' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'$' if bytes[i + 1] == b'{' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// `[A-Z][A-Z0-9_]*` — ALL_CAPS identifier convention. Used to mark a
/// bareword in an arg slot as a term-ref (capture) rather than an atom.
fn is_caps_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}
