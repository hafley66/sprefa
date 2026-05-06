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
//! Inline-pipe args (`name | next`-style positional values) are not yet
//! supported here — the parser lane has not landed. Slots that don't
//! classify as Atom or `:Atom` emit `compile/inline-pipe-unsupported`
//! and the call is skipped; the rest of the program continues to walk.

use std::sync::Arc;

use effect_runtime::v2::{ByteRange, Diag, Pipe};

use crate::Cursor;

use super::ast::{OpCall, PipeAst, SlotText};
use super::lower::ctx::LowerCtx;
use super::lower::op_def::{DslBody, DslInterp};
use super::lower::registry::Registry;
use super::lower::value::Value;

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
    for op in &p.steps {
        match walk_op(op, reg, ctx, diags) {
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
    op:    &OpCall,
    reg:   &Registry,
    ctx:   &mut LowerCtx,
    diags: &mut Vec<Diag>,
) -> Option<Pipe<Cursor>> {
    // Resolve flow slot.
    let flow: Option<(Value, ByteRange)> = match &op.flow {
        Some(slot) => match classify_slot(slot, reg, ctx, diags) {
            Some(v) => Some((v, slot.span)),
            None    => return None,
        },
        None => None,
    };

    // Resolve paren args.
    let mut args: Vec<(Value, ByteRange)> = Vec::with_capacity(op.args.len());
    let mut had_arg_err = false;
    for slot in &op.args {
        match classify_slot(slot, reg, ctx, diags) {
            Some(v) => args.push((v, slot.span)),
            None    => { had_arg_err = true; }
        }
    }
    if had_arg_err { return None; }

    // Resolve dsl body via the op's parse_dsl. Unknown op → registry
    // will emit `lower/unknown-op` below; here we just skip dsl parse.
    let dsl: Option<(DslBody, ByteRange)> = match &op.dsl {
        Some(dsl_text) => {
            let interps: Vec<DslInterp> = match reg.get(&op.name) {
                Some(def) => match def.parse_dsl(&dsl_text.raw) {
                    Ok(v)  => v,
                    Err(e) => {
                        diags.push(
                            Diag::error("compile/dsl-parse",
                                format!("op `{}` dsl parse: {e}", op.name))
                                .with_span(dsl_text.span.lo, dsl_text.span.hi));
                        return None;
                    }
                },
                None => Vec::new(),
            };
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

    // Dispatch.
    match reg.lower(ctx, &op.name, flow, args, block, dsl, op.span) {
        Ok(pipe) => Some(pipe),
        Err(ds)  => { diags.extend(ds); None }
    }
}

/// Classify a `SlotText` into `Value::Atom` or `Value::Pipe`.
///
/// Today: `:foo` → Atom("foo"); bare identifier / quoted string → Atom.
/// Anything else → emit `compile/inline-pipe-unsupported` and return
/// `None`. The full inline-pipe parser is parser-lane work.
pub fn classify_slot(
    slot:  &SlotText,
    _reg:  &Registry,
    _ctx:  &mut LowerCtx,
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
    // `:foo` colon-prefixed atom.
    if let Some(rest) = raw.strip_prefix(':') {
        if !rest.is_empty() {
            return Some(Value::Atom(Arc::<str>::from(rest)));
        }
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
    // Anything else looks like an inline pipe. Not implemented in this
    // lane.
    diags.push(
        Diag::error("compile/inline-pipe-unsupported",
            format!("inline pipe args not yet supported: `{raw}`"))
            .with_span(slot.span.lo, slot.span.hi));
    None
}

fn is_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') { return false; }
    bytes[1..].iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}
