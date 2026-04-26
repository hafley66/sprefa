//! `read` — load file bytes into `cursor.content`.
//!
//! Takes no arguments. Requires an upstream `fs(...)` op to have bound
//! `cursor.fs`. Puts a [`ReadBytesEffect`] and rebases the cursor onto
//! the returned bytes with byte_range covering the whole file.
//!
//! Cursors whose `fs` is unset pass through unchanged; this lets a pipe
//! that intentionally operates on cursor.repo/rev without a file still
//! run. The lowerer does not know enough to forbid that shape.
//!
//! A response of `None` (source has no bytes for that coordinate) leaves
//! the cursor unchanged and emits no cursors downstream — the caller
//! sees an empty pipe, same as an `fs` listing miss. A present-but-empty
//! `Arc<[u8]>` is a legitimate empty file and the cursor is rebased onto
//! it with `byte_range = 0..0`.
//!
//! Spec: parse.md §14.5m.

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::effects::ReadBytesEffect;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;

#[derive(Debug)]
pub struct ReadOp;

impl ReadOp {
    pub fn new() -> Self {
        ReadOp
    }

    pub fn from_source(src: &str) -> Result<Self, Vec<PatternDiagnostic>> {
        let arg = src.trim();
        if !arg.is_empty() {
            return Err(vec![PatternDiagnostic {
                code: "read/unexpected-arg",
                message: "read takes no arguments (e.g. fs(**) > read)".into(),
                byte_range: 0..src.len(),
            }]);
        }
        Ok(ReadOp)
    }
}

impl OpCtor for ReadOp {
    const NAME: &'static str = "read";
    const BODY_GRAMMAR: &'static str = "none";
    const DOC:  &'static str = "\
**read**

Load bytes of the file at `cursor.fs` into `cursor.content`, via
`ReadBytesEffect` (cached per `(repo, rev, path)`). Pairs with `fs`:

```
fs(**/*.rs) > read > comment(\"// TODO:\") > print
```

Cursors without an upstream `fs` bind pass through unchanged. A read
miss drops the cursor from the pipe.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        if values.is_empty() {
            Ok(ReadOp)
        } else {
            Err(vec![PatternDiagnostic {
                code: "read/unexpected-arg",
                message: "read takes no arguments (e.g. fs(**) > read)".into(),
                byte_range: 0..0,
            }])
        }
    }
}

impl Op for ReadOp {
    fn name(&self) -> &'static str { "read" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: std::sync::Arc<[Cursor]>) -> BoxFuture<'a, std::sync::Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            // Delegate to ensure_content_loaded so the content_hash
            // plumbing (sprefa-upb Phase B) lives in one place.
            // Cursors without `fs` pass through (Some(c) unchanged);
            // miss returns None and the cursor drops.
            match crate::effects::ensure_content_loaded(ctx, c).await {
                Some(c) => vec![c],
                None    => Vec::new(),
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Arc;

    use crate::effects::ReadBytesBatcher;
    use crate::readers::{FileSource, MemFileSource};
    use effect_runtime::RtCtxBuilder;

    fn rt(src: Arc<dyn FileSource>) -> effect_runtime::RtCtx {
        RtCtxBuilder::new()
            .register_pure::<ReadBytesEffect, _>(16, ReadBytesBatcher::new(src))
            .build()
    }

    fn cursor_with_fs(repo: &str, rev: &str, path: &str) -> Cursor {
        let mut c = Cursor::default();
        c.repo = Arc::from(repo);
        c.rev = Arc::from(rev);
        c.fs = Some(Arc::from(Path::new(path)));
        c
    }

    #[tokio::test]
    async fn rebase_and_miss_and_no_fs() {
        let src: Arc<dyn FileSource> = Arc::new(
            MemFileSource::new().with_file_bytes("r", "main", "a.rs", b"hello world"),
        );
        let ctx = rt(src);
        let op = ReadOp;

        // Hit: cursor rebases onto the bytes.
        let out = op.pipe(&ctx, Arc::from(vec![cursor_with_fs("r", "main", "a.rs")])).await;
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].content, b"hello world");
        assert_eq!(out[0].byte_range, 0..11);

        // Phase B: content_hash populated by ensure_content_loaded.
        // Same bytes -> same blake3; bytes differ -> hash differs.
        let h_a = out[0].content_hash.clone().expect("hash set");
        let out2 = op.pipe(&ctx, Arc::from(vec![cursor_with_fs("r", "main", "a.rs")])).await;
        assert_eq!(out2[0].content_hash.as_deref(), Some(&*h_a));

        let src2: Arc<dyn FileSource> = Arc::new(
            MemFileSource::new().with_file_bytes("r", "main", "a.rs", b"hello other"),
        );
        let ctx2 = rt(src2);
        let out3 = op.pipe(&ctx2, Arc::from(vec![cursor_with_fs("r", "main", "a.rs")])).await;
        assert_ne!(out3[0].content_hash.as_deref(), Some(&*h_a));

        // Miss: unknown path → empty output.
        let out = op.pipe(&ctx, Arc::from(vec![cursor_with_fs("r", "main", "gone.rs")])).await;
        assert!(out.is_empty());

        // No fs: cursor passes through unchanged.
        let mut c = Cursor::default();
        c.repo = Arc::from("r");
        c.rev = Arc::from("main");
        let out = op.pipe(&ctx, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 1);
        assert!(out[0].fs.is_none());
    }

    #[test]
    fn rejects_args() {
        let err = ReadOp::from_source("anything").unwrap_err();
        assert_eq!(err[0].code, "read/unexpected-arg");
    }

    #[test]
    fn empty_body_accepted() {
        assert!(ReadOp::from_source("   ").is_ok());
    }
}
