use std::sync::Arc;

use effect_runtime::v2::{Component, Node, Pipe, RenderCtx};

use crate::Cursor;
use crate::lower::ctx::{LowerCtx, LowerError};
use crate::lower::op_def::{DslBody, DslShape, OperatorDef};
use crate::lower::value::{run_once_const, Value};

pub struct StrConstComponent {
    pub literal: Arc<str>,
}

impl Component for StrConstComponent {
    type Next = Cursor;

    fn render(&self, _ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
        let mut next = c.clone();
        next.value = self.literal.clone();
        Node::Emit(Arc::new(next))
    }
}

/// Convenience builder: a constant-emitting `Pipe<Cursor>`. Used by
/// callers (and by ctx.bindings) to wrap a plain string as a Value.
pub fn str_pipe(s: &str) -> Pipe<Cursor> {
    Pipe::new().step(Arc::new(StrConstComponent { literal: Arc::from(s) }))
}

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
        // validate guarantees dsl is Some.
        let body = dsl.expect("str: dsl body present (validate)");
        if body.interps.is_empty() {
            return Ok(Pipe::new().step(Arc::new(StrConstComponent {
                literal: body.raw.clone(),
            })));
        }
        // Substitute interpolations at lower-time. Walk byte ranges in
        // order; copy raw between, ground the binding to a string at
        // each hole.
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
