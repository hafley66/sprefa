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

use effect_runtime::{RtCtx, SubjectRegistry};
use pipeline::_0_cursor::{Capture, CaptureKind, Cursor};
use pipeline::_2_pipeline::Pipeline;
use pipeline::effects::{LspDiagEffect, LspSeverity, PrintBatcher, StagedWriteRangeRow};
use pipeline::relation_store::{RelationStore, RelationWake};
use pipeline::readers::FileSource;

pub struct Backend {
    client: Client,
    /// Shared process state (cache, watchers, batcher, registry, ...).
    /// Cloned by every per-WS Backend; one daemon, one set of resources.
    state: Arc<ServerState>,
    sessions: Arc<Mutex<HashMap<Url, SessionEntry>>>,
    /// Per .sprf source URI → set of target file URIs we last published
    /// diagnostics on. On every hover-driven republish, targets that drop
    /// out of the new set get cleared with an empty `publish_diagnostics`.
    /// did_close drains all entries for the source.
    published_targets: Arc<Mutex<HashMap<Url, std::collections::HashSet<Url>>>>,
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
            published_targets: Arc::new(Mutex::new(HashMap::new())),
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

    /// Publish the new per-target diag set for `source` and clear any
    /// URIs that this source previously published into but are not in
    /// the new set. Empty diag vec on a target acts as a clear in LSP.
    async fn republish_target_diagnostics(
        &self,
        source: &Url,
        mut new_targets: HashMap<Url, Vec<Diagnostic>>,
    ) {
        let prev: std::collections::HashSet<Url> = {
            let guard = self.published_targets.lock().await;
            guard.get(source).cloned().unwrap_or_default()
        };
        for old in &prev {
            if !new_targets.contains_key(old) {
                self.client.publish_diagnostics(old.clone(), vec![], None).await;
            }
        }
        let mut next: std::collections::HashSet<Url> =
            std::collections::HashSet::with_capacity(new_targets.len());
        for (target, diags) in new_targets.drain() {
            self.client.publish_diagnostics(target.clone(), diags, None).await;
            next.insert(target);
        }
        let mut guard = self.published_targets.lock().await;
        if next.is_empty() {
            guard.remove(source);
        } else {
            guard.insert(source.clone(), next);
        }
    }

    /// Clear every target diagnostic this `.sprf` source published.
    /// Called from `did_close` so closing the rule file removes its
    /// squiggles from every `.rs`/`.ts`/etc. it had decorated.
    async fn clear_target_diagnostics(&self, source: &Url) {
        let prev = {
            let mut guard = self.published_targets.lock().await;
            guard.remove(source).unwrap_or_default()
        };
        for old in prev {
            self.client.publish_diagnostics(old, vec![], None).await;
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

    /// Like `publish`, but also drains every pipe in the file end-to-end
    /// to surface `sprf/no-output` choke diagnostics without needing a
    /// hover. Scoped to this single `.sprf` URI; never workspace-wide.
    /// Wired to did_open / did_save (NOT did_change — keystrokes keep
    /// the cheap parse-only `publish` path).
    async fn publish_with_drain(&self, uri: &Url) {
        let cancel = self.state.cancel_root.child_token();
        let snapshot = {
            let mut guard = self.sessions.lock().await;
            let Some(entry) = guard.get_mut(uri) else { return; };
            if let Some(prev) = entry.in_flight.remove("drain") { prev.cancel(); }
            entry.in_flight.insert("drain", cancel.clone());
            let session = &entry.session;
            let mut diags = parse_errors_to_lsp(session.source(), session.parse_errors());
            diags.extend(binding_diags_to_lsp(
                session.source(),
                session.binding_diagnostics(),
            ));
            diags.extend(lower_diags_to_lsp(session.source(), session.lowered_pipes()));
            Some((
                diags,
                session.config().clone(),
                session.lowered_pipes().to_vec(),
                session.source().to_string(),
            ))
        };
        let Some((mut diags, config, lowered_pipes, source_text)) = snapshot else { return };

        // Lazy-install per-seed watchers (same as hover path).
        for seed in &config.seeds {
            self.ensure_watcher(
                Arc::from(seed.slug.as_str()),
                seed.root.clone(),
            ).await;
        }

        let n_pipes = lowered_pipes.len();
        let t0 = std::time::Instant::now();
        let chokes = drain_all_pipes_for_chokes(
            &lowered_pipes, &config, self.cache(), &self.state,
            &source_text, cancel.clone(),
        ).await;
        let elapsed = t0.elapsed();
        diags.extend(chokes);

        if cancel.is_cancelled() {
            // A newer drain superseded us; drop our results on the floor.
            return;
        }
        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;

        if elapsed > std::time::Duration::from_millis(2000) {
            self.client.show_message(
                MessageType::WARNING,
                format!(
                    "sprf drain took {} ms over {} pipe{}",
                    elapsed.as_millis(),
                    n_pipes,
                    if n_pipes == 1 { "" } else { "s" },
                ),
            ).await;
        }

        if !cancel.is_cancelled() {
            let mut guard = self.sessions.lock().await;
            if let Some(entry) = guard.get_mut(uri) {
                entry.in_flight.remove("drain");
            }
        }
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
        if !is_sprf_uri(&uri) { return; }
        self.open_or_replace(&uri, p.text_document.text).await;
        self.publish_with_drain(&uri).await;
    }

    async fn did_change(&self, p: DidChangeTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        if !is_sprf_uri(&uri) { return; }
        if let Some(change) = p.content_changes.into_iter().last() {
            self.open_or_replace(&uri, change.text).await;
            // Cancel any in-flight drain — its choke diagnostics are
            // about to be stale anyway. Keystroke path stays cheap.
            {
                let mut guard = self.sessions.lock().await;
                if let Some(entry) = guard.get_mut(&uri) {
                    if let Some(prev) = entry.in_flight.remove("drain") {
                        prev.cancel();
                    }
                }
            }
            self.publish(&uri).await;
        }
    }

    async fn did_close(&self, p: DidCloseTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        if !is_sprf_uri(&uri) { return; }
        self.clear_target_diagnostics(&uri).await;
        // Clear the .sprf's own diagnostic set as well.
        self.client.publish_diagnostics(uri.clone(), vec![], None).await;
        let mut g = self.sessions.lock().await;
        g.remove(&uri);
    }

    async fn did_save(&self, p: DidSaveTextDocumentParams) {
        let uri = p.text_document.uri.clone();
        if !is_sprf_uri(&uri) { return; }
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
        self.publish_with_drain(&uri).await;
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

        let mut target_diags: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        let source_basename = uri
            .path()
            .rsplit('/')
            .next()
            .unwrap_or("<unknown>")
            .to_string();
        let source_text: Option<String> = {
            let guard = self.sessions.lock().await;
            guard.get(&uri).map(|e| e.session.source().to_string())
        };
        let body = match (snap.plan, snap.lowered_slice) {
            (Some(plan), Some(slice)) => {
                render_enriched_hover(
                    &plan, &slice, &snap.config, &snap.config_source,
                    &snap.registry, self.cache(), &self.state,
                    &mut target_diags,
                    Some(&uri), source_text.as_deref().unwrap_or(""),
                    &source_basename,
                ).await
            }
            _ => snap.static_doc,
        };

        // Multi-seed hover (HEAD + test/1/2/3) drains the same pipe
        // against overlapping trees; collapse identical diagnostics
        // before publishing so the user sees one squiggle per rule×span.
        dedupe_target_diagnostics(&mut target_diags);

        // Publish target-file diagnostics + clear any URIs that fell
        // out of the new set since this .sprf last hovered.
        self.republish_target_diagnostics(&uri, target_diags).await;

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
    diagnostics_out: &mut HashMap<Url, Vec<Diagnostic>>,
    source_uri: Option<&Url>,
    source_text: &str,
    _source_basename: &str,
) -> Option<String> {
    // Build the .sprf-side related-information location: the focused
    // op's invocation span, lifted to LSP coordinates against the
    // session's host source. Reused for every diagnostic this hover
    // produces.
    let source_link: Option<Location> = source_uri.and_then(|u| {
        let r = lowered.get(plan.focus_step).map(|lo| lo.byte_range.clone())?;
        let (sl, sc) = offset_to_position(source_text, r.start);
        let (el, ec) = offset_to_position(source_text, r.end);
        Some(Location {
            uri: u.clone(),
            range: Range {
                start: Position { line: sl, character: sc },
                end:   Position { line: el, character: ec },
            },
        })
    });
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
    // Per-step input/output totals summed across all seeds. A step is
    // a "universal choke" only when at least one seed sent cursors into
    // it but no seed produced any output from it. Per-seed filtering
    // (e.g. `rev(:HEAD)` rejecting non-HEAD seeds) is silent — that's
    // selective, not broken. Only the global blocker gets a diagnostic.
    let mut step_input_totals:  Vec<usize> = vec![0; pipeline_arcs.len()];
    let mut step_output_totals: Vec<usize> = vec![0; pipeline_arcs.len()];

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
    // Dry-run staged writes for the focused-step prefix, accumulated
    // across seeds. Surfaced as a HINT on the focused slot when the
    // focused op is `write_cursor`.
    let mut staged_focus: Vec<StagedWriteRangeRow> = Vec::new();
    let mut rendered = 0usize;
    md.push_str("\n---\n\n**Cursors after this op:**\n");

    let mut seed_roots: std::collections::HashMap<Arc<str>, PathBuf> =
        std::collections::HashMap::new();
    // Read each target file at most once across all seeds. Range
    // conversion needs the full file text; LANDMINES.sprf-style runs
    // can hit the same file dozens of times.
    let mut file_text_cache: HashMap<PathBuf, Option<String>> = HashMap::new();
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
            crate::run::build_seed_source(seed.root.clone(), seed.rev.clone());
        let relation_store = Arc::new(RelationStore::new());
        let subject_registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        // Single source-of-truth registration list. Hover hovering uses
        // the same builder as `/run`'s per-seed loop; adding an effect
        // kind happens in `run::build_seed_ctx` exactly once.
        let seed_ctx = crate::run::build_seed_ctx(
            state,
            file_source,
            op_cache.clone(),
            relation_store.clone(),
            subject_registry.clone(),
            PrintBatcher::buffer().0,
            std::sync::Arc::new(pipeline::effects::WritePolicy::DryRun),
        );
        let lsp_diags = seed_ctx.lsp_diags.clone();
        let staged_ranges = seed_ctx.staged_ranges.clone();
        let ctx = seed_ctx.ctx;

        let _drain_t0 = std::time::Instant::now();
        let drain = drain_pipe_steps(&ctx, &pipeline_arcs, &seed.slug, &seed.rev).await;
        for i in 0..pipeline_arcs.len() {
            step_input_totals[i]  += drain.in_counts[i];
            step_output_totals[i] += drain.out_counts[i];
        }
        let rows = drain.final_rows;
        tracing::info!(
            target: "sprefa::hover",
            seed = %seed.slug,
            step_counts = ?drain.out_counts,
            "seed.step_counts"
        );
        let summary = ctx.telemetry().summary();
        tracing::info!(
            target: "sprefa::hover",
            seed = %seed.slug,
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
        // Harvest target-file diagnostics from `lsp[severity](...)` ops.
        // Only rules that explicitly tail-emit a diagnostic produce a
        // squiggle; cursors that just match are NOT diagnostics by
        // default. The op carries the rule-author's message/code/hint
        // verbatim — far more useful than a generic "(match)" string.
        // Take any dry-run staged write rows this seed produced.
        {
            let mut g = staged_ranges.lock().unwrap();
            staged_focus.extend(std::mem::take(&mut *g));
        }
        let drained: Vec<LspDiagEffect> = {
            let mut g = lsp_diags.lock().unwrap();
            std::mem::take(&mut *g)
        };
        for effect in &drained {
            if let Some((uri, diag)) = lsp_effect_to_diagnostic(
                effect, &seed.root, source_link.as_ref(), &mut file_text_cache,
            ) {
                diagnostics_out.entry(uri).or_default().push(diag);
            }
        }
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

    // Surface per-step choke diagnostics on the .sprf source: for each
    // op that received >0 cursors but emitted 0, attach a HINT squiggle
    // on the op's invocation span. Multi-seed runs aggregate seeds per
    // step into one diagnostic.
    // Universal choke: first step where some seed sent cursors in but
    // no seed produced any output. Per-seed filters (`rev(:HEAD)` etc.)
    // produce no diagnostic since at least one seed survives them.
    let universal_choke: Option<usize> = (0..pipeline_arcs.len()).find(|&i| {
        step_input_totals[i] > 0 && step_output_totals[i] == 0
    });
    tracing::info!(
        target: "sprefa::hover",
        step_input_totals = ?step_input_totals,
        step_output_totals = ?step_output_totals,
        universal_choke = ?universal_choke,
        "render_enriched_hover.choke_scan"
    );
    if let (Some(uri), Some(step)) = (source_uri, universal_choke) {
        if let Some(slot) = lowered.get(step) {
            // Squiggle just the op-name span (e.g. `json`, `ast[ts]`)
            // up to the `(`, so the underline reads as "this op
            // starved" without highlighting the entire body.
            let full = slot.byte_range.clone();
            let name_end = if slot.paren_origin > 0 {
                slot.paren_origin.saturating_sub(1)
            } else {
                full.end
            };
            let r = full.start..name_end;
            if r.start <= source_text.len()
                && r.end <= source_text.len()
                && r.start < r.end
            {
                let (sl, sc) = offset_to_position(source_text, r.start);
                let (el, ec) = offset_to_position(source_text, r.end);
                let in_total  = step_input_totals[step];
                let out_total = step_output_totals[step];
                let diag = Diagnostic {
                    range: Range {
                        start: Position { line: sl, character: sc },
                        end:   Position { line: el, character: ec },
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("sprf/no-output".into())),
                    code_description: None,
                    source: Some("sprf".into()),
                    message: format!(
                        "step `{}` received {} cursors across {} seeds but emitted 0",
                        slot.name, in_total, config.seeds.len()
                    ),
                    related_information: None,
                    tags: None,
                    data: None,
                };
                let _ = out_total;
                diagnostics_out.entry(uri.clone()).or_default().push(diag);
            }
        }
    }

    // write_cursor HINT on the focused slot. Hover only drains the
    // focused-step prefix, so dry-run writes can only attach to the
    // focused slot (the last entry in `lowered`).
    if let (Some(uri), Some(slot)) = (source_uri, lowered.last()) {
        if slot.name.as_ref() == "write_cursor" {
            for d in write_pending_diags(slot, &staged_focus, source_text) {
                diagnostics_out.entry(uri.clone()).or_default().push(d);
            }
        }
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

/// Per-step input/output counts for one drain of one pipe.
struct PipeDrain {
    in_counts:  Vec<usize>,
    out_counts: Vec<usize>,
    final_rows: Vec<Cursor>,
}

/// Unified per-step drain: feed a fresh seed cursor into the head of
/// `ops`, materialize each stage's output, count cursors in/out at each
/// step, and return the per-step totals plus the final stage's rows.
///
/// Caller owns the seed loop and the `RtCtx` build (different sites
/// want different post-drain bookkeeping — hover renders rows, the
/// file-open path only wants choke counts). This is the shared inner
/// loop used by `render_enriched_hover` and `drain_all_pipes_for_chokes`.
async fn drain_pipe_steps(
    ctx: &RtCtx,
    ops: &[Arc<dyn pipeline::Op>],
    seed_slug: &str,
    seed_rev:  &str,
) -> PipeDrain {
    let mut in_counts  = vec![0usize; ops.len()];
    let mut out_counts = vec![0usize; ops.len()];

    let mut c = Cursor::default();
    c.repo = Arc::from(seed_slug);
    c.rev  = Arc::from(seed_rev);
    let mut s: futures::stream::BoxStream<'_, Arc<[Cursor]>> = Box::pin(
        stream::iter(vec![Arc::<[Cursor]>::from(vec![c])]),
    );

    let mut prev_count: usize = 1;
    for (i, op) in ops.iter().enumerate() {
        in_counts[i] = prev_count;
        let stage = Pipeline::Op(op.clone());
        let stream_in = std::mem::replace(
            &mut s,
            Box::pin(stream::empty::<Arc<[Cursor]>>()),
        );
        let mut stage_out = stage.run(ctx, stream_in);
        let mut buf: Vec<Arc<[Cursor]>> = Vec::new();
        let mut count = 0usize;
        while let Some(b) = stage_out.next().await {
            count += b.len();
            buf.push(b);
        }
        out_counts[i] = count;
        prev_count = count;
        s = Box::pin(stream::iter(buf));
    }

    let mut final_rows: Vec<Cursor> = Vec::new();
    while let Some(b) = s.next().await {
        final_rows.extend(b.iter().cloned());
    }
    PipeDrain { in_counts, out_counts, final_rows }
}

/// File-wide drain: run every pipe in `lowered_pipes` end-to-end across
/// every seed and emit one `sprf/no-output` diagnostic per pipe whose
/// universal choke is non-empty. Same per-step counting logic as
/// `render_enriched_hover` but no markdown, no per-seed table rendering,
/// no target-file diagnostics — just the source-side squiggles for
/// "this op starved" so the user sees them on file open / save without
/// having to hover each rule individually.
///
/// Pipes with any unlowered slot (lower-phase diagnostic already
/// published) are skipped — no point draining a pipeline that the
/// lowerer rejected.
async fn drain_all_pipes_for_chokes(
    lowered_pipes: &[Vec<LoweredOp>],
    config: &Config,
    op_cache: Arc<OpCache>,
    state: &Arc<ServerState>,
    source_text: &str,
    cancel: CancellationToken,
) -> Vec<Diagnostic> {
    // Per-pipe (input_totals, output_totals) summed across seeds.
    let mut totals: Vec<(Vec<usize>, Vec<usize>)> = lowered_pipes
        .iter()
        .map(|p| (vec![0usize; p.len()], vec![0usize; p.len()]))
        .collect();
    // Per-pipe lowered op vector, or None if any slot failed to lower.
    let pipe_arcs: Vec<Option<Vec<Arc<dyn pipeline::Op>>>> = lowered_pipes
        .iter()
        .map(|p| p.iter().map(|s| s.op.clone()).collect::<Option<Vec<_>>>())
        .collect();

    // Dry-run staged writes per pipe, accumulated across seeds. Used to
    // surface a HINT on write_cursor invocations.
    let mut staged_per_pipe: Vec<Vec<StagedWriteRangeRow>> =
        (0..lowered_pipes.len()).map(|_| Vec::new()).collect();

    for seed in &config.seeds {
        if cancel.is_cancelled() { return vec![]; }
        let file_source: Arc<dyn FileSource> =
            crate::run::build_seed_source(seed.root.clone(), seed.rev.clone());
        let relation_store = Arc::new(RelationStore::new());
        let subject_registry = Arc::new(SubjectRegistry::<RelationWake>::new());
        let seed_ctx = crate::run::build_seed_ctx(
            state,
            file_source,
            op_cache.clone(),
            relation_store.clone(),
            subject_registry.clone(),
            PrintBatcher::buffer().0,
            std::sync::Arc::new(pipeline::effects::WritePolicy::DryRun),
        );
        let staged_ranges = seed_ctx.staged_ranges.clone();
        let ctx = seed_ctx.ctx;

        for (pidx, arcs_opt) in pipe_arcs.iter().enumerate() {
            if cancel.is_cancelled() { return vec![]; }
            let Some(arcs) = arcs_opt else { continue };
            let drain = drain_pipe_steps(&ctx, arcs, &seed.slug, &seed.rev).await;
            for i in 0..arcs.len() {
                totals[pidx].0[i] += drain.in_counts[i];
                totals[pidx].1[i] += drain.out_counts[i];
            }
            // Drain rows accumulated by THIS pipe and tag them by pidx.
            // Doing it per-pipe (vs once per seed) lets us attribute to
            // the right write_cursor span.
            let drained: Vec<StagedWriteRangeRow> = {
                let mut g = staged_ranges.lock().unwrap();
                std::mem::take(&mut *g)
            };
            staged_per_pipe[pidx].extend(drained);
        }
    }

    let mut diags: Vec<Diagnostic> = Vec::new();
    for (pidx, pipe) in lowered_pipes.iter().enumerate() {
        if pipe_arcs[pidx].is_none() { continue }
        let (in_t, out_t) = &totals[pidx];
        let Some(step) = (0..pipe.len()).find(|&i| in_t[i] > 0 && out_t[i] == 0)
        else { continue };
        let slot = &pipe[step];
        let full = slot.byte_range.clone();
        let name_end = if slot.paren_origin > 0 {
            slot.paren_origin.saturating_sub(1)
        } else {
            full.end
        };
        let r = full.start..name_end;
        if !(r.start <= source_text.len()
            && r.end <= source_text.len()
            && r.start < r.end) { continue }
        let (sl, sc) = offset_to_position(source_text, r.start);
        let (el, ec) = offset_to_position(source_text, r.end);
        diags.push(Diagnostic {
            range: Range {
                start: Position { line: sl, character: sc },
                end:   Position { line: el, character: ec },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("sprf/no-output".into())),
            code_description: None,
            source: Some("sprf".into()),
            message: format!(
                "step `{}` received {} cursors across {} seeds but emitted 0",
                slot.name, in_t[step], config.seeds.len()
            ),
            related_information: None,
            tags: None,
            data: None,
        });
    }
    // write_cursor HINT: for any pipe that staged dry-run writes, attach
    // a HINT to each `write_cursor` invocation in the pipe.
    for (pidx, pipe) in lowered_pipes.iter().enumerate() {
        let rows = &staged_per_pipe[pidx];
        if rows.is_empty() { continue }
        for slot in pipe.iter() {
            if slot.name.as_ref() != "write_cursor" { continue }
            diags.extend(write_pending_diags(slot, rows, source_text));
        }
    }
    diags
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
    // Defensive: render-side ops (e.g. `render[plain]`) replace
    // `content` with rendered bytes while leaving `byte_range` pinned
    // at the upstream span. `c.active()` panics in that case. Clamp
    // the range to content.len() and bail out if the start is OOB.
    let len = c.content.len();
    let s = c.byte_range.start.min(len);
    let e = c.byte_range.end.min(len);
    if s >= e { return String::new(); }
    let active = &c.content[s..e];
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
            // Defensive clamp: render-side ops can swap `content` to the
            // rendered bytes while capture byte_ranges still point at
            // upstream spans. Slicing OOB panics, so clamp.
            let len = c.content.len();
            let s = cap.byte_range.start.min(len);
            let e = cap.byte_range.end.min(len);
            if s >= e {
                String::new()
            } else {
                String::from_utf8_lossy(&c.content[s..e]).into_owned()
            }
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

/// Build a target-file `Diagnostic` from one `lsp[severity](...)` op
/// emission. Source/message/code/severity are taken verbatim from the
/// rule author's invocation. Range is converted from the effect's
/// byte_range against the target file's text (cached across calls so
/// runs that hit the same file repeatedly read it once).
///
/// Returns `None` when the effect carries no `file`, the file cannot
/// be read, or the byte_range falls outside the file's bytes.
fn lsp_effect_to_diagnostic(
    e:               &LspDiagEffect,
    seed_root:       &Path,
    source_link:     Option<&Location>,
    text_cache:      &mut HashMap<PathBuf, Option<String>>,
) -> Option<(Url, Diagnostic)> {
    let rel = e.file.as_ref()?;
    let abs = seed_root.join(rel.as_ref());
    let canonical = std::fs::canonicalize(&abs).unwrap_or(abs);
    let uri = Url::from_file_path(&canonical).ok()?;

    let text = text_cache
        .entry(canonical.clone())
        .or_insert_with(|| std::fs::read_to_string(&canonical).ok())
        .as_deref()?;
    if e.byte_range.start > text.len() || e.byte_range.end > text.len() {
        return None;
    }
    // Shift start past leading whitespace on its line so the squiggle
    // hugs the actual non-blank content. comment(...) emits byte_range
    // starting at column 0 of the marker line (before indent); without
    // this shift, the squiggle paints leading spaces and reads as off.
    let line_lead_end = {
        let mut i = e.byte_range.start;
        let bytes = text.as_bytes();
        while i < e.byte_range.end && (bytes[i] == b' ' || bytes[i] == b'\t') {
            i += 1;
        }
        i
    };
    let (sl, sc) = offset_to_position(text, line_lead_end);
    let (mut el, mut ec) = offset_to_position(text, e.byte_range.end);
    // Clamp to a single line. comment(...) and other narrowing ops
    // produce cursors that cover whole regions (comment-to-comment, full
    // file scopes) by design. A diagnostic squiggle that wraps half the
    // file is unhelpful in the editor; end it at the first newline.
    if el != sl {
        let end_of_line = text[line_lead_end..]
            .find('\n')
            .map(|n| line_lead_end + n)
            .unwrap_or(e.byte_range.end);
        let (line, col) = offset_to_position(text, end_of_line);
        el = line;
        ec = col;
    }

    let severity = match e.severity {
        LspSeverity::Error   => DiagnosticSeverity::ERROR,
        LspSeverity::Warning => DiagnosticSeverity::WARNING,
        LspSeverity::Hint    => DiagnosticSeverity::HINT,
        LspSeverity::Info    => DiagnosticSeverity::INFORMATION,
    };

    // related_information: rule definition + optional rule-author hint.
    let mut related: Vec<DiagnosticRelatedInformation> = Vec::new();
    if let Some(loc) = source_link {
        related.push(DiagnosticRelatedInformation {
            location: loc.clone(),
            message:  "rule defined here".into(),
        });
        if let Some(hint) = &e.hint {
            related.push(DiagnosticRelatedInformation {
                location: loc.clone(),
                message:  hint.to_string(),
            });
        }
    }

    let diag = Diagnostic {
        range: Range {
            start: Position { line: sl, character: sc },
            end:   Position { line: el, character: ec },
        },
        severity: Some(severity),
        code: e.code.as_ref().map(|c| NumberOrString::String(c.to_string())),
        code_description: None,
        source: Some("sprf".into()),
        message: e.message.to_string(),
        related_information: if related.is_empty() { None } else { Some(related) },
        tags: None,
        data: None,
    };
    Some((uri, diag))
}

/// Collapse duplicate diagnostics within a target URI. Multi-seed
/// hover (e.g. HEAD + test/1/2/3) re-runs the same pipe against
/// overlapping trees; identical (range, code, message) diagnostics
/// would otherwise stack. Insertion-order preserved: the first
/// occurrence wins, rest dropped.
/// Build one HINT diagnostic per dry-run staged write row for a
/// `write_cursor` slot. Each row shows file, byte_range, mode, length,
/// and a preview of the bytes that *would* be spliced. Per-row (not
/// aggregated) so multi-seed runs surface stale-byte_range duplicates
/// instead of hiding them behind a single "N writes" summary.
fn write_pending_diags(
    slot:        &LoweredOp,
    rows:        &[StagedWriteRangeRow],
    source_text: &str,
) -> Vec<Diagnostic> {
    if rows.is_empty() { return Vec::new(); }
    let full = slot.byte_range.clone();
    let name_end = if slot.paren_origin > 0 {
        slot.paren_origin.saturating_sub(1)
    } else { full.end };
    let r = full.start..name_end;
    if !(r.start <= source_text.len() && r.end <= source_text.len() && r.start < r.end) {
        return Vec::new();
    }
    let (sl, sc) = offset_to_position(source_text, r.start);
    let (el, ec) = offset_to_position(source_text, r.end);

    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let file = row.effect.file.display().to_string();
            let bytes = row.effect.new_bytes.len();
            let span  = &row.effect.byte_range;
            let mode  = row.effect.mode.as_str();
            let preview = String::from_utf8_lossy(&row.effect.new_bytes)
                .replace('\n', "⏎")
                .replace('`', "\\`");
            let preview = if preview.chars().count() > 60 {
                let mut s: String = preview.chars().take(60).collect();
                s.push('…');
                s
            } else { preview };
            Diagnostic {
                range: Range {
                    start: Position { line: sl, character: sc },
                    end:   Position { line: el, character: ec },
                },
                severity: Some(DiagnosticSeverity::HINT),
                code:     Some(NumberOrString::String("sprf/write-pending".into())),
                code_description: None,
                source:   Some("sprf".into()),
                message:  format!(
                    "write[{}] {} {} bytes in {}[{}..{}]: \"{}\"",
                    i, mode, bytes, file, span.start, span.end, preview,
                ),
                related_information: None,
                tags: None,
                data: None,
            }
        })
        .collect()
}

fn dedupe_target_diagnostics(map: &mut HashMap<Url, Vec<Diagnostic>>) {
    for diags in map.values_mut() {
        let mut seen: std::collections::HashSet<(u32, u32, u32, u32, String, String)> =
            std::collections::HashSet::new();
        diags.retain(|d| {
            let code = match &d.code {
                Some(NumberOrString::String(s)) => s.clone(),
                Some(NumberOrString::Number(n)) => n.to_string(),
                None => String::new(),
            };
            seen.insert((
                d.range.start.line, d.range.start.character,
                d.range.end.line,   d.range.end.character,
                code, d.message.clone(),
            ))
        });
    }
}

fn uri_to_path(uri: &Url) -> PathBuf {
    uri.to_file_path()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Buffer-overlay scope: only `.sprf` URIs participate in did_open /
/// did_change / did_save sync. Other files reach the server only through
/// hover / code-action requests; sprf decides what files to read via its
/// own fs ops (DiskFileSource) and never needs the editor's in-memory
/// buffer for them. The VS Code client widens documentSelector to all
/// files so requests route here, then filters didOpen/didChange in
/// middleware. This is the server-side defense in depth.
fn is_sprf_uri(uri: &Url) -> bool {
    uri.path()
        .rsplit('/')
        .next()
        .map(|name| name.ends_with(".sprf"))
        .unwrap_or(false)
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
    fn lsp_effect_to_diagnostic_renders_author_message() {
        // Sample on-disk file; effect span lands on line 2 cols 0..10.
        let body = b"alpha\nbeta gamma\ndelta\nepsilon\n";
        let needle_start = body.windows(4).position(|w| w == b"beta").unwrap();
        let needle_end   = needle_start + 10;
        let dir = std::env::temp_dir().join(format!(
            "sprefa_lspdiag_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos()).unwrap_or(0),
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let target_rel = std::path::PathBuf::from("src/lib.rs");
        let target_abs = dir.join(&target_rel);
        std::fs::create_dir_all(target_abs.parent().unwrap()).unwrap();
        std::fs::write(&target_abs, body).unwrap();

        let effect = LspDiagEffect {
            file:       Some(Arc::<Path>::from(target_rel.as_path())),
            byte_range: needle_start..needle_end,
            severity:   LspSeverity::Warning,
            message:    Arc::from("load-bearing landmine: read the comment before editing"),
            code:       Some(Arc::from("sprf/landmine")),
            hint:       Some(Arc::from("SPRF-LANDMINE markers tag correctness contracts")),
        };
        let source_link = Location {
            uri: Url::parse("file:///tmp/LANDMINES.sprf").unwrap(),
            range: Range {
                start: Position { line: 12, character: 4 },
                end:   Position { line: 17, character: 1 },
            },
        };
        let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
        let (uri, diag) = lsp_effect_to_diagnostic(
            &effect, &dir, Some(&source_link), &mut cache,
        ).expect("diag produced");

        assert!(uri.path().ends_with("src/lib.rs"));
        assert_eq!(diag.range.start, Position { line: 1, character: 0 });
        assert_eq!(diag.range.end,   Position { line: 1, character: 10 });
        assert_eq!(diag.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(
            diag.code,
            Some(NumberOrString::String("sprf/landmine".to_string())),
        );
        assert_eq!(diag.source.as_deref(), Some("sprf"));
        // Message is the rule-author's message verbatim.
        assert_eq!(diag.message, effect.message.to_string());
        // related_information has rule-defined-here + the author's hint.
        let related = diag.related_information.as_ref().expect("related");
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].message, "rule defined here");
        assert!(related[1].message.contains("SPRF-LANDMINE"));

        // Re-running on the same file uses the cache (file_text is Some).
        let mut cache2 = cache.clone();
        let _ = lsp_effect_to_diagnostic(&effect, &dir, None, &mut cache2);
        assert_eq!(cache2.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lsp_effect_to_diagnostic_drops_when_no_file() {
        let effect = LspDiagEffect {
            file:       None,
            byte_range: 0..0,
            severity:   LspSeverity::Hint,
            message:    Arc::from("x"),
            code:       None,
            hint:       None,
        };
        let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
        let out = lsp_effect_to_diagnostic(&effect, Path::new("/tmp"), None, &mut cache);
        assert!(out.is_none());
    }

    #[test]
    fn dedupe_target_diagnostics_collapses_identical_entries() {
        // Multi-seed fanout shape: same range, same code, same message,
        // four times. Dedupe collapses to one.
        let uri = Url::parse("file:///tmp/x.rs").unwrap();
        let mk = |line: u32| Diagnostic {
            range: Range {
                start: Position { line, character: 0 },
                end:   Position { line, character: 5 },
            },
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(NumberOrString::String("ast".into())),
            code_description: None,
            source: Some("sprf:x.sprf".into()),
            message: "matched by `ast` (in x.sprf)".into(),
            related_information: None,
            tags: None,
            data: None,
        };
        let mut map: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
        map.insert(uri.clone(), vec![mk(10), mk(10), mk(10), mk(10), mk(20)]);
        dedupe_target_diagnostics(&mut map);
        let v = map.get(&uri).unwrap();
        assert_eq!(v.len(), 2, "4 dupes on line 10 collapse to 1; line 20 distinct");
        assert_eq!(v[0].range.start.line, 10);
        assert_eq!(v[1].range.start.line, 20);
    }

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
        {
            let mut diags: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            render_enriched_hover(
                &plan, &lowered, &cfg, &ConfigSource::CwdFallback,
                session.registry(), op_cache, &state, &mut diags,
                None, src, "t.sprf",
            ).await
        }
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
        {
            let mut diags: HashMap<Url, Vec<Diagnostic>> = HashMap::new();
            render_enriched_hover(
                &plan, &lowered, &cfg, &ConfigSource::CwdFallback,
                session.registry(), op_cache, &state, &mut diags,
                None, src, "t.sprf",
            ).await
        }
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
