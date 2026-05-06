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
use crate::fact::FactRead;
use crate::lower::ctx::{LowerCtx, LowerError};
use crate::lower::op_def::{
    ArgKind, ArgSig, BlockShape, DslBody, DslShape, OperatorDef,
};
use crate::lower::value::{run_once_const, Value};
use crate::pipeline::StrConstComponent;
use crate::rule::Rule;

// ─── str ──────────────────────────────────────────────────────────────────

pub struct StrDef;

impl OperatorDef for StrDef {
    fn name(&self) -> &'static str { "str" }
    fn dsl_body(&self) -> Option<DslShape> { Some(DslShape::Plain) }

    fn lower(
        &self,
        ctx:    &LowerCtx,
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
        let raw = body.raw.as_ref();
        let mut interps = body.interps.clone();
        interps.sort_by_key(|i| i.range.lo);
        let mut out = String::with_capacity(raw.len());
        let mut cursor: usize = 0;
        for interp in &interps {
            let lo = interp.range.lo as usize;
            let hi = interp.range.hi as usize;
            if lo < cursor || hi > raw.len() || lo > hi {
                return Err(LowerError::Unknown(format!(
                    "str: bad interp range for ${{{}}}: {}..{}",
                    interp.name, lo, hi)));
            }
            out.push_str(&raw[cursor..lo]);
            let pipe = ctx.bindings.get(&interp.name).ok_or_else(||
                LowerError::UnboundCapture(interp.name.clone()))?;
            out.push_str(&run_once_const(pipe, ctx)?);
            cursor = hi;
        }
        out.push_str(&raw[cursor..]);
        Ok(Pipe::new().step(Arc::new(StrConstComponent {
            literal: Arc::from(out.as_str()),
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
        kind: ArgKind::Variadic(&ArgKind::Atom),
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
        let mut col_strings: Vec<String> = Vec::with_capacity(args.len().saturating_sub(1));
        for a in &args[1..] {
            match a {
                Value::Atom(s) => col_strings.push(s.to_string()),
                _ => unreachable!("validate ensured Atom"),
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
