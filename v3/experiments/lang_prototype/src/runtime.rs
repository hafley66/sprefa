use super::cursor::{Cursor, PathSeg, SprfPath};
use super::diag::{Diag, Diags};
use super::lower::{Callee, CursorRef, OpAction, OpCall, Pipeline, Rule, Scalar, Term, TermMode};
use super::ops::{self, OpCtx};
use super::store::Store;
use std::sync::Arc;

fn empty_cursor() -> Cursor {
    Cursor::new(SprfPath::default(), Arc::from(&[][..]))
}

pub struct Ctx<'a> {
    pub rules: &'a [Rule],
    pub store: &'a mut Store,
    pub diags: &'a mut Diags,
}

pub fn run_pipeline(ctx: &mut Ctx, pipeline: &Pipeline, input: Vec<Cursor>) -> Result<Vec<Cursor>, Diags> {
    match pipeline {
        Pipeline::Op(call) => run_op_call(ctx, call, input),
        Pipeline::Chain(stages) => {
            let mut cur = input;
            for stage in stages {
                cur = run_pipeline(ctx, stage, cur)?;
            }
            Ok(cur)
        }
        Pipeline::Group(inner) => run_pipeline(ctx, inner, input),
        Pipeline::Fork(arms) => {
            let mut out = Vec::new();
            for (i, arm) in arms.iter().enumerate() {
                let mut arm_out = run_pipeline(ctx, arm, input.clone())?;
                for c in &mut arm_out {
                    c.path.push(PathSeg::ForkArm(i));
                }
                out.extend(arm_out);
            }
            Ok(out)
        }
    }
}

fn run_op_call(ctx: &mut Ctx, call: &OpCall, input: Vec<Cursor>) -> Result<Vec<Cursor>, Diags> {
    match &call.callee {
        Callee::Builtin(name) => {
            let mut all_out = Vec::new();
            if input.is_empty() {
                // Source ops produce even with no input
                let modes = resolve_terms_no_cursor(&call.terms)?;
                let mut op_ctx = OpCtx { store: ctx.store, diags: ctx.diags };
                match dispatch_builtin(&mut op_ctx, name, &modes, vec![])? {
                    OpAction::Emit(cursors) => all_out.extend(cursors),
                    OpAction::Drop => {}
                }
            } else {
                for c in input {
                    let modes = resolve_terms(&call.terms, &c)?;
                    let mut op_ctx = OpCtx { store: ctx.store, diags: ctx.diags };
                    match dispatch_builtin(&mut op_ctx, name, &modes, vec![c])? {
                        OpAction::Emit(cursors) => all_out.extend(cursors),
                        OpAction::Drop => {}
                    }
                }
            }
            Ok(all_out)
        }
        Callee::Rule(name) => {
            let rule = ctx.rules.iter().find(|r| r.name.as_ref() == name.as_ref())
                .ok_or_else(|| vec![Diag::new(format!("rule not found: {name}"))])?
                .clone();
            let mut out = Vec::new();
            if input.is_empty() {
                // No upstream cursors: seed once. Bind params against an empty cursor
                // so literal args evaluate to Bound, term refs to Unbound.
                let seed = empty_cursor();
                let mut next = seed.clone();
                for (param, term) in rule.params.iter().zip(call.terms.iter()) {
                    if let Ok(val) = eval_term(term, &seed) {
                        next.captures.insert(param.slot, val);
                    }
                }
                let input_vec = if rule.params.is_empty() && call.terms.is_empty() {
                    vec![]
                } else {
                    vec![next]
                };
                let result = run_pipeline(ctx, &rule.body, input_vec)?;
                out.extend(result);
            } else {
                for c in input {
                    let mut next = c.clone();
                    for (param, term) in rule.params.iter().zip(call.terms.iter()) {
                        let val = eval_term(term, &next)?;
                        next.captures.insert(param.slot, val);
                    }
                    let result = run_pipeline(ctx, &rule.body, vec![next])?;
                    out.extend(result);
                }
            }
            Ok(out)
        }
    }
}

fn dispatch_builtin(ctx: &mut OpCtx, name: &str, modes: &[TermMode], input: Vec<Cursor>) -> Result<OpAction, Diags> {
    match name.as_ref() {
        "fs" => ops::fs::run(ctx, modes, input),
        "re" => ops::re::run(ctx, modes, input),
        "tag" => ops::tag::run(ctx, modes, input),
        "tags" => ops::tags::run(ctx, modes, input),
        _ => Err(vec![Diag::new(format!("unknown builtin: {name}"))]),
    }
}

fn resolve_terms_no_cursor(terms: &[Term]) -> Result<Vec<TermMode>, Diags> {
    terms.iter().map(|t| {
        match t {
            Term::Scalar(v) => Ok(TermMode::Bound(v.clone())),
            _ => Err(vec![Diag::new("unbound term in source op with no cursor")]),
        }
    }).collect()
}

fn resolve_terms(terms: &[Term], cursor: &Cursor) -> Result<Vec<TermMode>, Diags> {
    terms.iter().map(|t| eval_term_mode(t, cursor)).collect()
}

fn eval_term_mode(term: &Term, cursor: &Cursor) -> Result<TermMode, Diags> {
    Ok(match term {
        Term::Scalar(v) => TermMode::Bound(v.clone()),
        Term::CaptureRef(slot) => {
            match cursor.captures.get(*slot) {
                Some(v) => TermMode::Bound(v.clone()),
                None => TermMode::Unbound,
            }
        }
        Term::CursorRef(CursorRef::Capture(slot)) => {
            match cursor.captures.get(*slot) {
                Some(v) => TermMode::Bound(v.clone()),
                None => TermMode::Unbound,
            }
        }
        Term::CursorRef(CursorRef::Content) => {
            TermMode::Bound(Scalar::String(String::from_utf8_lossy(cursor.active()).into()))
        }
        Term::Xref { .. } => {
            // Prototype: simplified xref — not yet resolved at runtime
            TermMode::Unbound
        }
        Term::Carveout(_) => {
            return Err(vec![Diag::new("carveouts not implemented in prototype")]);
        }
    })
}

fn eval_term(term: &Term, cursor: &Cursor) -> Result<Scalar, Diags> {
    match eval_term_mode(term, cursor)? {
        TermMode::Bound(v) => Ok(v),
        TermMode::Unbound => Err(vec![Diag::new("unbound term where bound required")]),
    }
}
