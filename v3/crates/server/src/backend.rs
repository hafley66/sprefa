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
use pipeline::_0_cursor::{Capture, CaptureKind, Cursor, PathSeg};
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

    // Lower the upstream pipeline once. Keep the lowered focus op
    // alongside so we can ask it for term_positions (capture spans
    // inside opaque paren bodies — json/ast/str).
    let lowered = lower_plan_keeping_ops(&plan.pipe_ops, registry);
    let Some((pipeline, lowered_ops)) = lowered else {
        md.push_str(
            "\n_pipeline lowering failed — one of the ops returned a body diagnostic._\n",
        );
        return Some(md);
    };

    // Resolve the focused capture: hover sits inside a `${NAME?}`
    // recorded in the focus op's `term_positions`.
    let focus_capture: Option<Arc<str>> = lowered_ops
        .get(plan.focus_step)
        .and_then(|op| {
            op.term_positions().iter().find_map(|tp| {
                if tp.range.contains(&plan.hover_offset) || plan.hover_offset == tp.range.end {
                    Some(tp.name.clone())
                } else {
                    None
                }
            })
        });

    if let Some(name) = &focus_capture {
        md.push_str(&format!(
            "\n---\n\n**Focused capture:** `${name}` _(declared by `{}`)_  \n\
             Below: every cursor that binds `${name}`, with the value linking back \
             to its source span.\n",
            plan.focus_name,
        ));
    }

    // Run per seed, aggregate.
    let mut total_rows = 0usize;
    let mut rendered = 0usize;
    md.push_str("\n---\n\n**Cursors after this op:**\n");

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
            md.push_str(&render_cursor_block(rendered + 1, c, &root, focus_capture.as_deref()));
            rendered += 1;
        }
    }

    if total_rows == 0 {
        md.push_str("\n_(no cursors emitted)_\n");
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

/// Lower the planned ops AND keep an `Arc<dyn Op>` view of each so the
/// renderer can call `term_positions()` on the focus op without a
/// second lowering pass.
fn lower_plan_keeping_ops(
    ops:      &[PlannedOp],
    registry: &pipeline::registry::Registry,
) -> Option<(Pipeline, Vec<Arc<dyn pipeline::Op>>)> {
    use sprefa_parse::host_parse;
    use std::path::PathBuf;
    use std::sync::Arc;

    let mut seq: Vec<Pipeline> = Vec::with_capacity(ops.len());
    let mut shared: Vec<Arc<dyn pipeline::Op>> = Vec::with_capacity(ops.len());
    for op in ops {
        // Re-lower the op's paren body through the host parser + paren-slot
        // lowerer. We synthesize `name(body)` here, so paren positions
        // inside the synth source don't match the host source. The
        // render path uses `op.paren_origin` from the host source for
        // the term_positions comparison, and term_positions already
        // came in absolute from the original lowering — wait: this
        // synth path re-lowers from a fake source, which produces
        // term_positions absolute into the synth. Re-base them to host
        // by subtracting (synth paren origin) and adding `paren_origin`.
        let synth = format!("{}({})", op.name, op.body);
        let synth_paren_origin = op.name.len() + 1; // after "name("
        let file: Arc<std::path::Path> = Arc::from(PathBuf::from("<hover>").as_path());
        let (parsed, errs) = host_parse(&synth, file);
        if !errs.is_empty() {
            return None;
        }
        let inv = parsed.pipes.first()?.ops.first()?;
        let mut diags = Vec::new();
        let built = registry.build_from_node(&op.name, inv.node(), synth.as_bytes(), &mut diags)?;
        let op_box = built.ok()?;
        let arc: Arc<dyn pipeline::Op> = Arc::from(op_box);
        // Wrap with a re-based view so callers see term_positions in
        // host coordinates. For ops whose term_positions is empty this
        // is a no-op.
        let rebased: Arc<dyn pipeline::Op> = if arc.term_positions().is_empty() {
            arc.clone()
        } else {
            Arc::new(RebasedOp {
                inner:        arc.clone(),
                synth_origin: synth_paren_origin,
                host_origin:  op.paren_origin,
                cached_positions: arc
                    .term_positions()
                    .iter()
                    .map(|tp| pipeline::TermPosition {
                        name:  tp.name.clone(),
                        range: (op.paren_origin + (tp.range.start - synth_paren_origin))
                              ..(op.paren_origin + (tp.range.end   - synth_paren_origin)),
                    })
                    .collect(),
            })
        };
        shared.push(rebased);
        // Pipeline::Op holds Box<dyn Op>; reuse the lowered op via a
        // second build call to get a fresh Box. Cheaper alternative
        // would be returning Arc directly from the registry, but that
        // is a wider change.
        let mut diags2 = Vec::new();
        let built2 = registry.build_from_node(&op.name, inv.node(), synth.as_bytes(), &mut diags2)?;
        let op_box2 = built2.ok()?;
        seq.push(Pipeline::Op(op_box2));
    }
    Some((Pipeline::Seq(seq), shared))
}

/// Wrapper that exposes an Op's `term_positions()` re-based from
/// synth-source coordinates to host-source coordinates. All other
/// trait methods delegate.
#[derive(Debug)]
struct RebasedOp {
    inner:            Arc<dyn pipeline::Op>,
    #[allow(dead_code)]
    synth_origin:     usize,
    #[allow(dead_code)]
    host_origin:      usize,
    cached_positions: Vec<pipeline::TermPosition>,
}

impl pipeline::Op for RebasedOp {
    fn name(&self) -> &'static str { self.inner.name() }
    fn pipe<'a>(
        &'a self,
        ctx: &'a effect_runtime::RtCtx,
        c:   pipeline::Cursor,
    ) -> effect_runtime::BoxFuture<'a, Vec<pipeline::Cursor>> {
        self.inner.pipe(ctx, c)
    }
    fn bound_captures(&self) -> &[Arc<str>] { self.inner.bound_captures() }
    fn term_positions(&self) -> &[pipeline::TermPosition] { &self.cached_positions }
}

/// Compact per-cursor block: header line with file link + repo@rev +
/// byte range, optional match preview as blockquote, captures as a
/// bullet list with each value linking to its source span.
///
/// `focus_capture` (when set) marks the bullet for that capture name
/// with a `→` indicator and bolds the value link, so the user's eye
/// lands on the right line per row.
fn render_cursor_block(n: usize, c: &Cursor, root: &Path, focus_capture: Option<&str>) -> String {
    let mut out = String::new();

    // Header line. File link sits first because it's the thing the user
    // actually clicks; metadata trails.
    let header_anchor = match &c.fs {
        Some(rel) => {
            let abs = root.join(rel.as_ref());
            let line = line_of_offset(&c.content, c.byte_range.start);
            format!(
                "[`{}#L{line}`](file://{}#L{line})",
                rel.to_string_lossy(),
                abs.display(),
            )
        }
        None => "_(no file)_".into(),
    };
    out.push_str(&format!(
        "\n**{n}.** {header_anchor} · `{}@{}` · `bytes {}..{}`\n",
        c.repo, c.rev, c.byte_range.start, c.byte_range.end,
    ));

    // Match preview. Blockquote keeps it visually separate from the
    // capture list and avoids the table-cell escape gymnastics.
    let preview = render_preview(c);
    if !preview.is_empty() {
        out.push_str(&format!("> `{preview}`\n"));
    }

    // Capture list. SpanBacked → linked value pointing at its byte_range
    // line; Synthesized → inline value, no link (no source span).
    if !c.captures.is_empty() {
        for cap in &c.captures {
            let cell = render_capture_line(c, cap, root);
            let is_focus = focus_capture.is_some_and(|f| f == &*cap.name);
            let marker = if is_focus { "▶" } else { "-" };
            if is_focus {
                out.push_str(&format!("{marker} **{cell}**\n"));
            } else {
                out.push_str(&format!("{marker} {cell}\n"));
            }
        }
    }

    // last_bound and SprfPath are diagnostic; only render when present.
    if let Some(name) = c.last_bound.as_ref() {
        out.push_str(&format!("- _last_bound:_ `${name}`\n"));
    }
    if !c.path.is_empty() {
        let path = c
            .path
            .iter()
            .map(|s| match s {
                PathSeg::Op { name, step } => format!("{name}#{step}"),
                PathSeg::ForkArm { index } => format!("arm{index}"),
            })
            .collect::<Vec<_>>()
            .join(" > ");
        out.push_str(&format!("- _SprfPath:_ `{path}`\n"));
    }

    out
}

fn render_preview(c: &Cursor) -> String {
    let active = c.active();
    if active.is_empty() { return String::new(); }
    let s = String::from_utf8_lossy(active);
    let s = s.replace('\n', " ⏎ ");
    let s = s.replace('`', "\\`");
    truncate(&s, 80)
}

fn render_capture_line(c: &Cursor, cap: &Capture, root: &Path) -> String {
    let value_str = match &cap.kind {
        CaptureKind::Synthesized { value } => value.to_string(),
        CaptureKind::SpanBacked => {
            String::from_utf8_lossy(&c.content[cap.byte_range.clone()]).into_owned()
        }
    };
    let value_short = truncate(&value_str.replace('`', "\\`"), 48);

    match &cap.kind {
        CaptureKind::SpanBacked => {
            // Link the value text into the source file at the capture's
            // start line. file:// + #Lnn lets editors jump straight to it.
            let target = match &c.fs {
                Some(rel) => {
                    let abs = root.join(rel.as_ref());
                    let line = line_of_offset(&c.content, cap.byte_range.start);
                    Some(format!("file://{}#L{line}", abs.display()))
                }
                None => None,
            };
            match target {
                Some(href) => format!(
                    "`${}` → [`{}`]({href}) `bytes {}..{}`",
                    cap.name, value_short, cap.byte_range.start, cap.byte_range.end,
                ),
                None => format!(
                    "`${}` → `{}` `bytes {}..{}`",
                    cap.name, value_short, cap.byte_range.start, cap.byte_range.end,
                ),
            }
        }
        CaptureKind::Synthesized { .. } => {
            format!("`${}` → `{}` _(synthesized)_", cap.name, value_short)
        }
    }
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
    fn cursor_block_renders_capture_links_to_file_lines() {
        // Synthetic: 3-line JSON, capture spans line 2 bytes 7..18.
        let body = b"{\n  \"name\": \"alice\"\n}\n";
        let mut c = Cursor::new(Arc::from(body.as_slice()));
        c.repo = Arc::from("repo-x");
        c.rev  = Arc::from("HEAD");
        c.fs   = Some(Arc::from(std::path::PathBuf::from("Cargo.toml").as_path()));
        c.byte_range = 0..body.len();
        let name_pos = body.windows(7).position(|w| w == b"\"alice\"").unwrap();
        c.captures.push(Capture::span_backed(
            Arc::from("N"),
            name_pos..(name_pos + 7),
        ));

        let root = std::path::PathBuf::from("/tmp/root");
        let md = render_cursor_block(1, &c, &root, None);

        assert!(md.contains("[`Cargo.toml#L1`]"), "header file link present: {md}");
        assert!(md.contains("(file:///tmp/root/Cargo.toml#L"), "absolute file:// URL: {md}");
        assert!(
            md.contains("`$N` → [`\"alice\"`]"),
            "capture value rendered as a link: {md}"
        );
        assert!(
            md.contains(&format!("#L2)")),
            "capture link points at line 2 of the source: {md}"
        );
    }

    #[test]
    fn json_op_exposes_term_positions_for_brace_captures() {
        // The LSP hover dispatcher relies on Op::term_positions() to
        // light up captures inside opaque paren bodies. Build a JsonOp
        // through the registry exactly the way render_enriched_hover
        // does, and assert that ${N?} / ${V?} land at their host-source
        // byte ranges.
        let src = "json({ name: ${N?}, version: ${V?} })";
        let file: Arc<std::path::Path> =
            Arc::from(std::path::PathBuf::from("t.sprf").as_path());
        let (parsed, errs) = sprefa_parse::host_parse(src, file);
        assert!(errs.is_empty(), "host parse errors: {errs:?}");

        let inv = parsed.pipes.first().unwrap().ops.first().unwrap();
        let registry = Registry::with_stdlib();
        let mut diags = Vec::new();
        let built = registry
            .build_from_node("json", inv.node(), src.as_bytes(), &mut diags)
            .expect("registry knows json");
        let op = built.expect("json lowered cleanly");

        let positions: Vec<_> = op
            .term_positions()
            .iter()
            .map(|tp| (tp.name.to_string(), tp.range.clone()))
            .collect();
        assert_eq!(
            positions.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            vec!["N", "V"],
            "two captures recorded in source order: {positions:?}",
        );
        // Each range must round-trip back to the same `${...}` token in src.
        for (name, r) in &positions {
            let token = &src[r.clone()];
            assert!(
                token.starts_with("${") && token.ends_with('}'),
                "term_position[{name}] = {r:?} → {token:?}",
            );
        }
    }

    #[test]
    fn cursor_block_renders_synthesized_capture_inline() {
        let mut c = Cursor::new(Arc::from(&b""[..]));
        c.captures.push(Capture::synthesized(Arc::from("ARGS"), Arc::from("a , b")));
        let md = render_cursor_block(1, &c, std::path::Path::new("/"), None);
        assert!(md.contains("`$ARGS` → `a , b` _(synthesized)_"), "synthesized capture: {md}");
    }

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
