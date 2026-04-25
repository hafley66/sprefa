//! `write_file(<path>)` — emit `cursor.active()` bytes to a path.
//!
//! Polarity: read-side ops (`fs`, `read`) load bytes onto a cursor;
//! `write_file` is the inverse — bytes leave the cursor for durable
//! state. Cursor flows through unchanged, like `print`.
//!
//! Path forms (first and only positional arg):
//!   * `Value::Str` / `Value::Atom`        — literal path.
//!   * `Value::Term { mode: Read }`        — bound capture's bytes
//!                                           interpreted as a UTF-8 path.
//!   * `Value::Op` — sub-op evaluated at runtime against the input
//!                   cursor; the resulting cursor's `StrValue` slot
//!                   supplies the path. Idiomatic form is
//!                   `write_file(str("dir/", ${X}, ".out"))`.
//!
//! Diagnostics:
//!   * No args                                     → `write_file/missing-path`.
//!   * More than one arg                           → `write_file/too-many-args`.
//!   * `Value::Term` in `Unbound` mode             → `term/unbound-not-allowed-in-write-file`.
//!   * Sub-op produces no `StrValue` slot at run   → `write_file/no-path` (runtime).
//!
//! Effect: [`WriteFileEffect`]. The runtime sink (`WriteFileSink::Disk`
//! or `Buffer`) decides whether bytes hit disk or a test buffer.
//!
//! Spec: parse.md §14.5 (write side, currently undocumented; this op
//! lands the surface).

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::effects::WriteFileEffect;
use crate::op_ctor::OpCtor;
use crate::ops::str_op::StrValue;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{TermMode, Value};

#[derive(Debug)]
pub enum PathSpec {
    Literal(Arc<str>),
    Term(Arc<str>),
    SubOp(Arc<dyn Op>),
}

#[derive(Debug)]
pub struct WriteFileOp {
    pub path: PathSpec,
}

impl OpCtor for WriteFileOp {
    const NAME: &'static str = "write_file";
    const BODY_GRAMMAR: &'static str = "single path expression";
    const DOC:  &'static str = "\
**write_file**(_path_)

Emit `cursor.active()` bytes to the given path. `path` is a string
literal, a Read-mode `${NAME}` capture, or a sub-op that produces a
`StrValue` slot (typical form: `str(\"dir/\", ${X}, \".out\")`).

Cursor flows through unchanged; the write happens as a `WriteFileEffect`
batched at end-of-step. The sink (`Disk` or `Buffer`) is chosen at
`RtCtxBuilder` registration time.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        let mut iter = values.into_iter();
        let first = match iter.next() {
            Some(v) => v,
            None => {
                return Err(vec![PatternDiagnostic {
                    code: "write_file/missing-path",
                    message: "write_file requires a path argument".into(),
                    byte_range: 0..0,
                }]);
            }
        };
        if iter.next().is_some() {
            return Err(vec![PatternDiagnostic {
                code: "write_file/too-many-args",
                message: "write_file takes exactly one path argument".into(),
                byte_range: 0..0,
            }]);
        }

        let path = match first {
            Value::Str(s) | Value::Atom(s) => PathSpec::Literal(s),
            Value::Term { name, mode: TermMode::Read } => PathSpec::Term(name),
            Value::Term { name, mode: TermMode::Unbound } => {
                return Err(vec![PatternDiagnostic {
                    code: "term/unbound-not-allowed-in-write-file",
                    message: format!(
                        "`${{{name}?}}` introducer not allowed as write_file path; \
                         path must already be bound"
                    ),
                    byte_range: 0..0,
                }]);
            }
            Value::Op(op) => PathSpec::SubOp(op),
            Value::Int(_) | Value::Float(_) => {
                return Err(vec![PatternDiagnostic {
                    code: "value/wrong-kind",
                    message: "write_file path must be string/atom/term/sub-op".into(),
                    byte_range: 0..0,
                }]);
            }
        };

        Ok(WriteFileOp { path })
    }
}

impl WriteFileOp {
    /// Resolve the path from the cursor according to PathSpec.
    /// `Some(_)` = path determined; `None` = silently drop (caller
    /// emits a diagnostic via stderr or skips).
    async fn resolve_path(&self, ctx: &RtCtx, c: &Cursor) -> Option<Arc<str>> {
        match &self.path {
            PathSpec::Literal(s) => Some(s.clone()),
            PathSpec::Term(name) => c.capture(name).and_then(|cap| {
                std::str::from_utf8(cap.bytes(&c.content))
                    .ok()
                    .map(Arc::from)
            }),
            PathSpec::SubOp(op) => {
                let outs = op.pipe(ctx, c.clone()).await;
                let out = outs.into_iter().next()?;
                let v = out.get_slot::<StrValue>()?;
                Some(v.0.clone())
            }
        }
    }
}

impl Op for WriteFileOp {
    fn name(&self) -> &'static str { "write_file" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };
            let path_str = match self.resolve_path(ctx, &c).await {
                Some(p) => p,
                None => {
                    // No path resolved; drop the cursor, no write.
                    return vec![c];
                }
            };
            let active = c.active();
            let bytes: Arc<[u8]> = Arc::from(active);
            let path: Arc<std::path::Path> =
                Arc::from(std::path::Path::new(path_str.as_ref()));
            let _ = ctx.put(WriteFileEffect { path, bytes }).await;
            vec![c]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Capture;
    use crate::effects::{WriteFileBatcher, WriteFileEffect};
    use crate::ops::str_op::{StrOp, StrSeg};
    use effect_runtime::RtCtxBuilder;

    fn rt_with_buffer() -> (
        RtCtx,
        Arc<std::sync::Mutex<Vec<(Arc<std::path::Path>, Arc<[u8]>)>>>,
    ) {
        let (batcher, buf) = WriteFileBatcher::buffer();
        let rt = RtCtxBuilder::new()
            .register::<WriteFileEffect, _>(batcher)
            .build();
        (rt, buf)
    }

    #[tokio::test]
    async fn literal_path_writes_active_bytes() {
        let (rt, buf) = rt_with_buffer();
        let op = WriteFileOp::from_values(vec![Value::Str(Arc::from("out/a.txt"))]).unwrap();
        let c = Cursor::new(Arc::from(b"hello world".as_slice())).narrow(0..5);
        let out = op.pipe(&rt, c).await;
        assert_eq!(out.len(), 1);
        let written = buf.lock().unwrap().clone();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0.to_str().unwrap(), "out/a.txt");
        assert_eq!(written[0].1.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn term_path_resolves_from_capture() {
        let (rt, buf) = rt_with_buffer();
        let op = WriteFileOp::from_values(vec![Value::Term {
            name: Arc::from("P"),
            mode: TermMode::Read,
        }])
        .unwrap();
        let content: Arc<[u8]> = Arc::from(b"DEST=tmp/x.bin payload".as_slice());
        let mut c = Cursor::new(content).narrow(15..22);
        c.captures
            .push(Capture::span_backed(Arc::from("P"), 5..14));
        op.pipe(&rt, c).await;
        let written = buf.lock().unwrap().clone();
        assert_eq!(written[0].0.to_str().unwrap(), "tmp/x.bin");
        assert_eq!(written[0].1.as_ref(), b"payload");
    }

    #[tokio::test]
    async fn subop_path_via_str_template() {
        let (rt, buf) = rt_with_buffer();
        // str("dir/", ${X}, ".out") — sub-op produces StrValue.
        let str_op: Arc<dyn Op> = Arc::new(StrOp {
            template: vec![
                StrSeg::Lit(Arc::from("dir/")),
                StrSeg::Term(Arc::from("X")),
                StrSeg::Lit(Arc::from(".out")),
            ],
        });
        let op = WriteFileOp::from_values(vec![Value::Op(str_op)]).unwrap();
        let content: Arc<[u8]> = Arc::from(b"---name=alice---body".as_slice());
        let mut c = Cursor::new(content).narrow(16..20);
        c.captures
            .push(Capture::span_backed(Arc::from("X"), 8..13));
        op.pipe(&rt, c).await;
        let written = buf.lock().unwrap().clone();
        assert_eq!(written[0].0.to_str().unwrap(), "dir/alice.out");
        assert_eq!(written[0].1.as_ref(), b"body");
    }

    #[test]
    fn missing_path_diagnoses() {
        let err = WriteFileOp::from_values(vec![]).unwrap_err();
        assert_eq!(err[0].code, "write_file/missing-path");
    }

    #[test]
    fn too_many_args_diagnose() {
        let err = WriteFileOp::from_values(vec![
            Value::Str(Arc::from("a")),
            Value::Str(Arc::from("b")),
        ])
        .unwrap_err();
        assert_eq!(err[0].code, "write_file/too-many-args");
    }

    #[test]
    fn unbound_term_diagnoses() {
        let err = WriteFileOp::from_values(vec![Value::Term {
            name: Arc::from("X"),
            mode: TermMode::Unbound,
        }])
        .unwrap_err();
        assert_eq!(err[0].code, "term/unbound-not-allowed-in-write-file");
    }
}
