//! The Unix-domain-socket transport: accept task + one task per connection.
//! Replaces the old `dl-accept` thread + thread-per-connection `handle_connection`
//! shape. The wire protocol is unchanged (`Content-Length`-framed JSON-RPC), so
//! every installed CLI/LSP/MCP client keeps working byte-for-byte.
//!
//! Engine dispatch (`crate::daemon::handle_request`) is synchronous and touches
//! `lock_eng` / the `prog` lock, so it runs on the `spawn_blocking` pool — the two
//! shell workers stay free to accept sockets and answer `/health` while a tick is
//! in flight. `subscribe` is the one method handled inline (async): its response
//! goes out over this connection's write half, then that write half moves into
//! the subscriber pump so `diag_changed` / `rev_advanced` frames push over the
//! SAME socket the client kept open.

use std::os::unix::net::UnixListener as StdUnixListener;
use std::sync::Arc;

use serde_json::Value;
use tokio::io::BufReader;
use tokio::net::UnixListener;

use super::{read_frame, write_frame, BroadcastMsg, ShellCtx};
use crate::daemon::{self, Daemon};
use crate::rpc::{Response, INTERNAL_ERROR, PARSE_ERROR};

/// Spawn the accept task. The std listener was bound in `run_daemon` (fail-fast
/// on a double-bind) and set non-blocking; `from_std` adopts it into the reactor
/// inside the runtime. One task per accepted connection, cancelled by the token.
pub(crate) fn spawn_accept(ctx: &ShellCtx, daemon: Arc<Daemon>, std_listener: StdUnixListener) {
    let ctx_task = ctx.clone();
    ctx.rt.spawn(async move {
        let listener = match UnixListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                tracing::error!("[daemon] adopt UDS listener into runtime: {e}");
                return;
            }
        };
        loop {
            tokio::select! {
                _ = ctx_task.cancel.cancelled() => break,
                res = listener.accept() => match res {
                    Ok((stream, _addr)) => {
                        let d = daemon.clone();
                        let c = ctx_task.clone();
                        ctx_task.rt.spawn(handle_connection(d, c, stream));
                    }
                    Err(e) => tracing::warn!("[daemon] accept error: {e}"),
                },
            }
        }
    });
}

/// One connection: read framed requests, dispatch each on the blocking pool,
/// write the framed response. `subscribe` is intercepted here (see module doc);
/// `shutdown` cancels the token AFTER its response is flushed, matching the old
/// handler's "respond, then exit" ordering.
async fn handle_connection(daemon: Arc<Daemon>, ctx: ShellCtx, stream: tokio::net::UnixStream) {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    loop {
        let body = match read_frame(&mut reader).await {
            Ok(Some(b)) => b,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!("[daemon] read error: {e}");
                return;
            }
        };
        let v: Value = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(e) => {
                let r = Response::err(0, PARSE_ERROR, format!("parse: {e}"));
                let out = serde_json::to_string(&r.to_json()).unwrap_or_else(|_| "{}".into());
                if write_frame(&mut write, &out).await.is_err() {
                    return;
                }
                continue;
            }
        };
        let req = match daemon::parse_request(v) {
            Some(r) => r,
            None => continue,
        };

        // `subscribe` keeps the socket open for server-sent notifications: ack
        // over the write half, then hand that write half to the pump. The read
        // half is dropped; a disconnect surfaces as a failed push (best-effort).
        if req.method == "subscribe" {
            let events = req
                .params
                .get("events")
                .cloned()
                .unwrap_or(Value::Array(vec![]));
            let resp = Response::ok(req.id, serde_json::json!({"events": events, "ok": true}));
            let out = serde_json::to_string(&resp.to_json()).unwrap_or_else(|_| "{}".into());
            if write_frame(&mut write, &out).await.is_err() {
                return;
            }
            let _ = ctx.broadcast_tx.send(BroadcastMsg::Add(write));
            return;
        }

        let is_shutdown = req.method == "shutdown";
        let req_id = crate::reqid::next();
        let method = req.method.clone();
        let root = req.params.get("root").and_then(|v| v.as_str()).map(String::from);
        let bytes_in = body.len();
        let t = std::time::Instant::now();
        let d = daemon.clone();
        let req_id_for_dispatch = req_id.clone();
        // Engine dispatch is sync and lock-taking: run it on the blocking pool.
        let resp = match tokio::task::spawn_blocking(move || {
            daemon::handle_request(&d, &req, &req_id_for_dispatch)
        })
        .await
        {
            Ok(r) => r,
            Err(_) => Response::err(0, INTERNAL_ERROR, "dispatch task panicked"),
        };
        let out = serde_json::to_string(&resp.to_json()).unwrap_or_else(|_| "{}".into());
        daemon::log_access(
            "sock", &req_id, &method, root.as_deref(),
            t.elapsed().as_millis() as u64, resp.error.is_none(), bytes_in, out.len(),
        );
        if write_frame(&mut write, &out).await.is_err() {
            return;
        }
        if is_shutdown {
            // The response is flushed; now drive the graceful stop. The shutdown
            // task (awaiting the token) runs `shutdown_cleanup` + exits.
            ctx.cancel.cancel();
            return;
        }
    }
}
