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

use effect_runtime::v2::Pipe;

use crate::Cursor;
use crate::chan::{Next, NextQ};
use crate::fact::{FactRead, FactWrite};
use crate::term::Term;
use crate::v2_ops::{FsComponent, ReComponent};
use crate::compile::lower::ctx::{LowerCtx, LowerError};
use crate::compile::lower::op_def::{
    ArgKind, ArgSig, BlockShape, DslBody, DslShape, OperatorDef,
};
use crate::compile::lower::value::{run_once_const, Value};
use crate::pipeline::{GlobComponent, StrConstComponent, StrTemplateComponent};
use crate::rule::Rule;

// ─── str ──────────────────────────────────────────────────────────────────

pub struct StrDef;

impl OperatorDef for StrDef {
    fn name(&self) -> &'static str { "str" }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }

    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        _args:  &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
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
            raw:     body.raw.clone(),
            interps: Arc::new(interps),
        })))
    }
}

// ─── rule ─────────────────────────────────────────────────────────────────

pub struct RuleDef;

const RULE_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom, name: "name",
        doc: "rule + sink table name", required: true,
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
        name: "cols", doc: "sink columns", required: false,
    },
];

impl OperatorDef for RuleDef {
    fn name(&self) -> &'static str { "rule" }
    fn paren_args(&self) -> &[ArgSig] { RULE_SPEC }
    fn brace_block(&self) -> Option<BlockShape> { Some(BlockShape::Pipe) }

    fn lower(
        &self,
        ctx:   &LowerCtx,
        _flow: Option<Value>,
        args:  &[Value],
        block: Option<Pipe<Cursor>>,
        _dsl:  Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let name = match &args[0] {
            Value::Atom(s) => s.clone(),
            _ => unreachable!("validate ensured Atom"),
        };
        // Column args. Plain atoms become declared sink columns; pipe
        // args (the ALL_CAPS bareword desugar — `NAME` / `NAME?`) are
        // schema-silent for now; the runtime read/bind still happens
        // through the pipeline expansion via cursor.terms.
        let mut col_strings: Vec<String> = Vec::with_capacity(args.len().saturating_sub(1));
        for a in &args[1..] {
            if let Value::Atom(s) = a {
                col_strings.push(s.to_string());
            }
        }
        let cols: Vec<&str> = col_strings.iter().map(|s| s.as_str()).collect();
        let body = block.expect("validate ensured Pipe block");
        let rule = Rule::new(
            name.clone(),
            ctx.store.clone(),
            name,
            &cols,
            body,
        );
        Ok(rule.into_pipe())
    }
}

// ─── fact_read ────────────────────────────────────────────────────────────

pub struct FactReadDef;

const FACT_READ_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom, name: "table",
        doc: "fact table name", required: true,
    },
    ArgSig {
        kind: ArgKind::Atom, name: "key_term",
        doc: "cursor term used as join key", required: true,
    },
    ArgSig {
        kind: ArgKind::Variadic(&ArgKind::Atom),
        name: "project", doc: "projected col names", required: false,
    },
];

impl OperatorDef for FactReadDef {
    fn name(&self) -> &'static str { "fact_read" }
    fn paren_args(&self) -> &[ArgSig] { FACT_READ_SPEC }

    fn lower(
        &self,
        ctx:    &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let atom = |i: usize| -> Arc<str> {
            match &args[i] {
                Value::Atom(s) => s.clone(),
                _ => unreachable!("validate ensured Atom"),
            }
        };
        let table    = atom(0);
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
            ctx.store.clone(), table, key_term, &project_refs,
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
fn atom_string_vec(args: &[Value]) -> Vec<String> {
    args.iter().map(|a| match a {
        Value::Atom(s) => s.to_string(),
        _ => unreachable!("validate ensured Atom"),
    }).collect()
}

// ─── fs ───────────────────────────────────────────────────────────────────

pub struct FsDef;

const FS_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Variadic(&ArgKind::Atom),
        name: "exts",
        doc: "extensions to include (e.g. rs, ts). Empty = no filter.",
        required: false,
    },
];

impl OperatorDef for FsDef {
    fn name(&self) -> &'static str { "fs" }
    fn paren_args(&self) -> &[ArgSig] { FS_SPEC }

    fn lower(
        &self,
        ctx:    &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // TODO: LowerCtx batch-size knob; hardcoded for now.
        let exts = atom_string_vec(args);
        Ok(Pipe::new().step(Arc::new(FsComponent::new(
            ctx.root.clone(), exts, 1024,
        ))))
    }
}

// ─── glob ─────────────────────────────────────────────────────────────────

pub struct GlobDef;

const GLOB_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom, name: "pattern",
        doc: "glob pattern (e.g. **/*.rs); matched against paths \
              relative to ctx.root",
        required: true,
    },
];

impl OperatorDef for GlobDef {
    fn name(&self) -> &'static str { "glob" }
    fn paren_args(&self) -> &[ArgSig] { GLOB_SPEC }

    fn lower(
        &self,
        ctx:    &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let pattern = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(GlobComponent::new(
            ctx.root.clone(), pattern,
        ))))
    }
}

// ─── ast ──────────────────────────────────────────────────────────────────
// `AstNmComponent::new(pat_src, lang)` needs a `SupportLang` that
// `OperatorDef::lower` can't surface today. Lane C's `parse_dsl`
// extension should carry a Lang alongside the pattern source. Until
// then the wrapper registers the name (so the LSP/Registry sees it)
// and errors at lower time.

pub struct AstDef;

impl OperatorDef for AstDef {
    fn name(&self) -> &'static str { "ast" }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }

    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        _args:  &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // TODO: LowerCtx needs a Lang threading mechanism; awaiting
        // Lane C parse_dsl extension on OperatorDef.
        Err(LowerError::Unknown(
            "ast: Lang threading not yet wired (Lane C parse_dsl pending)".into()
        ))
    }
}

// ─── re ───────────────────────────────────────────────────────────────────

pub struct ReDef;

impl OperatorDef for ReDef {
    fn name(&self) -> &'static str { "re" }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }

    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        _args:  &[Value],
        _block: Option<Pipe<Cursor>>,
        dsl:    Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let body = dsl.expect("re: dsl body present (validate)");
        // TODO: surface named captures via Lane C parse_dsl so
        // ReComponent's `capture_names` can be populated.
        Ok(Pipe::new().step(Arc::new(ReComponent::new(body.raw.as_ref(), &[]))))
    }
}

// ─── term (read) ──────────────────────────────────────────────────────────

const TERM_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom, name: "name",
        doc: "term (capture) name", required: true,
    },
];

pub struct TermReadDef;

impl OperatorDef for TermReadDef {
    fn name(&self) -> &'static str { "term" }
    fn paren_args(&self) -> &[ArgSig] { TERM_SPEC }

    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let name = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(Term::read(name))))
    }
}

// ─── term_bind ────────────────────────────────────────────────────────────

pub struct TermBindDef;

impl OperatorDef for TermBindDef {
    fn name(&self) -> &'static str { "term_bind" }
    fn paren_args(&self) -> &[ArgSig] { TERM_SPEC }

    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let name = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(Term::bind(name))))
    }
}

// ─── fact_write ───────────────────────────────────────────────────────────

pub struct FactWriteDef;

const FACT_WRITE_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom, name: "table",
        doc: "fact table name", required: true,
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
    fn name(&self) -> &'static str { "fact_write" }
    fn paren_args(&self) -> &[ArgSig] { FACT_WRITE_SPEC }

    fn lower(
        &self,
        ctx:    &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        // TODO: `FactWrite::new` only takes the table name today. Cols
        // are accepted by the slot spec for arity/LSP but discarded
        // until FactWrite grows column projection.
        let table = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(FactWrite::new(ctx.store.clone(), table))))
    }
}

// ─── next / next_q ────────────────────────────────────────────────────────

const CHAN_SPEC: &[ArgSig] = &[
    ArgSig {
        kind: ArgKind::Atom, name: "chan",
        doc: "channel name", required: true,
    },
];

pub struct NextDef;

impl OperatorDef for NextDef {
    fn name(&self) -> &'static str { "next" }
    fn paren_args(&self) -> &[ArgSig] { CHAN_SPEC }

    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let chan = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(Next::new(chan))))
    }
}

pub struct NextQDef;

impl OperatorDef for NextQDef {
    fn name(&self) -> &'static str { "next_q" }
    fn paren_args(&self) -> &[ArgSig] { CHAN_SPEC }

    fn lower(
        &self,
        _ctx:   &LowerCtx,
        _flow:  Option<Value>,
        args:   &[Value],
        _block: Option<Pipe<Cursor>>,
        _dsl:   Option<&DslBody>,
    ) -> Result<Pipe<Cursor>, LowerError> {
        let chan = atom_arg(args, 0);
        Ok(Pipe::new().step(Arc::new(NextQ::new(chan))))
    }
}
