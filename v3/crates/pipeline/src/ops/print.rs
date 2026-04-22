//! `print([prefix])` — emit `cursor.active()` via `PrintEffect`.
//!
//! The op is a side-tap: the cursor flows through unchanged, and one
//! line is sent to the registered `PrintBatcher` per input cursor.
//! This is the first write-side effect in the v3 pipeline, exercising
//! the non-pure `RtCtxBuilder::register` path (vs the cache-aware
//! `register_pure` that `FsListFilesEffect` rides).
//!
//! Optional prefix: `print("tag")` prepends `tag: ` to every line so
//! several `print` sites in the same pipe stay distinguishable in the
//! sink output.
//!
//! Spec: parse.md §14.5k.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::effects::PrintEffect;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{ArgSpec, Value};

const PRINT_ARG_SPEC: &[ArgSpec] = &[ArgSpec::Str];

#[derive(Debug)]
pub struct PrintOp {
    prefix: Option<Arc<str>>,
}

impl PrintOp {
    /// Construct from lowered `Vec<Value>`. No args → no prefix; one
    /// `Atom`/`Str` arg → prefix. Anything else is a diagnostic.
    pub fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        match values.as_slice() {
            [] => Ok(PrintOp { prefix: None }),
            [Value::Atom(s)] | [Value::Str(s)] => Ok(PrintOp { prefix: Some(s.clone()) }),
            [_] => Err(vec![PatternDiagnostic {
                code: "value/wrong-kind",
                message: "print prefix must be a string or atom".into(),
                byte_range: 0..0,
            }]),
            _ => Err(vec![PatternDiagnostic {
                code: "print/too-many-args",
                message: "print accepts at most one prefix argument".into(),
                byte_range: 0..0,
            }]),
        }
    }

    pub fn from_source(src: &str) -> Result<Self, Vec<PatternDiagnostic>> {
        let arg = src.trim();
        if arg.is_empty() {
            return Ok(PrintOp { prefix: None });
        }
        // Accept quoted and unquoted forms. Bodies with commas are
        // rejected for now — a future shape takes more slots.
        if arg.contains(',') {
            return Err(vec![PatternDiagnostic {
                code: "print/too-many-args",
                message: "print accepts at most one prefix argument".into(),
                byte_range: 0..src.len(),
            }]);
        }
        let cleaned = trim_quotes(arg);
        Ok(PrintOp { prefix: Some(Arc::from(cleaned)) })
    }
}

impl OpCtor for PrintOp {
    const NAME: &'static str = "print";
    const DOC:  &'static str = "\
**print**([_prefix_])

Emit `cursor.active()` as one line via `PrintEffect`. Optional prefix
prepended with `: ` separator. The cursor flows through unchanged;
useful as a side-tap for debugging or as the first write-side effect
to exercise the runtime.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        Self::from_values(values)
    }
}

impl Op for PrintOp {
    fn name(&self) -> &'static str { "print" }

    fn arg_spec(&self) -> &[ArgSpec] { PRINT_ARG_SPEC }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };
            let active = c.active();
            let body = String::from_utf8_lossy(active);
            let line = match &self.prefix {
                Some(p) => format!("{p}: {body}"),
                None => body.into_owned(),
            };
            ctx.put(PrintEffect { line: Arc::from(line) }).await;
            vec![c]
        })
    }
}

fn trim_quotes(s: &str) -> &str {
    if s.len() >= 2 {
        let b = s.as_bytes();
        if (b[0] == b'"' && b[b.len() - 1] == b'"')
            || (b[0] == b'\'' && b[b.len() - 1] == b'\'')
        {
            return &s[1..s.len() - 1];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::{PrintBatcher, PrintEffect};
    use effect_runtime::RtCtxBuilder;

    #[tokio::test]
    async fn emits_active_bytes_and_passes_cursor_through() {
        let (batcher, buf) = PrintBatcher::buffer();
        let rt = RtCtxBuilder::new().register::<PrintEffect, _>(batcher).build();

        // No prefix: line is exactly the active slice.
        let op = PrintOp::from_source("").unwrap();
        let c = Cursor::new(Arc::from(b"hello world".as_slice())).narrow(0..5);
        let out = op.pipe(&rt, c).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].byte_range, 0..5);

        // With prefix: "tag: active".
        let op = PrintOp::from_source("tag").unwrap();
        let c = Cursor::new(Arc::from(b"hello world".as_slice())).narrow(6..11);
        op.pipe(&rt, c).await;

        let lines = buf.lock().unwrap().clone();
        assert_eq!(lines, vec!["hello".to_string(), "tag: world".to_string()]);
    }

    #[test]
    fn diagnostics() {
        assert_eq!(
            PrintOp::from_source("a, b").unwrap_err()[0].code,
            "print/too-many-args"
        );
    }

    #[test]
    fn quoted_prefix_unwraps() {
        let op = PrintOp::from_source("\"quoted\"").unwrap();
        assert_eq!(op.prefix.as_deref(), Some("quoted"));
    }
}
