//! `$$` — resolve the implicit binding written by the upstream op.
//!
//! Looks up `cursor.last_bound` → `cursor.captures`. Narrows the cursor
//! to the binding's byte range so downstream ops see the captured span
//! as their active region. If `last_bound` is None or the named capture
//! is missing, the cursor is dropped (lower / LSP layer should already
//! have flagged this with a diag).

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;

pub struct AnsRefOp;

impl Op for AnsRefOp {
    fn name(&self) -> &'static str { "ans_ref" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let Some(cap) = c.ans_capture().cloned() else {
                return Vec::new();
            };
            vec![c.narrow(cap.byte_range)]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::CaptureWriteOp;
    use std::sync::Arc;

    #[tokio::test]
    async fn narrows_to_last_bound_capture() {
        let ctx = RtCtx::default();
        let c   = Cursor::new(Arc::from(b"hello world".as_slice())).narrow(6..11);
        let c   = CaptureWriteOp::new("WORD").pipe(&ctx, c).await.pop().unwrap();
        // Widen the cursor to prove ans_ref re-narrows.
        let c   = c.narrow(0..11);
        let out = AnsRefOp.pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].byte_range, 6..11);
        assert_eq!(out[0].active(), b"world");
    }

    #[tokio::test]
    async fn drops_when_no_last_bound() {
        let ctx = RtCtx::default();
        let c   = Cursor::new(Arc::from(b"data".as_slice()));
        let out = AnsRefOp.pipe(&ctx, c).await;
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn drops_when_capture_missing() {
        let ctx = RtCtx::default();
        let mut c = Cursor::new(Arc::from(b"data".as_slice()));
        c.last_bound = Some(Arc::from("MISSING"));
        let out = AnsRefOp.pipe(&ctx, c).await;
        assert!(out.is_empty());
    }
}
