//! tower-lsp `LanguageServer` impl for v3.
//!
//! Per-URI `DocSession` map behind a mutex. did_open/did_change/did_save
//! recompute and publish diagnostics. Hover and completion are stubbed
//! to `None` until the DocSession grows past parse-layer analysis.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pipeline::binding_graph::BindingDiagnostic;
use pipeline::registry::Registry;
use sprefa_parse::{ParseError, ParseErrorKind};
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::config::Config;
use crate::position::{offset_to_position, position_to_offset};
use crate::session::{ConfigSource, DocSession, HoverPlan, PlannedOp, SuggestionKind};

use effect_runtime::RtCtxBuilder;
use pipeline::_0_cursor::{CaptureKind, Cursor, PathSeg};
use pipeline::_2_pipeline::Pipeline;
use pipeline::effects::{
    FsListFilesBatcher, FsListFilesEffect, PrintBatcher, PrintEffect,
    ReadBytesBatcher, ReadBytesEffect,
};
use pipeline::readers::FileSource;

pub struct Backend {
    client: Client,
    sessions: Arc<Mutex<HashMap<Url, DocSession>>>,
    registry: Registry,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            registry: Registry::with_stdlib(),
        }
    }

    async fn open_or_replace(&self, uri: &Url, source: String) {
        let file = uri_to_path(uri);
        let file: Arc<std::path::Path> = Arc::from(file.as_path());
        let mut guard = self.sessions.lock().await;
        match guard.get_mut(uri) {
            Some(entry) => entry.on_source_change(source),
            None => {
                guard.insert(
                    uri.clone(),
                    DocSession::new(file, source, self.registry.clone()),
                );
            }
        }
    }

    async fn publish(&self, uri: &Url) {
        let diags = {
            let guard = self.sessions.lock().await;
            let Some(entry) = guard.get(uri) else { return; };
            let mut diags = parse_errors_to_lsp(entry.source(), entry.parse_errors());
            diags.extend(binding_diags_to_lsp(
                entry.source(),
                entry.binding_diagnostics(),
            ));
            diags
        };
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _p: InitializeParams) -> RpcResult<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(
                        ["$", ">", "(", " "].iter().map(|s| s.to_string()).collect(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "sprefa-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {}
    async fn shutdown(&self) -> RpcResult<()> { Ok(()) }

    async fn did_open(&self, p: DidOpenTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        self.open_or_replace(&uri, p.text_document.text).await;
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
        let uri = p.text_document.uri;
        self.publish(&uri).await;
    }

    async fn hover(&self, p: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = p.text_document_position_params.text_document.uri;
        let pos = p.text_document_position_params.position;

        // Snapshot everything we need out of the mutex before running
        // the pipeline so an expensive hover does not block
        // did_change / did_save on other docs.
        let snapshot = {
            let guard = self.sessions.lock().await;
            let Some(session) = guard.get(&uri) else { return Ok(None) };
            let offset = position_to_offset(session.source(), pos.line, pos.character);
            let plan = session.hover_plan(offset);
            let static_doc = session.hover_at(offset);
            Some(HoverSnapshot {
                config: session.config().clone(),
                config_source: session.config_source().clone(),
                registry: session.registry().clone(),
                plan,
                static_doc,
            })
        };
        let Some(snap) = snapshot else { return Ok(None) };

        let body = match snap.plan {
            Some(plan) => {
                render_enriched_hover(&plan, &snap.config, &snap.config_source, &snap.registry).await
            }
            None => snap.static_doc,
        };
        let Some(value) = body else { return Ok(None) };

        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        }))
    }

    async fn completion(
        &self,
        p: CompletionParams,
    ) -> RpcResult<Option<CompletionResponse>> {
        let uri = p.text_document_position.text_document.uri;
        let pos = p.text_document_position.position;
        let guard = self.sessions.lock().await;
        let Some(session) = guard.get(&uri) else { return Ok(None) };
        let offset = position_to_offset(session.source(), pos.line, pos.character);
        let items: Vec<CompletionItem> = session
            .completions_at(offset)
            .into_iter()
            .map(|s| CompletionItem {
                label: s.label,
                detail: s.detail,
                documentation: s.documentation.map(|doc| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: doc,
                    })
                }),
                kind: Some(match s.kind {
                    SuggestionKind::Op => CompletionItemKind::FUNCTION,
                    SuggestionKind::Capture => CompletionItemKind::VARIABLE,
                }),
                ..Default::default()
            })
            .collect();
        if items.is_empty() {
            return Ok(None);
        }
        Ok(Some(CompletionResponse::Array(items)))
    }
}

struct HoverSnapshot {
    config: Config,
    config_source: ConfigSource,
    registry: pipeline::registry::Registry,
    plan: Option<HoverPlan>,
    static_doc: Option<String>,
}

/// Row cap to keep hover responses snappy under large listings.
const HOVER_ROW_CAP: usize = 20;

async fn render_enriched_hover(
    plan: &HoverPlan,
    config: &Config,
    config_source: &ConfigSource,
    registry: &pipeline::registry::Registry,
) -> Option<String> {
    let mut md = String::new();

    // Op doc at the top.
    if let Some(doc) = registry.doc(&plan.focus_name) {
        md.push_str(doc);
        md.push_str("\n\n---\n\n");
    }

    md.push_str(&format!(
        "**Pipe position:** step {} / {}  \n",
        plan.focus_step + 1,
        plan.pipe_len
    ));
    md.push_str(&format!(
        "**Body grammar:** {}  \n",
        registry.body_grammar(&plan.focus_name).unwrap_or("(see op doc)")
    ));

    // Config banner.
    md.push_str("**Config:** ");
    let cfg_line = match config_source {
        ConfigSource::File(p) => {
            format!("[`{}`](file://{})", p.display(), p.display())
        }
        ConfigSource::CwdFallback => "cwd fallback (no `.sprefa.toml` found)".into(),
    };
    md.push_str(&cfg_line);
    md.push_str("\n\n| slug | rev | root |\n| --- | --- | --- |\n");
    for s in &config.seeds {
        let root_display = s.root.display().to_string();
        let root_link = if s.root.is_absolute() {
            format!("[`{}`](file://{})", root_display, root_display)
        } else if let Ok(abs) = s.root.canonicalize() {
            format!(
                "[`{}`](file://{})",
                root_display,
                abs.display()
            )
        } else {
            format!("`{}`", root_display)
        };
        md.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            s.slug, s.rev, root_link
        ));
    }

    // Lower the upstream pipeline once.
    let Some(pipeline) = lower_plan(&plan.pipe_ops, registry) else {
        md.push_str(
            "\n_pipeline lowering failed — one of the ops returned a body diagnostic._\n",
        );
        return Some(md);
    };

    // Run per seed, aggregate.
    let mut total_rows = 0usize;
    let mut rendered = 0usize;
    md.push_str("\n**Cursors after this op:**\n\n");
    md.push_str("| # | repo@rev | fs | byte_range | match | captures | last_bound | SprfPath |\n");
    md.push_str("| --- | --- | --- | --- | --- | --- | --- | --- |\n");

    let mut seed_roots: std::collections::HashMap<Arc<str>, PathBuf> =
        std::collections::HashMap::new();
    for seed in &config.seeds {
        seed_roots.insert(Arc::from(seed.slug.as_str()), seed.root.clone());
        let source: Arc<dyn FileSource> =
            Arc::new(DiskFileSource::new(seed.root.clone(), seed.rev.clone()));
        let ctx = RtCtxBuilder::new()
            .register_pure::<FsListFilesEffect, _>(
                256,
                FsListFilesBatcher::new(source.clone()),
            )
            .register_pure::<ReadBytesEffect, _>(
                256,
                ReadBytesBatcher::new(source),
            )
            .register::<PrintEffect, _>(PrintBatcher::buffer().0)
            .build();

        let mut c = Cursor::default();
        c.repo = Arc::from(seed.slug.as_str());
        c.rev = Arc::from(seed.rev.as_str());
        let rows = pipeline.run(&ctx, vec![c]).await;
        let root = seed.root.clone();
        for c in &rows {
            total_rows += 1;
            if rendered >= HOVER_ROW_CAP { continue; }
            md.push_str(&render_cursor_row(rendered + 1, c, &root));
            rendered += 1;
        }
    }

    if total_rows == 0 {
        md.push_str("| _(no cursors emitted)_ |  |  |  |  |  |  |  |\n");
    }
    md.push_str(&format!(
        "\n**Total rows:** {}{}\n",
        total_rows,
        if total_rows > HOVER_ROW_CAP {
            format!(" _(showing first {HOVER_ROW_CAP})_")
        } else {
            String::new()
        },
    ));

    Some(md)
}

fn lower_plan(ops: &[PlannedOp], registry: &pipeline::registry::Registry) -> Option<Pipeline> {
    use sprefa_parse::host_parse;
    use pipeline::registry::lower_paren_slot;
    use std::path::PathBuf;
    use std::sync::Arc;

    let mut seq: Vec<Pipeline> = Vec::with_capacity(ops.len());
    for op in ops {
        // Re-lower the op's paren body through the host parser + paren-slot
        // lowerer so the factory receives a Vec<Value>. This mirrors how
        // the main lower.rs path would handle this invocation if it were
        // inline in a .sprf source.
        let synth = format!("{}({})", op.name, op.body);
        let file: Arc<std::path::Path> = Arc::from(PathBuf::from("<hover>").as_path());
        let (parsed, errs) = host_parse(&synth, file);
        if !errs.is_empty() {
            return None;
        }
        let inv = parsed.pipes.first()?.ops.first()?;
        let paren = inv.node().child_by_field_name("paren");
        let values = match paren {
            Some(p) => {
                let mut diags = Vec::new();
                lower_paren_slot(p, synth.as_bytes(), registry, &mut diags)
            }
            None => Vec::new(),
        };
        let built = registry.build(&op.name, values)?;
        let op_box = built.ok()?;
        seq.push(Pipeline::Op(op_box));
    }
    Some(Pipeline::Seq(seq))
}

fn render_cursor_row(n: usize, c: &Cursor, root: &Path) -> String {
    let fs_cell = match &c.fs {
        Some(rel) => {
            let rel_str = rel.to_string_lossy().into_owned();
            let abs = root.join(rel.as_ref());
            let line = line_of_offset(&c.content, c.byte_range.start);
            let link = format!("file://{}#L{}", abs.display(), line);
            format!("[`{}`]({})", escape_pipe(&rel_str), link)
        }
        None => "-".into(),
    };
    let caps = c
        .captures
        .iter()
        .map(|cap| {
            let val = match &cap.kind {
                CaptureKind::Synthesized { value } => value.to_string(),
                CaptureKind::SpanBacked => {
                    String::from_utf8_lossy(&c.content[cap.byte_range.clone()]).into_owned()
                }
            };
            format!("`{}`=`{}`", cap.name, truncate(&val, 24))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let path = c
        .path
        .iter()
        .map(|s| match s {
            PathSeg::Op { name, step } => format!("{name}#{step}"),
            PathSeg::ForkArm { index } => format!("arm{index}"),
        })
        .collect::<Vec<_>>()
        .join(" > ");
    let last_bound = c
        .last_bound
        .as_ref()
        .map(|s| format!("`${s}`"))
        .unwrap_or_else(|| "-".into());
    let match_cell = render_match_cell(c);
    format!(
        "| {} | `{}@{}` | {} | `{}..{}` | {} | {} | {} | `{}` |\n",
        n,
        c.repo,
        c.rev,
        fs_cell,
        c.byte_range.start,
        c.byte_range.end,
        match_cell,
        if caps.is_empty() { "-".into() } else { caps },
        last_bound,
        path,
    )
}

fn render_match_cell(c: &Cursor) -> String {
    let active = c.active();
    if active.is_empty() {
        return "-".into();
    }
    let preview = String::from_utf8_lossy(active);
    let preview = preview.replace('\n', " ⏎ ");
    let preview = escape_pipe(&preview);
    let preview = preview.replace('`', "\\`");
    format!("`{}`", truncate(&preview, 80))
}

fn line_of_offset(bytes: &[u8], offset: usize) -> usize {
    let end = offset.min(bytes.len());
    1 + bytes[..end].iter().filter(|b| **b == b'\n').count()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

fn escape_pipe(s: &str) -> String { s.replace('|', "\\|") }

struct DiskFileSource {
    root: PathBuf,
    rev: String,
}

impl DiskFileSource {
    fn new(root: PathBuf, rev: String) -> Self { Self { root, rev } }
}

impl FileSource for DiskFileSource {
    fn files(&self, _repo: &str, rev: &str) -> Vec<Arc<Path>> {
        if rev != self.rev { return Vec::new(); }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out
    }

    fn file_bytes(&self, _repo: &str, rev: &str, path: &Path) -> Option<Arc<[u8]>> {
        if rev != self.rev { return None; }
        std::fs::read(self.root.join(path)).ok().map(Arc::from)
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Arc<Path>>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(root, &path, out);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(Arc::from(rel));
            }
        }
    }
}

fn uri_to_path(uri: &Url) -> PathBuf {
    uri.to_file_path()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn parse_errors_to_lsp(source: &str, errors: &[ParseError]) -> Vec<Diagnostic> {
    errors
        .iter()
        .map(|e| {
            let (sl, sc) = offset_to_position(source, e.byte_range.start);
            let (el, ec) = offset_to_position(source, e.byte_range.end);
            let (code, message) = match &e.kind {
                ParseErrorKind::SyntaxError => (
                    "parse/syntax".to_string(),
                    e.message.to_string(),
                ),
                ParseErrorKind::Missing { expected } => (
                    "parse/missing".to_string(),
                    format!("missing `{expected}`"),
                ),
            };
            Diagnostic {
                range: Range {
                    start: Position { line: sl, character: sc },
                    end: Position { line: el, character: ec },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: Some(NumberOrString::String(code)),
                source: Some("sprefa".into()),
                message,
                ..Default::default()
            }
        })
        .collect()
}

fn binding_diags_to_lsp(source: &str, diags: &[BindingDiagnostic]) -> Vec<Diagnostic> {
    diags
        .iter()
        .map(|d| {
            let (sl, sc) = offset_to_position(source, d.byte_range.start);
            let (el, ec) = offset_to_position(source, d.byte_range.end);
            Diagnostic {
                range: Range {
                    start: Position { line: sl, character: sc },
                    end: Position { line: el, character: ec },
                },
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String(d.code.to_string())),
                source: Some("sprefa".into()),
                message: d.message.clone(),
                ..Default::default()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_errors_convert_with_utf16_ranges() {
        let src = "foo > > bar";
        let (_p, errs) = sprefa_parse::host_parse(
            src,
            Arc::from(std::path::PathBuf::from("t.sprf").as_path()),
        );
        assert!(!errs.is_empty());
        let diags = parse_errors_to_lsp(src, &errs);
        assert!(!diags.is_empty());
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert!(d.code.is_some());
        assert_eq!(d.source.as_deref(), Some("sprefa"));
    }

    #[test]
    fn binding_diags_render_as_lsp_warnings() {
        use crate::session::DocSession;
        let file: Arc<std::path::Path> =
            Arc::from(std::path::PathBuf::from("t.sprf").as_path());
        let session = DocSession::new(
            file,
            "rev(${X})".into(),
            Registry::with_stdlib(),
        );
        let diags = binding_diags_to_lsp(session.source(), session.binding_diagnostics());
        assert_eq!(diags.len(), 1, "expected one term/unbound diag");
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(
            d.code,
            Some(NumberOrString::String("term/unbound".to_string())),
        );
        assert_eq!(d.source.as_deref(), Some("sprefa"));
    }
}
