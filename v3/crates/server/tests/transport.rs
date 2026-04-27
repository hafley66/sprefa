//! Integration tests for the four daemon endpoints: /status, /run (SSE),
//! /lsp (WS), /shutdown. Each spins up a ServerState, binds a fresh unix
//! socket under TMPDIR, exercises the endpoint, and tears down.
//!
//! These complement the in-process backend tests (which exercise hover /
//! did_change / cursor rendering directly) by validating the wire shape:
//! JSON for /status, SSE framing for /run, tower-lsp Content-Length
//! framing inside WS Binary frames for /lsp, and JSON ack for /shutdown.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper::{Request, Uri};
use hyper_util::rt::TokioIo;
use serde_json::Value;
use tokio::net::UnixStream;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request as WsRequest;
use tokio_tungstenite::tungstenite::protocol::Message;

use server::{serve_http, HttpOpts, ServerOpts, ServerState};

struct Daemon {
    sock_path: PathBuf,
    state:     Arc<ServerState>,
    serve:     tokio::task::JoinHandle<()>,
}

impl Daemon {
    async fn spawn() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let sock_path = std::env::temp_dir()
            .join(format!("sprefa-test-{}-{}.sock", std::process::id(), nonce));
        let info_path = std::env::temp_dir()
            .join(format!("sprefa-test-{}-{}.json", std::process::id(), nonce));

        let opts = ServerOpts {
            http_unix:  Some(sock_path.clone()),
            http_tcp:   None,
            log_path:   None,
            info_path:  Some(info_path),
            foreground: true,
        };
        let state = ServerState::new(&opts).await.unwrap();
        let http_opts = HttpOpts { unix: Some(sock_path.clone()), tcp: None };
        let state_for_serve = state.clone();
        let serve = tokio::spawn(async move {
            let _ = serve_http(state_for_serve, http_opts).await;
        });

        // Wait for the socket to exist.
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if sock_path.exists() { break; }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Self { sock_path, state, serve }
    }

    async fn shutdown(self) {
        self.state.cancel_root.cancel();
        let _ = self.serve.await;
        let _ = std::fs::remove_file(&self.sock_path);
    }
}

async fn http_get(sock: &std::path::Path, path: &str) -> (u16, String) {
    let stream = UnixStream::connect(sock).await.unwrap();
    let (mut sender, conn) = http1::handshake(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(conn);
    let req = Request::builder()
        .method("GET").uri(path.parse::<Uri>().unwrap())
        .header("host", "localhost")
        .body(Full::<Bytes>::new(Bytes::new())).unwrap();
    let resp = sender.send_request(req).await.unwrap();
    let status = resp.status().as_u16();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

async fn http_post(sock: &std::path::Path, path: &str, body: String) -> (u16, String) {
    let stream = UnixStream::connect(sock).await.unwrap();
    let (mut sender, conn) = http1::handshake(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(conn);
    let req = Request::builder()
        .method("POST").uri(path.parse::<Uri>().unwrap())
        .header("host", "localhost")
        .header("content-type", "application/json")
        .body(Full::<Bytes>::new(Bytes::from(body))).unwrap();
    let resp = sender.send_request(req).await.unwrap();
    let status = resp.status().as_u16();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

async fn http_post_sse_collect(sock: &std::path::Path, path: &str, body: String) -> Vec<String> {
    let stream = UnixStream::connect(sock).await.unwrap();
    let (mut sender, conn) = http1::handshake(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(conn);
    let req = Request::builder()
        .method("POST").uri(path.parse::<Uri>().unwrap())
        .header("host", "localhost")
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .body(Full::<Bytes>::new(Bytes::from(body))).unwrap();
    let resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 200, "/run not 200");
    let mut body = resp.into_body();
    let mut buf = String::new();
    let mut frames: Vec<String> = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.unwrap();
        if let Some(data) = frame.data_ref() {
            buf.push_str(std::str::from_utf8(data).unwrap());
            while let Some(idx) = buf.find("\n\n") {
                let event: String = buf.drain(..=idx + 1).collect();
                for line in event.lines() {
                    if let Some(json) = line.strip_prefix("data: ") {
                        frames.push(json.to_string());
                    }
                }
            }
        }
    }
    frames
}

#[tokio::test(flavor = "multi_thread")]
async fn status_endpoint_returns_workspaces_and_cache_len() {
    let d = Daemon::spawn().await;
    let (code, body) = http_get(&d.sock_path, "/status").await;
    assert_eq!(code, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert!(v.get("version").is_some(), "version field missing: {body}");
    assert_eq!(v["workspaces"], 0);
    assert_eq!(v["cache_len"], 0);
    d.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn run_endpoint_streams_sse_frames_for_minimal_source() {
    // Source with a noop pipe — exercises parse + lower + drain + Done
    // without needing a real seed root that produces cursors.
    let d = Daemon::spawn().await;
    let body = serde_json::json!({
        "source": "void(*)\n",
        "source_path": null,
        "workspace_hint": std::env::temp_dir(),
        "out_db": null,
        "cache": false,
    }).to_string();
    let frames = http_post_sse_collect(&d.sock_path, "/run", body).await;
    assert!(!frames.is_empty(), "no SSE frames received");
    // First frame is Meta with rule_names.
    let first: Value = serde_json::from_str(&frames[0]).unwrap();
    assert_eq!(first["kind"], "meta", "first frame must be meta: {first}");
    // Last frame is Done.
    let last: Value = serde_json::from_str(frames.last().unwrap()).unwrap();
    assert_eq!(last["kind"], "done", "last frame must be done: {last}");
    d.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn run_endpoint_surfaces_parse_diags_for_invalid_source() {
    // Garbage source: triggers parse errors that should surface as Diag
    // frames before Done.
    let d = Daemon::spawn().await;
    let body = serde_json::json!({
        "source": "this is not valid sprf > > >",
        "source_path": null,
        "workspace_hint": std::env::temp_dir(),
        "out_db": null,
        "cache": false,
    }).to_string();
    let frames = http_post_sse_collect(&d.sock_path, "/run", body).await;
    let kinds: Vec<String> = frames.iter()
        .map(|s| serde_json::from_str::<Value>(s).unwrap()["kind"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(kinds.first().map(|s| s.as_str()), Some("meta"));
    assert_eq!(kinds.last().map(|s| s.as_str()), Some("done"));
    // At least one diag in between (parse error).
    assert!(kinds.iter().any(|k| k == "diag"), "expected diag frame: {kinds:?}");
    d.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn lsp_endpoint_responds_to_initialize_over_websocket() {
    let d = Daemon::spawn().await;

    // Hand-rolled WS handshake over the unix socket.
    let stream = UnixStream::connect(&d.sock_path).await.unwrap();
    let req = WsRequest::builder()
        .method("GET").uri("ws://localhost/lsp")
        .header("host", "localhost")
        .header("upgrade", "websocket")
        .header("connection", "Upgrade")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", generate_key())
        .body(()).unwrap();
    let (ws_stream, _resp) = tokio_tungstenite::client_async(req, stream).await.unwrap();
    let (mut tx, mut rx) = ws_stream.split();

    // Send a tower-lsp initialize request, framed with Content-Length.
    let init = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "processId": null,
            "rootUri": null,
            "workspaceFolders": null,
        }
    }).to_string();
    let framed = format!("Content-Length: {}\r\n\r\n{init}", init.len());
    tx.send(Message::Binary(framed.into_bytes().into())).await.unwrap();

    // Read response frames until we see the initialize result. tower-lsp
    // may split header + body into separate WS frames.
    let mut acc = Vec::<u8>::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut got_result = false;
    while Instant::now() < deadline && !got_result {
        let msg = match tokio::time::timeout(Duration::from_millis(500), rx.next()).await {
            Ok(Some(Ok(m))) => m,
            _ => continue,
        };
        match msg {
            Message::Binary(b) => acc.extend_from_slice(&b),
            Message::Text(t)   => acc.extend_from_slice(t.as_bytes()),
            Message::Close(_)  => break,
            _ => continue,
        }
        // Look for `\r\n\r\n` separator; everything after is the JSON body.
        if let Some(idx) = find_sep(&acc) {
            let header = std::str::from_utf8(&acc[..idx]).unwrap_or_default();
            let len: Option<usize> = header.lines()
                .find_map(|l| l.strip_prefix("Content-Length: ").and_then(|n| n.trim().parse().ok()));
            if let Some(n) = len {
                let body_start = idx + 4;
                if acc.len() >= body_start + n {
                    let body_bytes = &acc[body_start..body_start + n];
                    let v: Value = serde_json::from_slice(body_bytes).unwrap();
                    assert_eq!(v["id"], 1, "initialize response id mismatch: {v}");
                    assert!(
                        v.get("result").is_some(),
                        "expected result field on initialize response: {v}",
                    );
                    let caps = &v["result"]["capabilities"];
                    assert!(
                        caps.get("hoverProvider").is_some()
                            || caps.get("hover_provider").is_some(),
                        "hover_provider missing from initialize response: {v}",
                    );
                    got_result = true;
                }
            }
        }
    }
    assert!(got_result, "initialize did not round-trip within deadline");

    // Polite WS close.
    let _ = tx.close().await;
    d.shutdown().await;
}

fn find_sep(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_endpoint_trips_cancel_root_and_returns_ack() {
    let d = Daemon::spawn().await;
    let cancel = d.state.cancel_root.clone();
    let sock_path = d.sock_path.clone();
    assert!(!cancel.is_cancelled());
    let (code, body) = http_post(&sock_path, "/shutdown", "{}".into()).await;
    assert_eq!(code, 200);
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["shutting_down"], true);
    assert!(cancel.is_cancelled(), "/shutdown did not trip cancel_root");
    // serve_http observes the cancel and exits; the standard teardown
    // joins it under shutdown().
    d.shutdown().await;
}
