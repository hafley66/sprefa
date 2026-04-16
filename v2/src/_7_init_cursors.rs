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

use crate::_0_types::{
    Capture, Cursor, CursorExpr, OpId, ParseSeg, ParseSite, RunEvent, RunId,
};
use crate::_1_diagnostic::Diagnostic;
use crate::_2_config::Config;
use crate::_3_reader::Reader;
use crate::_4_writer::Writer;
use crate::_5_op::{DiagSink, EventSink, OpCtx};
use crate::_12_result_store::ResultStore;
use crate::mutations::MutationRequest;
use crate::store::{
    Batch, CaptureColumn, ExprBatch, ExprTableSpec, RowInsert, Store,
};

/// Full plumbing bundle for a single evaluator run.
pub struct InitInputs {
    pub exprs:        Vec<CursorExpr>,
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
            exprs, config: _config, store, reader, writer,
            mutations_tx, cancel, run_id, scanner_hash, result_store,
        } = inp;
        let shared_result_store = result_store;

        // Register schemas for every named expr up front.
        let mut spec_by_name: HashMap<Arc<str>, ExprTableSpec> = HashMap::new();
        for expr in &exprs {
            let Some(name) = expr.name.clone() else { continue };
            let spec = build_expr_table_spec(name.clone(), expr);
            if let Err(e) = store.register_expr_schema(clone_spec(&spec)).await {
                let _ = tx.send(RunEvent::Diag { diag: Box::new(e) }).await;
            } else {
                spec_by_name.insert(name, spec);
            }
        }

        let mut all_cursors: HashMap<Arc<str>, Vec<Cursor>> = HashMap::new();

        'expr_loop: for expr in exprs.into_iter() {
            if cancel.is_cancelled() { break; }

            let diag_bucket: Arc<Mutex<Vec<Box<dyn Diagnostic>>>> =
                Arc::new(Mutex::new(Vec::new()));
            let sink_bucket = diag_bucket.clone();

            let (fresh_store, xref_seen) = OpCtx::fresh_xref_state();
            let result_store = shared_result_store.clone().unwrap_or(fresh_store);
            let ctx = OpCtx {
                run_id,
                op_id:  OpId(0),
                reader: reader.clone(),
                writer: writer.clone(),
                config: _config.clone(),
                diags:  DiagSink(Arc::new(move |d| {
                    sink_bucket.lock().unwrap().push(d);
                })),
                events: EventSink(Arc::new(|_| {})),
                result_store,
                xref_seen,
                store:        store.clone(),
                mutations:    mutations_tx.clone(),
                cancel:       cancel.clone(),
                expr_name:    expr.name.clone(),
                current_site: synthetic_site(),
            };

            let empty: BoxStream<'static, Arc<[Cursor]>> =
                futures_util::stream::iter(Vec::<Arc<[Cursor]>>::new()).boxed();

            let mut batches = expr.pipeline.run(empty, ctx);
            let expr_name_key = expr.name.clone().unwrap_or_else(|| Arc::from(""));

            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => break 'expr_loop,
                    next = batches.next() => match next {
                        Some(batch) => {
                            for c in batch.iter() {
                                all_cursors.entry(expr_name_key.clone())
                                    .or_default()
                                    .push(c.clone());
                                if tx.send(RunEvent::Cursor {
                                    expr_name: expr.name.clone(),
                                    cursor:    c.clone(),
                                }).await.is_err() { break 'expr_loop; }
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

        if !cancel.is_cancelled() {
            let batch = build_batch(&all_cursors, &spec_by_name, &scanner_hash);
            if !batch.per_expr.is_empty() {
                if let Err(e) = store.flush_batch(batch).await {
                    let _ = tx.send(RunEvent::Diag { diag: Box::new(e) }).await;
                }
            }
        }

        let _ = tx.send(RunEvent::Done).await;
    });

    Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
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
        extract_hash: crate::store::_3_ddl::extract_hash_of(expr),
    };
    spec.schema_hash = crate::store::_3_ddl::schema_hash_of(&spec);
    spec
}

fn collect_captures(pipeline: &crate::_5_op::Pipeline) -> Vec<Arc<str>> {
    use crate::_5_op::Pipeline;
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
