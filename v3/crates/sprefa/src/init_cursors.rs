//! `init_cursors` — single evaluator entry point.
//!
//! # Role
//! Drive a `Vec<CursorExpr>` against a `Store` + `Reader` + `Writer` and
//! surface the run as a `BoxStream<RunEvent>`. Consumers (CLI,
//! DocSession, stress tests) read the stream, handle `MutationPrompt`
//! events via the handler task they spawned beforehand, and collect
//! results until `RunEvent::Done`.
//!
//! # Ownership + lifecycle
//! `InitInputs` is consumed by value. The returned stream owns the work;
//! dropping it cancels outstanding ops via `inp.cancel`. Diagnostics the
//! pipeline emits are captured through a sink on `OpCtx.diags` and
//! surface as `RunEvent::Diag`.
//!
//! # Who mutates
//! The stream body appends to a per-run diag collector and a per-expr
//! cursor list. `Store::flush_batch` runs once after every expr drains,
//! folding captures into per-expr rows. `Store::register_expr_schema`
//! runs once per named expr before its pipeline fires.
//!
//! # Failure modes
//! Cancel wins at every await gate: the `select! { biased; cancel | work }`
//! shortcuts to `RunEvent::Done` without flush. Store errors bubble as
//! `RunEvent::Diag` and the run continues.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_core::stream::BoxStream;
use futures_util::stream::StreamExt;
use tracing::{info_span, Instrument};

use crate::types::{
    Capture, Cursor, CursorExpr, OpId, ParseSeg, ParseSite, RunEvent, RunId,
};
use crate::diagnostic::Diagnostic;
use crate::config::Config;
use crate::reader::Reader;
use crate::writer::Writer;
use crate::op::{DiagSink, EventSink, OpCtx};
use crate::result_store::ResultStore;
use crate::mutations::MutationRequest;
use crate::store::{
    Batch, CaptureColumn, ExprBatch, ExprTableSpec, RowInsert, Store,
};

/// Full plumbing bundle for a single evaluator run.
pub struct InitInputs {
    pub exprs:        Vec<CursorExpr>,
    /// Optional dependency-ordered level assignment. `levels[i]` is a
    /// list of indices into `exprs`. Exprs within a level run
    /// concurrently; level N+1 starts only after level N drains. `None`
    /// falls back to sequential (one expr per level) so callers without
    /// DAG info keep today's semantics.
    pub expr_levels: Option<Vec<Vec<usize>>>,
    pub config:       Arc<Config>,
    pub store:        Arc<dyn Store>,
    pub reader:       Arc<dyn Reader>,
    pub writer:       Arc<dyn Writer>,
    pub mutations_tx: tokio::sync::mpsc::Sender<MutationRequest>,
    pub cancel:       tokio_util::sync::CancellationToken,
    pub run_id:       RunId,
    pub scanner_hash: Arc<str>,
    /// Optional shared row store. `Some` when the caller is a scan-loop
    /// driver that needs check_scan_pointers to see the same rows after
    /// init_cursors drains. `None` falls back to a fresh store per run.
    pub result_store: Option<Arc<ResultStore>>,
}

/// Collected outcome of a run for in-process consumers (DocSession, tests).
pub struct RunReport {
    pub cursors_by_expr: HashMap<Arc<str>, Vec<Cursor>>,
    pub diags:           Vec<Box<dyn Diagnostic>>,
}

/// Drain a run stream into a `RunReport`. Blocks until `RunEvent::Done`.
pub async fn collect_run_report(mut s: BoxStream<'static, RunEvent>) -> RunReport {
    let mut cursors_by_expr: HashMap<Arc<str>, Vec<Cursor>> = HashMap::new();
    let mut diags: Vec<Box<dyn Diagnostic>> = Vec::new();
    while let Some(ev) = s.next().await {
        match ev {
            RunEvent::Cursor { expr_name, cursor } => {
                let key = expr_name.clone().unwrap_or_else(|| Arc::from(""));
                cursors_by_expr.entry(key).or_default().push(cursor);
            }
            RunEvent::ExprDone { .. }       => {}
            RunEvent::Diag { diag }         => diags.push(diag),
            RunEvent::MutationPrompt { .. } => {}
            RunEvent::Done                  => break,
        }
    }
    RunReport { cursors_by_expr, diags }
}

pub fn init_cursors(inp: InitInputs) -> BoxStream<'static, RunEvent> {
    let (tx, rx) = tokio::sync::mpsc::channel::<RunEvent>(
        (inp.config.runtime.buffer_size as usize).max(16),
    );

    tokio::spawn(async move {
        let InitInputs {
            exprs, expr_levels, config: _config, store, reader, writer,
            mutations_tx, cancel, run_id, scanner_hash, result_store,
        } = inp;
        let shared_result_store = result_store;

        // Register schemas for every named expr up front.
        let mut spec_by_name: HashMap<Arc<str>, ExprTableSpec> = HashMap::new();
        async {
            for expr in &exprs {
                let Some(name) = expr.name.clone() else { continue };
                let spec = build_expr_table_spec(name.clone(), expr);
                let reg_fut = store.register_expr_schema(clone_spec(&spec))
                    .instrument(info_span!("store.register_expr_schema", expr = %name));
                if let Err(e) = reg_fut.await {
                    let _ = tx.send(RunEvent::Diag { diag: Box::new(e) }).await;
                } else {
                    spec_by_name.insert(name, spec);
                }
            }
        }.instrument(info_span!("runner.register_schemas", n_exprs = exprs.len())).await;

        // Default sequential schedule when caller did not supply a DAG.
        let levels: Vec<Vec<usize>> = expr_levels
            .unwrap_or_else(|| (0..exprs.len()).map(|i| vec![i]).collect());

        let exprs_arc: Vec<Arc<CursorExpr>> =
            exprs.into_iter().map(Arc::new).collect();

        let all_cursors: Arc<Mutex<HashMap<Arc<str>, Vec<Cursor>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let n_levels = levels.len();
        'level_loop: for (level_ix, level) in levels.into_iter().enumerate() {
            if cancel.is_cancelled() { break; }

            let mut handles: Vec<tokio::task::JoinHandle<()>> =
                Vec::with_capacity(level.len());
            for idx in level {
                let Some(expr) = exprs_arc.get(idx).cloned() else { continue };

                let tx            = tx.clone();
                let reader        = reader.clone();
                let writer        = writer.clone();
                let config        = _config.clone();
                let store         = store.clone();
                let mutations_tx  = mutations_tx.clone();
                let cancel        = cancel.clone();
                let shared_rs     = shared_result_store.clone();
                let all_cursors   = all_cursors.clone();
                let expr_name_dbg: Arc<str> = expr.name.clone().unwrap_or_else(|| Arc::from(""));

                let drain_span = info_span!(
                    "runner.drain_expr.spawn",
                    expr = %expr_name_dbg,
                    level = level_ix,
                );
                handles.push(tokio::spawn(async move {
                    drain_one_expr(
                        expr, tx, reader, writer, config, store,
                        mutations_tx, cancel, run_id, shared_rs, all_cursors,
                    ).await;
                }.instrument(drain_span)));
            }
            for h in handles {
                let _ = h.await;
            }
            if cancel.is_cancelled() { break 'level_loop; }
        }

        if !cancel.is_cancelled() {
            let all = {
                let _s = info_span!("runner.build_batch.snapshot").entered();
                all_cursors.lock().unwrap().clone()
            };
            let batch = {
                let _s = info_span!("runner.build_batch", n_exprs = all.len()).entered();
                build_batch(&all, &spec_by_name, &scanner_hash)
            };
            if !batch.per_expr.is_empty() {
                let timing = std::env::var_os("SPREFA_TIMING").is_some();
                let t0 = std::time::Instant::now();
                let row_count: usize = batch.per_expr.iter().map(|e| e.rows.len()).sum();
                let flush_fut = store.flush_batch(batch).instrument(info_span!(
                    "store.flush_batch",
                    n_rows = row_count,
                    n_exprs = spec_by_name.len(),
                ));
                if let Err(e) = flush_fut.await {
                    let _ = tx.send(RunEvent::Diag { diag: Box::new(e) }).await;
                }
                if timing {
                    eprintln!(
                        "[timing] flush_batch             {:>6} ms ({} rows across {} exprs)",
                        t0.elapsed().as_millis(),
                        row_count,
                        spec_by_name.len(),
                    );
                }
            }
        }

        let _ = tx.send(RunEvent::Done).await;
    });

    Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
}

// ---------------------------------------------------------------------------
// Per-expr drain
// ---------------------------------------------------------------------------

/// Drive one expr's pipeline to completion. Emits Cursor / Diag / ExprDone
/// events on `tx`. Runs in its own spawned task so sibling exprs inside
/// the same DAG level can race without blocking each other on io.
async fn drain_one_expr(
    expr:          Arc<CursorExpr>,
    tx:            tokio::sync::mpsc::Sender<RunEvent>,
    reader:        Arc<dyn Reader>,
    writer:        Arc<dyn Writer>,
    config:        Arc<Config>,
    store:         Arc<dyn Store>,
    mutations_tx:  tokio::sync::mpsc::Sender<MutationRequest>,
    cancel:        tokio_util::sync::CancellationToken,
    run_id:        RunId,
    shared_rs:     Option<Arc<ResultStore>>,
    all_cursors:   Arc<Mutex<HashMap<Arc<str>, Vec<Cursor>>>>,
) {
    if cancel.is_cancelled() { return; }

    let diag_bucket: Arc<Mutex<Vec<Box<dyn Diagnostic>>>> =
        Arc::new(Mutex::new(Vec::new()));
    let sink_bucket = diag_bucket.clone();

    let (fresh_store, xref_seen) = OpCtx::fresh_xref_state();
    let result_store = shared_rs.unwrap_or(fresh_store);
    let rt = crate::effects::build_default_rt(&reader);
    let ctx = OpCtx {
        run_id,
        op_id:  OpId(0),
        reader,
        writer,
        config,
        rt,
        diags: DiagSink(Arc::new(move |d| {
            sink_bucket.lock().unwrap().push(d);
        })),
        events: EventSink(Arc::new(|_| {})),
        result_store,
        xref_seen,
        store,
        mutations:    mutations_tx,
        cancel:       cancel.clone(),
        expr_name:    expr.name.clone(),
        current_site: synthetic_site(),
    };

    let empty: BoxStream<'static, Arc<[Cursor]>> =
        futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();

    let expr_name_dbg: Arc<str> = expr.name.clone().unwrap_or_else(|| Arc::from(""));

    let mut batches = info_span!("runner.pipeline_run", expr = %expr_name_dbg)
        .in_scope(|| expr.pipeline.run(empty, ctx));
    let expr_name_key = expr.name.clone().unwrap_or_else(|| Arc::from(""));

    let mut n_batches: u64 = 0;
    let mut n_cursors: u64 = 0;

    'drain: loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => break 'drain,
            next = batches.next() => match next {
                Some(batch) => {
                    n_batches += 1;
                    for c in batch.iter() {
                        n_cursors += 1;
                        all_cursors.lock().unwrap()
                            .entry(expr_name_key.clone())
                            .or_default()
                            .push(c.clone());
                        if tx.send(RunEvent::Cursor {
                            expr_name: expr.name.clone(),
                            cursor:    c.clone(),
                        }).await.is_err() { break 'drain; }
                    }
                }
                None => break,
            }
        }
    }

    let drained: Vec<Box<dyn Diagnostic>> =
        diag_bucket.lock().unwrap().drain(..).collect();
    for d in drained {
        let _ = tx.send(RunEvent::Diag { diag: d }).await;
    }
    let _ = tx.send(RunEvent::ExprDone { expr_name: expr.name.clone() }).await;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn synthetic_site() -> Arc<ParseSite> {
    Arc::new(ParseSite {
        file:       Arc::from(std::path::Path::new("")),
        path:       Arc::from(Vec::<ParseSeg>::new().into_boxed_slice()),
        byte_range: 0..0,
    })
}

fn clone_spec(s: &ExprTableSpec) -> ExprTableSpec {
    ExprTableSpec {
        expr_name:    s.expr_name.clone(),
        namespace:    s.namespace.clone(),
        captures:     s.captures.iter().map(|c| CaptureColumn {
            name:         c.name.clone(),
            scan_pointer: c.scan_pointer.clone(),
        }).collect(),
        schema_hash:  s.schema_hash.clone(),
        extract_hash: s.extract_hash.clone(),
    }
}

fn build_expr_table_spec(name: Arc<str>, expr: &CursorExpr) -> ExprTableSpec {
    let captures = collect_captures(&expr.pipeline)
        .into_iter()
        .map(|n| CaptureColumn { name: n, scan_pointer: None })
        .collect();
    let mut spec = ExprTableSpec {
        expr_name:    name,
        namespace:    None,
        captures,
        schema_hash:  Arc::from(""),
        extract_hash: crate::store::ddl::extract_hash_of(expr),
    };
    spec.schema_hash = crate::store::ddl::schema_hash_of(&spec);
    spec
}

fn collect_captures(pipeline: &crate::op::Pipeline) -> Vec<Arc<str>> {
    use crate::op::Pipeline;
    let mut out: Vec<Arc<str>> = Vec::new();
    let mut seen: std::collections::HashSet<Arc<str>> = std::collections::HashSet::new();
    fn walk(p: &Pipeline, out: &mut Vec<Arc<str>>, seen: &mut std::collections::HashSet<Arc<str>>) {
        match p {
            Pipeline::Op(lop) => {
                for name in lop.op.binds_captures().iter() {
                    if seen.insert(name.clone()) {
                        out.push(name.clone());
                    }
                }
            }
            Pipeline::Seq(children) => {
                for c in children { walk(c, out, seen); }
            }
            Pipeline::Fork(arms) => {
                for a in arms { walk(&a.pipeline, out, seen); }
            }
            Pipeline::Switch { arms, .. } => {
                for (_, a) in arms { walk(a, out, seen); }
            }
        }
    }
    walk(pipeline, &mut out, &mut seen);
    out
}

fn build_batch(
    cursors_by_expr: &HashMap<Arc<str>, Vec<Cursor>>,
    specs:           &HashMap<Arc<str>, ExprTableSpec>,
    scanner_hash:    &Arc<str>,
) -> Batch {
    let mut per_expr = Vec::<ExprBatch>::new();
    for (name, cursors) in cursors_by_expr {
        if name.is_empty() || cursors.is_empty() { continue; }
        if specs.get(name).is_none() { continue; }
        let rows: Vec<RowInsert> = cursors
            .iter()
            .filter_map(|c| {
                let file = c.fs.clone()?;
                Some(RowInsert {
                    repo:     c.repo.clone(),
                    rev:      c.rev.clone(),
                    file,
                    captures: clone_captures(&c.captures),
                    evidence: Arc::from(c.evidence.clone().into_boxed_slice()),
                })
            })
            .collect();
        per_expr.push(ExprBatch { expr_name: name.clone(), rows });
    }
    Batch { scanner_hash: scanner_hash.clone(), per_expr }
}

fn clone_captures(caps: &HashMap<Arc<str>, Capture>) -> HashMap<Arc<str>, Capture> {
    let mut out = HashMap::with_capacity(caps.len());
    for (k, v) in caps {
        out.insert(k.clone(), v.clone());
    }
    out
}
