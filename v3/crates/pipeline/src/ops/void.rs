//! `> void` — drop the cursor. Returns zero outputs.

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;

pub struct VoidOp;

impl Op for VoidOp {
    fn name(&self) -> &'static str { "void" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, _c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move { Vec::new() })
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
