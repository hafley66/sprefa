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
use pipeline::OpCache;
use sprefa_parse::{ParseError, ParseErrorKind};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::config::Config;
use crate::position::{offset_to_position, position_to_offset};
use crate::session::{ConfigSource, DocSession, HoverPlan, LoweredOp, SuggestionKind};
use crate::state::ServerState;

use effect_runtime::SubjectRegistry;
use pipeline::_0_cursor::{Capture, CaptureKind, Cursor};
use pipeline::_2_pipeline::Pipeline;
use pipeline::effects::PrintBatcher;
use pipeline::relation_store::{RelationStore, RelationWake};
use pipeline::readers::{DiskFileSource, FileSource};

pub struct Backend {
    client: Client,
    /// Shared process state (cache, watchers, batcher, registry, ...).
    /// Cloned by every per-WS Backend; one daemon, one set of resources.
    state: Arc<ServerState>,
    sessions: Arc<Mutex<HashMap<Url, SessionEntry>>>,
}

struct SessionEntry {
    session:   DocSession,
    /// Canonical workspace root for this URI's file. Set on did_open.
    workspace: Option<PathBuf>,
    /// switchMap-style cancel slots, keyed by op (`"hover"`, etc.).
    /// did_change cancels every entry; a fresh hover replaces "hover".
    in_flight: HashMap<&'static str, CancellationToken>,
}

impl Backend {
    /// Stdio entry point — boots a private ServerState. Used by the
    /// legacy stdio binary path; daemon mode goes through `with_state`.
    pub fn new(client: Client) -> Self {
        // Block on async ServerState construction by deferring to a
        // helper task; new() is only called inside an executor.
        let state = futures::executor::block_on(async {
            ServerState::new(&Default::default()).await.expect("ServerState::new")
        });
        Self::with_state(client, state)
    }

    /// Shared-state entry point. Used by `transport_lsp` for every WS
    /// connection, so all editors against the daemon share resources.
    pub fn with_state(client: Client, state: Arc<ServerState>) -> Self {
        Self {
            client,
            state,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn registry(&self) -> Registry { self.state.registry.clone() }
    fn cache(&self) -> Arc<OpCache> { self.state.cache.clone() }

    async fn ensure_watcher(&self, slug: Arc<str>, root: PathBuf) {
        self.state.ensure_watcher(slug, root).await;
    }

    async fn open_or_replace(&self, uri: &Url, source: String) {
        let file = uri_to_path(uri);
        let file_arc: Arc<std::path::Path> = Arc::from(file.as_path());

        // Resolve workspace, push the new bytes into its overlay so
        // subsequent pipes scanning this path see editor bytes.
        let workspace = self.state.workspaces
            .write().await
            .resolve(&file, &self.state.cancel_root)
            .await;
        if let Ok(rel) = file.strip_prefix(&workspace.root) {
            workspace.source.set(rel.to_path_buf(), Arc::from(source.as_bytes().to_vec()));
        }

        let mut guard = self.sessions.lock().await;
        match guard.get_mut(uri) {
            Some(entry) => {
                // Cancel everything in flight on this doc — switchMap.
                for (_, tok) in entry.in_flight.drain() { tok.cancel(); }
                entry.session.on_source_change(source);
                entry.workspace = Some(workspace.root.clone());
            }
            None => {
                guard.insert(
                    uri.clone(),
                    SessionEntry {
                        session:   DocSession::new(file_arc, source, self.registry()),
                        workspace: Some(workspace.root.clone()),
                        in_flight: HashMap::new(),
                    },
                );
            }
        }
    }

    async fn publish(&self, uri: &Url) {
        let diags = {
            let guard = self.sessions.lock().await;
            let Some(entry) = guard.get(uri) else { return; };
            let session = &entry.session;
            let mut diags = parse_errors_to_lsp(session.source(), session.parse_errors());
            diags.extend(binding_diags_to_lsp(
                session.source(),
                session.binding_diagnostics(),
            ));
            diags.extend(lower_diags_to_lsp(session.source(), session.lowered_pipes()));
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
        let uri = p.text_document.uri.clone();
        // Disk has caught up — drop the buffer overlay so subsequent
        // pipes read live disk bytes again.
        let file = uri_to_path(&uri);
        let workspace = self.state.workspaces
            .write().await
            .resolve(&file, &self.state.cancel_root)
            .await;
        if let Ok(rel) = file.strip_prefix(&workspace.root) {
            workspace.source.clear(rel);
        }
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
        // switchMap: cancel the prior hover for this URI, install a
        // fresh per-request token. The token is wired into the per-put
        // cancel inside `BoundedWorkSteal::run`, so an in-flight ast
        // parse pool short-circuits the moment a newer hover lands.
        let cancel = self.state.cancel_root.child_token();
        let snapshot = {
            let mut guard = self.sessions.lock().await;
            let Some(entry) = guard.get_mut(&uri) else { return Ok(None) };
            if let Some(prev) = entry.in_flight.remove("hover") {
                prev.cancel();
            }
            entry.in_flight.insert("hover", cancel.clone());
            let session = &entry.session;
            let offset = position_to_offset(session.source(), pos.line, pos.character);
            let plan = session.hover_plan(offset);
            let static_doc = session.hover_at(offset);
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

        // Ensure each configured seed has a live fs watcher before the
        // pipe drains. First-touch is lazy; subsequent hovers are no-ops.
        for seed in &snap.config.seeds {
            self.ensure_watcher(
                Arc::from(seed.slug.as_str()),
                seed.root.clone(),
            ).await;
        }

        let body = match (snap.plan, snap.lowered_slice) {
            (Some(plan), Some(slice)) => {
                render_enriched_hover(
                    &plan, &slice, &snap.config, &snap.config_source,
                    &snap.registry, self.cache(), &self.state,
                ).await
            }
            _ => snap.static_doc,
        };

        // Settle: drop the in_flight slot if our token wasn't replaced
        // by a newer hover. A concurrent did_change / hover would have
        // tripped our token, so is_cancelled() is the same-identity proxy.
        if !cancel.is_cancelled() {
            let mut guard = self.sessions.lock().await;
            if let Some(entry) = guard.get_mut(&uri) {
                entry.in_flight.remove("hover");
            }
        }
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
        let Some(entry) = guard.get(&uri) else { return Ok(None) };
        let session = &entry.session;
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
    op_cache: Arc<OpCache>,
    state: &Arc<ServerState>,
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
        let file_source: Arc<dyn FileSource> =
            Arc::new(DiskFileSource::new(seed.root.clone(), seed.rev.clone()));
        let relation_store = Arc::new(RelationStore::new());
        let subject_registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        // Single source-of-truth registration list. Hover hovering uses
        // the same builder as `/run`'s per-seed loop; adding an effect
        // kind happens in `run::build_seed_ctx` exactly once.
        let ctx = crate::run::build_seed_ctx(
            state,
            file_source,
            op_cache.clone(),
            relation_store.clone(),
            subject_registry.clone(),
            PrintBatcher::buffer().0,
        );

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
        let take = (HOVER_ROW_CAP.saturating_sub(rendered)).min(rows.len());
        total_rows += rows.len();
        if take > 0 {
            md.push_str(&render_cursor_tables(
                &rows[..take], rendered, &root, focus_capture.as_deref(),
            ));
            rendered += take;
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

/// Render a batch of cursors as one or more wide markdown tables —
/// one table per group of cursors that share the same capture-name
/// signature. Each row is one cursor; columns are
/// `# | file | bytes | match | $X | $Y | ... | last_bound`.
///
/// Grouping by signature keeps columns stable per table; pipelines
/// that emit cursors with diverging capture sets get a small number
/// of tables instead of one ragged super-table.
///
/// `start_index` is added to the row number so multi-seed runs
/// keep ordinals globally consistent.
fn render_cursor_tables(
    cursors:       &[Cursor],
    start_index:   usize,
    root:          &Path,
    focus_capture: Option<&str>,
) -> String {
    let mut out = String::new();

    // Group by capture-name signature, preserving first-seen order.
    let mut groups: Vec<(Vec<Arc<str>>, Vec<usize>)> = Vec::new();
    for (i, c) in cursors.iter().enumerate() {
        let sig: Vec<Arc<str>> = c.captures.iter().map(|cap| cap.name.clone()).collect();
        if let Some((_, idxs)) = groups.iter_mut().find(|(s, _)| s == &sig) {
            idxs.push(i);
        } else {
            groups.push((sig, vec![i]));
        }
    }

    for (gi, (sig, idxs)) in groups.iter().enumerate() {
        if gi > 0 {
            out.push('\n');
        }
        if groups.len() > 1 {
            let cap_list: Vec<String> = sig.iter().map(|n| format!("`${n}`")).collect();
            out.push_str(&format!(
                "\n_Group {} of {} ({} rows; captures: {})_\n",
                gi + 1,
                groups.len(),
                idxs.len(),
                if cap_list.is_empty() { "_(none)_".into() } else { cap_list.join(", ") },
            ));
        }

        // Header row.
        out.push_str("\n| # | file | bytes | match |");
        for n in sig {
            let is_focus = focus_capture.is_some_and(|f| f == n.as_ref());
            if is_focus {
                out.push_str(&format!(" **`${n}`** ▶ |"));
            } else {
                out.push_str(&format!(" `${n}` |"));
            }
        }
        out.push_str(" last_bound |\n");
        out.push_str("| --- | --- | --- | --- |");
        for _ in sig { out.push_str(" --- |"); }
        out.push_str(" --- |\n");

        // Data rows.
        for &i in idxs {
            let c = &cursors[i];
            let n = start_index + i + 1;

            let file_cell = match &c.fs {
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

            let bytes_cell = format!("`{}..{}`", c.byte_range.start, c.byte_range.end);
            let preview = render_preview(c);
            let match_cell = if preview.is_empty() {
                String::new()
            } else {
                format!("`{}`", escape_pipe(&preview))
            };

            out.push_str(&format!(
                "| {n} | {file_cell} | {bytes_cell} | {match_cell} |",
            ));

            // Capture cells in signature order. Every cursor in this
            // group has the same capture-name set by construction.
            for cap_name in sig {
                let cell = c
                    .captures
                    .iter()
                    .find(|cap| &cap.name == cap_name)
                    .map(|cap| render_capture_value(c, cap, root))
                    .unwrap_or_default();
                let is_focus = focus_capture.is_some_and(|f| f == cap_name.as_ref());
                if is_focus {
                    out.push_str(&format!(" **{cell}** |"));
                } else {
                    out.push_str(&format!(" {cell} |"));
                }
            }

            let last = c
                .last_bound
                .as_ref()
                .map(|n| format!("`${n}`"))
                .unwrap_or_default();
            out.push_str(&format!(" {last} |\n"));
        }
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

/// Value-only cell for the cursor table. The capture name lives in the
/// field column, so this returns just the rhs: a linked code-span for
/// span-backed captures, an inline `…` _(synthesized)_ for synthesized.
fn render_capture_value(c: &Cursor, cap: &Capture, root: &Path) -> String {
    let value_str = match &cap.kind {
        CaptureKind::Synthesized { value } => value.to_string(),
        CaptureKind::SpanBacked => {
            String::from_utf8_lossy(&c.content[cap.byte_range.clone()]).into_owned()
        }
    };
    let value_short = escape_pipe(&truncate(
        &value_str.replace('\n', " ⏎ ").replace('`', "\\`"),
        48,
    ));

    match &cap.kind {
        CaptureKind::SpanBacked => {
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
                    "[`{value_short}`]({href}) `bytes {}..{}`",
                    cap.byte_range.start, cap.byte_range.end,
                ),
                None => format!(
                    "`{value_short}` `bytes {}..{}`",
                    cap.byte_range.start, cap.byte_range.end,
                ),
            }
        }
        CaptureKind::Synthesized { .. } => {
            format!("`{value_short}` _(synthesized)_")
        }
    }
}

/// Escape `|` so cell values don't terminate a markdown table column.
fn escape_pipe(s: &str) -> String { s.replace('|', "\\|") }

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
        let md = render_cursor_tables(std::slice::from_ref(&c), 0, &root, None);

        assert!(md.contains("[`Cargo.toml#L1`]"), "header file link present: {md}");
        assert!(md.contains("(file:///tmp/root/Cargo.toml#L"), "absolute file:// URL: {md}");
        assert!(md.contains("`$N`"), "capture column header present: {md}");
        assert!(md.contains("[`\"alice\"`]"), "capture value rendered as link: {md}");
        assert!(
            md.contains(&format!("#L2)")),
            "capture link points at line 2 of the source: {md}"
        );
        assert!(md.contains("| --- |"), "table separator present: {md}");
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
        let md = render_cursor_tables(std::slice::from_ref(&c), 0, std::path::Path::new("/"), None);
        assert!(md.contains("`$ARGS`"), "synthesized capture column header: {md}");
        assert!(
            md.contains("`a , b` _(synthesized)_"),
            "synthesized value cell: {md}",
        );
    }

    #[test]
    fn cursor_block_escapes_newlines_in_capture_values() {
        // Newlines in a capture value would otherwise terminate the
        // markdown table row, breaking every row that follows. Pin
        // that they get rewritten to ` ⏎ ` and the table stays intact.
        let mut c = Cursor::new(Arc::from(&b""[..]));
        c.captures.push(Capture::synthesized(
            Arc::from("BODY"),
            Arc::from("for d in dirs {\n    fs::create_dir_all(r"),
        ));
        let md = render_cursor_tables(std::slice::from_ref(&c), 0, std::path::Path::new("/"), None);
        assert!(
            !md.contains("dirs {\n    fs::create"),
            "raw newline leaked into table cell: {md}",
        );
        assert!(
            md.contains("⏎"),
            "newline must be replaced with ⏎ glyph: {md}",
        );
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
        let op_cache = Arc::new(OpCache::new(true));
        let state = ServerState::new(&Default::default()).await.expect("ServerState::new");
        render_enriched_hover(&plan, &lowered, &cfg, &ConfigSource::CwdFallback, session.registry(), op_cache, &state).await
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

    #[tokio::test]
    async fn hover_ast_yaml_multiline_body_renders_without_choking() {
        // Repro: ast_yaml_audit.sprf — multi-line YAML body with a
        // ${X?} carveout buried inside a `pattern:` string spread over
        // several physical lines. The whole hover render must come
        // back as Some(_), with no panic and no truncated empty body.
        let src = "ast_yaml[rs](\n  rule:\n    pattern: \"${X?}.to_string()\"\n    not:\n      inside:\n        kind: mod_item\n)";
        let off = off_of(src, "${X?}");
        let md = enriched_at(src, off).await
            .expect("hover plan present at ${X?} inside multi-line ast_yaml body");
        assert!(
            md.contains("**Focused capture:** `$X`"),
            "focused capture must surface for multiline ast_yaml:\n{md}",
        );
        assert!(
            !md.contains("_pipeline lowering failed:_"),
            "ast_yaml lowering should succeed on multi-line body:\n{md}",
        );
        // Op-doc header used to be `**ast_yaml**[_lang_](_yaml_)`,
        // which markdown parses as a literal hyperlink and turned the
        // VS Code hover into a broken-link soup. Pinning the inline
        // code form so any regression flips this test.
        assert!(
            md.contains("`ast_yaml[lang](yaml)`"),
            "op header must be inline code, not a stray markdown link:\n{md}",
        );
        assert!(
            !md.contains("**ast_yaml**[_lang_]"),
            "old broken markdown-link header sneaked back in:\n{md}",
        );
    }

    /// Regression: hovering on the ast_yaml op of the shipped
    /// `fixtures/ast_yaml_parity.sprf` must run the seeded fs walk and
    /// render the actual cursor table — not just the static op doc.
    /// Drift between the ast and ast_yaml pipes is what chokes VS Code
    /// hover; this test seeds a tempdir with a Rust source matching
    /// the pattern, points a Seed at it, and asserts BOTH pipes emit
    /// the same cursor row count with the same `$N` capture binding.
    #[tokio::test]
    async fn hover_ast_yaml_parity_fixture_emits_cursor_rows() {
        let src = include_str!("../fixtures/ast_yaml_parity.sprf");

        // Tempdir with two functions in one .rs file. Both pipes match
        // `fn $N($$$ARGS) { $$$BODY }` (no return type — that fn shape
        // is what the ast-grep pattern hits; adding `-> T` puts a node
        // between `)` and `{` and breaks the match).
        let dir = seeded_tempdir(&[(
            "lib.rs",
            "fn add(a: i32, b: i32) { let _ = a + b; }\nfn sub(a: i32, b: i32) { let _ = a - b; }\n",
        )]);
        let cfg = Config {
            seeds: vec![crate::config::Seed {
                slug: "tmp".into(),
                root: dir.clone(),
                rev:  "HEAD".into(),
            }],
            run: Default::default(),
        };

        // For each pipe, hover lands on the ast / ast_yaml op name and
        // the seeded fs walk runs end-to-end. Pin the cursor render:
        // both functions surface, `$N` binds to `add` and `sub`, and
        // the rendered total matches the file's two function defs.
        let hover_sites: Vec<(&str, &str, &str)> = vec![
            ("ast[rs](",      "ast",      "`ast[lang](pattern)`"),
            ("ast_yaml[rs](", "ast_yaml", "`ast_yaml[lang](yaml)`"),
        ];
        let mut row_counts: Vec<usize> = Vec::with_capacity(2);
        for (anchor, op_name, header) in &hover_sites {
            let pos = src.find(anchor).unwrap_or_else(|| {
                panic!("anchor `{anchor}` missing from fixture");
            });
            let off = pos + anchor.len() / 2;
            let md = enriched_at_with_cfg(src, off, cfg.clone()).await
                .unwrap_or_else(|| panic!("no hover plan at `{op_name}`"));

            // Lowering must succeed end-to-end; a stuck pipe surfaces
            // as `_pipeline lowering failed:_` and zero seeded rows.
            assert!(
                !md.contains("_pipeline lowering failed:_"),
                "[{op_name}] lowering failed:\n{md}",
            );
            assert!(
                md.contains(header),
                "[{op_name}] op header missing inline-code form `{header}`:\n{md}",
            );
            // Old broken `**op**[_lang_](_pattern_)` form parsed as a
            // markdown link, the choke that started this regression.
            assert!(
                !md.contains("](_pattern_)") && !md.contains("](_yaml_)"),
                "[{op_name}] broken markdown-link header crept back:\n{md}",
            );

            // The seeded walk must hit both `fn add` and `fn sub`. The
            // cursor table renders one row per match; capture column
            // header is `$N` (declared by the focus op), values are
            // span-backed links to the seeded file.
            assert!(
                md.contains("**Total rows:** 2"),
                "[{op_name}] expected 2 cursor rows from seeded walk:\n{md}",
            );
            assert!(
                md.contains("`$N`"),
                "[{op_name}] capture column `$N` missing from cursor table:\n{md}",
            );
            assert!(
                md.contains("[`add`]") && md.contains("[`sub`]"),
                "[{op_name}] both fn names must appear as $N values:\n{md}",
            );
            // File-link should resolve to the seeded tempdir, never
            // collapse to a bare basename without a target.
            assert!(
                md.contains("[`lib.rs#L1`]"),
                "[{op_name}] cursor row file-link missing:\n{md}",
            );

            row_counts.push(2);
        }

        // Belt-and-suspenders: parity is the whole point of the
        // fixture, so make the assertion explicit.
        assert_eq!(
            row_counts[0], row_counts[1],
            "ast vs ast_yaml row counts diverged: {row_counts:?}",
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ─── Seeded-walk hover assertions ───────────────────────────────────
    //
    // Snapshot tests above run with `Config { seeds: vec![] }` so the
    // fs walk + cursor render path is exercised by zero rows. These
    // tests stand up a tempdir + a real Seed and assert that
    // `render_enriched_hover` actually emits cursors when the pipe
    // matches, and emits zero (with the explicit "no cursors" line)
    // when a `rev` filter rejects the seed rev. Pins the failure mode
    // the LSP was hitting: `.sprefa.toml` schema mismatch dropped the
    // user into cwd-fallback (rev "wt"), and `rev(:HEAD)` filtered
    // every cursor out silently.

    fn seeded_tempdir(files: &[(&str, &str)]) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "sprefa-hover-{}-{}", std::process::id(), nanos,
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for (rel, body) in files {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() { std::fs::create_dir_all(parent).unwrap(); }
            std::fs::write(p, body).unwrap();
        }
        dir
    }

    async fn enriched_at_with_cfg(
        src: &str,
        byte_offset: usize,
        cfg: Config,
    ) -> Option<String> {
        use crate::session::DocSession;
        let file: Arc<std::path::Path> =
            Arc::from(std::path::PathBuf::from("t.sprf").as_path());
        let session = DocSession::new(file, src.into(), Registry::with_stdlib());
        let plan = session.hover_plan(byte_offset)?;
        let lowered: Vec<LoweredOp> = session
            .lowered_pipes()[plan.pipe_idx][..=plan.focus_step]
            .to_vec();
        let op_cache = Arc::new(OpCache::new(true));
        let state = ServerState::new(&Default::default()).await.expect("ServerState::new");
        render_enriched_hover(&plan, &lowered, &cfg, &ConfigSource::CwdFallback, session.registry(), op_cache, &state).await
    }

    #[tokio::test]
    async fn hover_seeded_fs_walk_emits_cursor_per_matched_file() {
        let dir = seeded_tempdir(&[
            ("a.rs", "fn a() {}\n"),
            ("nested/b.rs", "fn b() {}\n"),
            ("c.txt", "skip me\n"),
        ]);
        let cfg = Config {
            seeds: vec![crate::config::Seed {
                slug: "tmp".into(),
                root: dir.clone(),
                rev:  "HEAD".into(),
            }],
            run: Default::default(),
        };
        let src = "fs(glob(**/*.rs))";
        let md = enriched_at_with_cfg(src, off_of(src, "fs("), cfg).await
            .expect("hover plan present at fs op");
        assert!(
            md.contains("**Total rows:** 2"),
            "expected two .rs cursors, got:\n{md}",
        );
        assert!(md.contains("a.rs"),       "row for a.rs missing:\n{md}");
        assert!(md.contains("nested/b.rs"), "row for nested/b.rs missing:\n{md}");
        assert!(!md.contains("c.txt"),     "txt file leaked through glob:\n{md}");
    }

    #[tokio::test]
    async fn hover_rev_atom_filter_passes_when_seed_rev_matches() {
        // `rev(:HEAD)` lowers to Value::Atom("HEAD") (colon stripped),
        // which becomes a glob matching exactly "HEAD" against
        // cursor.rev. Seed rev "HEAD" must pass; the file then walks.
        let dir = seeded_tempdir(&[("a.rs", "fn a() {}\n")]);
        let cfg = Config {
            seeds: vec![crate::config::Seed {
                slug: "tmp".into(),
                root: dir,
                rev:  "HEAD".into(),
            }],
            run: Default::default(),
        };
        let src = "rev(:HEAD) > fs(glob(**/*.rs))";
        let md = enriched_at_with_cfg(src, off_of(src, "fs("), cfg).await
            .expect("hover plan present at fs");
        assert!(
            md.contains("**Total rows:** 1"),
            "rev(:HEAD) + seed rev=HEAD must pass one cursor:\n{md}",
        );
    }

    #[tokio::test]
    async fn hover_rev_atom_filter_drops_when_seed_rev_mismatches() {
        // Pins the silent-drop bug the user hit: `.sprefa.toml` schema
        // mismatch sent the LSP into cwd-fallback with rev "wt", and
        // `rev(:HEAD)` then filtered every cursor out. With seed rev
        // "wt" against `rev(:HEAD)`, total_rows must be 0 and the
        // "(no cursors emitted)" sentinel must be present.
        let dir = seeded_tempdir(&[("a.rs", "fn a() {}\n")]);
        let cfg = Config {
            seeds: vec![crate::config::Seed {
                slug: "tmp".into(),
                root: dir,
                rev:  "wt".into(),
            }],
            run: Default::default(),
        };
        let src = "rev(:HEAD) > fs(glob(**/*.rs))";
        let md = enriched_at_with_cfg(src, off_of(src, "fs("), cfg).await
            .expect("hover plan present at fs");
        assert!(
            md.contains("**Total rows:** 0"),
            "seed rev=wt must be filtered out by rev(:HEAD):\n{md}",
        );
        assert!(
            md.contains("(no cursors emitted)"),
            "explicit zero-row sentinel missing:\n{md}",
        );
    }
}
