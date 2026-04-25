//! `> void` — drop the cursor. Returns zero outputs.

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

#[derive(Debug)]
pub struct VoidOp;

impl Op for VoidOp {
    fn name(&self) -> &'static str { "void" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, _c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move { Vec::new() })
    }
}

impl OpCtor for VoidOp {
    const NAME: &'static str = "void";
    const BODY_GRAMMAR: &'static str = "none";
    const DOC:  &'static str = "\
**void**

Drop the cursor. Used at the tail of a fork arm to discard side-chains
after they write a relation or run an effect.
";

    fn from_values(_: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Ok(VoidOp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn drops_cursor() {
        let ctx = RtCtx::default();
        let c = Cursor::new(Arc::from(b"data".as_slice()));
        let out = VoidOp.pipe(&ctx, c).await;
        assert!(out.is_empty());
    }
}
