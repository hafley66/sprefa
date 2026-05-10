//! sprefa-lsp — every LSP method shims into the unified `SprfClient`.
//!
//! Backend holds a `SprfClient`. By default this is the in-process
//! `axum::Router` used by `sprefa-run`; when `SPREFA_LSP_DAEMON_URL`
//! is set it becomes an HTTP client for `sprefa-daemon`. Every domain
//! operation goes through one of the registered RPCs in
//! `v4::app::sprf_rpc!`. Backend's only local state is a text cache so
//! byte-offset diags can be converted to LSP line/col.
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
    CompletionItem, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, InitializedParams, InlayHint, InlayHintLabel,
    InlayHintParams, MarkedString, MessageType, NumberOrString, OneOf,
    Position, Range, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
    WorkDoneProgressOptions,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::EnvFilter;

mod dsl_lookup;
mod inlay;
mod semantic;

use v4::app::{
    build_in_process, GetDiagsReq, GetInlaysReq, HttpClient,
    LspChangeReq, LspCloseReq, LspHoverReq, LspLocateDslReq, LspLocateDslResp,
    LspOpenReq, SprfClient, SprfDiag,
};

struct Backend {
    client: Client,
    sprf:   Arc<dyn SprfClient>,
    docs:   Mutex<HashMap<Url, DocEntry>>,
}

#[allow(dead_code)]
struct DocEntry {
    text:    String,
    version: i32,
}

impl Backend {
    fn new(client: Client) -> Self {
        let sprf: Arc<dyn SprfClient> = match std::env::var("SPREFA_LSP_DAEMON_URL") {
            Ok(base) if !base.trim().is_empty() => Arc::new(HttpClient::new(base)),
            _ => {
                let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let (_state, sprf) = build_in_process(root);
                Arc::new(sprf)
            }
        };
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
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    // `$` opens host pipe-holes; `:` opens atoms; `.` is
                    // reserved future "field access". Triggering on these
                    // keystrokes makes type-ahead feel native instead of
                    // ctrl-space-only.
                    trigger_characters: Some(vec![
                        "$".into(), ":".into(), ".".into(),
                    ]),
                    resolve_provider: Some(false),
                    all_commit_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    completion_item: None,
                }),
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

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let text = {
            let g = self.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let host_byte = position_to_byte(&text, pos);
        let resp = match self.sprf.lsp_hover(LspHoverReq {
            uri:  uri.to_string(),
            byte: host_byte as u32,
        }).await {
            Ok(h)  => h,
            Err(_) => return Ok(None),
        };
        let Some(contents) = resp.contents else { return Ok(None); };
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(contents)),
            range: None,
        }))
    }

    async fn completion(
        &self, params: CompletionParams,
    ) -> RpcResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let text = {
            let g = self.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let host_byte = position_to_byte(&text, pos);
        let hit: LspLocateDslResp = match self.sprf.lsp_locate_dsl(LspLocateDslReq {
            uri:  uri.to_string(),
            byte: host_byte as u32,
        }).await {
            Ok(h)  => h,
            // Outside any dsl body, or unopened uri — host-level
            // completion (op names, atoms, term refs) lives later.
            Err(_) => return Ok(Some(CompletionResponse::Array(Vec::new()))),
        };
        let v4_items: Vec<v4_lsp_types::CompletionItem> = match (hit.op_name, hit.body_raw) {
            (Some(op_name), Some(body_raw)) => match dsl_lookup::provider_for(&op_name) {
                Some(handle) => match handle.lsp() {
                    Some(lsp) => lsp.completions(body_raw.as_bytes(), hit.body_byte as usize),
                    None      => Vec::new(),
                },
                None => Vec::new(),
            },
            _ => Vec::new(),
        };
        let items: Vec<CompletionItem> = v4_items.iter()
            .filter_map(|it| crosswalk::<_, CompletionItem>(it))
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
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

/// `lsp_types` v0.97 (used by v4 lib) ↔ v0.94 (used by tower-lsp 0.20).
/// They share JSON shape, so serde round-trip is the cheapest bridge.
/// Returns `None` only if the source value fails to serialize, which
/// is essentially never for the structures we cross.
fn crosswalk<S: serde::Serialize, D: serde::de::DeserializeOwned>(src: &S) -> Option<D> {
    serde_json::from_value(serde_json::to_value(src).ok()?).ok()
}

/// Re-export of v4 lib's lsp-types under a non-clashing name. Hover
/// and CompletionItem returned by `DslBodyLsp` are 0.97 types; main.rs
/// otherwise speaks 0.94 (tower-lsp's vendored version).
mod v4_lsp_types {
    pub use lsp_types::CompletionItem;
}

/// Convert an LSP (line, utf16-col) position into a host-source byte
/// offset. Inverse of `byte_to_position`. Saturates at end-of-line on
/// past-end columns and at end-of-document on past-end lines.
fn position_to_byte(src: &str, p: Position) -> usize {
    let bytes = src.as_bytes();
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let mut i: usize = 0;
    while i < bytes.len() && line < p.line {
        if bytes[i] == b'\n' { line += 1; line_start = i + 1; }
        i += 1;
    }
    if line < p.line { return bytes.len(); }
    let line_end = (line_start..bytes.len()).find(|&j| bytes[j] == b'\n').unwrap_or(bytes.len());
    let line_str = &src[line_start..line_end];
    let mut col_left: u32 = p.character;
    let mut byte = line_start;
    if line_str.is_ascii() {
        byte += (col_left as usize).min(line_str.len());
    } else {
        for ch in line_str.chars() {
            let w = ch.len_utf16() as u32;
            if col_left < w { break; }
            col_left -= w;
            byte += ch.len_utf8();
        }
    }
    byte
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
