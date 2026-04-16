//! sprefa v2 LSP — Layer 4.
//!
//! Thin tower-lsp dispatcher. Per-URI DocSession. Position translation is
//! character-count based (ASCII-approximation — non-ASCII sources may be off
//! for multi-byte code points). Adequate for .sprf files in practice.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use v2::analysis::DocSession;
use v2::ops::default_registry;
use v2::readers::{
    BufferOverlay, CheckoutLocator, ConfigLocator, GitBlobReader, InMemoryLocator, MemReader,
};
use v2::{Config, OperatorRegistry, ParseSite, Reader, Renderer, RuntimeConfig, Severity};

// ---------------------------------------------------------------------------
// CapturingRenderer — collects render() output into strings for LSP messages.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CapturingRenderer {
    header_msg: String,
    notes:      Vec<String>,
}

impl Renderer for CapturingRenderer {
    fn header(&mut self, _code: &str, _sev: Severity, message: &str) {
        self.header_msg = message.to_string();
    }
    fn primary(&mut self, _site: &ParseSite) {}
    fn related(&mut self, _site: &ParseSite, message: &str) {
        self.notes.push(message.to_string());
    }
    fn note(&mut self, message: &str) {
        self.notes.push(message.to_string());
    }
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

struct Backend {
    client:   Client,
    sessions: Arc<Mutex<HashMap<Url, SessionEntry>>>,
    registry: Arc<OperatorRegistry>,
    /// Workspace root captured from `initialize` params (`root_uri`, or
    /// `root_path` fallback). `None` under single-file mode; per-document
    /// auto-detection in `resolve_reader` is the fallback.
    init_root: Arc<Mutex<Option<PathBuf>>>,
}

struct SessionEntry {
    session:        DocSession,
    source:         String,
    /// Absolute on-disk git root (or first-configured repo path) for this
    /// document. Used to rewrite repo-relative `` `path` `` backticks in
    /// hover markdown into clickable `[path](file:///ABS)` links.
    workspace_root: Option<PathBuf>,
}

fn make_config_from_locator(loc: &dyn CheckoutLocator) -> Arc<Config> {
    let repos = loc.repos();
    let mut revs: Vec<Arc<str>> = Vec::new();
    for r in &repos {
        for rv in loc.revs(r) {
            if !revs.contains(&rv) {
                revs.push(rv);
            }
        }
    }
    Arc::new(Config {
        repos,
        revs,
        fs_exclude:  vec![],
        sprf_files:  vec![],
        shell_allow: vec![],
        runtime: RuntimeConfig {
            worker_threads:    1,
            buffer_size:       256,
            flush_interval_ms: 100,
            collect_witnesses: true,
            xref_cartesian_limit: 10_000,
            max_passes:           8,
            max_claims_per_pass:  10_000,
            max_cursors_per_root: 1_000_000,
        },
        content_hash: 0,
    })
}

fn walk_up_for_file(start: &Path, name: &str) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(d) = cur {
        let c = d.join(name);
        if c.is_file() {
            return Some(c);
        }
        cur = d.parent();
    }
    None
}

fn walk_up_for_git_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

/// Resolve a (Reader, Config) for an opened .sprf file.
///
/// Priority:
/// 1. `.sprefa.toml` walking up from the file → `ConfigLocator` + `GitBlobReader`
/// 2. `.git` walking up from the file → auto single-repo `InMemoryLocator` (slug = git-root basename, revs = ["HEAD"]) + `GitBlobReader`
/// 3. Fallback: empty `MemReader` (no repos, no revs — captures stay empty)
fn resolve_reader(uri: &Url) -> (Arc<BufferOverlay>, Arc<Config>, Option<PathBuf>) {
    let file_path = uri.to_file_path().ok();
    let start_dir = file_path
        .as_ref()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf);

    if let Some(d) = &start_dir {
        if let Some(toml) = walk_up_for_file(d, ".sprefa.toml") {
            match ConfigLocator::from_toml_file(&toml) {
                Ok(loc) => {
                    let loc_arc: Arc<dyn CheckoutLocator> = Arc::new(loc);
                    let cfg = make_config_from_locator(&*loc_arc);
                    // First repo's path is the workspace root for hover link rewriting.
                    let workspace_root = loc_arc.repos().first()
                        .and_then(|slug| loc_arc.locate(slug, "HEAD"));
                    let git: Arc<dyn Reader + Send + Sync> =
                        Arc::new(GitBlobReader::new(loc_arc, cfg.clone()));
                    let overlay = Arc::new(BufferOverlay::new(git));
                    eprintln!(
                        "[sprefa-v2-lsp] reader: ConfigLocator from {}",
                        toml.display()
                    );
                    return (overlay, cfg, workspace_root);
                }
                Err(e) => {
                    eprintln!(
                        "[sprefa-v2-lsp] .sprefa.toml at {} failed to parse: {e}; falling back",
                        toml.display()
                    );
                }
            }
        }
    }

    if let Some(d) = &start_dir {
        if let Some(root) = walk_up_for_git_root(d) {
            let slug = root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("repo")
                .to_string();
            let loc = InMemoryLocator::new().with_repo(&slug, root.clone(), &["HEAD"]);
            let loc_arc: Arc<dyn CheckoutLocator> = Arc::new(loc);
            let cfg = make_config_from_locator(&*loc_arc);
            let git: Arc<dyn Reader + Send + Sync> =
                Arc::new(GitBlobReader::new(loc_arc, cfg.clone()));
            let overlay = Arc::new(BufferOverlay::new(git));
            eprintln!(
                "[sprefa-v2-lsp] reader: auto git-root {} (slug={slug}, revs=[HEAD])",
                root.display()
            );
            return (overlay, cfg, Some(root));
        }
    }

    let cfg: Arc<Config> = Arc::new(Config {
        repos:        vec![],
        revs:         vec![],
        fs_exclude:   vec![],
        sprf_files:   vec![],
        shell_allow:  vec![],
        runtime: RuntimeConfig {
            worker_threads:    1,
            buffer_size:       256,
            flush_interval_ms: 100,
            collect_witnesses: true,
            xref_cartesian_limit: 10_000,
            max_passes:           8,
            max_claims_per_pass:  10_000,
            max_cursors_per_root: 1_000_000,
        },
        content_hash: 0,
    });
    let mem: Arc<dyn Reader + Send + Sync> = Arc::new(MemReader::new(cfg.clone()));
    let overlay = Arc::new(BufferOverlay::new(mem));
    eprintln!("[sprefa-v2-lsp] reader: fallback MemReader (no config, no .git)");
    (overlay, cfg, None)
}

/// Rewrite repo-relative `` `path` `` bullets in hover markdown into
/// clickable absolute `file://` links when a workspace root is known.
/// Heuristic: a bullet line `- \`X\`` where `workspace_root/X` exists on disk.
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

fn make_registry() -> Arc<OperatorRegistry> {
    Arc::new(default_registry())
}

// ---------------------------------------------------------------------------
// Position translation — thin wrappers around v2::position (UTF-16 correct).
// ---------------------------------------------------------------------------

fn position_to_offset(text: &str, pos: Position) -> usize {
    v2::position::position_to_offset(text, pos.line, pos.character)
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let (line, character) = v2::position::offset_to_position(text, offset);
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

// ---------------------------------------------------------------------------
// LanguageServer impl
// ---------------------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, p: InitializeParams) -> RpcResult<InitializeResult> {
        // Capture workspace root from LSP client. Prefer `root_uri` (URL),
        // fall back to deprecated `root_path` (already a filesystem path).
        // Both may be absent in single-file mode — leave `None` and let
        // `resolve_reader` auto-detect per-document.
        #[allow(deprecated)]
        let root_path: Option<PathBuf> = p
            .root_uri
            .as_ref()
            .and_then(|u| match u.to_file_path() {
                Ok(pb) => Some(pb),
                Err(()) => {
                    eprintln!("[sprefa-v2-lsp] initialize: root_uri {u} not a file:// path; ignoring");
                    None
                }
            })
            .or_else(|| p.root_path.clone().map(PathBuf::from));
        if let Some(rp) = &root_path {
            eprintln!("[sprefa-v2-lsp] initialize: workspace root = {}", rp.display());
        }
        *self.init_root.lock().await = root_path;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider:      Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions::default()),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name:    "sprefa-v2-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> RpcResult<()> { Ok(()) }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        let uri    = p.text_document.uri.clone();
        let source = p.text_document.text.clone();
        self.open_or_replace(&uri, source).await;
        self.publish(&uri).await;
    }

    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        // FULL sync — last change contains whole doc.
        if let Some(change) = p.content_changes.into_iter().last() {
            self.open_or_replace(&uri, change.text).await;
            self.publish(&uri).await;
        }
    }

    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        let mut guard = self.sessions.lock().await;
        if let Some(entry) = guard.get_mut(&uri) {
            entry.session.ensure_run();
        }
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
                contents: HoverContents::Markup(MarkupContent {
                    kind:  MarkupKind::Markdown,
                    value,
                }),
                range: None,
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
            // When text_edit is set, insert_text is ignored — skip it to avoid
            // client confusion.
            let insert_text = if text_edit.is_some() {
                None
            } else {
                c.insert.map(|s| s.to_string())
            };
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

/// Best-effort mapping from op-authored kind hints (free-form strings) to
/// LSP `CompletionItemKind` icons. Unknown hints fall back to `None`, which
/// clients render as a generic bullet.
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

impl Backend {
    async fn open_or_replace(&self, uri: &Url, source: String) {
        let (overlay, _cfg, auto_root) = resolve_reader(uri);
        // Resolution priority for workspace_root:
        //   1. resolve_reader auto-detection (`.sprefa.toml` first repo, else
        //      walked `.git` root) — closest to the file, most precise.
        //   2. initialize params `root_uri` / `root_path` — LSP client's
        //      workspace folder. Fallback when no git root is reachable.
        let init_root = self.init_root.lock().await.clone();
        let workspace_root = auto_root.or(init_root);
        let root_arc: Option<Arc<std::path::Path>> =
            workspace_root.as_ref().map(|p| Arc::from(p.as_path()));
        let mut guard = self.sessions.lock().await;
        if let Some(entry) = guard.get_mut(uri) {
            entry.session.on_source_change(source.clone());
            entry.session.set_workspace_root(root_arc);
            entry.source = source;
            entry.workspace_root = workspace_root;
            entry.session.ensure_run();
        } else {
            let mut session = DocSession::new(source.clone(), overlay, self.registry.clone());
            session.set_workspace_root(root_arc);
            session.ensure_run();
            guard.insert(uri.clone(), SessionEntry { session, source, workspace_root });
        }
    }

    async fn publish(&self, uri: &Url) {
        let mut guard = self.sessions.lock().await;
        let Some(entry) = guard.get_mut(uri) else { return; };
        // Collect parse diags + run diags, deduplicated by (code, byte range).
        // Run diags can fire per-cursor (e.g. json/no-match across N files × M revs)
        // but LSP users want one marker per source site, with a hit count suffix.
        // When the same parse_site produced a successful match for *some* cursor,
        // demote per-cursor `no-match` / `partial-match` to Hint — the user gets
        // hover-time detail without an Error/Warn squiggle drowning everything.
        type DKey = (String, usize, usize);
        let success_sites = entry.session.successful_sites();
        let mut counts: std::collections::HashMap<DKey, usize> = std::collections::HashMap::new();
        let mut firsts: Vec<(DKey, Severity, String)> = Vec::new();
        for d in entry.session.parse_diagnostics().iter().chain(entry.session.diagnostics().iter()) {
            let site = d.primary();
            let key = (d.code().to_string(), site.byte_range.start, site.byte_range.end);
            let n = counts.entry(key.clone()).or_insert(0);
            *n += 1;
            if *n == 1 {
                let mut cap = CapturingRenderer::default();
                d.render(&mut cap);
                let body = if cap.notes.is_empty() {
                    cap.header_msg
                } else {
                    format!("{}\n\n{}", cap.header_msg, cap.notes.join("\n\n"))
                };
                let sev = if matches!(d.code(), c if c.ends_with("/no-match") || c.ends_with("/partial-match"))
                    && success_sites.contains(&(site.file.clone(), site.byte_range.clone()))
                {
                    Severity::Hint
                } else {
                    d.severity()
                };
                firsts.push((key, sev, body));
            }
        }
        let mut diags: Vec<Diagnostic> = Vec::new();
        for (key, sev, body) in firsts {
            let (code, byte_start, byte_end) = key.clone();
            let n = counts.get(&key).copied().unwrap_or(1);
            let start = offset_to_position(&entry.source, byte_start);
            let end   = offset_to_position(&entry.source, byte_end);
            let msg = if n > 1 { format!("{body} (×{n})") } else { body };
            diags.push(Diagnostic {
                range:    Range { start, end },
                severity: Some(severity_to_lsp(sev)),
                code:     Some(NumberOrString::String(code)),
                source:   Some("sprefa-v2".to_string()),
                message:  msg,
                ..Default::default()
            });
        }
        let uri_owned = uri.clone();
        drop(guard);
        self.client.publish_diagnostics(uri_owned, diags, None).await;
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let registry = make_registry();
    let (service, socket) = LspService::new(|client| Backend {
        client,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        registry,
        init_root: Arc::new(Mutex::new(None)),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
