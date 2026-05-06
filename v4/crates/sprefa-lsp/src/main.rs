//! sprefa-lsp — every LSP method shims into the unified `SprfClient`.
//!
//! Backend holds an `InProcessClient` (the same `axum::Router` that
//! `sprefa-run` and `sprefa-daemon` consume). Every domain operation
//! goes through one of the registered RPCs in `v4::app::sprf_rpc!`.
//! Backend's only local state is a text cache so byte-offset diags can
//! be converted to LSP line/col.
//!
//! The shim is mechanical: LSP request → SprfClient method → LSP reply.
//! Adding a new sprf RPC adds zero LSP code unless a new LSP request
//! type wants to surface it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, InitializeParams,
    InitializeResult, InitializedParams, InlayHint, InlayHintLabel,
    InlayHintParams, MessageType, NumberOrString, OneOf, Position, Range,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend,
    SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
    WorkDoneProgressOptions,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::EnvFilter;

mod inlay;
mod semantic;

use v4::app::{
    build_in_process, GetDiagsReq, GetInlaysReq, InProcessClient,
    LspChangeReq, LspCloseReq, LspOpenReq, SprfClient, SprfDiag,
};

struct Backend {
    client: Client,
    sprf:   InProcessClient,
    docs:   Mutex<HashMap<Url, DocEntry>>,
}

#[allow(dead_code)]
struct DocEntry {
    text:    String,
    version: i32,
}

impl Backend {
    fn new(client: Client) -> Self {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let (_state, sprf) = build_in_process(root);
        Self { client, sprf, docs: Mutex::new(HashMap::new()) }
    }

    /// Push doc through the sprf RPC, then publish converted diags.
    async fn refresh(&self, uri: Url, text: String, version: i32, is_open: bool) {
        let req_uri = uri.to_string();
        let res = if is_open {
            self.sprf.lsp_open(LspOpenReq {
                uri: req_uri.clone(), text: text.clone(), version,
            }).await
        } else {
            self.sprf.lsp_change(LspChangeReq {
                uri: req_uri.clone(), text: text.clone(), version,
            }).await
        };
        if let Err(e) = res {
            tracing::error!(uri = %uri, %e, "sprf RPC failed");
            return;
        }

        let diags: Vec<SprfDiag> = match self.sprf
            .get_diags(GetDiagsReq { uri: req_uri }).await
        {
            Ok(v)  => v,
            Err(e) => {
                tracing::error!(uri = %uri, %e, "get_diags failed");
                return;
            }
        };

        let lsp_diags: Vec<Diagnostic> = diags.iter()
            .map(|d| to_lsp_diag(&text, d)).collect();

        {
            let mut g = self.docs.lock().await;
            g.insert(uri.clone(), DocEntry { text, version });
        }
        self.client.publish_diagnostics(uri, lsp_diags, Some(version)).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> RpcResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: SemanticTokensLegend {
                                token_types:     semantic::legend_token_types(),
                                token_modifiers: semantic::legend_token_modifiers(),
                            },
                            range: Some(false),
                            full:  Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                inlay_hint_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name:    "sprefa-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "sprefa-lsp ready").await;
    }

    async fn shutdown(&self) -> RpcResult<()> { Ok(()) }

    async fn semantic_tokens_full(
        &self, params: SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        // Tokens computed locally (Backend already has text cached);
        // adding a `/lsp/semantic_tokens` RPC is a one-liner the day
        // a non-LSP shell wants the same data.
        let uri = params.text_document.uri;
        let text = {
            let g = self.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let data = semantic::tokens_for(&text);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None, data,
        })))
    }

    async fn inlay_hint(
        &self, params: InlayHintParams,
    ) -> RpcResult<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let text = {
            let g = self.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let probes = match self.sprf.get_inlays(GetInlaysReq {
            uri: uri.to_string(),
        }).await {
            Ok(v)  => v,
            Err(_) => return Ok(Some(Vec::new())),
        };
        let hints: Vec<InlayHint> = probes.into_iter()
            .map(|p| {
                let pos = byte_to_position(&text, p.hi as usize);
                InlayHint {
                    position: pos,
                    label:    InlayHintLabel::String(format!("→ {} cursor(s)", p.count)),
                    kind:     None,
                    text_edits: None,
                    tooltip: None,
                    padding_left:  Some(true),
                    padding_right: None,
                    data: None,
                }
            }).collect();
        Ok(Some(hints))
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let td = params.text_document;
        self.refresh(td.uri, td.text, td.version, true).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = match params.content_changes.pop() {
            Some(c) => c.text,
            None    => return,
        };
        self.refresh(uri, text, version, false).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let _ = self.sprf.lsp_close(LspCloseReq { uri: uri.to_string() }).await;
        { let mut g = self.docs.lock().await; g.remove(&uri); }
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }
}

// ── diag conversion ──────────────────────────────────────────────────

fn to_lsp_diag(src: &str, d: &SprfDiag) -> Diagnostic {
    let range = match (d.lo, d.hi) {
        (Some(lo), Some(hi)) => Range::new(
            byte_to_position(src, lo as usize),
            byte_to_position(src, hi as usize),
        ),
        _ => Range::new(Position::new(0, 0), Position::new(0, 0)),
    };
    Diagnostic {
        range,
        severity: Some(map_severity(&d.severity)),
        code:     Some(NumberOrString::String(d.code.clone())),
        code_description: None,
        source:   Some("sprefa".into()),
        message:  d.message.clone(),
        related_information: None,
        tags:     None,
        data:     None,
    }
}

fn map_severity(s: &str) -> DiagnosticSeverity {
    match s {
        "error"   => DiagnosticSeverity::ERROR,
        "warning" => DiagnosticSeverity::WARNING,
        "info"    => DiagnosticSeverity::INFORMATION,
        _         => DiagnosticSeverity::HINT,
    }
}

fn byte_to_position(src: &str, off: usize) -> Position {
    let off = off.min(src.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (i, b) in src.as_bytes().iter().enumerate() {
        if i == off { break; }
        if *b == b'\n' { line += 1; line_start = i + 1; }
    }
    let slice = &src[line_start..off];
    let col: u32 = if slice.is_ascii() {
        slice.len() as u32
    } else {
        slice.chars().map(|c| c.len_utf16() as u32).sum()
    };
    Position::new(line, col)
}

// suppress unused warning on Arc until something needs it later
#[allow(dead_code)]
fn _arc_marker() -> Arc<()> { Arc::new(()) }

// ── entry point ──────────────────────────────────────────────────────

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
