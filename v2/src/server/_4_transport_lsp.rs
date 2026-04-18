//! LSP-over-WebSocket transport.
//!
//! axum extracts the WebSocket upgrade, then a tokio duplex pipe bridges
//! framed JSON-RPC bytes into `tower_lsp::Server::new(reader, writer, socket)`.
//! Clients (the thin `sprefa-lsp` proxy) send stdin bytes verbatim as WS
//! binary messages; this side reassembles them into one byte stream so
//! tower-lsp's own Content-Length framing still applies.
//!
//! Per-WS-connection Backend holds a local per-URI session map. Workspaces
//! resolve through ServerState.workspaces so every connection sees the same
//! cached WorkspaceCtx for a given root. Registry is shared via Arc.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::analysis::DocSession;
use crate::{ParseSite, Renderer, Severity};

use super::_0_state::ServerState;

// ---------------------------------------------------------------------------
// Axum handler — upgrades WebSocket and runs tower-lsp over the duplex bridge
// ---------------------------------------------------------------------------

pub async fn lsp_ws_handler(
    State(state): State<Arc<ServerState>>,
    ws:           WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| handle_ws(state, socket))
}

async fn handle_ws(state: Arc<ServerState>, ws: WebSocket) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    // server_io is what tower-lsp reads/writes; proxy_io is our outside end.
    let (server_io, proxy_io) = tokio::io::duplex(1 << 16);
    let (server_rd, server_wr) = tokio::io::split(server_io);
    let (mut proxy_rd, mut proxy_wr) = tokio::io::split(proxy_io);

    // Pump WebSocket inbound → pipe (server reads LSP frames from here).
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

    // Pump pipe → WebSocket outbound.
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

    let (service, socket) = LspService::new(|client| Backend {
        client,
        state,
        sessions:  Arc::new(Mutex::new(HashMap::new())),
        init_root: Arc::new(Mutex::new(None)),
    });
    Server::new(server_rd, server_wr, socket).serve(service).await;

    inbound.abort();
    outbound.abort();
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

struct Backend {
    client:    Client,
    state:     Arc<ServerState>,
    sessions:  Arc<Mutex<HashMap<Url, SessionEntry>>>,
    init_root: Arc<Mutex<Option<PathBuf>>>,
}

struct SessionEntry {
    session:        DocSession,
    source:         String,
    workspace_root: Option<PathBuf>,
}

impl Backend {
    async fn open_or_replace(&self, uri: &Url, source: String) {
        let hint = uri_to_hint(uri);
        let workspace = self.state.workspaces.write().await.resolve(&hint).await;
        let init_root = self.init_root.lock().await.clone();
        let workspace_root = Some(workspace.root.to_path_buf()).or(init_root);
        let root_arc: Option<Arc<Path>> = workspace_root.as_ref().map(|p| Arc::from(p.as_path()));

        let mut guard = self.sessions.lock().await;
        match guard.get_mut(uri) {
            Some(entry) => {
                entry.session.on_source_change(source.clone());
                entry.session.set_workspace_root(root_arc);
                entry.source = source;
                entry.workspace_root = workspace_root;
                entry.session.ensure_run();
            }
            None => {
                let mut session = DocSession::new(
                    source.clone(),
                    workspace.reader.clone(),
                    self.state.registry.clone(),
                );
                session.set_workspace_root(root_arc);
                session.ensure_run();
                guard.insert(uri.clone(), SessionEntry { session, source, workspace_root });
            }
        }
    }

    async fn publish(&self, uri: &Url) {
        let mut guard = self.sessions.lock().await;
        let Some(entry) = guard.get_mut(uri) else { return; };
        type DKey = (String, usize, usize);
        let success_sites = entry.session.successful_sites();
        let mut counts: std::collections::HashMap<DKey, usize> = std::collections::HashMap::new();
        let mut firsts: Vec<(DKey, Severity, String)> = Vec::new();
        for d in entry.session.parse_diagnostics().iter().chain(entry.session.diagnostics().iter()) {
            let site = d.primary();
            let key  = (d.code().to_string(), site.byte_range.start, site.byte_range.end);
            let n = counts.entry(key.clone()).or_insert(0);
            *n += 1;
            if *n == 1 {
                let mut cap = CapturingRenderer::default();
                d.render(&mut cap);
                let body = if cap.notes.is_empty() { cap.header_msg }
                           else { format!("{}\n\n{}", cap.header_msg, cap.notes.join("\n\n")) };
                let code_str = d.code().to_string();
                let sev = if (code_str.ends_with("/no-match") || code_str.ends_with("/partial-match"))
                    && success_sites.contains(&(site.file.clone(), site.byte_range.clone()))
                { Severity::Hint } else { d.severity() };
                firsts.push((key, sev, body));
            }
        }
        let mut diags: Vec<Diagnostic> = Vec::new();
        for (key, sev, body) in firsts {
            let (code, byte_start, byte_end) = key.clone();
            let n = counts.get(&key).copied().unwrap_or(1);
            let start = offset_to_position(&entry.source, byte_start);
            let end   = offset_to_position(&entry.source, byte_end);
            let msg   = if n > 1 { format!("{body} (×{n})") } else { body };
            diags.push(Diagnostic {
                range:    Range { start, end },
                severity: Some(severity_to_lsp(sev)),
                code:     Some(NumberOrString::String(code)),
                source:   Some("sprefa".to_string()),
                message:  msg,
                ..Default::default()
            });
        }
        let uri_owned = uri.clone();
        drop(guard);
        self.client.publish_diagnostics(uri_owned, diags, None).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, p: InitializeParams) -> RpcResult<InitializeResult> {
        #[allow(deprecated)]
        let root_path: Option<PathBuf> = p.root_uri.as_ref()
            .and_then(|u| u.to_file_path().ok())
            .or_else(|| p.root_path.clone().map(PathBuf::from));
        *self.init_root.lock().await = root_path;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
                hover_provider:      Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name:    "sprefa-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {}
    async fn shutdown(&self) -> RpcResult<()> { Ok(()) }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        self.open_or_replace(&uri, p.text_document.text.clone()).await;
        self.publish(&uri).await;
    }

    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        if let Some(change) = p.content_changes.into_iter().last() {
            self.open_or_replace(&uri, change.text).await;
            self.publish(&uri).await;
        }
    }

    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        let mut guard = self.sessions.lock().await;
        if let Some(entry) = guard.get_mut(&uri) { entry.session.ensure_run(); }
        drop(guard);
        self.publish(&uri).await;
    }

    async fn hover(&self, p: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri.clone();
        let pos = p.text_document_position_params.position;
        let mut guard = self.sessions.lock().await;
        let Some(entry) = guard.get_mut(&uri) else { return Ok(None); };
        let offset = position_to_offset(&entry.source, pos);
        let md = entry.session.hover_at(offset);
        let workspace_root = entry.workspace_root.clone();
        Ok(md.map(|value| {
            let value = match &workspace_root {
                Some(root) => rewrite_hover_paths(&value, root),
                None       => value,
            };
            Hover {
                contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value }),
                range:    None,
            }
        }))
    }

    async fn completion(&self, p: CompletionParams) -> RpcResult<Option<CompletionResponse>> {
        let uri = p.text_document_position.text_document.uri.clone();
        let pos = p.text_document_position.position;
        let mut guard = self.sessions.lock().await;
        let Some(entry) = guard.get_mut(&uri) else { return Ok(None); };
        let offset = position_to_offset(&entry.source, pos);
        let items = entry.session.completions_at(offset);
        let source = entry.source.clone();
        let lsp_items: Vec<CompletionItem> = items.into_iter().map(|c| {
            let text_edit = c.replace_range.as_ref().map(|r| {
                let start = offset_to_position(&source, r.start);
                let end   = offset_to_position(&source, r.end);
                CompletionTextEdit::Edit(TextEdit {
                    range:    Range { start, end },
                    new_text: c.label.clone(),
                })
            });
            let insert_text = if text_edit.is_some() { None } else { c.insert.map(|s| s.to_string()) };
            CompletionItem {
                label:         c.label,
                detail:        Some(c.detail),
                insert_text,
                text_edit,
                filter_text:   c.filter_text.map(|s| s.to_string()),
                kind:          c.kind_hint.as_deref().and_then(kind_from_hint),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind:  MarkupKind::Markdown,
                    value: c.doc,
                })),
                ..Default::default()
            }
        }).collect();
        Ok(Some(CompletionResponse::Array(lsp_items)))
    }
}

// ---------------------------------------------------------------------------
// Helpers (ported from sprefa_v2_lsp.rs)
// ---------------------------------------------------------------------------

fn uri_to_hint(uri: &Url) -> PathBuf {
    uri.to_file_path()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn position_to_offset(text: &str, pos: Position) -> usize {
    crate::position::position_to_offset(text, pos.line, pos.character)
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let (line, character) = crate::position::offset_to_position(text, offset);
    Position { line, character }
}

fn severity_to_lsp(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warn  => DiagnosticSeverity::WARNING,
        Severity::Info  => DiagnosticSeverity::INFORMATION,
        Severity::Hint  => DiagnosticSeverity::HINT,
    }
}

fn kind_from_hint(hint: &str) -> Option<CompletionItemKind> {
    match hint {
        "op"     => Some(CompletionItemKind::FUNCTION),
        "repo"   => Some(CompletionItemKind::MODULE),
        "rev"    => Some(CompletionItemKind::REFERENCE),
        "fs"     => Some(CompletionItemKind::FILE),
        "branch" => Some(CompletionItemKind::REFERENCE),
        "tag"    => Some(CompletionItemKind::REFERENCE),
        _        => None,
    }
}

fn rewrite_hover_paths(md: &str, workspace_root: &Path) -> String {
    let mut out = String::with_capacity(md.len());
    for line in md.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("- `") {
            if let Some(end) = rest.find('`') {
                let path = &rest[..end];
                let abs = workspace_root.join(path);
                if abs.exists() {
                    let tail = &rest[end + 1..];
                    out.push_str(&format!("- [`{path}`](file://{})", abs.display()));
                    out.push_str(tail);
                    continue;
                }
            }
        }
        out.push_str(line);
    }
    out
}

#[derive(Default)]
struct CapturingRenderer {
    header_msg: String,
    notes:      Vec<String>,
}

impl Renderer for CapturingRenderer {
    fn header(&mut self, _code: &str, _sev: Severity, message: &str) { self.header_msg = message.to_string(); }
    fn primary(&mut self, _site: &ParseSite) {}
    fn related(&mut self, _site: &ParseSite, message: &str) { self.notes.push(message.to_string()); }
    fn note(&mut self, message: &str) { self.notes.push(message.to_string()); }
}
