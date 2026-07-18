//! The client half of the daemon's HTTP-over-UDS transport: one hyper HTTP/1
//! connection on the singleton's Unix socket, driven by a private
//! current-thread tokio runtime so every caller (CLI one-shots, the LSP
//! subscriber thread, the MCP pump, hooks) stays synchronous.
//!
//! Library posture (plan 2026-07-18-infra-library-adoption.md section 2.5,
//! resolved here): raw hyper client-conn on a `tokio::net::UnixStream`, not
//! hyperlocal — one non-pooled connection per client needs no connector/URI
//! layer, and hyper is already in the tree under axum. The RPC envelope is
//! unchanged JSON-RPC 2.0 (`crate::rpc`); only the wire around it moved from
//! bespoke `Content-Length` framing to HTTP (`POST /rpc`, `GET /watch` SSE).
//!
//! Wedge posture (inherited from the old framed client's `read_frame_watched`):
//! a response wait polls in 10s slices, logging the daemon's current phase from
//! the on-disk `why.jsonl` trail each slice, and gives up with exit 75 after
//! `DL_MAX_WALL_SECS` total (default 300; `0` disables the deadline but keeps
//! the heartbeat).

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};

use crate::rpc::{Request, Response};

/// One client connection to the daemon: a current-thread runtime + a hyper
/// HTTP/1 send handle over the UDS socket. Sequential requests reuse the
/// connection (HTTP/1 keep-alive), matching the old one-stream-many-frames
/// clients (hooks, MCP).
pub struct DaemonClient {
    rt: tokio::runtime::Runtime,
    sender: hyper::client::conn::http1::SendRequest<Full<Bytes>>,
}

impl DaemonClient {
    /// Connect to the singleton socket at `sock`. The connection driver runs as
    /// a task on this client's own runtime; it is polled whenever a call blocks
    /// on that runtime.
    pub fn connect_to(sock: &Path) -> Result<Self> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("build daemon-client runtime")?;
        let sender = rt.block_on(async {
            let stream = tokio::net::UnixStream::connect(sock)
                .await
                .with_context(|| format!("connect daemon socket {}", sock.display()))?;
            let (sender, conn) =
                hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
                    .await
                    .context("http handshake on daemon socket")?;
            tokio::spawn(conn);
            Ok::<_, anyhow::Error>(sender)
        })?;
        Ok(Self { rt, sender })
    }

    /// Send one JSON-RPC request over `POST /rpc`, wait (watched) for the
    /// response. Both the 200 (executed) and 400 (malformed / retired method)
    /// paths carry a JSON-RPC `Response` body, so status only matters when the
    /// body is not one.
    pub fn call(&mut self, req: &Request) -> Result<Response> {
        let body = serde_json::to_string(&req.to_json())?;
        let (status, resp_body) = self.post_rpc_watched(body)?;
        let v: serde_json::Value = serde_json::from_str(&resp_body)
            .with_context(|| format!("daemon /rpc returned non-JSON (status {status})"))?;
        Response::from_value(v)
    }

    /// `POST /rpc` with the watched wait: 10s heartbeat slices naming the
    /// daemon's phase, `DL_MAX_WALL_SECS` give-up (exit 75).
    fn post_rpc_watched(&mut self, body: String) -> Result<(u16, String)> {
        let budget = crate::watchdog::max_wall_secs();
        let Self { rt, sender } = self;
        rt.block_on(async {
            let req = hyper::Request::post("/rpc")
                .header(hyper::header::HOST, "dl-daemon")
                .header(hyper::header::CONTENT_TYPE, "application/json")
                .body(Full::new(Bytes::from(body)))
                .context("build /rpc request")?;
            let exchange = async {
                let resp = sender.send_request(req).await.context("send /rpc request")?;
                let status = resp.status().as_u16();
                let bytes = resp
                    .into_body()
                    .collect()
                    .await
                    .context("read /rpc response body")?
                    .to_bytes();
                Ok::<_, anyhow::Error>((status, String::from_utf8(bytes.to_vec())?))
            };
            let mut exchange = Box::pin(exchange);
            let start = Instant::now();
            loop {
                match tokio::time::timeout(Duration::from_secs(WAIT_SLICE_SECS), &mut exchange).await {
                    Ok(result) => return result,
                    Err(_elapsed) => {
                        let waited = start.elapsed().as_secs();
                        if budget != 0 && waited >= budget {
                            // @eprintln-ok: final user-facing error before process exit
                            eprintln!("[daemon] no response after {waited}s — the daemon is busy or wedged");
                            // @eprintln-ok: final user-facing error before process exit
                            eprintln!("  run `dl daemon why` to see what it is doing; giving up (75)");
                            std::process::exit(75);
                        }
                        let phase = crate::why::last_phase(&crate::daemon::daemon_home())
                            .unwrap_or_else(|| "phase unknown, run: dl daemon why".to_string());
                        tracing::debug!("waiting on daemon ({waited}s): {phase}");
                    }
                }
            }
        })
    }

    /// Consume this client as a push-notification stream: `GET /watch`, then
    /// feed every SSE `data:` payload (a JSON-RPC notification envelope,
    /// parsed) to `on_event` until it returns `false`, the stream ends, or the
    /// transport errors. Best-effort by design — callers treat any return as
    /// "push is over" (the old framed subscriber loop's contract).
    pub fn watch(self, mut on_event: impl FnMut(serde_json::Value) -> bool) -> Result<()> {
        let Self { rt, mut sender } = self;
        rt.block_on(async move {
            let req = hyper::Request::get("/watch")
                .header(hyper::header::HOST, "dl-daemon")
                .body(Full::new(Bytes::new()))
                .context("build /watch request")?;
            let resp = sender.send_request(req).await.context("send /watch request")?;
            if resp.status() != hyper::StatusCode::OK {
                bail!("GET /watch: status {}", resp.status());
            }
            let mut body = resp.into_body();
            let mut buffer = String::new();
            loop {
                let Some(frame) = body.frame().await else {
                    return Ok(()); // clean end of stream (daemon shut down)
                };
                let frame = frame.context("read /watch stream")?;
                let Some(chunk) = frame.data_ref() else { continue };
                buffer.push_str(&String::from_utf8_lossy(chunk));
                // SSE events are blank-line separated; each carries `data:`
                // lines (axum writes one per event) and possibly `:` keep-alive
                // comment lines, which the SSE spec says to ignore.
                while let Some(boundary) = buffer.find("\n\n") {
                    let event_text: String = buffer[..boundary].to_string();
                    buffer.drain(..boundary + 2);
                    for line in event_text.lines() {
                        let Some(data) = line.strip_prefix("data:") else { continue };
                        let Ok(note) = serde_json::from_str::<serde_json::Value>(data.trim())
                        else {
                            continue;
                        };
                        if !on_event(note) {
                            return Ok(());
                        }
                    }
                }
            }
        })
    }
}

/// Watched-wait heartbeat slice, matching the old framed client's 10s poll.
const WAIT_SLICE_SECS: u64 = 10;
