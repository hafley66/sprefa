//! The daemon's ONE handler layer (plan 2026-07-18-infra-library-adoption.md
//! section 2.4 verdict): a single axum `Router` served by TWO thin listeners —
//! the localhost TCP socket and the Unix domain socket both carry HTTP. The
//! bespoke `Content-Length`-framed JSON-RPC wire the UDS socket used to speak
//! (and the kept-open-socket `subscribe` pump with it) is gone; RPC is a plain
//! JSON route reachable over either transport.
//!
//!   - `POST /rpc`   — body IS the JSON-RPC request; dispatched through the same
//!     `crate::daemon::handle_request` on the blocking pool (engine dispatch is
//!     sync). `shutdown` additionally schedules the cancel token after the
//!     response goes out. `subscribe` is rejected pointing at `GET /watch`.
//!   - `GET /health` — liveness WITHOUT touching any engine mutex (roots-map lock
//!     only), so it answers even while a tick holds an engine lock on a
//!     `spawn_blocking` thread. This is the D-arc property, preserved.
//!   - `GET /watch`  — SSE stream of the daemon's push notifications
//!     (`diag_changed` / `rev_advanced` JSON-RPC notification envelopes, one per
//!     `data:` event). Replaces the framed-socket `subscribe` method; works over
//!     both transports.
//!
//! Which transport a request arrived on is an `Extension` the two serve entry
//! points stamp (`"http"` for TCP, `"sock"` for UDS), so the access log keeps
//! its historical surface labels. `TraceLayer` puts a request-scoped tracing
//! span around every route (the verdict's observability requirement).
//!
//! Shutdown: both serves share the one `CancellationToken` via
//! `with_graceful_shutdown`. A long-held `/watch` stream never blocks shutdown:
//! the cancel-driven exit path in `run_daemon` runs `shutdown_cleanup` and
//! exits the process without waiting for the drain (plan 2.5 drain question).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde_json::{json, Value};
use tokio_stream::StreamExt;

use super::ShellCtx;
use crate::daemon::{self, Daemon};
use crate::rpc::{Response as RpcResponse, INVALID_PARAMS, INVALID_REQUEST, PARSE_ERROR};

/// Which listener a request came in on: `"http"` (localhost TCP) or `"sock"`
/// (UDS). Stamped per-serve as an `Extension` so the shared handlers keep the
/// access log's historical surface labels.
#[derive(Clone, Copy)]
struct Transport(&'static str);

/// State every handler sees: the daemon (dispatch + health) and the shell
/// handles (`/watch` subscribes to the broadcast channel, `shutdown` cancels).
#[derive(Clone)]
struct HttpState {
    daemon: Arc<Daemon>,
    ctx: ShellCtx,
}

/// The ONE router. Both listeners serve clones of this; only the `Transport`
/// extension differs.
fn router(daemon: Arc<Daemon>, ctx: &ShellCtx, transport: &'static str) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/rpc", post(rpc))
        .route("/watch", get(watch))
        .fallback(fallback)
        .with_state(HttpState {
            daemon,
            ctx: ctx.clone(),
        })
        .layer(Extension(Transport(transport)))
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

/// Bind `127.0.0.1:0`, publish `http.json`, and spawn the TCP serve task. A
/// bind failure is returned (logged non-fatal by the caller; the UDS transport
/// stays authoritative).
pub(crate) fn spawn_tcp(ctx: &ShellCtx, daemon: Arc<Daemon>, build_id: &str) -> Result<()> {
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| anyhow::anyhow!("http bind 127.0.0.1:0: {e}"))?;
    let port = std_listener
        .local_addr()
        .context("http server bound to a non-IP address")?
        .port();
    std_listener
        .set_nonblocking(true)
        .context("set http listener non-blocking")?;
    publish_http_json(port, build_id)?;
    tracing::info!(
        "[daemon] http listening on http://127.0.0.1:{port} ({})",
        crate::daemon_http::http_json_path().display()
    );

    let app = router(daemon, ctx, "http");
    let cancel = ctx.cancel.clone();
    ctx.rt.spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("[daemon] adopt http listener into runtime: {e}");
                return;
            }
        };
        let served = axum::serve(listener, app)
            .with_graceful_shutdown(async move { cancel.cancelled().await })
            .await;
        if let Err(e) = served {
            tracing::warn!("[daemon] http server exited: {e}");
        }
    });
    Ok(())
}

/// Serve the SAME router over the UDS socket. The std listener was bound in
/// `run_daemon` (fail-fast on a double-bind) and set non-blocking; the task
/// adopts it into the reactor. `axum::serve` accepts a `tokio::net::UnixListener`
/// directly since 0.8 — this is the one-framework-two-listeners shape.
pub(crate) fn spawn_uds(
    ctx: &ShellCtx,
    daemon: Arc<Daemon>,
    std_listener: std::os::unix::net::UnixListener,
) {
    let app = router(daemon, ctx, "sock");
    let cancel = ctx.cancel.clone();
    ctx.rt.spawn(async move {
        let listener = match tokio::net::UnixListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("[daemon] adopt UDS listener into runtime: {e}");
                return;
            }
        };
        let served = axum::serve(listener, app)
            .with_graceful_shutdown(async move { cancel.cancelled().await })
            .await;
        if let Err(e) = served {
            tracing::warn!("[daemon] uds server exited: {e}");
        }
    });
}

/// Publish `<home>/http.json` atomically (write a sibling `.tmp`, then rename).
/// A bare truncate-then-write races a reader into an empty doc; `rename` on the
/// same filesystem is atomic. Byte-identical to the old serve path.
fn publish_http_json(port: u16, build_id: &str) -> Result<()> {
    let pid = std::process::id();
    let info = json!({ "port": port, "pid": pid, "build_id": build_id });
    let path: PathBuf = crate::daemon_http::http_json_path();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&info)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// `GET /health`: roots-map lock only, never `lock_eng`, so it answers while a
/// tick holds an engine mutex. Inline async is correct here — no blocking, no
/// engine lock — which is exactly why 50 concurrent `/health` all answer under
/// contention: engine work is on the `spawn_blocking` pool, not the shell
/// workers, so a shell worker is always free to serve this in microseconds.
async fn health(State(state): State<HttpState>) -> Response {
    let body = json!({
        "build_id": &*state.daemon.build_id,
        "pid": std::process::id(),
        "roots": state.daemon.served_root_count(),
    })
    .to_string();
    json_response(StatusCode::OK, body)
}

/// `GET /watch`: the push stream, as SSE. Each broadcast frame (a pre-serialized
/// JSON-RPC notification envelope: `diag_changed` / `rev_advanced`) becomes one
/// `data:` event. A lagged subscriber (slower than the 256-frame buffer) just
/// skips the overwritten frames — same best-effort policy the old kept-open
/// socket pump had, where a slow subscriber was dropped outright.
async fn watch(
    State(state): State<HttpState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.ctx.broadcast_tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
        .filter_map(|frame| frame.ok().map(|body| Ok(Event::default().data(body))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// `POST /rpc`: 200 with the serialized `Response` when the RPC executed; 400
/// with a JSON error for a malformed body or the retired `subscribe` method.
/// Engine dispatch runs on the blocking pool. `shutdown` responds first, then
/// drives the cancel token (the "respond, then exit" ordering the old framed
/// handler had) — the short delay lets the response flush before the
/// cancel-driven exit path runs `process::exit`.
async fn rpc(
    State(state): State<HttpState>,
    Extension(transport): Extension<Transport>,
    body: String,
) -> Response {
    let value: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return json_response(
                StatusCode::BAD_REQUEST,
                err_json(0, PARSE_ERROR, format!("parse: {e}")),
            )
        }
    };
    let req = match daemon::parse_request(value) {
        Some(r) => r,
        None => {
            return json_response(
                StatusCode::BAD_REQUEST,
                err_json(
                    0,
                    INVALID_REQUEST,
                    "request needs a numeric `id` and a string `method`",
                ),
            )
        }
    };
    if req.method == "subscribe" {
        // The kept-open-socket subscribe died with the framed wire; the push
        // stream is `GET /watch` on either transport now.
        return json_response(
            StatusCode::BAD_REQUEST,
            err_json(
                req.id,
                INVALID_PARAMS,
                "`subscribe` is gone — stream push notifications from `GET /watch` (SSE) instead",
            ),
        );
    }
    let is_shutdown = req.method == "shutdown";
    let req_id = crate::reqid::next();
    // req_id cancellation (class-18 residual): a client that disconnects
    // mid-request drops this handler future; the guard then cancels the jobs
    // that request caused — pending rows are killed, an in-flight one is
    // flagged and aborts at the worker's next job boundary. Disarmed on the
    // normal completion path below.
    let mut disconnect_guard = CancelOnDisconnect {
        rt: state.ctx.rt.clone(),
        daemon: state.daemon.clone(),
        req_id: req_id.clone(),
        armed: true,
    };
    let method = req.method.clone();
    let root_owned = req
        .params
        .get("root")
        .and_then(|v| v.as_str())
        .map(String::from);
    let bytes_in = body.len();
    let t = std::time::Instant::now();
    let d = state.daemon.clone();
    let req_id_for_dispatch = req_id.clone();
    let resp = match tokio::task::spawn_blocking(move || {
        daemon::handle_request(&d, &req, &req_id_for_dispatch)
    })
    .await
    {
        Ok(r) => r,
        Err(_) => RpcResponse::err(0, crate::rpc::INTERNAL_ERROR, "dispatch task panicked"),
    };
    disconnect_guard.armed = false;
    let out = serde_json::to_string(&resp.to_json()).unwrap_or_else(|_| "{}".into());
    daemon::log_access(
        transport.0,
        &req_id,
        &method,
        root_owned.as_deref(),
        t.elapsed().as_millis() as u64,
        resp.error.is_none(),
        bytes_in,
        out.len(),
    );
    if is_shutdown && resp.error.is_none() {
        let cancel = state.ctx.cancel.clone();
        state.ctx.rt.spawn(async move {
            // Flush window before the cancel-driven `process::exit` path fires;
            // over a local socket the response bytes land well inside this.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            cancel.cancel();
        });
    }
    json_response(StatusCode::OK, out)
}

/// Drop guard for the `/rpc` handler: an armed drop means the client
/// disconnected before the response went out (axum drops the handler future
/// when the connection dies). It cancels the request's jobs off-thread —
/// `cancel_req` is sync SQLite, so it rides `spawn_blocking`.
struct CancelOnDisconnect {
    rt: tokio::runtime::Handle,
    daemon: Arc<Daemon>,
    req_id: String,
    armed: bool,
}

impl Drop for CancelOnDisconnect {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let daemon = self.daemon.clone();
        let req_id = std::mem::take(&mut self.req_id);
        self.rt.spawn(async move {
            let cancelled =
                tokio::task::spawn_blocking(move || daemon.jobs.cancel_req(&req_id)).await;
            if let Ok(Err(e)) = cancelled {
                tracing::warn!("[daemon] disconnect cancel: {e}");
            }
        });
    }
}

/// Anything but the three routes: 404 with a JSON error, mirroring the old
/// `no route for <METHOD> <path>` body.
async fn fallback(method: axum::http::Method, uri: axum::http::Uri) -> Response {
    let path = uri.path();
    json_response(
        StatusCode::NOT_FOUND,
        json!({ "error": format!("no route for {} {path}", method.as_str()) }).to_string(),
    )
}

/// A JSON-RPC error envelope as a string, for the HTTP failure paths.
fn err_json(id: u64, code: i64, message: impl Into<String>) -> String {
    serde_json::to_string(&RpcResponse::err(id, code, message).to_json())
        .unwrap_or_else(|_| "{}".into())
}

/// Build a JSON response with the `application/json` content type.
fn json_response(status: StatusCode, body: String) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}
