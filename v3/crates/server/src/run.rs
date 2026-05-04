//! `run_pipeline` — the single primitive shared by `/run` SSE and the
//! `sprefa-run` HTTP CLI. This is the v3 port of the body that used to
//! live inside `bin/sprefa-run.rs`. It owns the RtCtx build (so the
//! shared `Arc<BoundedWorkSteal<AstParseEffect>>` from `ServerState`
//! flows in via clone), the per-seed pipeline drain, and the SQLite
//! dump.
//!
//! Shape:
//!
//! ```ignore
//! let start = run_pipeline(state, RunRequest { source, source_path,
//!     workspace_hint, out_db, cache, cancel }).await;
//! while let Some(ev) = start.stream.next().await { ... }
//! ```
//!
//! `RunStart.rule_names` carries the top-level pipe order so a UI can
//! print `rule NAME — N rows` in source order. `stream` emits
//! `RunEvent::{Meta,Cursor,ExprDone,Diag,Done}` lazily.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use futures::stream::{self, BoxStream, StreamExt};
use tokio_util::sync::CancellationToken;

use effect_runtime::{RtCtx, RtCtxBuilder, SubjectRegistry, Yield, YieldBatcher};
use pipeline::_0_cursor::Cursor;
use pipeline::_2_pipeline::Pipeline;
use pipeline::cache_key::OpCache;
use pipeline::disk_cache::DiskCache;
use pipeline::effects::{
    AstParseEffect, FsListFilesBatcher, FsListFilesEffect, LspDiagBatcher, LspDiagEffect,
    PrintBatcher, PrintEffect, ReadBytesBatchBatcher, ReadBytesBatchEffect, ReadBytesBatcher,
    ReadBytesEffect, StagedWriteFileRow, StagedWriteRangeRow, WriteApproval, WriteDecision,
    ShBatcher, ShEffect, StagedShRow,
    WriteFileBatcher, WriteFileEffect, WritePolicy, WriteRangeBatcher, WriteRangeEffect,
};
use pipeline::ops::relation::RelationArg;
use pipeline::ops::{RuleCallOp, RulePredicateOp};
use pipeline::readers::{BufferOverlay, DiskFileSource, FileSource, GitFileSource};
use pipeline::registry::{lower_paren_slot, Registry};
use pipeline::relation_store::{RelationStore, RelationWake, WriteBatcher, WriteEffect};
use pipeline::rule_def;
use pipeline::rule_hash::{self, RuleHashes};
use pipeline::value::{TermMode, Value};
use sprefa_parse::{host_parse_with_injections, OpInvocation, Pipe, PipeStep};

use crate::config::{Config, RunOpts};
use crate::state::ServerState;

pub struct RunRequest {
    pub source:         String,
    pub source_path:    Option<PathBuf>,
    pub workspace_hint: Option<PathBuf>,
    /// Output sqlite path. `None` ⇒ no drain (events only).
    pub out_db:         Option<PathBuf>,
    /// sprf_meta cache toggle.
    pub cache:          bool,
    /// Per-request cancel. Drop or trip to abort. For LSP hover
    /// switchMap-style cancellation the LSP layer constructs a per-doc
    /// child token from `state.cancel_root`.
    pub cancel:         CancellationToken,
    /// Staging policy consulted by the write sinks. Default
    /// `ApproveAll` mirrors prior behavior.
    pub write_policy:   Arc<WritePolicy>,
}

pub struct RunStart {
    pub rule_names: Vec<Arc<str>>,
    pub stream:     BoxStream<'static, RunEvent>,
}

pub enum RunEvent {
    Cursor   { expr_name: Option<Arc<str>>, cursor: Cursor },
    ExprDone { expr_name: Option<Arc<str>> },
    Diag     { code: String, severity: String, message: String },
    Done,
}

/// Map a (decision, result) onto the (severity, code, outcome string)
/// used in the staged-write diag panel. `kind` is the non-rejected code
/// namespace (`staged/write_range` or `staged/write_file`); rejected
/// rows always surface as `staged/rejected` Warning so they stand out.
fn decision_render(
    decision: WriteDecision,
    result:   &Result<(), Arc<str>>,
    kind:     &str,
) -> (String, String, String) {
    match (decision, result) {
        (WriteDecision::Approved, Ok(()))  => ("Info".into(),    kind.to_string(),         "ok".to_string()),
        (WriteDecision::Approved, Err(e))  => ("Info".into(),    kind.to_string(),         format!("err: {e}")),
        (WriteDecision::DryRun,   _)       => ("Info".into(),    kind.to_string(),         "dry-run".to_string()),
        (WriteDecision::Rejected, _)       => ("Warning".into(), "staged/rejected".into(), "rejected by policy".to_string()),
    }
}

pub async fn run_pipeline(state: Arc<ServerState>, req: RunRequest) -> RunStart {
    // 1. Resolve workspace.
    let hint: PathBuf = req.workspace_hint.clone()
        .or_else(|| req.source_path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf)))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let workspace = state.workspaces.write().await
        .resolve(&hint, &state.cancel_root).await;

    // 2. Parse.
    let file_path: Arc<Path> = req.source_path.clone()
        .map(|p| Arc::from(p.as_path()))
        .unwrap_or_else(|| Arc::from(Path::new("<memory>")));
    let (parsed, parse_errors) = host_parse_with_injections(
        &req.source,
        file_path.clone(),
        &pipeline::op_languages::language_of,
    );

    let mut diag_events: Vec<RunEvent> = parse_errors.iter()
        .map(|e| RunEvent::Diag {
            code:     format!("parse/{:?}", e.kind).to_lowercase(),
            severity: "Error".into(),
            message:  format!("at {}..{}: {}", e.byte_range.start, e.byte_range.end, e.message),
        })
        .collect();

    if parsed.pipes.is_empty() {
        diag_events.push(RunEvent::Done);
        return RunStart {
            rule_names: vec![],
            stream:     stream::iter(diag_events).boxed(),
        };
    }

    // 3. Top-level rule-name extraction (matches sprefa-run heuristic:
    //    first op's paren_src text trimmed; empty → unnamed).
    let rule_names: Vec<Arc<str>> = parsed.pipes.iter().map(|p| {
        p.ops.first()
            .map(|op| Arc::<str>::from(op.name.as_ref()))
            .unwrap_or_else(|| Arc::<str>::from(""))
    }).collect();

    // 4. Rule defs + hashes (deterministic from source; shared across seeds).
    let rule_defs = rule_def::collect(
        &parsed, req.source.as_bytes(), &state.registry, &file_path,
    );
    let rec_errors = rule_def::self_recursion_errors(&rule_defs);
    if !rec_errors.is_empty() {
        for e in &rec_errors {
            diag_events.push(RunEvent::Diag {
                code:     format!("parse/{:?}", e.kind).to_lowercase(),
                severity: "Error".into(),
                message:  format!("at {}..{}: {}", e.byte_range.start, e.byte_range.end, e.message),
            });
        }
        diag_events.push(RunEvent::Done);
        return RunStart {
            rule_names,
            stream:     stream::iter(diag_events).boxed(),
        };
    }
    let rule_hashes = rule_hash::compute(&rule_defs, req.source.as_bytes());

    // 5. Configure run-shaped knobs from the request.
    let mut cfg: Config = (*workspace.config).clone();
    cfg.run = RunOpts { out_db: req.out_db.clone(), cache: req.cache };

    // 6. Ensure watchers per seed.
    for seed in &cfg.seeds {
        state.ensure_watcher(Arc::from(seed.slug.as_str()), seed.root.clone()).await;
    }

    // 7. Wrap the prelude diags + the per-seed drain stream + the
    //    sqlite drain into one BoxStream. The drain runs inside the
    //    same async-stream block so it sees rows before Done emits.
    let source_text = req.source.clone();
    let registry = state.registry.clone();
    let cancel = req.cancel.clone();
    let workspace_for_stream = workspace.clone();
    let state_for_stream = state.clone();
    let write_policy = req.write_policy.clone();

    let rule_names_for_stream = rule_names.clone();
    let stream = async_stream::stream! {
        let rule_names = rule_names_for_stream;
        let state = state_for_stream;
        for ev in diag_events { yield ev; }

        let relation_store = Arc::new(RelationStore::new());
        let subject_registry = Arc::new(SubjectRegistry::<RelationWake>::new());

        for (seed_idx, seed) in cfg.seeds.iter().enumerate() {
            if cancel.is_cancelled() { yield RunEvent::Done; return; }

            // Per-seed file source: BufferOverlay<DiskFileSource> from
            // the workspace, NOT a fresh DiskFileSource. Editor edits
            // shadow disk reads automatically.
            let file_source: Arc<dyn FileSource> = wrap_seed_source(
                workspace_for_stream.source.clone(),
                seed.root.clone(),
                seed.rev.clone(),
            );

            let mut op_cache = OpCache::new(cfg.run.cache);
            if cfg.run.cache {
                if let Some(db_path) = cfg.run.out_db.clone() {
                    if let Ok(disk) = DiskCache::open(&db_path) {
                        op_cache = op_cache.with_l2(Arc::new(disk));
                    }
                }
            }
            let op_cache = Arc::new(op_cache);

            let SeedRtCtx {
                ctx,
                staged_ranges,
                staged_files,
                staged_sh,
                lsp_diags,
                // SPRF-LANDMINE bind-not-discard: this MUST be bound, not
                // swept under `..`. Sink holds Weak<cell>; cell dropped =
                // splice -> invalidate is a silent no-op = stale fs cache
                // = file corruption on cross-pipe writes. (sprefa-5ld)
                _invalidator_cell,
                ..
            } = build_seed_ctx(
                &state,
                file_source.clone(),
                op_cache.clone(),
                relation_store.clone(),
                subject_registry.clone(),
                PrintBatcher::stdout(),
                write_policy.clone(),
            );
            // Phase 1a: staged write rows are auto-approved inline by
            // the sink. The buffers are still populated for downstream
            // (Phase 1b LSP code-actions, Phase 2 refactor_pending sqlite
            // table). Bound to underscore-prefixed names to keep the
            // Arcs alive for the seed's lifetime, then drop with the
            // seed's scope so per-seed buffers don't accumulate across
            // a multi-seed run.

            // Pass 1: rule defs (auto-fire arity-0 rules).
            let mut known_rules: HashSet<Arc<str>> = HashSet::new();
            let mut auto_fire: Vec<Arc<str>> = Vec::new();
            for def in rule_defs.values() {
                let Some(brace_range) = def.brace_range.clone() else { continue };
                let Some(body) = lower_rule_body(
                    &source_text, brace_range, &registry, &def.name,
                ) else { continue };
                relation_store.bind_params(def.name.clone(), def.params.clone());
                relation_store.bind_body(def.name.clone(), Arc::new(body));
                known_rules.insert(def.name.clone());
                auto_fire.push(def.name.clone());
            }

            for name in &auto_fire {
                if cancel.is_cancelled() { yield RunEvent::Done; return; }
                let params = relation_store.params(name);
                let args: Vec<RelationArg> = params.iter()
                    .map(|_| RelationArg::Literal(Arc::from("")))
                    .collect();
                let call = Pipeline::Op(Arc::new(RuleCallOp::new(name.clone(), args)));
                let pipeline = Pipeline::Seq(vec![call]);
                let expr_name = Some(name.clone());
                let mut s = pipeline.run(&ctx, seed_upstream(seed));
                while let Some(batch) = s.next().await {
                    if cancel.is_cancelled() { yield RunEvent::Done; return; }
                    for c in batch.iter() {
                        yield RunEvent::Cursor { expr_name: expr_name.clone(), cursor: c.clone() };
                    }
                }
                yield RunEvent::ExprDone { expr_name };
            }

            // Pass 2: non-rule top-level pipes.
            for (i, pipe) in parsed.pipes.iter().enumerate() {
                if cancel.is_cancelled() { yield RunEvent::Done; return; }
                let Some(head) = pipe.ops.first() else { continue };
                if &*head.name == "rule" { continue; }
                // Single-seed gate: pipes containing write ops run only on
                // seed₀. Multi-seed write coordination (shared FsInvalidator
                // + serial loop with cache eviction) is the proper fix; this
                // gate prevents the multi-splice clobber documented in
                // fixtures/golden_kitchen_sink.sprf §7.
                let has_write = pipe.ops.iter().any(|o| {
                    matches!(&*o.name, "write_cursor" | "write_file")
                });
                if has_write && seed_idx > 0 {
                    yield RunEvent::Diag {
                        code: "write/single-seed-gate".into(),
                        severity: "Info".into(),
                        message: format!(
                            "skipped pipe #{i} on seed `{}` (write op + seed_idx>0)",
                            seed.slug,
                        ),
                    };
                    continue;
                }
                let Some(p) = lower_pipe(pipe, &source_text, &registry, &known_rules) else {
                    continue;
                };
                let expr_name = Some(rule_names.get(i).cloned()
                    .unwrap_or_else(|| Arc::<str>::from("")));
                let expr_name = expr_name.filter(|s| !s.is_empty());
                let mut s = p.run(&ctx, seed_upstream(seed));
                while let Some(batch) = s.next().await {
                    if cancel.is_cancelled() { yield RunEvent::Done; return; }
                    for c in batch.iter() {
                        yield RunEvent::Cursor { expr_name: expr_name.clone(), cursor: c.clone() };
                    }
                }
                yield RunEvent::ExprDone { expr_name };
            }

            // Drain per-seed lsp[severity] diagnostics emitted during the
            // pipeline runs. Severity strings match the LspSeverity enum
            // (Error/Warning/Hint/Info) so SSE consumers can color them.
            let drained: Vec<LspDiagEffect> = {
                let mut g = lsp_diags.lock().expect("lsp diag buffer poisoned");
                std::mem::take(&mut *g)
            };
            for d in drained {
                let sev = match d.severity {
                    pipeline::effects::LspSeverity::Error   => "Error",
                    pipeline::effects::LspSeverity::Warning => "Warning",
                    pipeline::effects::LspSeverity::Hint    => "Hint",
                    pipeline::effects::LspSeverity::Info    => "Info",
                };
                let loc = d.file
                    .as_ref()
                    .map(|p| format!("{} ", p.display()))
                    .unwrap_or_default();
                let code = d.code.as_deref().unwrap_or("lsp/diag").to_string();
                let mut msg = format!("{loc}{}..{}: {}", d.byte_range.start, d.byte_range.end, d.message);
                if let Some(h) = d.hint.as_deref() {
                    msg.push_str(" (hint: ");
                    msg.push_str(h);
                    msg.push(')');
                }
                yield RunEvent::Diag { code, severity: sev.into(), message: msg };
            }

            // Drain per-seed staged writes (Phase 1 staging cockpit).
            // Each row carries a content-stable id; surface as Info diags
            // so the CLI / SSE consumer renders them in the same panel
            // as parse + lsp diagnostics. Future flag `--approve <ids>`
            // will let the user re-run with only listed ids splicing.
            let drained_ranges: Vec<StagedWriteRangeRow> = {
                let mut g = staged_ranges.lock().expect("staged write_range buffer poisoned");
                std::mem::take(&mut *g)
            };
            for row in drained_ranges {
                let (severity, code, outcome) =
                    decision_render(row.decision, &row.result, "staged/write_range");
                let msg = format!(
                    "[id={}] {} {}..{} +{}B mode={} -> {}",
                    row.id,
                    row.effect.file.display(),
                    row.effect.byte_range.start,
                    row.effect.byte_range.end,
                    row.effect.new_bytes.len(),
                    row.effect.mode.as_str(),
                    outcome,
                );
                yield RunEvent::Diag { code, severity, message: msg };
            }
            let drained_files: Vec<StagedWriteFileRow> = {
                let mut g = staged_files.lock().expect("staged write_file buffer poisoned");
                std::mem::take(&mut *g)
            };
            for row in drained_files {
                let (severity, code, outcome) =
                    decision_render(row.decision, &row.result, "staged/write_file");
                let msg = format!(
                    "[id={}] {} +{}B -> {}",
                    row.id,
                    row.path.display(),
                    row.bytes.len(),
                    outcome,
                );
                yield RunEvent::Diag { code, severity, message: msg };
            }
            // Drain per-seed staged sh invocations. Each row carries the
            // rendered cmd_line, the decision (approved / dry-run / rejected),
            // and the response (exit + stdout/stderr lengths). Surface as
            // Info diags so the operator sees what bash got handed.
            let drained_sh: Vec<StagedShRow> = {
                let mut g = staged_sh.lock().expect("staged sh buffer poisoned");
                std::mem::take(&mut *g)
            };
            for row in drained_sh {
                let (severity, code, outcome) = match (row.decision, &row.result) {
                    (pipeline::effects::ShDecision::Approved, Some(Ok(r))) => (
                        if r.exit_code == 0 { "Info".to_string() } else { "Warning".to_string() },
                        "staged/sh".to_string(),
                        format!("exit={} stdout={}B stderr={}B in {}ms",
                            r.exit_code, r.stdout.len(), r.stderr.len(), r.elapsed_ms),
                    ),
                    (pipeline::effects::ShDecision::Approved, Some(Err(e))) => (
                        "Warning".to_string(), "staged/sh".to_string(),
                        format!("err: {e}"),
                    ),
                    (pipeline::effects::ShDecision::DryRun, _) => (
                        "Info".to_string(), "staged/sh".to_string(),
                        "dry-run".to_string(),
                    ),
                    (pipeline::effects::ShDecision::Rejected, _) => (
                        "Warning".to_string(), "staged/sh-rejected".to_string(),
                        "rejected".to_string(),
                    ),
                    (_, None) => (
                        "Warning".to_string(), "staged/sh".to_string(),
                        "no result captured".to_string(),
                    ),
                };
                let msg = format!(
                    "[id={}] policy={} `{}` -> {}",
                    row.id,
                    row.effect.policy.as_str(),
                    row.effect.cmd_line,
                    outcome,
                );
                yield RunEvent::Diag { code, severity, message: msg };
            }
        }

        // SQLite drain — runs after all seeds. Out_db None ⇒ skip.
        if let Some(db_path) = cfg.run.out_db.clone() {
            let sprf_path = req.source_path.clone()
                .unwrap_or_else(|| PathBuf::from("memory.sprf"));
            // dump_to_sqlite returns Box<dyn StdError>; coerce to a
            // String here so the future stays Send. Any sqlx error
            // inside the box is `Send`, but the trait object isn't
            // declared `+ Send` and futures::stream::BoxStream needs
            // Send to box.
            let drain_res: Result<(), String> = dump_to_sqlite(
                &relation_store, &db_path, &sprf_path, &rule_hashes, &cfg.run,
            ).await.map_err(|e| e.to_string());
            if let Err(msg) = drain_res {
                yield RunEvent::Diag {
                    code:     "server/sqlite-drain".into(),
                    severity: "Error".into(),
                    message:  msg,
                };
            }
        }

        yield RunEvent::Done;
    };

    RunStart { rule_names, stream: stream.boxed() }
}

/// Bundle returned by [`build_seed_ctx`] — the per-seed `RtCtx` plus
/// staging handles the host needs to inspect / drive after the pipeline
/// stream completes (Phase 1 write-effect staging loop). Callers that
/// don't care about staging just take `.ctx` and ignore the rest.
pub(crate) struct SeedRtCtx {
    pub ctx:            RtCtx,
    pub write_approval: Arc<SubjectRegistry<WriteApproval>>,
    pub staged_ranges:  Arc<Mutex<Vec<StagedWriteRangeRow>>>,
    pub staged_files:   Arc<Mutex<Vec<StagedWriteFileRow>>>,
    pub staged_sh:      Arc<Mutex<Vec<StagedShRow>>>,
    /// Drained at end-of-seed by `/run` (RunEvent::Diag) and by the LSP
    /// hover/save path (publishDiagnostics). The `lsp[severity]` op
    /// pushes one entry per cursor.
    pub lsp_diags:      Arc<Mutex<Vec<LspDiagEffect>>>,
    /// Strong owner of the fs-invalidation cell. The staging sinks hold a
    /// Weak ref to this cell to avoid a refcount cycle; this field keeps
    /// the cell alive for the lifetime of the ctx so splice -> invalidate
    /// stays wired. Drops with SeedRtCtx, breaking the cycle cleanly.
    _invalidator_cell:  Arc<OnceLock<RtCtx>>,
}

/// Single source-of-truth for the per-seed effect registration list.
/// Both `/run` (this file) and the LSP hover render path
/// (`backend::render_enriched_hover`) build their RtCtx through here so
/// adding or renaming an effect kind only happens in one place.
///
/// Caller supplies: the file source (per-seed reader stack), the cache
/// (shared `state.cache` for live LSP, fresh per-run for /run), and the
/// relation store + subject registry (caller controls drain ordering).
///
/// Phase 1 staging: this function also allocates a fresh
/// `SubjectRegistry<WriteApproval>` plus the staged-write buffers and
/// wires `WriteRangeBatcher::stage_and_approve` /
/// `WriteFileBatcher::stage_and_approve` so every write effect routes
/// through the staging sink. Auto-approve happens inline in the sink
/// (Phase 1a). Phase 1b will add an LSP-driven approver that holds the
/// staging buffer open until a code-action resolves the key.
pub(crate) fn build_seed_ctx(
    state:            &ServerState,
    file_source:      Arc<dyn FileSource>,
    op_cache:         Arc<OpCache>,
    relation_store:   Arc<RelationStore>,
    subject_registry: Arc<SubjectRegistry<RelationWake>>,
    print:            PrintBatcher,
    write_policy:     Arc<WritePolicy>,
) -> SeedRtCtx {
    let write_approval = Arc::new(SubjectRegistry::<WriteApproval>::new());

    // Cell + closure that lets the staging sinks invalidate the "fs"
    // domain cache after each successful splice. The cell stays empty
    // until the ctx is built below; sinks pick up the live ctx the first
    // time they fire. Without this, two pipes that touch the same file
    // (or two ops in one pipe) hit the ReadBytesEffect cache for the
    // pre-splice bytes and corrupt the post-splice file with stale
    // byte_ranges. (sprefa-5ld)
    //
    // The closure holds a Weak reference to the cell, NOT a Strong one.
    // A Strong ref would form a cycle (closure -> cell -> ctx clone ->
    // registry -> sink -> closure) and leak the ctx forever. With Weak,
    // the cell is kept alive by the SeedRtCtx field below; once SeedRtCtx
    // drops, the cell drops, the ctx clone in it drops, registry refcount
    // hits zero, sinks drop, closure drops, Weak goes invalid (no-op).
    let invalidator_cell: Arc<OnceLock<RtCtx>> = Arc::new(OnceLock::new());
    // SPRF-LANDMINE weak-not-strong: closure MUST hold Weak, not Arc, on
    // the cell. Strong forms a cycle (closure -> cell -> ctx -> registry
    // -> sink -> closure) and leaks the ctx every seed iteration forever.
    // (sprefa-5ld)
    let weak_cell = Arc::downgrade(&invalidator_cell);
    let invalidate_fs: pipeline::effects::FsInvalidator = Arc::new(move || {
        if let Some(cell) = weak_cell.upgrade() {
            if let Some(c) = cell.get() {
                c.invalidate_domain("fs");
            }
        }
    });

    let (write_range_batcher, staged_ranges) = WriteRangeBatcher::stage_and_approve(
        write_approval.clone(), write_policy.clone(), invalidate_fs.clone(),
    );
    let (write_file_batcher, staged_files) = WriteFileBatcher::stage_and_approve(
        write_approval.clone(), write_policy, invalidate_fs,
    );
    let (lsp_diag_batcher, lsp_diags) = LspDiagBatcher::buffer();
    let (sh_batcher, staged_sh) = ShBatcher::new();

    let ctx = RtCtxBuilder::new()
        .with_gen_counter(state.gen.clone())
        .with_store(relation_store.clone())
        .with_store(subject_registry.clone())
        .with_store(write_approval.clone())
        .with_store(op_cache)
        .register_pure::<FsListFilesEffect, _>(
            1024,
            FsListFilesBatcher::new(file_source.clone()),
        )
        .register_pure::<ReadBytesEffect, _>(
            65_536,
            ReadBytesBatcher::new(file_source.clone()),
        )
        .register::<ReadBytesBatchEffect, _>(
            ReadBytesBatchBatcher::new(file_source),
        )
        .register::<AstParseEffect, _>(state.ast_parse_batcher.clone())
        .register::<PrintEffect, _>(print)
        .register::<Yield<RelationWake>, _>(YieldBatcher::new(subject_registry.clone()))
        .register::<Yield<WriteApproval>, _>(YieldBatcher::new(write_approval.clone()))
        .register::<WriteEffect, _>(
            WriteBatcher::new(relation_store, subject_registry),
        )
        .register::<WriteFileEffect, _>(write_file_batcher)
        .register::<WriteRangeEffect, _>(write_range_batcher)
        .register::<LspDiagEffect, _>(lsp_diag_batcher)
        .register::<ShEffect, _>(sh_batcher)
        .build();

    // Fill the cell so the staging sinks can reach the live ctx for
    // domain-invalidation. Set fails only if already populated, which
    // can't happen because the cell is freshly constructed above.
    let _ = invalidator_cell.set(ctx.clone());

    SeedRtCtx {
        ctx, write_approval, staged_ranges, staged_files, staged_sh, lsp_diags,
        _invalidator_cell: invalidator_cell,
    }
}

// ---------------------------------------------------------------------------
// Wrappers

/// Wrap an existing workspace overlay so the inner source is honored
/// for the per-seed (root, rev). We can't share one BufferOverlay across
/// seeds with different roots, so each seed gets a fresh
/// `DiskFileSource(seed.root, seed.rev)` wrapped in an overlay that
/// shares the workspace's overlay map for editor buffers.
///
/// Cheap workaround: each seed builds its own `DiskFileSource` and
/// wraps it in a per-seed overlay seeded from the workspace overlay's
/// current state. This loses the live overlay link if the workspace
/// overlay updates mid-drain, but a drain is a snapshot anyway.
fn wrap_seed_source(
    _ws_overlay: Arc<BufferOverlay<DiskFileSource>>,
    root: PathBuf,
    rev:  String,
) -> Arc<dyn FileSource> {
    // For now, single-seed workspaces just expose the workspace overlay.
    // Multi-seed runs fall back to fresh DiskFileSource per seed (no
    // overlay) — multi-repo overlays are out of scope for v3-now.
    build_seed_source(root, rev)
}

/// rev-aware FileSource dispatch. `wt` (and `:wt`) → worktree;
/// any other revspec → git2-backed `GitFileSource`. If the git open
/// or rev-resolve fails the worktree source is returned as a fallback
/// so a misconfigured rev surfaces as empty/wrong-content rather than
/// blowing up the run; the warning is logged.
pub fn build_seed_source(root: PathBuf, rev: String) -> Arc<dyn FileSource> {
    let label = rev.strip_prefix(':').unwrap_or(&rev);
    if label == "wt" || label == "unstaged" || label == "staged" {
        return Arc::new(DiskFileSource::new(root, rev));
    }
    match GitFileSource::open(root.clone(), &rev) {
        Ok(src) => Arc::new(src),
        Err(e) => {
            tracing::warn!(
                target: "sprefa::reader",
                root = %root.display(),
                rev = %rev,
                error = %e,
                "GitFileSource.open failed; falling back to DiskFileSource"
            );
            Arc::new(DiskFileSource::new(root, rev))
        }
    }
}

fn seed_upstream(seed: &crate::config::Seed) -> futures::stream::BoxStream<'static, Arc<[Cursor]>> {
    let mut c = Cursor::default();
    c.repo = Arc::from(seed.slug.as_str());
    c.rev = Arc::from(seed.rev.as_str());
    Box::pin(stream::iter(vec![Arc::<[Cursor]>::from(vec![c])]))
}

// ---------------------------------------------------------------------------
// Lowering helpers (verbatim port from sprefa-run.rs body)

fn lower_pipe(
    pipe:        &Pipe,
    source:      &str,
    registry:    &Registry,
    known_rules: &HashSet<Arc<str>>,
) -> Option<Pipeline> {
    let mut steps: Vec<Pipeline> = Vec::new();
    for step in &pipe.steps {
        match step {
            PipeStep::Op(inv) => {
                if let Some(p) = lower_op_invocation(inv, source, registry, known_rules) {
                    steps.push(p);
                }
            }
            PipeStep::Fork(arms) => {
                let lowered_arms: Vec<Pipeline> = arms.iter()
                    .filter_map(|a| lower_pipe(a, source, registry, known_rules))
                    .collect();
                if !lowered_arms.is_empty() {
                    steps.push(Pipeline::Fork(lowered_arms));
                }
            }
        }
    }
    if steps.is_empty() { None } else { Some(Pipeline::Seq(steps)) }
}

fn lower_op_invocation(
    inv:         &OpInvocation,
    source:      &str,
    registry:    &Registry,
    known_rules: &HashSet<Arc<str>>,
) -> Option<Pipeline> {
    if !inv.predicate && known_rules.contains(&inv.name) {
        let args = lower_call_args(inv, source, registry);
        return Some(Pipeline::Op(Arc::new(RuleCallOp::new(inv.name.clone(), args))));
    }
    if inv.predicate && known_rules.contains(&inv.name) {
        let values = lower_predicate_values(inv, source, registry);
        return match RulePredicateOp::classify(inv.name.clone(), values) {
            Ok(op) => Some(Pipeline::Op(Arc::new(op))),
            Err(_errs) => None,
        };
    }
    let lookup_name: Arc<str> = if inv.predicate {
        Arc::from(format!("{}?", inv.name))
    } else {
        inv.name.clone()
    };
    let mut diags = Vec::new();
    match registry.build_from_node(&lookup_name, inv.node(), source.as_bytes(), &mut diags) {
        Some(Ok(op)) => Some(Pipeline::Op(op)),
        _ => None,
    }
}

fn lower_predicate_values(
    inv:      &OpInvocation,
    source:   &str,
    registry: &Registry,
) -> Vec<Value> {
    let Some(paren) = inv.node().child_by_field_name("paren") else {
        return Vec::new();
    };
    let mut diags = Vec::new();
    lower_paren_slot(paren, source.as_bytes(), registry, &mut diags)
}

fn lower_call_args(
    inv:      &OpInvocation,
    source:   &str,
    registry: &Registry,
) -> Vec<RelationArg> {
    let Some(paren) = inv.node().child_by_field_name("paren") else {
        return Vec::new();
    };
    let mut diags = Vec::new();
    let values = lower_paren_slot(paren, source.as_bytes(), registry, &mut diags);
    let mut args: Vec<RelationArg> = Vec::with_capacity(values.len());
    for v in values {
        match v {
            Value::Term { name, mode: TermMode::Read } => {
                args.push(RelationArg::Capture(name));
            }
            Value::Atom(s) | Value::Str(s) => args.push(RelationArg::Literal(s)),
            Value::Int(n)   => args.push(RelationArg::Literal(Arc::from(n.to_string()))),
            Value::Float(n) => args.push(RelationArg::Literal(Arc::from(n.to_string()))),
            Value::Term { mode: TermMode::Unbound, .. } => {}
            Value::Op(_) => {}
        }
    }
    args
}

fn lower_rule_body(
    source:      &str,
    brace_range: std::ops::Range<usize>,
    registry:    &Registry,
    name:        &Arc<str>,
) -> Option<Pipeline> {
    let brace_src = source.get(brace_range)?;
    let (sub, _errs) = host_parse_with_injections(
        brace_src,
        Arc::from(Path::new("<rule-body>")),
        &pipeline::op_languages::language_of,
    );
    let mut body_known_rules: HashSet<Arc<str>> = HashSet::new();
    body_known_rules.insert(name.clone());
    let lowered_pipes: Vec<Pipeline> = sub.pipes.iter()
        .filter_map(|p| lower_pipe(p, brace_src, registry, &body_known_rules))
        .collect();
    match lowered_pipes.len() {
        0 => None,
        1 => Some(lowered_pipes.into_iter().next().unwrap()),
        _ => Some(Pipeline::Fork(lowered_pipes)),
    }
}

// ---------------------------------------------------------------------------
// SQLite drain (verbatim port from sprefa-run.rs body)

fn db_table_prefix(sprf_path: &Path) -> String {
    sprf_path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sprefa".to_string())
}

fn sprf_norm(value: &str) -> String {
    value.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

#[derive(Debug, Clone, Copy)]
enum MetaDiff { New, SchemaChanged, ExtractChanged, Unchanged }

impl MetaDiff {
    fn label(self) -> &'static str {
        match self {
            MetaDiff::New            => "New",
            MetaDiff::SchemaChanged  => "SchemaChanged",
            MetaDiff::ExtractChanged => "ExtractChanged",
            MetaDiff::Unchanged      => "Unchanged",
        }
    }
}

async fn dump_to_sqlite(
    store:       &RelationStore,
    db_path:     &Path,
    sprf_path:   &Path,
    rule_hashes: &HashMap<Arc<str>, RuleHashes>,
    run:         &RunOpts,
) -> Result<(), Box<dyn std::error::Error>> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
    use std::str::FromStr;

    let snapshot = store.rule_rows_snapshot();
    if snapshot.is_empty() {
        return Ok(());
    }

    let prefix = db_table_prefix(sprf_path);
    let url = format!("sqlite://{}", db_path.display());
    let opts = SqliteConnectOptions::from_str(&url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS sprf_meta (
            rule_name TEXT PRIMARY KEY,
            schema_hash TEXT NOT NULL,
            extract_hash TEXT NOT NULL,
            last_scanned_at TEXT NOT NULL
        )"#
    ).execute(&pool).await?;

    for (rule_name, rows) in &snapshot {
        let table = format!("{prefix}__{rule_name}");
        if !is_safe_ident(&table) { continue; }

        let curr = rule_hashes.get(rule_name);
        let prev: Option<(String, String)> = match curr {
            Some(_) => sqlx::query_as::<_, (String, String)>(
                "SELECT schema_hash, extract_hash FROM sprf_meta WHERE rule_name = ?",
            ).bind(rule_name.as_ref()).fetch_optional(&pool).await?,
            None => None,
        };
        let diff: MetaDiff = match (run.cache, curr, prev) {
            (false, _, _)         => MetaDiff::SchemaChanged,
            (true, None, _)       => MetaDiff::SchemaChanged,
            (true, Some(_), None) => MetaDiff::New,
            (true, Some(c), Some((ps, pe))) => {
                if c.schema_hash != ps        { MetaDiff::SchemaChanged }
                else if c.extract_hash != pe  { MetaDiff::ExtractChanged }
                else                          { MetaDiff::Unchanged }
            }
        };
        if matches!(diff, MetaDiff::Unchanged) {
            tracing::info!(target: "sprefa::server", table = %table, "sqlite: cached, skipped");
            continue;
        }

        let mut columns: Vec<Arc<str>> = Vec::new();
        let mut seen: BTreeSet<Arc<str>> = BTreeSet::new();
        for row in rows {
            for (k, _) in row {
                if seen.insert(k.clone()) { columns.push(k.clone()); }
            }
        }
        let safe_columns: Vec<&Arc<str>> = columns.iter().filter(|c| is_safe_ident(c)).collect();

        let full_rebuild = matches!(diff, MetaDiff::SchemaChanged | MetaDiff::New);
        if full_rebuild {
            for stmt in [
                format!(r#"DROP TRIGGER IF EXISTS "{table}_ai""#),
                format!(r#"DROP TRIGGER IF EXISTS "{table}_ad""#),
                format!(r#"DROP TRIGGER IF EXISTS "{table}_au""#),
                format!(r#"DROP TABLE IF EXISTS "{table}_fts""#),
                format!(r#"DROP TABLE IF EXISTS "{table}""#),
            ] {
                sqlx::query(&stmt).execute(&pool).await?;
            }
        } else {
            sqlx::query(&format!(r#"DELETE FROM "{table}""#)).execute(&pool).await?;
        }

        let mut col_decls: Vec<String> = Vec::new();
        for c in &safe_columns {
            col_decls.push(format!(r#""{c}" TEXT"#));
            col_decls.push(format!(r#""{c}_norm" TEXT"#));
        }
        let cols_sql = if col_decls.is_empty() { String::new() } else { format!(", {}", col_decls.join(", ")) };

        if full_rebuild {
            let create_sql = format!(
                r#"CREATE TABLE "{table}" (id INTEGER PRIMARY KEY AUTOINCREMENT{cols_sql})"#
            );
            sqlx::query(&create_sql).execute(&pool).await?;

            for c in &safe_columns {
                let idx_sql = format!(
                    r#"CREATE INDEX IF NOT EXISTS "{table}_{c}_norm_idx" ON "{table}"("{c}_norm")"#
                );
                sqlx::query(&idx_sql).execute(&pool).await?;
            }

            if !safe_columns.is_empty() {
                let fts_cols: Vec<String> = safe_columns.iter()
                    .map(|c| format!(r#""{c}_norm""#)).collect();
                let fts_create = format!(
                    r#"CREATE VIRTUAL TABLE "{table}_fts" USING fts5({}, content='{table}', content_rowid='id', tokenize='trigram')"#,
                    fts_cols.join(", "),
                );
                sqlx::query(&fts_create).execute(&pool).await?;

                let cols_csv = fts_cols.join(", ");
                let new_csv = safe_columns.iter()
                    .map(|c| format!(r#"new."{c}_norm""#))
                    .collect::<Vec<_>>().join(", ");
                let length_guard = safe_columns.iter()
                    .map(|c| format!(r#"length(new."{c}_norm") < 1000"#))
                    .collect::<Vec<_>>().join(" AND ");
                let ai = format!(
                    r#"CREATE TRIGGER "{table}_ai" AFTER INSERT ON "{table}" BEGIN
                        INSERT INTO "{table}_fts"(rowid, {cols_csv})
                        SELECT new.id, {new_csv} WHERE {length_guard};
                    END"#
                );
                sqlx::query(&ai).execute(&pool).await?;

                let old_csv = safe_columns.iter()
                    .map(|c| format!(r#"old."{c}_norm""#))
                    .collect::<Vec<_>>().join(", ");
                let ad = format!(
                    r#"CREATE TRIGGER "{table}_ad" AFTER DELETE ON "{table}" BEGIN
                        INSERT INTO "{table}_fts"("{table}_fts", rowid, {cols_csv}) VALUES('delete', old.id, {old_csv});
                    END"#
                );
                sqlx::query(&ad).execute(&pool).await?;

                let au = format!(
                    r#"CREATE TRIGGER "{table}_au" AFTER UPDATE ON "{table}" BEGIN
                        INSERT INTO "{table}_fts"("{table}_fts", rowid, {cols_csv}) VALUES('delete', old.id, {old_csv});
                        INSERT INTO "{table}_fts"(rowid, {cols_csv})
                        SELECT new.id, {new_csv} WHERE {length_guard};
                    END"#
                );
                sqlx::query(&au).execute(&pool).await?;
            }
        }

        if safe_columns.is_empty() {
            for _ in rows {
                sqlx::query(&format!(r#"INSERT INTO "{table}" DEFAULT VALUES"#))
                    .execute(&pool).await?;
            }
        } else {
            let col_list = safe_columns.iter()
                .flat_map(|c| [format!(r#""{c}""#), format!(r#""{c}_norm""#)])
                .collect::<Vec<_>>().join(", ");
            let placeholders = (0..safe_columns.len() * 2).map(|_| "?")
                .collect::<Vec<_>>().join(", ");
            let insert_sql = format!(
                r#"INSERT INTO "{table}" ({col_list}) VALUES ({placeholders})"#
            );
            for row in rows {
                let lookup: HashMap<&Arc<str>, &Arc<str>> =
                    row.iter().map(|(k, v)| (k, v)).collect();
                let mut q = sqlx::query(&insert_sql);
                for c in &safe_columns {
                    let raw: String = lookup.get(c).map(|a| a.to_string()).unwrap_or_default();
                    let norm = sprf_norm(&raw);
                    q = q.bind(raw).bind(norm);
                }
                q.execute(&pool).await?;
            }
        }

        if let Some(c) = curr {
            sqlx::query(
                r#"INSERT INTO sprf_meta (rule_name, schema_hash, extract_hash, last_scanned_at)
                   VALUES (?, ?, ?, datetime('now'))
                   ON CONFLICT(rule_name) DO UPDATE SET
                     schema_hash = excluded.schema_hash,
                     extract_hash = excluded.extract_hash,
                     last_scanned_at = excluded.last_scanned_at"#
            )
            .bind(rule_name.as_ref())
            .bind(&c.schema_hash)
            .bind(&c.extract_hash)
            .execute(&pool).await?;
        }

        tracing::info!(
            target: "sprefa::server",
            table = %table,
            rows = rows.len(),
            cols = safe_columns.len(),
            diff = diff.label(),
            "sqlite.drained",
        );
    }

    pool.close().await;
    Ok(())
}

fn is_safe_ident(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() { return false; }
    if bytes[0].is_ascii_digit() { return false; }
    bytes.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
}
