//! Transport channel for the serving loops (`dl --mcp`): framed JSON-RPC
//! messages over a byte stream. The engine never sees the transport — the
//! serve loop (`mcp.rs`) reads `Frame`s off a `Channel`, injects them into the
//! program's `@in(rpc)` port rel, ticks, and pushes the drained `@out(rpc)`
//! rows back. `StdioChannel` is the MCP stdio transport (newline-delimited
//! JSON, one message per line); `VecChannel` is the in-proc test wire. An
//! HTTP/SSE/WS channel is a second impl of this trait, not a new engine
//! feature — the rungs above stdio in chat_log/20260630.5's ladder.

use anyhow::Result;
use std::io::{BufRead, Write};

/// One transport message. JSON-RPC only today (MCP stdio); SSE events and
/// WebSocket frames are future variants.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Rpc(serde_json::Value),
}

/// A duplex message transport. `recv` blocks; `None` means the peer closed
/// (EOF) and the serve loop exits. Completion is channel-level for JSON-RPC
/// (1->1: every response closes its request), so the trait needs no explicit
/// close signal yet.
pub trait Channel {
    fn recv(&mut self) -> Result<Option<Frame>>;
    fn send(&mut self, frame: &Frame) -> Result<()>;
}

/// MCP stdio transport: newline-delimited JSON. A line that fails to parse is
/// skipped with a stderr note (MCP clients speak well-formed JSON; a -32700
/// parse-error response needs an id we don't have).
pub struct StdioChannel<R: BufRead, W: Write> {
    input: R,
    output: W,
}

impl<R: BufRead, W: Write> StdioChannel<R, W> {
    pub fn new(input: R, output: W) -> Self {
        Self { input, output }
    }
}

impl<R: BufRead, W: Write> Channel for StdioChannel<R, W> {
    fn recv(&mut self) -> Result<Option<Frame>> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.input.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let t = line.trim();
            if t.is_empty() {
                continue;
            }
            match serde_json::from_str(t) {
                Ok(v) => return Ok(Some(Frame::Rpc(v))),
                Err(e) => tracing::warn!("[mcp] unparseable frame skipped: {e}"),
            }
        }
    }

    fn send(&mut self, frame: &Frame) -> Result<()> {
        let Frame::Rpc(v) = frame;
        // serde_json's Display is compact and escapes newlines, so one message
        // is always one line.
        writeln!(self.output, "{v}")?;
        self.output.flush()?;
        Ok(())
    }
}

/// In-proc wire: inbound frames are queued up front, outbound frames collect
/// in `sent`. The permanent test transport — a serve-loop test needs no
/// subprocess, no stdio, no timing.
#[derive(Default)]
pub struct VecChannel {
    pub inbound: std::collections::VecDeque<Frame>,
    pub sent: Vec<Frame>,
}

impl VecChannel {
    pub fn new(inbound: impl IntoIterator<Item = Frame>) -> Self {
        Self { inbound: inbound.into_iter().collect(), sent: Vec::new() }
    }
}

impl Channel for VecChannel {
    fn recv(&mut self) -> Result<Option<Frame>> {
        Ok(self.inbound.pop_front())
    }

    fn send(&mut self, frame: &Frame) -> Result<()> {
        self.sent.push(frame.clone());
        Ok(())
    }
}
