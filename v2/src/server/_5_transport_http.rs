//! HTTP transport (axum 0.8). D2 ships /status only.
//!
//! Listens on unix socket (preferred) and/or TCP (fallback / escape hatch).
//! TCP uses `axum::serve`. Unix sockets hand-roll the accept loop via
//! `hyper_util::server::conn::auto` because axum::serve is TCP-only.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures_core::stream::Stream;
use futures_util::StreamExt;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as AutoBuilder;
use hyper_util::service::TowerToHyperService;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, UnixListener};

use crate::_0_types::RunEvent;

use super::_0_state::ServerState;
use super::_3_run::{run_pipeline, RunRequest};

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
        .route("/status",   get(status_handler))
        .route("/run",      post(run_handler))
        .route("/lsp",      get(super::_4_transport_lsp::lsp_ws_handler))
        .route("/shutdown", post(shutdown_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// /run — SSE stream of run events
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
pub struct RunDto {
    pub source:         String,
    #[serde(default)]
    pub source_path:    Option<PathBuf>,
    #[serde(default)]
    pub workspace_hint: Option<PathBuf>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RunFrame {
    Meta       { rule_names: Vec<String> },
    Cursor     { expr_name: Option<String>, cursor: CursorDto },
    ExprDone   { expr_name: Option<String> },
    Diag       { code: String, severity: String, message: String },
    Done,
}

#[derive(Serialize, Debug)]
struct CursorDto {
    repo:     String,
    rev:      String,
    fs:       Option<String>,
    captures: std::collections::BTreeMap<String, String>,
}

async fn run_handler(
    State(state): State<Arc<ServerState>>,
    Json(req):    Json<RunDto>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let start = run_pipeline(state.clone(), RunRequest {
        source:         req.source,
        source_path:    req.source_path,
        workspace_hint: req.workspace_hint,
    }).await;

    let meta = RunFrame::Meta {
        rule_names: start.rule_names.iter().map(|r| r.to_string()).collect(),
    };

    let head = futures_util::stream::once(async move { meta });
    let body = start.stream.filter_map(|ev| async move { run_event_to_frame(ev) });

    let combined = head.chain(body)
        .map(|frame| {
            let data = serde_json::to_string(&frame).unwrap_or_else(|e| {
                format!(r#"{{"kind":"diag","code":"server/sse-encode","severity":"Error","message":"{e}"}}"#)
            });
            Ok(Event::default().data(data))
        });

    Sse::new(combined).keep_alive(KeepAlive::default())
}

fn run_event_to_frame(ev: RunEvent) -> Option<RunFrame> {
    match ev {
        RunEvent::Cursor { expr_name, cursor } => {
            let captures = cursor.captures.iter()
                .map(|(k, v)| (k.to_string(), v.value.to_string()))
                .collect();
            Some(RunFrame::Cursor {
                expr_name: expr_name.map(|s| s.to_string()),
                cursor:    CursorDto {
                    repo: cursor.repo.to_string(),
                    rev:  cursor.rev.to_string(),
                    fs:   cursor.fs.as_ref().map(|fp| fp.0.display().to_string()),
                    captures,
                },
            })
        }
        RunEvent::ExprDone { expr_name } => Some(RunFrame::ExprDone {
            expr_name: expr_name.map(|s| s.to_string()),
        }),
        RunEvent::Diag { diag } => {
            let mut cap = CollectRenderer::default();
            diag.render(&mut cap);
            Some(RunFrame::Diag {
                code:     diag.code().to_string(),
                severity: format!("{:?}", diag.severity()),
                message:  cap.into_message(),
            })
        }
        RunEvent::Done                  => Some(RunFrame::Done),
        RunEvent::MutationPrompt { .. } => None, // CLI runs are AutoApprove; prompts never reach clients
    }
}

#[derive(Default)]
struct CollectRenderer {
    header: String,
    notes:  Vec<String>,
}

impl CollectRenderer {
    fn into_message(self) -> String {
        if self.notes.is_empty() { self.header }
        else { format!("{}\n\n{}", self.header, self.notes.join("\n\n")) }
    }
}

impl crate::_1_diagnostic::Renderer for CollectRenderer {
    fn header(&mut self, _code: &str, _sev: crate::_0_types::Severity, msg: &str) {
        self.header = msg.to_string();
    }
    fn primary(&mut self, _site: &crate::_0_types::ParseSite) {}
    fn related(&mut self, _site: &crate::_0_types::ParseSite, msg: &str) {
        self.notes.push(msg.to_string());
    }
    fn note(&mut self, msg: &str) {
        self.notes.push(msg.to_string());
    }
}

async fn shutdown_handler(State(state): State<Arc<ServerState>>) -> Json<serde_json::Value> {
    state.cancel_root.cancel();
    Json(serde_json::json!({ "shutting_down": true }))
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
