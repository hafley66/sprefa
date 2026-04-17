//! run_pipeline — ServerState-hosted port of the old bin/sprefa_v2.rs Run
//! body. Transport layers call this; transports own wire-serialization.
//!
//! Shape: caller passes `RunRequest { source, source_path, workspace_hint }`,
//! receives `RunStart { rule_names, stream }`. `rule_names` carries the
//! top-level pipe order so UIs can print `rule NAME — N rows` in source
//! order; `stream` is the raw `BoxStream<RunEvent>` as `init_cursors`
//! returns it. Diagnostics from parse + lower surface as `RunEvent::Diag`.
//!
//! Memory sentinel: when `source_path` is `None` the file on every
//! ParseSite becomes `<memory>`. Workspace resolution still runs and uses
//! `workspace_hint` (or cwd) to seed repos/revs, so an HTTP caller can
//! POST a `.sprf` body without a disk path and still match against the
//! configured repo set.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_core::stream::BoxStream;
use futures_util::{stream, StreamExt};

use crate::_0_types::{CursorExpr, RunEvent, RunId};
use crate::_7_init_cursors::{init_cursors, InitInputs};
use crate::_8_parse::host_parse;
use crate::_10_registry::lower_rules;
use crate::_5_op::ProgramCtx;
use crate::mutations::{spawn_handler, AutoApprove};
use crate::writers::MemWriter;

use super::_0_state::ServerState;

// ---------------------------------------------------------------------------
// Request / Start
// ---------------------------------------------------------------------------

pub struct RunRequest {
    pub source:         String,
    pub source_path:    Option<PathBuf>,
    pub workspace_hint: Option<PathBuf>,
}

pub struct RunStart {
    pub rule_names: Vec<Arc<str>>,
    pub stream:     BoxStream<'static, RunEvent>,
}

// ---------------------------------------------------------------------------
// run_pipeline
// ---------------------------------------------------------------------------

pub async fn run_pipeline(state: Arc<ServerState>, req: RunRequest) -> RunStart {
    // 1. Resolve workspace.
    let hint: PathBuf = req.workspace_hint.clone()
        .or_else(|| req.source_path.as_ref().and_then(|p| p.parent().map(Path::to_path_buf)))
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let workspace = state.workspaces.write().await.resolve(&hint);

    // 2. Parse.
    let file_path: Arc<Path> = req.source_path
        .map(|p| Arc::from(p.as_path()))
        .unwrap_or_else(|| Arc::from(Path::new("<memory>")));

    let invs = match host_parse(&req.source, file_path) {
        Ok(i) => i,
        Err(errs) => {
            let diags: Vec<RunEvent> = errs.into_iter()
                .map(|e| RunEvent::Diag { diag: Box::new(ParseErrDiag::from(e)) })
                .chain(std::iter::once(RunEvent::Done))
                .collect();
            return RunStart {
                rule_names: vec![],
                stream:     stream::iter(diags).boxed(),
            };
        }
    };

    // 3. Top-level rule-name extraction — same heuristic as old CLI: first
    //    op's paren_src, trimmed. Empty → unnamed.
    let rule_names: Vec<Arc<str>> = invs.iter().map(|p| {
        p.ops.first()
            .and_then(|op| op.paren_src.as_ref())
            .map(|ps| Arc::<str>::from(ps.src.trim()))
            .unwrap_or_else(|| Arc::<str>::from(""))
    }).collect();

    // 4. Lower.
    let pctx = ProgramCtx::new(workspace.config.clone(), state.registry.clone());
    let outcome = lower_rules(invs, pctx);

    if !outcome.diags.is_empty() {
        let diags: Vec<RunEvent> = outcome.diags.into_iter()
            .map(|d| RunEvent::Diag { diag: d })
            .chain(std::iter::once(RunEvent::Done))
            .collect();
        return RunStart { rule_names, stream: stream::iter(diags).boxed() };
    }

    // 5. Spawn handler (AutoApprove for CLI-originated runs).
    let cancel = state.cancel_root.child_token();
    let (mtx, mrx) = tokio::sync::mpsc::channel(64);
    let handler_guard = spawn_handler(Arc::new(AutoApprove), mrx, cancel.clone());

    // 6. init_cursors.
    let writer = Arc::new(MemWriter::new());
    let exprs: Vec<CursorExpr> = outcome.pipelines.into_iter()
        .zip(rule_names.iter().cloned())
        .map(|(pipeline, name)| CursorExpr {
            name: if name.is_empty() { None } else { Some(name) },
            pipeline,
        })
        .collect();

    let stream = init_cursors(InitInputs {
        exprs,
        config:       workspace.config.clone(),
        store:        state.store.clone(),
        reader:       workspace.reader.clone(),
        writer,
        mutations_tx: mtx,
        cancel:       cancel.clone(),
        run_id:       RunId(next_run_id()),
        scanner_hash: Arc::from(""),
        result_store: Some(state.results.clone()),
    });

    // Keep the handler alive for the lifetime of the stream.
    let stream = WithHandler { inner: stream, _guard: handler_guard, _cancel: cancel };

    RunStart { rule_names, stream: stream.boxed() }
}

// Monotonic run-id (per process). Only used as a tag on diagnostics; not
// persisted across restarts.
fn next_run_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    N.fetch_add(1, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// WithHandler — attach handler TaskGuard to stream lifetime
// ---------------------------------------------------------------------------

use std::pin::Pin;
use std::task::{Context, Poll};
use futures_core::Stream;

struct WithHandler<S> {
    inner:   S,
    _guard:  crate::_task_guard::TaskGuard,
    _cancel: tokio_util::sync::CancellationToken,
}

impl<S: Stream + Unpin> Stream for WithHandler<S> {
    type Item = S::Item;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

// ---------------------------------------------------------------------------
// ParseErrDiag — minimal Diagnostic wrapper for host_parse errors
// ---------------------------------------------------------------------------

use crate::_0_types::{ParseSite, Severity};
use crate::_1_diagnostic::{Diagnostic, Renderer};
use crate::_8_parse::ParseError;

#[derive(Debug)]
struct ParseErrDiag {
    message: String,
    site:    Arc<ParseSite>,
}

impl From<ParseError> for ParseErrDiag {
    fn from(e: ParseError) -> Self {
        let site = Arc::new(ParseSite {
            file:       Arc::from(Path::new("<memory>")),
            path:       Arc::from(Vec::<crate::_0_types::ParseSeg>::new().into_boxed_slice()),
            byte_range: e.offset..e.offset,
        });
        Self { message: e.message.to_string(), site }
    }
}

impl Diagnostic for ParseErrDiag {
    fn code(&self) -> &str { "host/parse-error" }
    fn severity(&self) -> Severity  { Severity::Error }
    fn primary(&self) -> &ParseSite { &self.site }
    fn render(&self, r: &mut dyn Renderer) {
        r.header(self.code(), self.severity(), &self.message);
        r.primary(&self.site);
    }
}
