//! The daemon's tokio SHELL: sockets, timers, dispatch — everything that is NOT
//! the tick engine. The engine (`crate::engine`, extraction/derive/SQLite) stays
//! strictly SYNCHRONOUS and is reached only through `tokio::task::spawn_blocking`.
//! The standing repo law "sync tick engine, NOT async DataLoader" is about the
//! ENGINE; it holds unchanged. What moved to tokio is the transport around it:
//!
//!   - the UDS listener + one task per connection (`uds`), same
//!     `Content-Length`-framed JSON wire protocol as before (byte-compatible with
//!     every installed CLI/LSP/MCP client — see `crate::rpc`);
//!   - the HTTP transport on `127.0.0.1:0` via axum (`http`), the same
//!     `POST /rpc` + `GET /health` bodies + `http.json` discovery file;
//!   - the poll + idle timers, the git/source watchers, and the job dispatcher
//!     (`jobs`), whose per-job RUN bodies are `spawn_blocking` engine calls.
//!
//! Why the shell no longer needs to block on the engine: reads are already
//! lock-free (`crate::daemon_read` opens read-only SQLite connections), and ticks
//! already flow through the durable job queue (`crate::jobq`). So a shell task
//! can accept a socket, answer `/health`, or coalesce a watcher burst while a
//! tick holds the engine mutex on a `spawn_blocking` thread.
//!
//! Runtime shape: ONE `new_multi_thread` runtime with a small FIXED worker count
//! (2), independent of the engine's rayon budget. The two shell workers only
//! drive IO + timers; every engine call lands on the separate `spawn_blocking`
//! pool, which stays naturally small because `jobq` serializes tick/drain work
//! per root by key (one `tick:{root}` / `sink:{root}` row at a time).

use std::sync::Arc;

use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub(crate) mod http;
pub(crate) mod jobs;
pub(crate) mod timers;
pub(crate) mod uds;
pub(crate) mod watch;

/// A message on the subscriber-push channel. `Add` registers a new subscriber's
/// write half (from a `subscribe` RPC); `Frame` is a pre-serialized
/// notification body (`diag_changed` / `rev_advanced`) to fan out. The channel
/// lets tick methods — which run on `spawn_blocking` threads (sync) — hand work
/// to the async subscriber pump without touching a socket directly.
pub(crate) enum BroadcastMsg {
    Add(OwnedWriteHalf),
    Frame(String),
}

/// Process-wide shell handles cloned wherever a shell task or an engine callback
/// needs to reach the runtime, cancel the daemon, ring the job doorbell, or push
/// to subscribers. Held by `Daemon` (for `add_root`'s watcher spawn) and mirrored
/// into `Shared` (for per-root enqueue + broadcast).
#[derive(Clone)]
pub(crate) struct ShellCtx {
    /// Runtime handle: `add_root` / the poll loop spawn tasks through it.
    pub rt: tokio::runtime::Handle,
    /// The one cancellation token; the shutdown RPC and SIGINT/SIGTERM both
    /// cancel it, every task selects on it for a graceful stop.
    pub cancel: CancellationToken,
    /// Job doorbell: `enqueue` rings it, dispatcher tasks await it.
    pub job_notify: Arc<Notify>,
    /// Subscriber-push sender; broadcasts + `subscribe` registration ride it.
    pub broadcast_tx: UnboundedSender<BroadcastMsg>,
}

/// Build the ONE shell runtime. `worker_threads(2)` is a small FIXED shell
/// budget, deliberately independent of `apply_daemon_budget`'s rayon count:
/// engine parallelism lives on rayon + the `spawn_blocking` pool, the shell only
/// needs enough workers to drive sockets and timers. QoS is inherited from the
/// spawning (main) thread, which already ran `apply_daemon_budget`.
pub(crate) fn build_runtime() -> anyhow::Result<tokio::runtime::Runtime> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("dl-tokio")
        .enable_all()
        .build()?;
    Ok(rt)
}

// ---------- async Content-Length framing (byte-identical to `crate::rpc`) ----------

/// Read one `Content-Length`-framed message off an async reader. `Ok(None)` on a
/// clean EOF between frames; error on a truncated/malformed header block. Headers
/// are ASCII (`read_line` is safe); the body is read as raw bytes then decoded,
/// matching `crate::rpc::read_frame`'s length-in-bytes contract.
pub(crate) async fn read_frame<R>(reader: &mut R) -> anyhow::Result<Option<String>>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};
    let mut content_length: Option<usize> = None;
    let mut line = String::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return if content_length.is_some() {
                anyhow::bail!("unexpected EOF mid-frame")
            } else {
                Ok(None)
            };
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(rest.trim().parse()?);
        }
    }
    let len = content_length.ok_or_else(|| anyhow::anyhow!("missing Content-Length header"))?;
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(Some(String::from_utf8(body)?))
}

/// Write one framed message. Byte-identical to `crate::rpc::write_frame`:
/// `Content-Length: N\r\n\r\n<body>`, then flush.
pub(crate) async fn write_frame<W>(w: &mut W, body: &str) -> anyhow::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    w.write_all(header.as_bytes()).await?;
    w.write_all(body.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}

// ---------- subscriber push pump ----------

/// The one task that owns the subscriber write halves and fans out pushed
/// frames. Runs entirely async; tick methods (on `spawn_blocking` threads) feed
/// it through the unbounded channel, so a slow/broken subscriber can never block
/// a tick. A write failure drops that subscriber (best-effort, same policy the
/// old inline broadcast had).
pub(crate) fn spawn_subscriber_pump(ctx: &ShellCtx, mut rx: UnboundedReceiver<BroadcastMsg>) {
    let cancel = ctx.cancel.clone();
    ctx.rt.spawn(async move {
        let mut subs: Vec<OwnedWriteHalf> = Vec::new();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                msg = rx.recv() => match msg {
                    None => return,
                    Some(BroadcastMsg::Add(w)) => subs.push(w),
                    Some(BroadcastMsg::Frame(body)) => {
                        let mut i = 0;
                        while i < subs.len() {
                            if write_frame(&mut subs[i], &body).await.is_err() {
                                subs.swap_remove(i);
                            } else {
                                i += 1;
                            }
                        }
                    }
                },
            }
        }
    });
}

// ---------- signal + shutdown tasks ----------

/// SIGINT/SIGTERM -> cancel the token (the same graceful path the shutdown RPC
/// drives). On non-unix or if signal registration fails, this task simply idles.
pub(crate) fn spawn_signal(ctx: &ShellCtx) {
    let cancel = ctx.cancel.clone();
    ctx.rt.spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let mut int = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("[daemon] SIGINT handler unavailable: {e}");
                return;
            }
        };
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("[daemon] SIGTERM handler unavailable: {e}");
                return;
            }
        };
        tokio::select! {
            _ = cancel.cancelled() => {}
            _ = int.recv() => { tracing::info!("[daemon] SIGINT — shutting down"); cancel.cancel(); }
            _ = term.recv() => { tracing::info!("[daemon] SIGTERM — shutting down"); cancel.cancel(); }
        }
    });
}
