//! `> $TARGET` — assign the cursor's active byte range to a capture
//! named `TARGET`. Sets `last_bound` so a downstream `$$` resolves
//! without naming.
//!
//! Does not change `content` or `byte_range`. The cursor flows through
//! unchanged except for the appended `Capture` and the `last_bound`
//! update.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;

pub struct CaptureWriteOp {
    pub target: Arc<str>,
}

impl CaptureWriteOp {
    pub fn new(target: impl Into<Arc<str>>) -> Self {
        Self { target: target.into() }
    }
}

impl Op for CaptureWriteOp {
    fn name(&self) -> &'static str { "capture_write" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let mut out = c;
            out.captures.push(Capture {
                name:       self.target.clone(),
                byte_range: out.byte_range.clone(),
            });
            out.last_bound = Some(self.target.clone());
            vec![out]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn writes_capture_and_sets_last_bound() {
        let ctx = RtCtx::default();
        let c = Cursor::new(Arc::from(b"hello".as_slice())).narrow(0..3);
        let op = CaptureWriteOp::new("X");
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        let r = &out[0];
        assert_eq!(r.captures.len(), 1);
        assert_eq!(&*r.captures[0].name, "X");
        assert_eq!(r.captures[0].byte_range, 0..3);
        assert_eq!(r.last_bound.as_deref(), Some("X"));
        // Active range untouched.
        assert_eq!(r.byte_range, 0..3);
    }

    #[tokio::test]
    async fn second_write_shadows_first_in_capture_lookup() {
        let ctx = RtCtx::default();
        let c = Cursor::new(Arc::from(b"hello".as_slice()));
        let mut out = CaptureWriteOp::new("X").pipe(&ctx, c).await.pop().unwrap();
        out.byte_range = 1..2;
        let out = CaptureWriteOp::new("X").pipe(&ctx, out).await.pop().unwrap();
        // Capture lookup returns the most recent one.
        let cap = out.capture("X").unwrap();
        assert_eq!(cap.byte_range, 1..2);
    }
}
