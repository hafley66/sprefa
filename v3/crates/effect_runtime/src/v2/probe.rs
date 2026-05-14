//! `ProbeSink` — fire-and-forget per-emit observation events.
//!
//! Mirror of `DiagSink`. Components fire one `Probe { span, cursor }`
//! per emitted child; sinks consume them. Default sink drops everything.
//!
//! Use case: editor inlay hints ("→ N cursors flowed through this op").
//! The walker wraps each Component in a `SpannedComponent` (v4 side)
//! that knows its source byte range; on every emit it fires
//! `probe.on_emit(Probe { span, cursor })`. The LSP collects the
//! probes per-span and renders inlays.
//!
//! Probes are events, not state. No one is required to hold a reference.
//! A probe that no sink consumes is gone.

use std::sync::{Arc, Mutex};

use super::diag::ByteRange;

/// One emit event from a Component.
#[derive(Clone, Debug)]
pub struct Probe<N> {
    /// Source byte range of the op that emitted this cursor.
    pub span: ByteRange,
    /// The cursor that was emitted. Arc-shared with the queue.
    pub cursor: Arc<N>,
}

/// Per-emit observation surface. Generic over the carrier `N` so the
/// sink sees the same Arc the queue sees.
pub trait ProbeSink<N>: Send + Sync + 'static {
    fn on_emit(&self, probe: Probe<N>);
}

/// Drops every probe. Used when no consumer is wired (the default).
pub struct NoopProbeSink;

impl<N: Send + Sync + 'static> ProbeSink<N> for NoopProbeSink {
    fn on_emit(&self, _: Probe<N>) {}
}

/// Buffers probes in-memory. Used by the LSP inlay-hint handler and by
/// tests that assert per-op cursor counts.
pub struct BufferProbeSink<N> {
    inner: Mutex<Vec<Probe<N>>>,
}

impl<N> Default for BufferProbeSink<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N> BufferProbeSink<N> {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Vec::new()),
        }
    }

    /// Take all buffered probes, leaving the buffer empty.
    pub fn drain(&self) -> Vec<Probe<N>> {
        std::mem::take(&mut *self.inner.lock().unwrap())
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

impl<N: Send + Sync + 'static> ProbeSink<N> for BufferProbeSink<N> {
    fn on_emit(&self, p: Probe<N>) {
        self.inner.lock().unwrap().push(p);
    }
}
