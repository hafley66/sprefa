//! Diagnostics: structured records emitted by DSL compilation, matching, and
//! LSP feature impls.
//!
//! Convention:
//! - Fatal failure returns `Err(Diag)` from a fallible function
//! - Non-fatal observations push through a `&dyn DiagSink`
//!
//! This keeps the binary success/failure of a call separate from the noisy
//! "many things to report" channel. A diagnostic sink can be `BufferSink`
//! (collect for LSP publish), `LogSink` (println via `log` crate), or
//! `SilentSink` (drop on the floor).

use std::ops::Range;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    pub severity:   Severity,
    pub code:       &'static str,
    pub message:    String,
    pub byte_range: Range<usize>,
    pub hint:       Option<String>,
}

impl Diag {
    pub fn error(code: &'static str, message: impl Into<String>, byte_range: Range<usize>) -> Self {
        Self { severity: Severity::Error, code, message: message.into(), byte_range, hint: None }
    }
    pub fn warning(code: &'static str, message: impl Into<String>, byte_range: Range<usize>) -> Self {
        Self { severity: Severity::Warning, code, message: message.into(), byte_range, hint: None }
    }
    pub fn info(code: &'static str, message: impl Into<String>, byte_range: Range<usize>) -> Self {
        Self { severity: Severity::Info, code, message: message.into(), byte_range, hint: None }
    }
    pub fn hint(code: &'static str, message: impl Into<String>, byte_range: Range<usize>) -> Self {
        Self { severity: Severity::Hint, code, message: message.into(), byte_range, hint: None }
    }
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

pub trait DiagSink: Send + Sync {
    fn push(&self, diag: Diag);
}

#[derive(Default)]
pub struct BufferSink {
    pub buf: Mutex<Vec<Diag>>,
}

impl BufferSink {
    pub fn new() -> Self { Self::default() }
    pub fn drain(&self) -> Vec<Diag> {
        std::mem::take(&mut *self.buf.lock().unwrap())
    }
    pub fn snapshot(&self) -> Vec<Diag> {
        self.buf.lock().unwrap().clone()
    }
    pub fn len(&self) -> usize {
        self.buf.lock().unwrap().len()
    }
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

impl DiagSink for BufferSink {
    fn push(&self, diag: Diag) {
        self.buf.lock().unwrap().push(diag);
    }
}

pub struct SilentSink;
impl DiagSink for SilentSink {
    fn push(&self, _: Diag) {}
}

pub struct LogSink;
impl DiagSink for LogSink {
    fn push(&self, d: Diag) {
        eprintln!("[{:?}] {} ({}): {}", d.severity, d.code, d.message, d.byte_range.start);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fatal_returns_err_nonfatal_pushes_to_sink() {
        fn parse(input: &str, sink: &dyn DiagSink) -> Result<usize, Diag> {
            if input.is_empty() {
                return Err(Diag::error("empty", "input is empty", 0..0));
            }
            for (i, c) in input.char_indices() {
                if c == '?' {
                    sink.push(Diag::warning("question", "question mark looks suspicious", i..i+1));
                }
            }
            Ok(input.len())
        }

        let sink = BufferSink::new();
        assert!(parse("", &sink).is_err());
        assert_eq!(sink.len(), 0);

        let sink = BufferSink::new();
        let n = parse("a?b?c", &sink).unwrap();
        assert_eq!(n, 5);
        let diags = sink.drain();
        assert_eq!(diags.len(), 2);
        assert!(diags.iter().all(|d| d.severity == Severity::Warning));
    }

    #[test]
    fn silent_sink_drops() {
        let s = SilentSink;
        s.push(Diag::error("x", "y", 0..0));
    }
}
