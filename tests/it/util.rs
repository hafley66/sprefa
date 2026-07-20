//! Shared helpers for it-tests that spawn a real `dl --daemon` child.

/// The stored rev a `scan("WORK", …)` produces in a sandbox with no git HEAD:
/// git's all-zero oid plus the dirty marker. `WORK` is an ALIAS resolved at
/// `Engine::resolve_rev`, so no stored rev ever holds the alias text (INV-1),
/// and a non-git sandbox is deterministic without any override.
#[allow(dead_code)]
pub const NO_HEAD_REV: &str = "0000000000000000000000000000000000000000+";

use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::process::Child;

/// One JSON-RPC exchange with a sandboxed daemon over its UDS socket. The
/// socket carries HTTP since the axum adoption arc (one Router, two listeners
/// — plan 2026-07-18-infra-library-adoption.md section 2.4), so this is a
/// minimal `POST /rpc` with `Connection: close` written by hand, mirroring the
/// raw-HTTP posture `daemon_http.rs` already uses for the TCP door. Returns
/// the response body (a JSON-RPC envelope on 200 AND on the 4xx error paths);
/// `None` on connect/transport failure.
pub fn uds_rpc(sock: &std::path::Path, body: &str) -> Option<String> {
    let mut stream = std::os::unix::net::UnixStream::connect(sock).ok()?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(60))).ok()?;
    let request = format!(
        "POST /rpc HTTP/1.1\r\nHost: dl-daemon\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw);
    let header_end = text.find("\r\n\r\n")? + 4;
    Some(text[header_end..].to_string())
}

/// Kill-on-drop wrapper for a spawned daemon child. If a test panics (or
/// returns) while the child is still running, `Drop` kills and reaps it so a
/// leaked daemon can't keep running in its tempdir sandbox, print `[daemon]`
/// chatter over the suite summary, or hold the write end of a pipe wrapping
/// `cargo test` output. A test that shut the daemon down cleanly has an
/// already-exited child, so `Drop` is a no-op.
pub struct DaemonGuard(pub Child);

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

impl Deref for DaemonGuard {
    type Target = Child;
    fn deref(&self) -> &Child { &self.0 }
}

impl DerefMut for DaemonGuard {
    fn deref_mut(&mut self) -> &mut Child { &mut self.0 }
}
