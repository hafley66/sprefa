//! `write_cursor(target, [mode])` — splice cursor.active() into a
//! target byte_range identified by a Read-mode capture name.
//!
//! Per session 20260426 vision lock:
//!   write_cursor(${TARGET?})              :replace (default)
//!   write_cursor(${TARGET?}, :append)     insert at byte_range.end
//!   write_cursor(${TARGET?}, :prepend)    insert at byte_range.start
//!   write_cursor(${TARGET?}, :wrap)       surround existing target
//!
//! Positional args, no kwargs. Backed by `WriteRangeEffect`. The same
//! effect powers future `write_file(range)` and LSP code-action edits.
//!
//! Runtime contract per cursor:
//!   - file       = cursor.fs                 (None ⇒ skip)
//!   - byte_range = cursor.capture(target)    (missing ⇒ skip)
//!   - new_bytes  = cursor.active()           (rendered bytes from upstream)
//!
//! Cursor flows through unchanged — the write happens as an effect.
//!
//! Lower diagnostics:
//!   write_cursor/missing-target   no positional args
//!   write_cursor/bad-target       first arg is not a Read-mode term
//!   write_cursor/bad-mode         second arg is not a known mode atom
//!   write_cursor/too-many-args    >2 args supplied

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::effects::{WriteMode, WriteRangeEffect};
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{TermMode, Value};

#[derive(Debug)]
pub struct WriteCursorOp {
    pub target: Arc<str>,
    pub mode:   WriteMode,
}

impl Op for WriteCursorOp {
    fn name(&self) -> &'static str { "write_cursor" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        Box::pin(crate::_1_op::per_cursor(batch, move |c| async move {
            let Some(file) = c.fs.clone() else { return vec![c]; };
            let Some(cap) = c.capture(&self.target) else { return vec![c]; };
            let byte_range = cap.byte_range.clone();
            let new_bytes: Arc<[u8]> = Arc::from(c.active());
            let _ = ctx.put(WriteRangeEffect {
                file,
                byte_range,
                new_bytes,
                mode: self.mode,
            }).await;
            vec![c]
        }))
    }
}

impl OpCtor for WriteCursorOp {
    const NAME: &'static str = "write_cursor";
    const BODY_GRAMMAR: &'static str = "`${TARGET}`, `[:replace|:append|:prepend|:wrap]`";
    const DOC: &'static str = "\
`write_cursor(${TARGET}, [:mode])`

Splice `cursor.active()` bytes into the byte_range of the named target
capture on `cursor.fs`. Mode atoms: `:replace` (default), `:append`,
`:prepend`, `:wrap`.

Backed by `WriteRangeEffect`. Cursor flows through unchanged.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        let mut iter = values.into_iter();
        let first = iter.next().ok_or_else(|| vec![PatternDiagnostic {
            code:       "write_cursor/missing-target",
            message:    "write_cursor requires a target capture: write_cursor(${TARGET?})".into(),
            byte_range: 0..0,
        }])?;
        let target = match first {
            Value::Term { name, mode } => match mode {
                TermMode::Read => name,
                TermMode::Unbound => {
                    return Err(vec![PatternDiagnostic {
                        code:       "term/unbound-not-allowed-in-write-cursor",
                        message:    format!(
                            "`${{{name}?}}` introducer not allowed as write_cursor target; \
                             target must be a previously-bound capture"
                        ),
                        byte_range: 0..0,
                    }]);
                }
            },
            _ => {
                return Err(vec![PatternDiagnostic {
                    code:       "write_cursor/bad-target",
                    message:    "write_cursor target must be a `${NAME}` capture reference".into(),
                    byte_range: 0..0,
                }]);
            }
        };

        let mode = match iter.next() {
            None => WriteMode::Replace,
            Some(Value::Atom(s)) => WriteMode::parse(&s).ok_or_else(|| vec![PatternDiagnostic {
                code:       "write_cursor/bad-mode",
                message:    format!(
                    "unknown mode `:{s}`; allowed: :replace, :append, :prepend, :wrap"
                ),
                byte_range: 0..0,
            }])?,
            Some(_) => {
                return Err(vec![PatternDiagnostic {
                    code:       "write_cursor/bad-mode",
                    message:    "write_cursor mode must be an atom: :replace | :append | :prepend | :wrap".into(),
                    byte_range: 0..0,
                }]);
            }
        };

        if iter.next().is_some() {
            return Err(vec![PatternDiagnostic {
                code:       "write_cursor/too-many-args",
                message:    "write_cursor accepts at most 2 args: target and optional mode".into(),
                byte_range: 0..0,
            }]);
        }

        Ok(WriteCursorOp { target, mode })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Capture;
    use crate::effects::{WriteRangeBatcher, WriteRangeEffect};
    use effect_runtime::RtCtxBuilder;
    use std::path::PathBuf;

    fn term_read(name: &str) -> Value {
        Value::Term { name: Arc::from(name), mode: TermMode::Read }
    }
    fn term_unbound(name: &str) -> Value {
        Value::Term { name: Arc::from(name), mode: TermMode::Unbound }
    }
    fn atom(s: &str) -> Value { Value::Atom(Arc::from(s)) }

    #[test]
    fn from_values_default_replace() {
        let op = WriteCursorOp::from_values(vec![term_read("SCOPE")]).unwrap();
        assert_eq!(&*op.target, "SCOPE");
        assert_eq!(op.mode, WriteMode::Replace);
    }

    #[test]
    fn from_values_with_mode() {
        let op = WriteCursorOp::from_values(vec![term_read("HERE"), atom("wrap")]).unwrap();
        assert_eq!(op.mode, WriteMode::Wrap);
        let op = WriteCursorOp::from_values(vec![term_read("HERE"), atom("append")]).unwrap();
        assert_eq!(op.mode, WriteMode::Append);
    }

    #[test]
    fn from_values_rejects_missing_target() {
        let err = WriteCursorOp::from_values(vec![]).unwrap_err();
        assert_eq!(err[0].code, "write_cursor/missing-target");
    }

    #[test]
    fn from_values_rejects_unbound_target() {
        let err = WriteCursorOp::from_values(vec![term_unbound("SCOPE")]).unwrap_err();
        assert_eq!(err[0].code, "term/unbound-not-allowed-in-write-cursor");
    }

    #[test]
    fn from_values_rejects_bad_mode() {
        let err = WriteCursorOp::from_values(vec![term_read("X"), atom("nope")]).unwrap_err();
        assert_eq!(err[0].code, "write_cursor/bad-mode");
    }

    #[test]
    fn from_values_rejects_extra_args() {
        let err = WriteCursorOp::from_values(vec![
            term_read("X"), atom("replace"), atom("extra"),
        ]).unwrap_err();
        assert_eq!(err[0].code, "write_cursor/too-many-args");
    }

    #[tokio::test]
    async fn pipe_emits_effect_with_target_byte_range_and_active_bytes() {
        let (batcher, buf) = WriteRangeBatcher::buffer();
        let rt = RtCtxBuilder::new()
            .register::<WriteRangeEffect, _>(batcher)
            .build();

        // Cursor: file=src/lib.rs, content="rendered bytes" (active=full),
        // capture SCOPE pointing into a *separate* original file range
        // (12..20). write_cursor reads SCOPE.byte_range as target, but
        // new_bytes from cursor.active().
        let mut c = Cursor::new(Arc::from(b"rendered output".as_slice()));
        c.fs = Some(Arc::<std::path::Path>::from(PathBuf::from("src/lib.rs")));
        c.captures.push(Capture::span_backed(Arc::from("SCOPE"), 12..20));

        let op = WriteCursorOp { target: Arc::from("SCOPE"), mode: WriteMode::Append };
        let out = op.pipe(&rt, Arc::from(vec![c])).await;
        assert_eq!(out.len(), 1);

        let effects = buf.lock().unwrap().clone();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].file.to_str(), Some("src/lib.rs"));
        assert_eq!(effects[0].byte_range, 12..20);
        assert_eq!(&effects[0].new_bytes[..], b"rendered output");
        assert_eq!(effects[0].mode, WriteMode::Append);
    }

    #[tokio::test]
    async fn pipe_skips_when_fs_missing() {
        let (batcher, buf) = WriteRangeBatcher::buffer();
        let rt = RtCtxBuilder::new()
            .register::<WriteRangeEffect, _>(batcher)
            .build();
        let mut c = Cursor::new(Arc::from(b"x".as_slice()));
        c.captures.push(Capture::span_backed(Arc::from("SCOPE"), 0..1));
        // c.fs is None
        let op = WriteCursorOp { target: Arc::from("SCOPE"), mode: WriteMode::Replace };
        op.pipe(&rt, Arc::from(vec![c])).await;
        assert!(buf.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn pipe_skips_when_target_capture_missing() {
        let (batcher, buf) = WriteRangeBatcher::buffer();
        let rt = RtCtxBuilder::new()
            .register::<WriteRangeEffect, _>(batcher)
            .build();
        let mut c = Cursor::new(Arc::from(b"x".as_slice()));
        c.fs = Some(Arc::<std::path::Path>::from(PathBuf::from("a.rs")));
        // no SCOPE capture written
        let op = WriteCursorOp { target: Arc::from("SCOPE"), mode: WriteMode::Replace };
        op.pipe(&rt, Arc::from(vec![c])).await;
        assert!(buf.lock().unwrap().is_empty());
    }
}
