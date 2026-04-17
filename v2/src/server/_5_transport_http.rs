//! HTTP transport (axum 0.8). D2 ships /status only.
//!
//! Listens on unix socket (preferred) and/or TCP (fallback / escape hatch).
//! TCP uses `axum::serve`. Unix sockets hand-roll the accept loop via
//! `hyper_util::server::conn::auto` because axum::serve is TCP-only.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as AutoBuilder;
use hyper_util::service::TowerToHyperService;
use serde::Serialize;
use tokio::net::{TcpListener, UnixListener};

use super::_0_state::ServerState;

// ---------------------------------------------------------------------------
// HttpOpts
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct HttpOpts {
    pub unix: Option<PathBuf>,
    pub tcp:  Option<SocketAddr>,
}

// ---------------------------------------------------------------------------
// /status payload
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug)]
pub struct StatusDto {
    pub version:       String,
    pub lsp_doc_count: usize,
    pub workspaces:    usize,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

fn build_router(state: Arc<ServerState>) -> Router {
    Router::new()
        .route("/status", get(status_handler))
        .with_state(state)
}

async fn status_handler(State(state): State<Arc<ServerState>>) -> Json<StatusDto> {
    let lsp_doc_count = state.lsp.doc_count().await;
    let workspaces    = state.workspaces.read().await.len();
    Json(StatusDto {
        version: env!("CARGO_PKG_VERSION").to_string(),
        lsp_doc_count,
        workspaces,
    })
}

// ---------------------------------------------------------------------------
// Serve
// ---------------------------------------------------------------------------

pub async fn serve_http(state: Arc<ServerState>, opts: HttpOpts) -> Result<()> {
    let app = build_router(state.clone());
    let cancel = state.cancel_root.clone();

    let mut tasks: Vec<tokio::task::JoinHandle<Result<()>>> = Vec::new();

    if let Some(sock_path) = opts.unix.clone() {
        if let Some(parent) = sock_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let _ = tokio::fs::remove_file(&sock_path).await;
        let listener = UnixListener::bind(&sock_path)?;
        tracing::info!(socket = %sock_path.display(), "http unix listening");

        let app = app.clone();
        let cancel = cancel.clone();
        tasks.push(tokio::spawn(async move {
            let res = serve_unix(listener, app, cancel).await;
            let _ = tokio::fs::remove_file(&sock_path).await;
            res
        }));
    }

    if let Some(addr) = opts.tcp {
        let listener = TcpListener::bind(addr).await?;
        tracing::info!(addr = %addr, "http tcp listening");
        let app = app.clone();
        let cancel = cancel.clone();
        tasks.push(tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { cancel.cancelled().await })
                .await?;
            Ok(())
        }));
    }

    if tasks.is_empty() {
        anyhow::bail!("serve_http: no listeners configured (need unix, tcp, or both)");
    }

    for t in tasks {
        t.await??;
    }
    Ok(())
}

/// Accept loop for unix sockets. Each connection runs on its own task with
/// hyper's auto HTTP/1+HTTP/2 negotiator (we only need HTTP/1 for unix, but
/// auto is the smallest working config).
async fn serve_unix(
    listener: UnixListener,
    app: Router,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<()> {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(());
            }
            accept = listener.accept() => {
                let (stream, _peer) = match accept {
                    Ok(pair) => pair,
                    Err(e) => {
                        tracing::warn!(error = %e, "unix accept failed");
                        continue;
                    }
                };
                let io = TokioIo::new(stream);
                let svc = TowerToHyperService::new(app.clone());
                let conn_cancel = cancel.clone();
                tokio::spawn(async move {
                    let builder = AutoBuilder::new(TokioExecutor::new());
                    tokio::select! {
                        res = builder.serve_connection(io, svc) => {
                            if let Err(e) = res {
                                tracing::debug!(error = %e, "unix conn closed with error");
                            }
                        }
                        _ = conn_cancel.cancelled() => {}
                    }
                });
            }
        }
    }
}
