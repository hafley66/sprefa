//! `Diag` — fire-and-forget diagnostic events.
//!
//! Components emit via `ctx.diag.emit(d)`. Sinks fan out: LSP
//! `publishDiagnostics`, CLI stderr, jsonl log file, telemetry. The
//! default sink drops everything (test harness, headless bench).
//!
//! Diagnostics are events, not state. No one is required to hold a
//! reference. A diag that no sink consumes is gone.

use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity { Error, Warn, Info, Hint }

#[derive(Debug, Clone)]
pub struct Diag {
    pub severity: Severity,
    pub code:     Arc<str>,
    pub message:  String,
    pub span:     Option<ByteRange>,
    pub op_path:  Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub lo: u32,
    pub hi: u32,
}

pub trait DiagSink: Send + Sync + 'static {
    fn emit(&self, diag: Diag);
}

/// No-op sink. Used when no consumer is wired.
pub struct NoopDiagSink;

impl DiagSink for NoopDiagSink {
    fn emit(&self, _diag: Diag) {}
}

impl Diag {
    pub fn error(code: impl Into<Arc<str>>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code:     code.into(),
            message:  message.into(),
            span:     None,
            op_path:  Vec::new(),
        }
    }

    pub fn warn(code: impl Into<Arc<str>>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warn,
            code:     code.into(),
            message:  message.into(),
            span:     None,
            op_path:  Vec::new(),
        }
    }

    pub fn info(code: impl Into<Arc<str>>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            code:     code.into(),
            message:  message.into(),
            span:     None,
            op_path:  Vec::new(),
        }
    }

    pub fn hint(code: impl Into<Arc<str>>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Hint,
            code:     code.into(),
            message:  message.into(),
            span:     None,
            op_path:  Vec::new(),
        }
    }

    pub fn with_span(mut self, lo: u32, hi: u32) -> Self {
        self.span = Some(ByteRange { lo, hi });
        self
    }

    pub fn with_op_path(mut self, path: Vec<u32>) -> Self {
        self.op_path = path;
        self
    }
}
