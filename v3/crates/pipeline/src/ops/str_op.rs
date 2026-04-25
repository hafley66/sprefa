//! `str(...)` — constant string literal op (§14.2).
//!
//! No sub-grammar, no term_ref parsing, no captures. The paren body is
//! read as raw bytes at lower time (from `ParseSite`) and stored here
//! as `Arc<str>`. At runtime the op attaches its value to the cursor
//! under a [`StrValue`] slot so downstream ops can read it by type.
//!
//! `str` is the only "pattern op" in the injection registry sense that
//! is a plain [`Op`] (not a [`crate::PatternOp`]). Per spec §14.5
//! closing note, `$NAME` inside `str(...)` is literal bytes — the
//! grammar does not even parse it.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

/// Compiled call-site for `str(...)`. One instance per invocation in
/// the lowered pipeline; `body` is the exact paren-body bytes as UTF-8.
#[derive(Debug)]
pub struct StrOp {
    pub body: Arc<str>,
}

impl StrOp {
    pub fn new(body: impl Into<Arc<str>>) -> Self {
        Self { body: body.into() }
    }
}

/// Typed slot key for the value produced by `str(...)`. Downstream ops
/// read it via `cursor.get_slot::<StrValue>()`.
#[derive(Clone)]
pub struct StrValue(pub Arc<str>);

impl OpCtor for StrOp {
    const NAME: &'static str = "str";
    const BODY_GRAMMAR: &'static str = "raw bytes";
    const DOC:  &'static str = "\
**str**(_bytes_)

Constant bytes, stashed in a slot for downstream consumers. No sub-
grammar; `$NAME` inside the body is literal text (§14.2).
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        match values.as_slice() {
            [] => Ok(StrOp::new("")),
            [Value::Str(s)] | [Value::Atom(s)] => Ok(StrOp::new(s.clone())),
            _ => Err(vec![PatternDiagnostic {
                code: "value/wrong-kind",
                message: "str takes a single string body".into(),
                byte_range: 0..0,
            }]),
        }
    }
}

impl Op for StrOp {
    fn name(&self) -> &'static str { "str" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let mut out = c;
            out.put_slot(StrValue(self.body.clone()));
            vec![out]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stores_body_verbatim_as_slot() {
        let ctx = RtCtx::default();
        let c = Cursor::new(Arc::from(b"cursor-content".as_slice()));
        let op = StrOp::new("hello world");
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        let v = out[0].get_slot::<StrValue>().expect("slot populated");
        assert_eq!(&*v.0, "hello world");
    }

    #[tokio::test]
    async fn dollar_sequence_is_literal() {
        // str is const: $NAME inside is bytes, never a term_ref.
        let ctx = RtCtx::default();
        let c = Cursor::new(Arc::from(b"".as_slice()));
        let op = StrOp::new("hello $NAME world");
        let v = op.pipe(&ctx, c).await.pop().unwrap()
            .get_slot::<StrValue>().unwrap();
        assert_eq!(&*v.0, "hello $NAME world");
    }

    #[tokio::test]
    async fn cursor_range_and_content_untouched() {
        let ctx = RtCtx::default();
        let c = Cursor::new(Arc::from(b"xyz".as_slice())).narrow(1..2);
        let out = StrOp::new("k").pipe(&ctx, c).await.pop().unwrap();
        assert_eq!(out.byte_range, 1..2);
        assert_eq!(out.active(), b"y");
    }
}
