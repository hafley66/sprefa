//! sprefa-lsp — diagnostics-only LSP server for `.sprf` files.
//!
//! Pipeline per did_open / did_change:
//!   1. cache full text under the document's URI
//!   2. host_parse(text) -> (Vec<PipeAst>, Vec<Diag>)
//!   3. walk_program(ast, registry, ctx) -> (_, Vec<Diag>)
//!   4. concat both diag streams, convert byte ranges to LSP line/col
//!   5. publish_diagnostics(uri, diags, version)
//!
//! Capabilities: TextDocumentSyncKind::FULL only. Hover, completion,
//! semantic tokens are deferred to later lanes.
//!
//! Logging goes to stderr (LSP convention). RUST_LOG=info or RUST_LOG=debug
//! to see request traces; default is silent.
//!
//! Panic story: trust host_parse + walk_program. If they panic the LSP
//! server crashes and the client respawns. No catch_unwind for now.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use effect_runtime::v2::{
    ByteRange, Diag as ErDiag, FactStore, MemFactStore, Severity,
};

use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, InitializeParams,
    InitializeResult, InitializedParams, MessageType, NumberOrString, Position,
    Range, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
    TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::EnvFilter;

use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};
use v4::Cursor;

struct Backend {
    client: Client,
    /// URI -> last full text. FULL sync, so each entry replaces wholesale.
    /// Held to support future hover/semantic-tokens lanes that need to
    /// re-read source after the request lands. Diags-only doesn't read it
    /// back, but the cache lives here so we don't refactor on first reuse.
    docs:   Mutex<HashMap<Url, DocEntry>>,
}

#[allow(dead_code)]
struct DocEntry {
    text:    String,
    version: i32,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self { client, docs: Mutex::new(HashMap::new()) }
    }

    /// Run parse + walk, publish concatenated diags.
    async fn refresh(&self, uri: Url, text: String, version: i32) {
        // 1. parse
        let (program, parse_diags) = host_parse(&text);

        // 2. walk
        let store: Arc<dyn FactStore<Cursor>> =
            Arc::new(MemFactStore::<Cursor>::new());
        let dir: PathBuf = uri.to_file_path().ok()
            .and_then(|p| p.parent().map(|q| q.to_path_buf()))
            .unwrap_or_else(|| std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from(".")));
        let reg = default_registry();
        let mut ctx = LowerCtx::new(store, dir);
        let (_pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);

        let total = parse_diags.len() + walk_diags.len();
        tracing::debug!(
            uri = %uri, version, parse = parse_diags.len(),
            walk = walk_diags.len(), total, "refresh"
        );

        // 3. convert + publish
        let lsp_diags: Vec<Diagnostic> = parse_diags.iter()
            .chain(walk_diags.iter())
            .map(|d| to_lsp_diag(&text, d))
            .collect();

        // Save text under the URI; bump version.
        {
            let mut g = self.docs.lock().await;
            g.insert(uri.clone(), DocEntry { text, version });
        }

        self.client
            .publish_diagnostics(uri, lsp_diags, Some(version))
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> RpcResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name:    "sprefa-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "sprefa-lsp ready")
            .await;
    }

    async fn shutdown(&self) -> RpcResult<()> { Ok(()) }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let td = params.text_document;
        self.refresh(td.uri, td.text, td.version).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        // FULL sync: exactly one change containing the new full text.
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = match params.content_changes.pop() {
            Some(c) => c.text,
            None    => return,
        };
        self.refresh(uri, text, version).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        {
            let mut g = self.docs.lock().await;
            g.remove(&uri);
        }
        // Clear diagnostics on close.
        self.client.publish_diagnostics(uri, Vec::new(), None).await;
    }
}

// ── diag conversion ──────────────────────────────────────────────────

fn to_lsp_diag(src: &str, d: &ErDiag) -> Diagnostic {
    let range = match d.span {
        Some(br) => byte_range_to_lsp(src, br),
        None     => Range::new(Position::new(0, 0), Position::new(0, 0)),
    };
    Diagnostic {
        range,
        severity: Some(map_severity(d.severity)),
        code:     Some(NumberOrString::String(d.code.to_string())),
        code_description: None,
        source:   Some("sprefa".into()),
        message:  d.message.clone(),
        related_information: None,
        tags:     None,
        data:     None,
    }
}

fn map_severity(s: Severity) -> DiagnosticSeverity {
    match s {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warn  => DiagnosticSeverity::WARNING,
        Severity::Info  => DiagnosticSeverity::INFORMATION,
        Severity::Hint  => DiagnosticSeverity::HINT,
    }
}

/// Convert a [lo, hi) byte range to an LSP `Range` (UTF-16 line/col,
/// 0-indexed). Single hand-rolled scan per call. For diags-only
/// workloads this is fine; if hover/semantic tokens land later, swap
/// in a per-document line-start cache.
fn byte_range_to_lsp(src: &str, br: ByteRange) -> Range {
    let start = byte_to_position(src, br.lo as usize);
    let end   = byte_to_position(src, br.hi as usize);
    Range::new(start, end)
}

fn byte_to_position(src: &str, off: usize) -> Position {
    let off = off.min(src.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let bytes = src.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i == off { break; }
        if *b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    // UTF-16 column: count code units in the slice [line_start, off).
    // For ASCII this equals (off - line_start). For non-ASCII we walk
    // the chars and sum char.len_utf16().
    let slice = &src[line_start..off];
    let col: u32 = if slice.is_ascii() {
        slice.len() as u32
    } else {
        slice.chars().map(|c| c.len_utf16() as u32).sum()
    };
    Position::new(line, col)
}

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
