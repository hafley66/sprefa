//! LSP-over-WebSocket transport.
//!
//! axum extracts the WebSocket upgrade. A `tokio::io::duplex` pipe
//! bridges WS frame bytes into `tower_lsp::Server`. Clients (the thin
//! `sprefa-lsp` proxy) send stdin bytes verbatim as WS binary messages;
//! tower-lsp's own Content-Length framing reassembles them.
//!
//! The Backend held inside the WS connection takes `Arc<ServerState>`
//! so all sessions share one OpCache, one ast_parse_batcher, one
//! workspace registry across the whole daemon.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower_lsp::{LspService, Server};

use crate::backend::Backend;
use crate::state::ServerState;

pub async fn lsp_ws_handler(
    State(state): State<Arc<ServerState>>,
    ws:           WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_ws(state, socket))
}

async fn handle_ws(state: Arc<ServerState>, ws: WebSocket) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (server_io, proxy_io) = tokio::io::duplex(1 << 16);
    let (server_rd, server_wr) = tokio::io::split(server_io);
    let (mut proxy_rd, mut proxy_wr) = tokio::io::split(proxy_io);

    let inbound = tokio::spawn(async move {
        while let Some(frame) = ws_rx.next().await {
            let Ok(msg) = frame else { break; };
            let bytes = match msg {
                Message::Text(t)   => t.as_bytes().to_vec(),
                Message::Binary(b) => b.to_vec(),
                Message::Close(_)  => break,
                _ => continue,
            };
            if proxy_wr.write_all(&bytes).await.is_err() { break; }
        }
        let _ = proxy_wr.shutdown().await;
    });

    let outbound = tokio::spawn(async move {
        let mut buf = vec![0u8; 16 << 10];
        loop {
            let n = match proxy_rd.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n)          => n,
            };
            if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() { break; }
        }
        let _ = ws_tx.close().await;
    });

    let (service, socket) = LspService::new(|client| Backend::with_state(client, state.clone()));
    Server::new(server_rd, server_wr, socket).serve(service).await;

    inbound.abort();
    outbound.abort();
}
