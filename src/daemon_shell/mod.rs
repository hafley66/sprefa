//! The daemon's tokio SHELL: sockets, timers, dispatch — everything that is NOT
//! the tick engine. The engine (`crate::engine`, extraction/derive/SQLite) stays
//! strictly SYNCHRONOUS and is reached only through `tokio::task::spawn_blocking`.
//! The standing repo law "sync tick engine, NOT async DataLoader" is about the
//! ENGINE; it holds unchanged. What moved to tokio is the transport around it:
//!
//!   - ONE axum `Router` (`http`) served by two thin listeners — localhost TCP
//!     and the UDS socket both carry HTTP (`POST /rpc`, `GET /health`,
//!     `GET /watch` SSE). The bespoke `Content-Length`-framed JSON-RPC wire the
//!     UDS socket used to speak is gone (plan
//!     2026-07-18-infra-library-adoption.md section 2.4);
//!   - the poll + idle timers, the git/source watchers, and the job dispatcher
//!     (`jobs`), whose per-job RUN bodies are `spawn_blocking` engine calls.
//!
//! Why the shell no longer needs to block on the engine: reads are already
//! lock-free (`crate::daemon_read` opens read-only SQLite connections), and ticks
//! already flow through the durable job queue (`crate::jobq`). So a shell task
//! can accept a socket, answer `/health`, or stream `/watch` events while a
//! tick holds the engine mutex on a `spawn_blocking` thread.
//!
//! Runtime shape: ONE `new_multi_thread` runtime with a small FIXED worker count
//! (2), independent of the engine's rayon budget. The two shell workers only
//! drive IO + timers; every engine call lands on the separate `spawn_blocking`
//! pool, which stays naturally small because `jobq` serializes tick/drain work
//! per root by key (one `tick:{root}` / `sink:{root}` row at a time).

use std::sync::Arc;

use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

pub(crate) mod http;
pub(crate) mod jobs;
pub(crate) mod timers;
pub(crate) mod watch;

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
    /// Subscriber-push broadcast: tick methods (on `spawn_blocking` threads,
    /// sync) send pre-serialized `diag_changed` / `rev_advanced` notification
    /// bodies; each open `GET /watch` SSE stream holds a receiver. Zero
    /// receivers = the send errors and is ignored (push is best-effort); a
    /// lagged receiver skips overwritten frames instead of blocking a tick.
    pub broadcast_tx: tokio::sync::broadcast::Sender<String>,
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
