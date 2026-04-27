//! tower-lsp `LanguageServer` impl for v3.
//!
//! Per-URI `DocSession` map behind a mutex. did_open/did_change/did_save
//! recompute and publish diagnostics. Hover and completion are stubbed
//! to `None` until the DocSession grows past parse-layer analysis.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::stream::{self, StreamExt};
use pipeline::binding_graph::BindingDiagnostic;
use pipeline::registry::Registry;
use sprefa_parse::{ParseError, ParseErrorKind};
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::config::Config;
use crate::position::{offset_to_position, position_to_offset};
use crate::session::{ConfigSource, DocSession, HoverPlan, LoweredOp, SuggestionKind};

use effect_runtime::batchers::BoundedWorkSteal;
use effect_runtime::{RtCtxBuilder, SubjectRegistry, Yield, YieldBatcher};
use pipeline::_0_cursor::{Capture, CaptureKind, Cursor, PathSeg};
use pipeline::_2_pipeline::Pipeline;
use pipeline::effects::{
    ast_parse, AstParseEffect, FsListFilesBatcher, FsListFilesEffect, PrintBatcher,
    PrintEffect, ReadBytesBatchBatcher, ReadBytesBatchEffect, ReadBytesBatcher,
    ReadBytesEffect,
};
use pipeline::relation_store::{RelationStore, RelationWake, WriteBatcher, WriteEffect};
use pipeline::readers::{DiskFileSource, FileSource};

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
            diags.extend(lower_diags_to_lsp(entry.source(), entry.lowered_pipes()));
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
        let _hover_t0 = std::time::Instant::now();
        tracing::info!(
            target: "sprefa::lsp",
            uri = %uri,
            line = pos.line,
            character = pos.character,
            "hover.req"
        );

        // Snapshot everything we need out of the mutex before running
        // the pipeline so an expensive hover does not block
        // did_change / did_save on other docs.
        let snapshot = {
            let guard = self.sessions.lock().await;
            let Some(session) = guard.get(&uri) else { return Ok(None) };
            let offset = position_to_offset(session.source(), pos.line, pos.character);
            let plan = session.hover_plan(offset);
            let static_doc = session.hover_at(offset);
            // Trigger the session's one-shot lowering inside the lock,
            // then clone out only the slice this hover needs (cheap —
            // each entry is an Arc<dyn Op> + small Vec of diagnostics).
            let lowered_slice: Option<Vec<LoweredOp>> = plan.as_ref().map(|p| {
                session.lowered_pipes()[p.pipe_idx][..=p.focus_step].to_vec()
            });
            Some(HoverSnapshot {
                config: session.config().clone(),
                config_source: session.config_source().clone(),
                registry: session.registry().clone(),
                plan,
                lowered_slice,
                static_doc,
            })
        };
        let Some(snap) = snapshot else { return Ok(None) };

        let body = match (snap.plan, snap.lowered_slice) {
            (Some(plan), Some(slice)) => {
                render_enriched_hover(&plan, &slice, &snap.config, &snap.config_source, &snap.registry).await
            }
            _ => snap.static_doc,
        };
        let Some(value) = body else {
            tracing::info!(
                target: "sprefa::lsp",
                elapsed_ms = _hover_t0.elapsed().as_millis() as u64,
                "hover.done.empty"
            );
            return Ok(None)
        };

        tracing::info!(
            target: "sprefa::lsp",
            elapsed_ms = _hover_t0.elapsed().as_millis() as u64,
            md_bytes = value.len(),
            "hover.done"
        );
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
    lowered_slice: Option<Vec<LoweredOp>>,
    static_doc: Option<String>,
}

/// Row cap to keep hover responses snappy under large listings.
const HOVER_ROW_CAP: usize = 20;

async fn render_enriched_hover(
    plan: &HoverPlan,
    lowered: &[LoweredOp],
    config: &Config,
    config_source: &ConfigSource,
    registry: &pipeline::registry::Registry,
) -> Option<String> {
    let _t0 = std::time::Instant::now();
    tracing::info!(
        target: "sprefa::hover",
        focus_op = %plan.focus_name,
        focus_step = plan.focus_step,
        pipe_len = plan.pipe_len,
        seeds = config.seeds.len(),
        "render_enriched_hover.start"
    );
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

    // The session already lowered every op once via the same call CLI
    // makes (`registry.build_from_node` over the host CST node). Bail
    // out only if any upstream slot still has no `Op` after that single
    // pathway — and surface the actual diagnostics instead of a generic
    // failure line.
    let mut pipeline_arcs: Vec<Arc<dyn pipeline::Op>> = Vec::with_capacity(lowered.len());
    for slot in lowered {
        let Some(op) = slot.op.clone() else {
            md.push_str("\n_pipeline lowering failed:_\n");
            for d in &slot.diagnostics {
                md.push_str(&format!("- `{}`: {}\n", d.code, d.message));
            }
            return Some(md);
        };
        pipeline_arcs.push(op);
    }
    let pipeline = Pipeline::Seq(
        pipeline_arcs.iter().cloned().map(Pipeline::Op).collect(),
    );

    // Resolve the focused capture: hover sits inside a `${NAME?}`
    // recorded in the focus op's `term_positions`. Coordinates are
    // host-relative because lowering ran against the original CST.
    let focus_capture: Option<Arc<str>> = pipeline_arcs
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
        let _seed_t0 = std::time::Instant::now();
        tracing::info!(
            target: "sprefa::hover",
            seed = %seed.slug,
            rev = %seed.rev,
            root = %seed.root.display(),
            "seed.start"
        );
        seed_roots.insert(Arc::from(seed.slug.as_str()), seed.root.clone());
        let source: Arc<dyn FileSource> =
            Arc::new(DiskFileSource::new(seed.root.clone(), seed.rev.clone()));
        let relation_store = Arc::new(RelationStore::new());
        let registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let op_cache = Arc::new(pipeline::cache_key::OpCache::new(true));
        let ctx = RtCtxBuilder::new()
            .with_store(relation_store.clone())
            .with_store(registry.clone())
            .with_store(op_cache.clone())
            .register_pure::<FsListFilesEffect, _>(
                256,
                FsListFilesBatcher::new(source.clone()),
            )
            .register_pure::<ReadBytesEffect, _>(
                256,
                ReadBytesBatcher::new(source.clone()),
            )
            .register::<ReadBytesBatchEffect, _>(
                ReadBytesBatchBatcher::new(source),
            )
            .register::<AstParseEffect, _>(
                BoundedWorkSteal::<AstParseEffect>::new(256, 8, ast_parse),
            )
            .register::<PrintEffect, _>(PrintBatcher::buffer().0)
            .register::<Yield<RelationWake>, _>(YieldBatcher::new(registry.clone()))
            .register::<WriteEffect, _>(
                WriteBatcher::new(relation_store, registry.clone()),
            )
            .build();

        let mut c = Cursor::default();
        c.repo = Arc::from(seed.slug.as_str());
        c.rev = Arc::from(seed.rev.as_str());
        let upstream: futures::stream::BoxStream<'_, Arc<[Cursor]>> = Box::pin(
            stream::iter(vec![Arc::<[Cursor]>::from(vec![c])]),
        );
        let _drain_t0 = std::time::Instant::now();
        let mut s = pipeline.run(&ctx, upstream);
        let mut rows: Vec<Cursor> = Vec::new();
        let mut batch_count = 0usize;
        while let Some(b) = s.next().await {
            batch_count += 1;
            rows.extend(b.iter().cloned());
        }
        let summary = ctx.telemetry().summary();
        tracing::info!(
            target: "sprefa::hover",
            seed = %seed.slug,
            batches = batch_count,
            rows = rows.len(),
            drain_ms = _drain_t0.elapsed().as_millis() as u64,
            seed_total_ms = _seed_t0.elapsed().as_millis() as u64,
            "seed.done"
        );
        tracing::info!(
            target: "sprefa::hover",
            "seed.effects:\n{}", summary
        );
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

    tracing::info!(
        target: "sprefa::hover",
        total_rows,
        rendered,
        elapsed_ms = _t0.elapsed().as_millis() as u64,
        "render_enriched_hover.done"
    );
    Some(md)
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
                ParseErrorKind::RecursionForbidden { rule } => (
                    "parse/recursion-forbidden".to_string(),
                    format!("rule `{rule}` calls itself; self-recursion is forbidden"),
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

/// Surface op-body lowering failures (the third diag channel produced
/// by `registry.build_from_node`) as LSP diagnostics. Severity ERROR
/// because a slot with `op = None` collapses the whole pipe — the
/// pipeline cannot run past it. Byte ranges from the registry are
/// already source-absolute (tree-sitter `Node::byte_range`).
fn lower_diags_to_lsp(source: &str, lowered: &[Vec<LoweredOp>]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for pipe in lowered {
        for slot in pipe {
            for d in &slot.diagnostics {
                let (sl, sc) = offset_to_position(source, d.byte_range.start);
                let (el, ec) = offset_to_position(source, d.byte_range.end);
                out.push(Diagnostic {
                    range: Range {
                        start: Position { line: sl, character: sc },
                        end:   Position { line: el, character: ec },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String(d.code.to_string())),
                    source: Some("sprefa".into()),
                    message: d.message.clone(),
                    ..Default::default()
                });
            }
        }
    }
    out
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
    fn ast_and_ast_yaml_parity_term_positions_and_captures() {
        // Parity contract: hovering on `${X?}` inside an `ast[rs](...)`
        // body and inside an `ast_yaml[rs](...)` body must both surface
        // the X capture via Op::term_positions() and Op::bound_captures().
        // If this drifts, ast_yaml hover stops lighting up metavars.
        let registry = Registry::with_stdlib();
        let file: Arc<std::path::Path> =
            Arc::from(std::path::PathBuf::from("t.sprf").as_path());

        for (name, src) in &[
            ("ast",      "ast[rs](${X?}.to_string())"),
            ("ast_yaml", "ast_yaml[rs](pattern: \"${X?}.to_string()\")"),
        ] {
            let (parsed, errs) = sprefa_parse::host_parse(src, file.clone());
            assert!(errs.is_empty(), "[{name}] host parse errors: {errs:?}");
            let inv = parsed.pipes.first().unwrap().ops.first().unwrap();
            let mut diags = Vec::new();
            let built = registry
                .build_from_node(name, inv.node(), src.as_bytes(), &mut diags)
                .unwrap_or_else(|| panic!("[{name}] registry has no factory"));
            let op = built
                .unwrap_or_else(|e| panic!("[{name}] lower failed: {e:?}"));

            let cap_names: Vec<&str> = op
                .bound_captures()
                .iter()
                .map(|s| s.as_ref())
                .collect();
            assert!(
                cap_names.contains(&"X"),
                "[{name}] bound_captures must include X: {cap_names:?}",
            );

            let positions: Vec<_> = op
                .term_positions()
                .iter()
                .map(|tp| (tp.name.to_string(), tp.range.clone()))
                .collect();
            assert!(
                !positions.is_empty(),
                "[{name}] term_positions must be non-empty so hover can find ${{X?}}",
            );
            let xs: Vec<_> = positions
                .iter()
                .filter(|(n, _)| n == "X")
                .collect();
            assert!(!xs.is_empty(), "[{name}] no term_position for X: {positions:?}");
            for (_, r) in xs {
                let token = &src[r.clone()];
                assert!(
                    token.starts_with("${") && token.ends_with('}'),
                    "[{name}] term_position for X = {r:?} → {token:?}",
                );
            }
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

    #[test]
    fn lower_diags_render_as_lsp_errors() {
        use crate::session::DocSession;
        let file: Arc<std::path::Path> =
            Arc::from(std::path::PathBuf::from("t.sprf").as_path());
        // Bare ident in value position triggers value/bare-ident from
        // registry::lower_paren_slot (the third diag channel).
        let session = DocSession::new(
            file,
            "fs(foo)".into(),
            Registry::with_stdlib(),
        );
        let diags = lower_diags_to_lsp(session.source(), session.lowered_pipes());
        assert!(!diags.is_empty(), "expected a lowering diag");
        let d = &diags[0];
        assert_eq!(d.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(d.source.as_deref(), Some("sprefa"));
        assert_eq!(
            d.code,
            Some(NumberOrString::String("value/bare-ident".to_string())),
        );
    }

    // ─── Hover snapshots ────────────────────────────────────────────────
    //
    // Pin the full markdown body produced by `render_enriched_hover` for
    // representative pattern-op fixtures. The Config is empty + the
    // ConfigSource is CwdFallback, so output is fully deterministic
    // (no on-disk paths, no seed rows, no actual file reads).
    //
    // A "lowering failed — one of the ops returned a body diagnostic"
    // line in any snapshot means the synth re-parse path in
    // `lower_plan_keeping_ops` could not round-trip the op invocation.

    async fn enriched_at(src: &str, byte_offset: usize) -> Option<String> {
        use crate::session::DocSession;
        let file: Arc<std::path::Path> =
            Arc::from(std::path::PathBuf::from("t.sprf").as_path());
        let session = DocSession::new(file, src.into(), Registry::with_stdlib());
        let plan = session.hover_plan(byte_offset)?;
        let lowered: Vec<LoweredOp> = session
            .lowered_pipes()[plan.pipe_idx][..=plan.focus_step]
            .to_vec();
        let cfg = Config { seeds: vec![], run: Default::default() };
        render_enriched_hover(&plan, &lowered, &cfg, &ConfigSource::CwdFallback, session.registry()).await
    }

    fn off_of(src: &str, needle: &str) -> usize {
        src.find(needle).unwrap_or_else(|| panic!("needle {needle:?} not in {src:?}"))
            + needle.len() / 2
    }

    #[tokio::test]
    async fn hover_snapshot_json_op_name() {
        let src = "json({ name: ${N?}, version: ${V?} })";
        let md = enriched_at(src, off_of(src, "json")).await
            .expect("hover plan present at op name");
        insta::assert_snapshot!(md);
    }

    #[tokio::test]
    async fn hover_snapshot_json_capture_inside_body() {
        let src = "json({ name: ${N?}, version: ${V?} })";
        let md = enriched_at(src, off_of(src, "${N?}")).await
            .expect("hover plan present at capture");
        insta::assert_snapshot!(md);
    }

    #[tokio::test]
    async fn hover_snapshot_ast_op_name() {
        let src = "ast[rust](fn ${N?}(${ARGS?}) { ${BODY?} })";
        let md = enriched_at(src, off_of(src, "ast")).await
            .expect("hover plan present at op name");
        insta::assert_snapshot!(md);
    }

    #[tokio::test]
    async fn hover_snapshot_ast_capture_inside_body() {
        let src = "ast[rust](fn ${N?}(${ARGS?}) { ${BODY?} })";
        let md = enriched_at(src, off_of(src, "${N?}")).await
            .expect("hover plan present at capture");
        insta::assert_snapshot!(md);
    }
}
