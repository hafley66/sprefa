//! Standard HTTP/JSON transport for the singleton daemon, alongside the Unix
//! domain socket (`crate::daemon`). Same request dispatch, second door: any
//! client that speaks HTTP (curl, a browser, a language without a UDS/framing
//! codec) can drive the daemon without the LSP-style `Content-Length` framing.
//!
//! Shape:
//!   - one `tiny_http::Server` bound on `127.0.0.1:0` (an ephemeral port)
//!   - `<daemon_home()>/http.json` publishes `{port, pid, build_id}` so a client
//!     finds the port the same way it finds the socket
//!   - `POST /rpc`  — body IS the JSON-RPC request; dispatched through the same
//!     `crate::daemon::handle_request` the UDS path uses
//!   - `GET /health` — liveness WITHOUT touching any engine mutex (roots-map
//!     lock only), so it answers even while a tick holds an engine lock
//!
//! This lives in a flat top-level module (not `crate::daemon::http`) because the
//! crate already carries a `crate::daemon` vs `crate::cli::daemon` name clash;
//! `daemon_http` sidesteps growing that ambiguity. It also keeps `daemon.rs`
//! (already over the file-budget rail) from taking on the HTTP surface — only a
//! few wiring lines there call into here.
//!
//! Deliberately sync + thread-per-request (`tiny_http`, one `dl-http-N` thread
//! per request), mirroring the UDS `accept_loop`. No tokio: the tick engine is
//! synchronous and an async runtime would only add a second scheduling model.

use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::daemon::{self, Daemon};
use crate::rpc::{Response, INVALID_PARAMS, INVALID_REQUEST, PARSE_ERROR};

/// Methods that need a kept-open subscriber stream (server-sent `diag_changed`
/// notifications pushed over the same connection). The HTTP transport is
/// request/response only, so these cannot ride it — a `POST /rpc` for one gets a
/// clean JSON error naming the UDS socket to use instead. Only `subscribe`
/// consumes the subscriber machinery (`handle_request`'s one `subscriber_stream`
/// user); every other method passes `None` on the UDS path too.
const SUBSCRIBER_METHODS: &[&str] = &["subscribe"];

/// `<home>/http.json` — the HTTP-endpoint discovery file (sibling of
/// `roots.json`/`daemon.pid`). Written on serve, deleted in `shutdown_cleanup`.
pub fn http_json_path() -> PathBuf {
    daemon::daemon_home().join("http.json")
}

/// Read the published base URL (`http://127.0.0.1:<port>`) from `http.json`.
/// Errors cleanly when the file is absent (daemon down or too old to publish it).
pub fn base_url() -> Result<String> {
    let path = http_json_path();
    let text = std::fs::read_to_string(&path).map_err(|_| {
        anyhow::anyhow!(
            "no HTTP endpoint published at {} — is the daemon running? `dl daemon start`",
            path.display()
        )
    })?;
    let info: Value = serde_json::from_str(&text)
        .with_context(|| format!("parse {}", path.display()))?;
    let port = info.get("port").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("{} has no port field", path.display()))?;
    Ok(format!("http://127.0.0.1:{port}"))
}

/// Bind the HTTP server on `127.0.0.1:0`, publish `http.json`, and spawn the
/// accept loop. Called once from `run_daemon` after the UDS listener binds.
pub fn serve(daemon: Arc<Daemon>, build_id: &str) -> Result<()> {
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| anyhow::anyhow!("http bind 127.0.0.1:0: {e}"))?;
    let port = server.server_addr().to_ip()
        .map(|addr| addr.port())
        .context("http server bound to a non-IP address")?;
    let pid = std::process::id();

    let info = json!({ "port": port, "pid": pid, "build_id": build_id });
    // Publish atomically: write a sibling `.tmp` then rename over `http.json`.
    // A bare `fs::write` truncates-then-writes in place, so a reader that opens
    // the file between truncate and write sees an empty/partial doc (the
    // `http_json_published_then_removed_on_stop` it-test flaked on exactly that
    // race). `rename` on the same filesystem is atomic: a reader sees either the
    // old file or the fully-written new one, never a torn intermediate.
    let path = http_json_path();
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&info)?)
        .with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    tracing::info!("[daemon] http listening on http://127.0.0.1:{port} ({})", http_json_path().display());

    std::thread::Builder::new()
        .name("dl-http-accept".into())
        .spawn(move || accept_loop(daemon, server))?;
    Ok(())
}

/// Accept HTTP requests, spawning one named `dl-http-N` thread per request,
/// mirroring the UDS `accept_loop`. Exits when shutdown is requested (the
/// process exits regardless; this just stops taking new work).
fn accept_loop(daemon: Arc<Daemon>, server: tiny_http::Server) {
    let mut next_id: u64 = 0;
    for request in server.incoming_requests() {
        if daemon.shutdown_requested.load(Ordering::Relaxed) {
            break;
        }
        next_id += 1;
        let d = daemon.clone();
        if std::thread::Builder::new()
            .name(format!("dl-http-{next_id}"))
            .spawn(move || handle_http(d, request))
            .is_err()
        {
            tracing::error!("[daemon] http thread spawn failed for request {next_id}");
        }
    }
}

/// Route one HTTP request. `GET /health` is lock-free; `POST /rpc` dispatches
/// through the shared `handle_request`; anything else is 404.
fn handle_http(daemon: Arc<Daemon>, mut request: tiny_http::Request) {
    let method = request.method().clone();
    let path = request.url().split('?').next().unwrap_or("").to_string();

    let (status, body) = match (&method, path.as_str()) {
        (tiny_http::Method::Get, "/health") => (200, health_body(&daemon)),
        (tiny_http::Method::Post, "/rpc") => rpc_body(&daemon, &mut request),
        _ => (404, json!({
            "error": format!("no route for {} {path}", method.as_str()),
        }).to_string()),
    };

    let response = tiny_http::Response::from_string(body)
        .with_status_code(status)
        .with_header(json_content_type());
    if let Err(e) = request.respond(response) {
        tracing::warn!("[daemon] http respond failed: {e}");
    }
}

/// `GET /health` body. Reads the roots-map lock only — never `lock_eng` — so it
/// answers while a tick holds an engine mutex.
fn health_body(daemon: &Arc<Daemon>) -> String {
    json!({
        "build_id": &*daemon.build_id,
        "pid": std::process::id(),
        "roots": daemon.served_root_count(),
    }).to_string()
}

/// `POST /rpc` body. 200 with the serialized `Response` when the RPC executed;
/// 400 with a JSON error when the body is not a valid JSON-RPC request; 400 with
/// a JSON error naming the UDS socket when the method needs a subscriber stream.
fn rpc_body(daemon: &Arc<Daemon>, request: &mut tiny_http::Request) -> (u16, String) {
    let mut raw = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut raw) {
        return (400, err_json(0, PARSE_ERROR, format!("read body: {e}")));
    }
    let value: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => return (400, err_json(0, PARSE_ERROR, format!("parse: {e}"))),
    };
    let req = match daemon::parse_request(value) {
        Some(r) => r,
        None => return (400, err_json(0, INVALID_REQUEST,
            "request needs a numeric `id` and a string `method`")),
    };
    if SUBSCRIBER_METHODS.contains(&req.method.as_str()) {
        return (400, err_json(req.id, INVALID_PARAMS, format!(
            "`{}` needs a kept-open subscriber stream; connect over the UDS socket {} instead",
            req.method,
            daemon::socket_path().display(),
        )));
    }
    // The UDS path passes a subscriber stream only for `subscribe`; every other
    // method gets `None`. Since subscriber methods are rejected above, `None` is
    // always correct here.
    let resp = daemon::handle_request(daemon, &req, None);
    (200, serde_json::to_string(&resp.to_json()).unwrap_or_else(|_| "{}".into()))
}

/// A JSON-RPC error envelope as a string, for the HTTP failure paths.
fn err_json(id: u64, code: i64, message: impl Into<String>) -> String {
    serde_json::to_string(&Response::err(id, code, message).to_json())
        .unwrap_or_else(|_| "{}".into())
}

fn json_content_type() -> tiny_http::Header {
    // Static ASCII; the parse cannot fail.
    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("static content-type header")
}
