//! The standard HTTP/JSON transport, now on axum, alongside the UDS socket.
//! Same two routes, same bodies, same `http.json` discovery file as the old
//! `tiny_http` server it replaces:
//!
//!   - `POST /rpc`   — body IS the JSON-RPC request; dispatched through the same
//!     `crate::daemon::handle_request` the UDS path uses (on the blocking pool,
//!     since engine dispatch is sync);
//!   - `GET /health` — liveness WITHOUT touching any engine mutex (roots-map lock
//!     only), so it answers even while a tick holds an engine lock on a
//!     `spawn_blocking` thread. This is the D-arc property, preserved.
//!
//! The listener is bound as a std `TcpListener` in `spawn` (fail-fast, and the
//! ephemeral port is known synchronously so `http.json` publishes it before the
//! server task starts), then adopted into the reactor via `from_std`.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};

use super::ShellCtx;
use crate::daemon::{self, Daemon};
use crate::rpc::{Response as RpcResponse, INVALID_PARAMS, INVALID_REQUEST, PARSE_ERROR};

/// Methods that need a kept-open subscriber stream (`subscribe`). The HTTP
/// transport is request/response only, so a `POST /rpc` for one gets a clean
/// JSON error naming the UDS socket to use instead (no HTTP streaming in this
/// arc). Unchanged from the tiny_http version.
const SUBSCRIBER_METHODS: &[&str] = &["subscribe"];

/// Bind `127.0.0.1:0`, publish `http.json`, and spawn the axum server task. A
/// bind failure is returned (logged non-fatal by the caller; UDS stays
/// authoritative). Mirrors the old `daemon_http::serve`.
pub(crate) fn spawn(ctx: &ShellCtx, daemon: Arc<Daemon>, build_id: &str) -> Result<()> {
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

    let app = Router::new()
        .route("/health", get(health))
        .route("/rpc", post(rpc))
        .fallback(fallback)
        .with_state(daemon);
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
async fn health(State(daemon): State<Arc<Daemon>>) -> Response {
    let body = json!({
        "build_id": &*daemon.build_id,
        "pid": std::process::id(),
        "roots": daemon.served_root_count(),
    })
    .to_string();
    json_response(StatusCode::OK, body)
}

/// `POST /rpc`: 200 with the serialized `Response` when the RPC executed; 400
/// with a JSON error for a malformed body or a subscriber-only method. Engine
/// dispatch runs on the blocking pool.
async fn rpc(State(daemon): State<Arc<Daemon>>, body: String) -> Response {
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
                err_json(0, INVALID_REQUEST, "request needs a numeric `id` and a string `method`"),
            )
        }
    };
    if SUBSCRIBER_METHODS.contains(&req.method.as_str()) {
        return json_response(
            StatusCode::BAD_REQUEST,
            err_json(
                req.id,
                INVALID_PARAMS,
                format!(
                    "`{}` needs a kept-open subscriber stream; connect over the UDS socket {} instead",
                    req.method,
                    daemon::socket_path().display(),
                ),
            ),
        );
    }
    let req_id = crate::reqid::next();
    let method = req.method.clone();
    let root_owned = req.params.get("root").and_then(|v| v.as_str()).map(String::from);
    let bytes_in = body.len();
    let t = std::time::Instant::now();
    let d = daemon.clone();
    let req_id_for_dispatch = req_id.clone();
    let resp = match tokio::task::spawn_blocking(move || {
        daemon::handle_request(&d, &req, &req_id_for_dispatch)
    })
    .await
    {
        Ok(r) => r,
        Err(_) => RpcResponse::err(0, crate::rpc::INTERNAL_ERROR, "dispatch task panicked"),
    };
    let out = serde_json::to_string(&resp.to_json()).unwrap_or_else(|_| "{}".into());
    daemon::log_access(
        "http", &req_id, &method, root_owned.as_deref(),
        t.elapsed().as_millis() as u64, resp.error.is_none(), bytes_in, out.len(),
    );
    json_response(StatusCode::OK, out)
}

/// Anything but the two routes: 404 with a JSON error, mirroring the old
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
