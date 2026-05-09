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

/// Walk a whole program. Each `PipeAst` becomes one `Pipe<Cursor>` on
/// success; on failure that pipe is dropped and its diags accumulated.
/// Continues past per-pipe errors to surface every diag in one pass.
pub fn walk_program(
    program: &[PipeAst],
    reg:     &Registry,
    ctx:     &mut LowerCtx,
) -> (Vec<Pipe<Cursor>>, Vec<Diag>) {
    let mut pipes = Vec::with_capacity(program.len());
    let mut diags = Vec::new();

    // ── pass 1: compile-time binding-graph analysis ────────────────
    // Walks each PipeAst tracking which captures are bound at each
    // step. Emits `lang/use-before-bind` and `lang/term-self-cycle`
    // diagnostics. Independent of lowering — runs once on the AST,
    // produces diags the LSP can surface alongside lower-time diags.
    diags.extend(super::binding_graph::analyze_program(program, reg));

    // ── pass 2: lower per-pipe ────────────────────────────────────
    for p in program {
        if let Some(pipe) = walk_pipe(p, reg, ctx, &mut diags) {
            pipes.push(pipe);
        }
    }
    (pipes, diags)
}

/// Walk one pipe. Concat per-op `Pipe<Cursor>` via `Pipe::extend`. If
/// any op fails to lower its diags are appended and that op is skipped
/// (the rest of the chain still walks; the resulting pipe may be
/// shorter than the source). Returns `None` only if every op failed.
pub fn walk_pipe(
    p:     &PipeAst,
    reg:   &Registry,
    ctx:   &mut LowerCtx,
    diags: &mut Vec<Diag>,
) -> Option<Pipe<Cursor>> {
    let mut acc: Pipe<Cursor> = Pipe::new();
    let mut any = false;
    for (idx, op) in p.steps.iter().enumerate() {
        match walk_op(op, reg, ctx, diags, idx) {
            Some(piece) => { acc = acc.extend(piece); any = true; }
            None => {} // diag already pushed
        }
    }
    if any { Some(acc) } else { None }
}

/// Walk one op: classify slots, recurse on block, parse dsl, dispatch
/// through the registry. On any classification or lower failure, push a
/// diag and return `None` so the caller can skip past it.
pub fn walk_op(
    op:        &OpCall,
    reg:       &Registry,
    ctx:       &mut LowerCtx,
    diags:     &mut Vec<Diag>,
    chain_pos: usize,
) -> Option<Pipe<Cursor>> {
    let lower_name: Arc<str> = if op.force && op.name.as_ref() == "sh" {
        Arc::<str>::from("sh!")
    } else {
        op.name.clone()
    };

    // Resolve flow slot.
    let flow: Option<(Value, ByteRange)> = match &op.flow {
        Some(slot) => match classify_slot(slot, reg, ctx, diags) {
            Some(v) => Some((v, slot.span)),
            None    => return None,
        },
        None => None,
    };

    // Resolve paren args.
    let mut args: Vec<(CallArg, ByteRange)> = Vec::with_capacity(op.args.len());
    let mut had_arg_err = false;
    for slot in &op.args {
        match classify_call_arg(slot, reg, ctx, diags) {
            Some(v) => args.push((v, slot.span)),
            None    => { had_arg_err = true; }
        }
    }
    if had_arg_err { return None; }

    // Resolve dsl body via the op's parse_dsl. Unknown op → registry
    // will emit `lower/unknown-op` below; here we just skip dsl parse.
    let dsl: Option<(DslBody, ByteRange)> = match &op.dsl {
        Some(dsl_text) => {
            let mut interps: Vec<DslInterp> = match reg.get(&lower_name) {
                Some(def) => match def.parse_dsl(&dsl_text.raw) {
                    Ok(v)  => v,
                    Err(e) => {
                        diags.push(
                            Diag::error("compile/dsl-parse",
                                format!("op `{lower_name}` dsl parse: {e}"))
                                .with_span(dsl_text.span.lo, dsl_text.span.hi));
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
                        let (sub_pipes, sub_diags) =
                            crate::compile::parse::host_parse(&frag);
                        // Rebase diag spans into outer source coordinates.
                        // Interp.range.lo is relative to dsl_text.raw; the
                        // SubPipe body sits at +2 (skip `${`). Errors are
                        // best-effort here: the inner_diags from walk_pipe
                        // get the same shift.
                        let body_offset = dsl_text.span.lo
                            .saturating_add(interp.range.lo + 2);
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
                DslBody { raw: dsl_text.raw.clone(), interps },
                dsl_text.span,
            ))
        }
        None => None,
    };

    // Recurse on block.
    let block: Option<(Pipe<Cursor>, ByteRange)> = match &op.block {
        Some(sub_ast) => match walk_pipe(sub_ast, reg, ctx, diags) {
            Some(pipe) => Some((pipe, sub_ast.span)),
            None       => return None,
        },
        None => None,
    };

    if op.name.as_ref() == "rule" && op.force {
        diags.push(
            Diag::error("lower/force-unsupported",
                "rule!: force applies to dotted rule apply, not rule declarations or writes")
                .with_span(op.span.lo, op.span.hi)
        );
        return None;
    }

    if op.name.as_ref() == "rule" && chain_pos >= 1 && block.is_none() {
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        match crate::sql::rule_write_pipe(ctx, &arg_vals) {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None    => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rule-write", e.to_string())
                        .with_span(op.span.lo, op.span.hi)
                );
                return None;
            }
        }
    }

    if op.predicate && reg.get(&lower_name).is_none() && ctx.store.declared_cols(&op.name).is_some() {
        diags.push(
            Diag::error("lower/rule-predicate-unsupported",
                format!("{}?(...): rule predicate syntax is outside the locked V4 surface; use a grounded {}(...) relation query", op.name, op.name))
                .with_span(op.span.lo, op.span.hi)
        );
        return None;
    }

    if op.force && !op.apply && reg.get(&lower_name).is_none() && ctx.store.declared_cols(&op.name).is_some() {
        diags.push(
            Diag::error("lower/rule-force-unsupported",
                format!("{}!: force is only valid with dotted apply as {}!.(...)", op.name, op.name))
                .with_span(op.span.lo, op.span.hi)
        );
        return None;
    }

    if op.apply && reg.get(&lower_name).is_none() && ctx.get_rule(&op.name).is_some() {
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        match crate::sql::rule_body_call_pipe(ctx, &op.name, op.force, &arg_vals) {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None    => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rule-body-call", e.to_string())
                        .with_span(op.span.lo, op.span.hi)
                );
                return None;
            }
        }
    }

    if op.apply && reg.get(&lower_name).is_none() && ctx.store.declared_cols(&op.name).is_some() {
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        match crate::sql::rule_apply_write_pipe(ctx, &op.name, &arg_vals) {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None    => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rule-apply", e.to_string())
                        .with_span(op.span.lo, op.span.hi)
                );
                return None;
            }
        }
    }

    // Declared rule-table calls. A declared table name in call position
    // is a relation read over that rule table:
    //   rule_name(A, B?)  -> row-producing query/project
    //   rule_name(A, B)   -> grounded relation query
    if reg.get(&lower_name).is_none() && ctx.store.declared_cols(&op.name).is_some() {
        let arg_vals: Vec<CallArg> = args.iter().map(|(v, _)| v.clone()).collect();
        match crate::sql::rule_table_call_pipe(ctx, &op.name, false, &arg_vals) {
            Ok(pipe) => {
                let pipe = match &ctx.probe {
                    Some(p) => super::probe_wrap::wrap_pipe_with_span(pipe, op.span, p.clone()),
                    None    => pipe,
                };
                return Some(pipe);
            }
            Err(e) => {
                diags.push(
                    Diag::error("lower/rule-call", e.to_string())
                        .with_span(op.span.lo, op.span.hi)
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
                None    => pipe,
            };
            Some(pipe)
        }
        Err(ds)  => { diags.extend(ds); None }
    }
}

fn classify_call_arg(
    slot:  &SlotText,
    reg:   &Registry,
    ctx:   &mut LowerCtx,
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
    let value_start_in_trimmed = colon + 1 + trimmed[colon + 1..].len().saturating_sub(trimmed[colon + 1..].trim_start().len());
    let value_end_in_trimmed = value_start_in_trimmed + value.len();
    let lo = slot.span.lo.saturating_add((leading + value_start_in_trimmed) as u32);
    let hi = slot.span.lo.saturating_add((leading + value_end_in_trimmed) as u32);
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
    slot:  &SlotText,
    reg:   &Registry,
    ctx:   &mut LowerCtx,
    diags: &mut Vec<Diag>,
) -> Option<Value> {
    let raw = slot.raw.as_ref().trim();
    if raw.is_empty() {
        diags.push(
            Diag::error("compile/empty-slot",
                "empty slot body".to_string())
                .with_span(slot.span.lo, slot.span.hi));
        return None;
    }
    // `${X}` and `${X?}` are template-literal interpolation holes —
    // valid ONLY inside backtick DSL bodies (re/glob/ast/json/plain `` ` ``),
    // never at host-arg position. Like JS: `\`hello ${name}\`` is a hole;
    // `f(${name})` is not. Emit a clear diag so the fallthrough doesn't
    // bury the real cause.
    if let Some(idx) = raw.find("${") {
        let lo = slot.span.lo.saturating_add(idx as u32);
        let hi_rel = raw[idx..].find('}').map(|j| idx + j + 1).unwrap_or(raw.len());
        let hi = slot.span.lo.saturating_add(hi_rel as u32).min(slot.span.hi);
        diags.push(
            Diag::error("lang/host-hole-illegal",
                "`${X}` interpolation is only valid inside a backtick DSL body. \
                 Use a bareword (`X`) or `:atom` here, or move the hole inside \
                 a `` ` `` template.".to_string())
                .with_span(lo, hi));
        return None;
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
            Diag::error("compile/inline-pipe-malformed",
                format!("inline pipe arg parsed to {} pipes (expected 1)",
                    sub_pipes.len()))
                .with_span(slot.span.lo, slot.span.hi));
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

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') { return false; }
    bytes[1..].iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}

/// `[A-Z][A-Z0-9_]*` — ALL_CAPS identifier convention. Used to mark a
/// bareword in an arg slot as a term-ref (capture) rather than an atom.
fn is_caps_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    if !bytes[0].is_ascii_uppercase() { return false; }
    bytes[1..].iter().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}
