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
pub enum Severity {
    Error,
    Warn,
    Info,
    Hint,
}

#[derive(Debug, Clone)]
pub struct Diag {
    pub severity: Severity,
    pub code: Arc<str>,
    pub message: String,
    pub span: Option<ByteRange>,
    pub op_path: Vec<u32>,
    /// Target file URI for this diagnostic. `None` => publish on the
    /// requesting `.sprf` URI (back-compat). `Some` => publish on this
    /// URI (set when a lint targets a non-`.sprf` file via FS column).
    pub target_uri: Option<String>,
    /// Line/col tuple resolved at emit time against the SAME bytes that
    /// `span` is keyed on. When present, the LSP publisher uses these
    /// directly instead of re-computing from disk at publish time (which
    /// races buffer edits and produces drift on file change).
    pub position: Option<DiagPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ByteRange {
    pub lo: u32,
    pub hi: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiagPosition {
    pub line_lo: u32,
    pub col_lo: u32,
    pub line_hi: u32,
    pub col_hi: u32,
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
            code: code.into(),
            message: message.into(),
            span: None,
            op_path: Vec::new(),
            target_uri: None,
            position: None,
        }
    }

    pub fn warn(code: impl Into<Arc<str>>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warn,
            code: code.into(),
            message: message.into(),
            span: None,
            op_path: Vec::new(),
            target_uri: None,
            position: None,
        }
    }

    pub fn info(code: impl Into<Arc<str>>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Info,
            code: code.into(),
            message: message.into(),
            span: None,
            op_path: Vec::new(),
            target_uri: None,
            position: None,
        }
    }

    pub fn hint(code: impl Into<Arc<str>>, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Hint,
            code: code.into(),
            message: message.into(),
            span: None,
            op_path: Vec::new(),
            target_uri: None,
            position: None,
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

    pub fn with_target_uri(mut self, uri: impl Into<String>) -> Self {
        self.target_uri = Some(uri.into());
        self
    }

    pub fn with_position(mut self, p: DiagPosition) -> Self {
        self.position = Some(p);
        self
    }
}
