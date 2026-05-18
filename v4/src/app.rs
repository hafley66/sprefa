//! Central RPC registry. ONE block declares every endpoint; the
//! `sprf_rpc!` macro expands it into:
//!
//!   • `trait SprfHandlers`   — implement once on `SprfState`
//!   • `trait SprfClient`     — typed call surface
//!   • `fn build_router(...)` — axum::Router with all routes
//!   • `struct InProcessClient` — `tower::ServiceExt::oneshot` over the Router
//!
//! Adding an RPC = adding one line to `sprf_rpc!{ … }`. No central enum,
//! no per-route ceremony.
//!
//! Wire format: POST + Json body for every endpoint. Same shape over
//! the wire (hyper) and in-process (oneshot). Daemon transport is the
//! same Router served by `hyper::Server` (slice 4, deferred).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use bytes::Bytes;
use effect_runtime::v2::{
    attach_dirty_to_queue, expand, BufferProbeSink, Component, Diag, DiagSink, EventBus,
    ExpandOpts, FactStore, HybridCfg, HybridQueue, MemFactStore, MemQueue, Node, Pipe,
    PipeInstance, ProbeSink, Purity, QueueBackend, SqliteFactStore,
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

use crate::compile::ast::{OpCall, PipeAst};
use crate::compile::parse::host_parse;
use crate::compile::walk::walk_program;
use crate::compile::{FusedKind, FusedRule};
use crate::cst::dsl::Dsl;
use crate::cst::dsls::sql::{SqlCol, SqlDsl, SqlLspCtx, SqlTable};
#[cfg(feature = "ghcache")]
use crate::git_watch::{dirty_notice, ghcache_dirty_notices, ghcache_source_wakes};
#[cfg(feature = "ghcache")]
pub use crate::git_watch::{DirtyNotice, GhcacheChangeReq, NotifyGhcacheChangeReq};
use crate::lower::{default_registry, LowerCtx, Registry};
use crate::lsp::LSP_HOVER_CODE;
use crate::runtime_graph::RuntimeGraph;
use crate::source_clock::SourceClock;
use crate::runtime_replay::GraphReplayRunner;
use crate::source::resolve_path_text;
use crate::telemetry::{PipelineTelemetry, RunPhaseTelemetry, RunTelemetry};
use crate::{Cursor, FOCAL_TERM};

// ───────────────────────────────────────────────────────────────────
// Errors
// ───────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum SprfError {
    #[error("io: {0}")]
    Io(String),
    #[error("unknown doc: {0}")]
    UnknownDoc(String),
    #[error("wire: {0}")]
    Wire(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl IntoResponse for SprfError {
    fn into_response(self) -> axum::response::Response {
        let code = match &self {
            SprfError::UnknownDoc(_) => StatusCode::NOT_FOUND,
            SprfError::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            SprfError::Wire(_) => StatusCode::BAD_REQUEST,
            SprfError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (code, Json(self)).into_response()
    }
}

// ───────────────────────────────────────────────────────────────────
// Request / Response payloads
// ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspOpenReq {
    pub uri: String,
    pub text: String,
    pub version: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspChangeReq {
    pub uri: String,
    pub text: String,
    pub version: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspCloseReq {
    pub uri: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetDiagsReq {
    pub uri: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetInlaysReq {
    pub uri: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReq {
    pub path: PathBuf,
    pub root: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlayProbe {
    pub lo: u32,
    pub hi: u32,
    pub count: u32,
    pub sample_value: Option<String>,
    pub sample_terms: Vec<(String, String)>,
}

/// Wire shape for `effect_runtime::v2::Diag` — that crate intentionally
/// has no serde dep, so we ferry diags through this DTO at the seam.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SprfDiag {
    pub lo: Option<u32>,
    pub hi: Option<u32>,
    pub severity: String, // "error" | "warning" | "info" | "hint"
    pub code: String,
    pub message: String,
}

impl From<&effect_runtime::v2::Diag> for SprfDiag {
    fn from(d: &effect_runtime::v2::Diag) -> Self {
        use effect_runtime::v2::Severity;
        Self {
            lo: d.span.map(|s| s.lo),
            hi: d.span.map(|s| s.hi),
            severity: match d.severity {
                Severity::Error => "error",
                Severity::Warn => "warning",
                Severity::Info => "info",
                Severity::Hint => "hint",
            }
            .into(),
            code: d.code.to_string(),
            message: d.message.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub parse_diags: Vec<SprfDiag>,
    pub walk_diags: Vec<SprfDiag>,
    pub runtime_diags: Vec<SprfDiag>,
    pub pipes: usize,
    /// Rule-table names harvested from the AST (one per `rule(:NAME)`).
    pub tables: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telemetry: Option<RunTelemetry>,
}

struct BufferDiagSink {
    rows: Mutex<Vec<Diag>>,
}

impl BufferDiagSink {
    fn new() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
        }
    }

    fn snapshot(&self) -> Vec<Diag> {
        self.rows.lock().unwrap().clone()
    }
}

impl DiagSink for BufferDiagSink {
    fn emit(&self, diag: Diag) {
        self.rows.lock().unwrap().push(diag);
    }
}

/// Locate the dsl-body span enclosing a host byte by walking the
/// CACHED `DocState.program`. No re-parse: the program is built once
/// per `lsp_open` / `lsp_change` and read here.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspLocateDslReq {
    pub uri: String,
    /// Host-source byte offset (the LSP server resolves line/utf16-col
    /// to bytes before crossing this boundary).
    pub byte: u32,
}

/// `op_name` is `None` when `byte` falls outside every dsl body in the
/// cached program — host position, not a dsl position.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspLocateDslResp {
    pub op_name: Option<String>,
    pub body_raw: Option<String>,
    pub body_off: u32,
    pub body_byte: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspHoverReq {
    pub uri: String,
    pub byte: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspHoverResp {
    pub contents: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspCompletionReq {
    pub uri: String,
    pub byte: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspDefinitionReq {
    pub uri: String,
    pub byte: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspDefinitionResp {
    pub lo: Option<u32>,
    pub hi: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct RuntimeHover {
    pub lo: u32,
    pub hi: u32,
    pub contents: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFactTableReq {
    pub name: String,
    /// Cap on rows returned. None = no cap (still bounded by store).
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactRow {
    pub fields: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactTable {
    pub name: String,
    /// Total rows in the store (pre-limit).
    pub total: usize,
    pub rows: Vec<FactRow>,
}

// ── refs_at — Layer 4 (subset) ───────────────────────────────────────
//
// Given (path, byte) resolve every `_refs` row whose coord covers the
// byte under the FileId that `path` resolves to. Each hit echoes the
// content-derived `ref_id` plus a wire copy of the resolved coord with
// `path_of(fs)` projected back to its first-seen path.
//
// Auto-views (CREATE VIEW <rule>_resolved) and LSP hover side-rail
// enrichment of these hits are deferred until rule emission to
// <rule>_facts is verified live in v4.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefsAtReq {
    pub path: String,
    pub byte: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefsAtResp {
    pub hits: Vec<RefHit>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefHit {
    pub ref_id: u64,
    pub coord: CoordWire,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoordWire {
    pub repo: u32,
    pub rev: u32,
    pub fs: u64,
    /// `path_of(fs)` resolution. None if the FileId has no `_files` row
    /// (e.g. the synthetic sentinel never gets surfaced here, but a
    /// foreign FileId in a stripped store would).
    pub fs_path: Option<String>,
    pub lo: u32,
    pub hi: u32,
}

// ───────────────────────────────────────────────────────────────────
// Codegen macro
// ───────────────────────────────────────────────────────────────────
//
// Each line declares one endpoint:
//   fn <method>(<ReqType>) -> <RespType> @ "<path>"
//
// Macro expands once into the four artifacts listed at the top of this
// file. Add an endpoint = add a line. Implementer fills in the body in
// the SprfHandlers impl.

macro_rules! sprf_rpc {
    (
        $(
            $(#[$meta:meta])*
            fn $method:ident ( $req:ty ) -> $resp:ty => $path:literal ;
        )*
    ) => {
        // ── handler trait — implement once on app state ───────────
        #[async_trait::async_trait]
        pub trait SprfHandlers: Send + Sync + 'static {
            $(
                $(#[$meta])*
                async fn $method(&self, req: $req) -> Result<$resp, SprfError>;
            )*
        }

        // ── typed client trait ────────────────────────────────────
        #[async_trait::async_trait]
        pub trait SprfClient: Send + Sync {
            $(
                $(#[$meta])*
                async fn $method(&self, req: $req) -> Result<$resp, SprfError>;
            )*
        }

        // ── per-route generic handler fns ─────────────────────────
        // Named (not closure) so axum's Handler trait inference is
        // unambiguous. `_route_*` keeps these out of the public API.
        pub mod _routes {
            use super::*;
            $(
                $(#[$meta])*
                pub async fn $method<H: SprfHandlers>(
                    State(h):   State<Arc<H>>,
                    Json(req):  Json<$req>,
                ) -> Result<Json<$resp>, SprfError> {
                    h.$method(req).await.map(Json)
                }
            )*
        }

        pub fn build_router<H: SprfHandlers>(h: Arc<H>) -> Router {
            let router = Router::new();
            $(
                $(#[$meta])*
                let router = router.route($path, post(_routes::$method::<H>));
            )*
            router.with_state(h)
        }

        // ── in-process client (oneshot the router) ────────────────
        #[derive(Clone)]
        pub struct InProcessClient {
            pub router: Router,
        }

        impl InProcessClient {
            pub fn new(router: Router) -> Self { Self { router } }

            async fn call_json<Req, Resp>(&self, path: &'static str, req: Req)
                -> Result<Resp, SprfError>
            where
                Req:  Serialize,
                Resp: serde::de::DeserializeOwned,
            {
                let body = serde_json::to_vec(&req)
                    .map_err(|e| SprfError::Wire(e.to_string()))?;
                let http_req = Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .map_err(|e| SprfError::Wire(e.to_string()))?;
                let resp = self.router.clone()
                    .oneshot(http_req).await
                    .map_err(|e| SprfError::Internal(e.to_string()))?;
                let status = resp.status();
                let bytes: Bytes = resp.into_body()
                    .collect().await
                    .map_err(|e| SprfError::Wire(e.to_string()))?
                    .to_bytes();
                if !status.is_success() {
                    if let Ok(err) = serde_json::from_slice::<SprfError>(&bytes) {
                        return Err(err);
                    }
                    return Err(SprfError::Internal(format!(
                        "http {status}: {}",
                        String::from_utf8_lossy(&bytes),
                    )));
                }
                serde_json::from_slice(&bytes)
                    .map_err(|e| SprfError::Wire(e.to_string()))
            }
        }

        #[async_trait::async_trait]
        impl SprfClient for InProcessClient {
            $(
                $(#[$meta])*
                async fn $method(&self, req: $req) -> Result<$resp, SprfError> {
                    self.call_json($path, req).await
                }
            )*
        }

        // ── HTTP client (talks to a remote sprefa-daemon) ─────────
        #[derive(Clone)]
        pub struct HttpClient {
            pub base: String,
            pub http: reqwest::Client,
        }

        impl HttpClient {
            pub fn new(base: impl Into<String>) -> Self {
                Self { base: base.into(), http: reqwest::Client::new() }
            }
        }

        #[async_trait::async_trait]
        impl SprfClient for HttpClient {
            $(
                $(#[$meta])*
                async fn $method(&self, req: $req) -> Result<$resp, SprfError> {
                    let url = format!("{}{}", self.base, $path);
                    let resp = self.http.post(&url).json(&req).send().await
                        .map_err(|e| SprfError::Wire(e.to_string()))?;
                    let status = resp.status();
                    let bytes  = resp.bytes().await
                        .map_err(|e| SprfError::Wire(e.to_string()))?;
                    if !status.is_success() {
                        if let Ok(err) = serde_json::from_slice::<SprfError>(&bytes) {
                            return Err(err);
                        }
                        return Err(SprfError::Internal(format!(
                            "http {status}: {}", String::from_utf8_lossy(&bytes),
                        )));
                    }
                    serde_json::from_slice(&bytes)
                        .map_err(|e| SprfError::Wire(e.to_string()))
                }
            )*
        }
    };
}

// ───────────────────────────────────────────────────────────────────
// THE REGISTRY — single source of truth
// ───────────────────────────────────────────────────────────────────

sprf_rpc! {
    fn lsp_open    (LspOpenReq)   -> ()                  => "/lsp/open";
    fn lsp_change  (LspChangeReq) -> ()                  => "/lsp/change";
    fn lsp_close   (LspCloseReq)  -> ()                  => "/lsp/close";
    fn get_diags   (GetDiagsReq)  -> Vec<SprfDiag>       => "/lsp/diags";
    fn get_inlays  (GetInlaysReq) -> Vec<InlayProbe>     => "/lsp/inlays";
    fn lsp_locate_dsl (LspLocateDslReq) -> LspLocateDslResp => "/lsp/locate-dsl";
    fn lsp_hover      (LspHoverReq) -> LspHoverResp      => "/lsp/hover";
    fn lsp_completion (LspCompletionReq) -> Vec<lsp_types::CompletionItem> => "/lsp/completion";
    fn lsp_definition (LspDefinitionReq) -> LspDefinitionResp => "/lsp/definition";
    fn run            (RunReq)          -> RunReport     => "/run";
    fn get_fact_table (GetFactTableReq) -> FactTable     => "/facts";
    fn refs_at        (RefsAtReq)       -> RefsAtResp    => "/refs-at";
    #[cfg(feature = "ghcache")]
    fn notify_ghcache_change (NotifyGhcacheChangeReq) -> Vec<DirtyNotice> => "/git/ghcache-change";
}

// ───────────────────────────────────────────────────────────────────
// State + Handlers impl
// ───────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct DocState {
    pub text: String,
    pub version: i32,
    pub program: Vec<PipeAst>,
    pub parse_diags: Vec<Diag>,
    pub walk_diags: Vec<Diag>,
    pub runtime_diags: Vec<Diag>,
    pub runtime_hovers: Vec<RuntimeHover>,
    pub probes: Vec<InlayProbe>,
}

pub struct SprfState {
    pub docs: Mutex<HashMap<String, DocState>>,
    pub facts: Arc<dyn FactStore<Cursor>>,
    pub queue: Arc<dyn QueueBackend<Cursor>>,
    pub bus: Arc<EventBus>,
    instances: Mutex<Vec<Arc<PipeInstance<Cursor>>>>,
    next_instance_id: Mutex<u64>,
    /// Layer 0c.2 — content-derived intern store wrapping `facts`.
    /// Threaded into `LowerCtx` so source/pattern emitters stamp the
    /// coord-space side of Cursor (`value_id`, `at`, `terms`).
    pub sprf_store: Arc<crate::store::SprfStore>,
    pub runtime_graph: Arc<RuntimeGraph>,
    /// Layer 5a — XDG repos config. Bare `repo()` reads from this.
    /// `SprfState::new` loads from `~/.config/sprefa/repos.toml` via
    /// `SprfConfig::load_default`; tests inject explicit configs via
    /// `with_config`.
    pub config: Arc<crate::config::SprfConfig>,
    pub registry: Arc<Registry>,
    pub root: PathBuf,
}

struct AnalysisPassThrough;

impl Component for AnalysisPassThrough {
    type Next = Cursor;

    fn render(&self, _ctx: &effect_runtime::v2::RenderCtx, c: &Cursor) -> Node<Cursor> {
        Node::Emit(Arc::new(c.clone()))
    }

    fn kind(&self) -> &'static str {
        "analysis_pass_through"
    }
}

impl SprfState {
    pub fn new(root: PathBuf) -> Self {
        Self::new_with_backends(
            root,
            Arc::new(MemFactStore::<Cursor>::new()),
            Arc::new(MemQueue::new()),
        )
    }

    pub fn new_with_sqlite_queue(root: PathBuf, queue_path: impl AsRef<std::path::Path>) -> Self {
        Self::new_with_backends(
            root,
            Arc::new(MemFactStore::<Cursor>::new()),
            Arc::new(
                HybridQueue::<Cursor>::open_file(queue_path.as_ref(), HybridCfg::default())
                    .expect("open hybrid sqlite queue"),
            ),
        )
    }

    pub fn new_with_sqlite_facts(root: PathBuf, fact_path: impl AsRef<std::path::Path>) -> Self {
        let facts: Arc<dyn FactStore<Cursor>> = Arc::new(
            SqliteFactStore::<Cursor>::open_file(fact_path.as_ref())
                .expect("open sqlite fact store"),
        );
        let mut state = Self::new_with_backends(root, facts, Arc::new(MemQueue::new()));
        state.runtime_graph = Arc::new(RuntimeGraph::new_with_compact_sources(
            state.sprf_store.clone(),
            state.facts.clone(),
            fact_path.as_ref(),
        ));
        state
    }

    pub fn new_with_sqlite_backends(
        root: PathBuf,
        fact_path: impl AsRef<std::path::Path>,
        queue_path: impl AsRef<std::path::Path>,
    ) -> Self {
        let facts: Arc<dyn FactStore<Cursor>> = Arc::new(
            SqliteFactStore::<Cursor>::open_file(fact_path.as_ref())
                .expect("open sqlite fact store"),
        );
        let mut state = Self::new_with_backends(
            root,
            facts,
            Arc::new(
                HybridQueue::<Cursor>::open_file(queue_path.as_ref(), HybridCfg::default())
                    .expect("open hybrid sqlite queue"),
            ),
        );
        state.runtime_graph = Arc::new(RuntimeGraph::new_with_compact_sources(
            state.sprf_store.clone(),
            state.facts.clone(),
            fact_path.as_ref(),
        ));
        state
    }

    pub fn new_with_backends(
        root: PathBuf,
        facts: Arc<dyn FactStore<Cursor>>,
        queue: Arc<dyn QueueBackend<Cursor>>,
    ) -> Self {
        let bus = Arc::new(EventBus::new());
        attach_dirty_to_queue(&bus, queue.clone());
        let sprf_store = crate::store::SprfStore::new(facts.clone());
        let runtime_graph = Arc::new(RuntimeGraph::new(sprf_store.clone(), facts.clone()));
        let config = Arc::new(crate::config::SprfConfig::load_default());
        Self {
            docs: Mutex::new(HashMap::new()),
            facts,
            queue,
            bus,
            instances: Mutex::new(Vec::new()),
            next_instance_id: Mutex::new(1),
            sprf_store,
            runtime_graph,
            config,
            registry: Arc::new(default_registry()),
            root,
        }
    }

    /// Test-side builder: replace the config loaded from disk with an
    /// explicit one. Returns `Self` so tests can chain.
    pub fn with_config(mut self, c: crate::config::SprfConfig) -> Self {
        self.config = Arc::new(c);
        self
    }

    fn mount_pipe(&self, pipe: Pipe<Cursor>, identity: u64) -> Arc<PipeInstance<Cursor>> {
        let mut inst = pipe.into_instance();
        let mut next = self.next_instance_id.lock().unwrap();
        inst.pipe_hash = identity;
        inst.instance_id = identity;
        *next += 1;
        let inst = Arc::new(inst);
        self.instances.lock().unwrap().push(inst.clone());
        inst
    }

    fn resume_mounted(&self, opts: ExpandOpts) -> u64 {
        let mut total_rendered = 0;
        for _ in 0..8 {
            let instances = self.instances.lock().unwrap().clone();
            let mut rendered = 0;
            for inst in instances {
                rendered +=
                    expand(inst.as_ref(), self.queue.clone(), Vec::new(), opts.clone()).rendered;
            }
            total_rendered += rendered;
            if rendered == 0 {
                break;
            }
            let generation = *self.next_instance_id.lock().unwrap();
            self.sprf_store.flush();
            self.facts.commit(generation, Some(&self.bus));
        }
        total_rendered
    }

    #[cfg(feature = "ghcache")]
    pub fn dispatch_ghcache_change(&self, change: &GhcacheChangeReq) -> Vec<DirtyNotice> {
        let mut notices = Vec::new();
        for (domain, key) in ghcache_dirty_notices(change) {
            notices.push(dirty_notice(domain, key));
        }
        let generation = *self.next_instance_id.lock().unwrap();
        for wake in ghcache_source_wakes(change, generation) {
            self.runtime_graph.dispatch_source_wake(wake);
        }
        notices
    }

    pub fn drain_ready(&self) {
        let opts = ExpandOpts::default()
            .with_bus(self.bus.clone())
            .with_runtime(self.runtime_graph.clone());
        self.drain_runtime_until_idle(opts);
    }

    fn drain_runtime_until_idle(&self, opts: ExpandOpts) -> u64 {
        let mut handled = 0;
        for _ in 0..8 {
            let graph_jobs = self.drain_graph_jobs();
            if graph_jobs > 0 {
                let generation = *self.next_instance_id.lock().unwrap();
                self.sprf_store.flush();
                self.facts.commit(generation, Some(&self.bus));
            }
            let rendered = self.resume_mounted(opts.clone());
            handled += graph_jobs as u64 + rendered;
            if graph_jobs == 0 && rendered == 0 {
                break;
            }
        }
        handled
    }

    pub fn drain_graph_jobs(&self) -> usize {
        GraphReplayRunner {
            facts: self.facts.clone(),
            queue: self.queue.clone(),
            bus: self.bus.clone(),
            sprf_store: self.sprf_store.clone(),
            runtime_graph: self.runtime_graph.clone(),
            instances: self.instances.lock().unwrap().clone(),
        }
        .drain()
    }

    fn ingest(&self, uri: String, text: String, version: i32) {
        let (program, parse_diags) = host_parse(&text);
        let probe_sink: Arc<BufferProbeSink<Cursor>> = Arc::new(BufferProbeSink::new());
        let runtime_diags = Arc::new(BufferDiagSink::new());
        let mut ctx = LowerCtx::new(self.facts.clone(), self.root.clone())
            .with_probe(probe_sink.clone() as Arc<dyn ProbeSink<Cursor>>)
            .with_sprf_store(self.sprf_store.clone())
            .with_config(self.config.clone());
        let (pipes, mut walk_diags) = walk_program(&program, &self.registry, &mut ctx);
        walk_diags.extend(sql_lsp_diagnostics(&program, self.facts.as_ref()));

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let opts = ExpandOpts::default().with_diag(runtime_diags.clone());
        for fused in pipes {
            let inst = analysis_safe_pipe(fused.into_pipe()).into_instance();
            expand(
                &inst,
                queue.clone(),
                vec![Arc::new(Cursor::default())],
                opts.clone(),
            );
        }

        let raw = probe_sink.drain();
        let mut by_span: HashMap<(u32, u32), ProbeAgg> = HashMap::new();
        for p in &raw {
            let entry = by_span.entry((p.span.lo, p.span.hi)).or_default();
            entry.count += 1;
            if entry.sample_value.is_none() {
                entry.sample_value = sample_text(p.cursor.value.as_ref(), 80);
                entry.sample_terms = p
                    .cursor
                    .terms
                    .iter()
                    .filter(|term| term.name.as_ref() != FOCAL_TERM)
                    .take(8)
                    .map(|term| {
                        (
                            term.name.to_string(),
                            sample_text(term.value.as_ref(), 48).unwrap_or_default(),
                        )
                    })
                    .collect();
            }
        }
        let mut probes: Vec<InlayProbe> = by_span
            .into_iter()
            .map(|((lo, hi), agg)| InlayProbe {
                lo,
                hi,
                count: agg.count,
                sample_value: agg.sample_value,
                sample_terms: agg.sample_terms,
            })
            .collect();
        probes.sort_by_key(|p| (p.lo, p.hi));

        let (runtime_hovers, runtime_diags) = split_runtime_hovers(runtime_diags.snapshot());

        self.docs.lock().unwrap().insert(
            uri,
            DocState {
                text,
                version,
                program,
                parse_diags,
                walk_diags,
                runtime_diags,
                runtime_hovers,
                probes,
            },
        );
    }
}

fn analysis_safe_pipe(pipe: Pipe<Cursor>) -> Pipe<Cursor> {
    let steps = pipe
        .steps
        .into_iter()
        .map(|component| {
            if component.purity() == Purity::Effectful {
                Arc::new(AnalysisPassThrough) as Arc<dyn Component<Next = Cursor>>
            } else {
                component
            }
        })
        .collect();
    Pipe::from_steps(steps)
}

/// Execute a `FusedKind::FullSql` rule's precomputed INSERT statements
/// directly against the SQLite fact store. Returns true if execution
/// happened; false if the store is not SQLite-backed (caller falls back
/// to legacy expand).
///
/// The fuser's SQL is written against the rules-as-tables aspirational
/// schema (`hits_facts` with bare `FS` / `LO` columns, `_id BLOB`). The
/// live `SqliteFactStore` schema has wide string-FK columns (`FS_id`,
/// `LO_id`) and a TEXT `_id`. The adapter rewrites the fuser SQL into an
/// equivalent statement against the live schema:
///
///   * source `<rule>_facts` references are routed through the existing
///     `create_fact_view` helper, which exposes user-named columns;
///   * `INSERT INTO <rule>_facts (_id, A, B, …) SELECT proj_a, proj_b, …`
///     is rewritten to insert into the live FK columns by wrapping each
///     projected text expression in `(SELECT id FROM sprf_strings WHERE
///     value = <expr>)`. `_id` uses `hex(sprf_blake3_id(...))` to match
///     the live TEXT format.
///
/// The `support_edges` insert is best-effort: the live schema doesn't
/// declare the table, so we `CREATE TABLE IF NOT EXISTS` it in the same
/// transaction. Failures there are demoted to a warning so the primary
/// fact insert still commits.
fn run_fused_sql(
    facts: &dyn FactStore<Cursor>,
    clock: &crate::source_clock::FactStoreClock,
    fused: &FusedRule,
) -> bool {
    let Some(store) = facts.as_any().downcast_ref::<SqliteFactStore<Cursor>>() else {
        return false;
    };
    let Some(stmts) = fused.fused_statements() else {
        return false;
    };
    let target = fused.name().to_string();
    let target_facts = format!("{target}_facts");
    let target_cols = match store.declared_cols(&target) {
        Some(c) if !c.is_empty() => c,
        _ => {
            tracing::warn!(
                target: "sprefa::fused",
                rule = %target,
                "full-SQL rule has no declared cols in store; falling back to legacy expand",
            );
            return false;
        }
    };

    // Discover source tables referenced by the fuser SQL. The fuser
    // emits raw `<rule>_facts` identifiers in FROM/JOIN positions for
    // source reads. We need view names (`<rule>`) for the rewrite.
    let combined = stmts.join("\n");
    let source_tables = source_tables_from_fused_sql(&combined, &target_facts);

    let mut inserted: u64 = 0;
    let mut support_inserted: u64 = 0;
    let mut had_error = false;

    store.with_connection(|conn| {
        let tx = match conn.transaction() {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(
                    target: "sprefa::fused",
                    rule = %target,
                    error = %e,
                    "could not open fused-SQL transaction; falling back",
                );
                had_error = true;
                return;
            }
        };

        // Create temp views for each source table so the fuser SQL's
        // bare-column references resolve through `sprf_strings`.
        let mut view_ok = true;
        for src in &source_tables {
            if let Err(e) = create_fact_view_in_tx(&tx, store, src) {
                tracing::warn!(
                    target: "sprefa::fused",
                    rule = %target,
                    source = %src,
                    error = %e,
                    "create_fact_view failed in fused-SQL path",
                );
                view_ok = false;
                break;
            }
        }
        if !view_ok {
            had_error = true;
            return;
        }

        // Ensure support_edges exists. Schema mirrors the fuser-emitted
        // INSERT's column list.
        let _ = tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS support_edges (
                parent_table TEXT NOT NULL,
                parent_id    BLOB NOT NULL,
                child_table  TEXT NOT NULL,
                child_id     TEXT NOT NULL,
                UNIQUE(parent_table, parent_id, child_table, child_id)
            )",
        );

        let primary_marker = format!(
            "INSERT OR IGNORE INTO {target_facts}"
        );
        let support_marker = "INSERT OR IGNORE INTO support_edges";
        for raw_sql in stmts.iter() {
            let upper = raw_sql.trim_start().to_ascii_uppercase();
            let primary_marker_upper = primary_marker.to_ascii_uppercase();
            let executed = if upper.starts_with(&primary_marker_upper) {
                // Primary facts insert: rewrite to the live FK schema.
                match build_live_facts_insert(
                    raw_sql,
                    &target,
                    &target_facts,
                    target_cols.as_slice(),
                    &source_tables,
                ) {
                    Some(rewritten) => match tx.execute(&rewritten, []) {
                        Ok(n) => {
                            inserted += n as u64;
                            true
                        }
                        Err(e) => {
                            tracing::warn!(
                                target: "sprefa::fused",
                                rule = %target,
                                error = %e,
                                sql = %rewritten,
                                "fused-SQL fact insert failed",
                            );
                            had_error = true;
                            false
                        }
                    },
                    None => {
                        tracing::warn!(
                            target: "sprefa::fused",
                            rule = %target,
                            "could not rewrite fused-SQL fact insert to live schema",
                        );
                        had_error = true;
                        false
                    }
                }
            } else if upper.starts_with(support_marker) {
                let rewritten = rewrite_fused_source_refs(raw_sql, &source_tables);
                match tx.execute(&rewritten, []) {
                    Ok(n) => {
                        support_inserted += n as u64;
                        true
                    }
                    Err(e) => {
                        // Demoted: support edges are best-effort.
                        tracing::debug!(
                            target: "sprefa::fused",
                            rule = %target,
                            error = %e,
                            "support_edges insert skipped (best-effort)",
                        );
                        false
                    }
                }
            } else {
                false
            };
            let _ = executed;
        }

        // Drop the temp views we created so the connection state stays
        // clean for the next caller.
        for src in &source_tables {
            let _ = tx.execute(
                &format!("DROP VIEW IF EXISTS temp.{}", quote_view_ident(src)),
                [],
            );
        }

        if let Err(e) = tx.commit() {
            tracing::warn!(
                target: "sprefa::fused",
                rule = %target,
                error = %e,
                "fused-SQL transaction commit failed",
            );
            had_error = true;
        }
    });

    if had_error {
        return false;
    }

    // Telemetry: one exec call for the rule, rows = facts inserted.
    store.record_sqlite_insert_exec(inserted);
    if support_inserted > 0 {
        store.record_sqlite_insert_exec(support_inserted);
    }

    if inserted > 0 {
        // Phase 1: a rule/fact table is a SourceId. The fused-SQL
        // write path is an event-layer bump, the same as a file edit.
        clock.bump(crate::source_clock::SourceId::for_table(&target));
        store.bump_table_version(&target);
        store.mark_table_dirty(&target);
    }

    true
}

/// Extract source table names from a fused-SQL `INSERT INTO X SELECT …
/// FROM <src>_facts …` shape. We scan FROM/JOIN positions, drop the
/// `_facts` suffix, and de-dup. The target's own `<target>_facts` is
/// excluded.
fn source_tables_from_fused_sql(sql: &str, exclude_table: &str) -> Vec<String> {
    let bytes = sql.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        // Skip strings.
        if bytes[i] == b'\'' || bytes[i] == b'`' {
            let q = bytes[i];
            i += 1;
            while i < bytes.len() && bytes[i] != q {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            continue;
        }
        if let Some(rest) = lower_keyword_at(bytes, i, "FROM")
            .or_else(|| lower_keyword_at(bytes, i, "JOIN"))
        {
            i = rest;
            // Skip whitespace.
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            // Read identifier.
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            if i > start {
                let ident = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                if ident == exclude_table {
                    continue;
                }
                if let Some(base) = ident.strip_suffix("_facts") {
                    if !out.iter().any(|s| s == base) {
                        out.push(base.to_string());
                    }
                }
            }
            continue;
        }
        i += 1;
    }
    out
}

fn lower_keyword_at(bytes: &[u8], i: usize, kw: &str) -> Option<usize> {
    let kb = kw.as_bytes();
    if i + kb.len() > bytes.len() {
        return None;
    }
    for (k, &c) in kb.iter().enumerate() {
        if bytes[i + k].eq_ignore_ascii_case(&c) {
            continue;
        }
        return None;
    }
    // Must be a token boundary on both sides.
    if i > 0
        && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')
    {
        return None;
    }
    let end = i + kb.len();
    if end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
    {
        return None;
    }
    Some(end)
}

fn quote_view_ident(s: &str) -> String {
    let escaped = s.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

/// Create a TEMP VIEW exposing `<table>_facts` joined through
/// `sprf_strings` so SELECTs can reference user-named text columns.
/// Mirrors `crate::sql::create_fact_view` but uses a `&Transaction`
/// instead of a `&mut Connection` since we're inside a tx scope.
fn create_fact_view_in_tx(
    tx: &rusqlite::Transaction<'_>,
    store: &SqliteFactStore<Cursor>,
    table: &str,
) -> Result<(), String> {
    let cols = store.declared_cols(table).unwrap_or_default();
    let _ = tx.execute(
        &format!("DROP VIEW IF EXISTS temp.{}", quote_view_ident(table)),
        [],
    );
    let mut select = Vec::with_capacity(cols.len().max(1));
    let mut joins = Vec::with_capacity(cols.len());
    for col in &cols {
        let sql_col = col.strip_prefix(':').unwrap_or(col.as_str());
        let alias = format!("s_{sql_col}");
        select.push(format!(
            "{}.value AS {}",
            quote_view_ident(&alias),
            quote_view_ident(col),
        ));
        joins.push(format!(
            "JOIN sprf_strings {} ON {}.id = t.{}",
            quote_view_ident(&alias),
            quote_view_ident(&alias),
            quote_view_ident(&format!("{sql_col}_id")),
        ));
    }
    if select.is_empty() {
        select.push("t._id AS _id".to_string());
    }
    let sql = format!(
        "CREATE TEMP VIEW {} AS SELECT {} FROM {} t {}",
        quote_view_ident(table),
        select.join(", "),
        quote_view_ident(&format!("{table}_facts")),
        joins.join(" "),
    );
    tx.execute(&sql, []).map_err(|e| e.to_string())?;
    Ok(())
}

/// Rewrite `<src>_facts` source references to the bare `<src>` view name.
/// Only touches FROM/JOIN positions; literal strings are skipped.
fn rewrite_fused_source_refs(sql: &str, source_tables: &[String]) -> String {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'\'' || bytes[i] == b'`' {
            let q = bytes[i];
            out.push(q as char);
            i += 1;
            while i < bytes.len() && bytes[i] != q {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    out.push(bytes[i] as char);
                    out.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                out.push(q as char);
                i += 1;
            }
            continue;
        }
        if i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_' {
            let mut replaced = false;
            for src in source_tables {
                let needle = format!("{src}_facts");
                let nb = needle.as_bytes();
                if i + nb.len() <= bytes.len() && &bytes[i..i + nb.len()] == nb {
                    let after = i + nb.len();
                    if after >= bytes.len()
                        || (!bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_')
                    {
                        out.push_str(src);
                        i = after;
                        replaced = true;
                        break;
                    }
                }
            }
            if replaced {
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Build the live-schema INSERT for a fuser facts INSERT statement.
///
/// Input shape:
///   INSERT OR IGNORE INTO <target>_facts (_id, A, B, …)
///   SELECT <id_expr> AS _id, <a_expr> AS A, <b_expr> AS B, …
///   FROM <src>_facts r0 [JOIN <src>_facts r1 ON …]
///   [WHERE …]
///
/// Output shape:
///   INSERT OR IGNORE INTO <target>_facts (_id, A_id, B_id, …)
///   SELECT hex(<id_expr>) AS _id,
///          (SELECT id FROM sprf_strings WHERE value = <a_expr>) AS A_id, …
///   FROM <src> r0 [JOIN <src> r1 ON …]
///   [WHERE …]
fn build_live_facts_insert(
    sql: &str,
    _target_name: &str,
    target_facts: &str,
    target_cols: &[String],
    source_tables: &[String],
) -> Option<String> {
    // Locate INSERT INTO <target_facts> (...)
    let upper = sql.to_ascii_uppercase();
    let insert_marker = format!("INSERT OR IGNORE INTO {target_facts}");
    let insert_marker_upper = insert_marker.to_ascii_uppercase();
    let insert_pos = upper.find(&insert_marker_upper)?;
    let after_insert = insert_pos + insert_marker.len();
    let paren_open = sql[after_insert..].find('(')? + after_insert;
    let paren_close = sql[paren_open + 1..].find(')')? + paren_open + 1;

    let cols_raw = &sql[paren_open + 1..paren_close];
    let cols: Vec<String> = cols_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    if cols.is_empty() || cols[0] != "_id" {
        return None;
    }
    // Drop _id, keep user cols.
    let user_cols: Vec<String> = cols.into_iter().skip(1).collect();
    if user_cols.len() != target_cols.len() {
        return None;
    }

    // Locate SELECT … FROM … in the rest of the SQL.
    let rest = &sql[paren_close + 1..];
    let rest_upper = rest.to_ascii_uppercase();
    let select_pos = rest_upper.find("SELECT")?;
    let from_pos = rest_upper[select_pos..].find("FROM")? + select_pos;
    let select_clause = rest[select_pos + 6..from_pos].trim(); // 6 = len("SELECT")
    let from_clause = &rest[from_pos..];

    // Split SELECT clause by top-level commas (depth-0 parentheses).
    let projections = split_top_level_commas(select_clause);
    if projections.len() != user_cols.len() + 1 {
        return None;
    }

    // Strip ` AS NAME` suffixes from each projection expression.
    let mut exprs: Vec<String> = Vec::with_capacity(projections.len());
    for p in &projections {
        exprs.push(strip_as_alias(p).to_string());
    }

    // Rewrite source-table refs in FROM and the projection exprs.
    let from_clause_rewritten = rewrite_fused_source_refs(from_clause, source_tables);
    let exprs_rewritten: Vec<String> = exprs
        .iter()
        .map(|e| rewrite_fused_source_refs(e, source_tables))
        .collect();

    // Compose the live INSERT.
    let mut col_list = String::from("_id");
    for c in &user_cols {
        let sql_col = c.strip_prefix(':').unwrap_or(c.as_str());
        col_list.push_str(", ");
        col_list.push_str(sql_col);
        col_list.push_str("_id");
    }

    let mut select_list = String::new();
    // Project _id: wrap the fuser's blake3-blob in hex() to match the live
    // TEXT column. Note: the resulting hex string does NOT match what the
    // Rust `content_id` routine produces (which sorts pairs by key first),
    // but `<target>_facts` is only written via this path, so dedup is
    // self-consistent within the rule.
    select_list.push_str(&format!("hex({}) AS _id", exprs_rewritten[0]));
    for (col, expr) in user_cols.iter().zip(exprs_rewritten.iter().skip(1)) {
        let sql_col = col.strip_prefix(':').unwrap_or(col.as_str());
        select_list.push_str(", (SELECT id FROM sprf_strings WHERE value = ");
        select_list.push_str(expr);
        select_list.push_str(&format!(") AS {sql_col}_id"));
    }

    Some(format!(
        "INSERT OR IGNORE INTO {target_facts} ({col_list})\nSELECT {select_list}\n{from_clause_rewritten}",
    ))
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\'' || c == b'`' {
            buf.push(c as char);
            i += 1;
            while i < bytes.len() && bytes[i] != c {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    buf.push(bytes[i] as char);
                    buf.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                buf.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() {
                buf.push(c as char);
                i += 1;
            }
            continue;
        }
        match c {
            b'(' => {
                depth += 1;
                buf.push('(');
            }
            b')' => {
                depth -= 1;
                buf.push(')');
            }
            b',' if depth == 0 => {
                out.push(buf.trim().to_string());
                buf.clear();
            }
            other => buf.push(other as char),
        }
        i += 1;
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

fn strip_as_alias(s: &str) -> &str {
    // Look for the last ` AS ` (case-insensitive) at depth 0; return the
    // prefix. We do this by scanning backwards token-style: easier
    // approximation is splitting on case-insensitive " AS ".
    let lower = s.to_ascii_lowercase();
    let mut search_from = 0usize;
    let mut last: Option<usize> = None;
    while let Some(pos) = lower[search_from..].find(" as ") {
        let absolute = search_from + pos;
        last = Some(absolute);
        search_from = absolute + 4;
    }
    match last {
        Some(p) => s[..p].trim(),
        None => s.trim(),
    }
}

#[async_trait::async_trait]
impl SprfHandlers for SprfState {
    async fn lsp_open(&self, req: LspOpenReq) -> Result<(), SprfError> {
        self.ingest(req.uri, req.text, req.version);
        Ok(())
    }
    async fn lsp_change(&self, req: LspChangeReq) -> Result<(), SprfError> {
        self.ingest(req.uri, req.text, req.version);
        Ok(())
    }
    async fn lsp_close(&self, req: LspCloseReq) -> Result<(), SprfError> {
        self.docs.lock().unwrap().remove(&req.uri);
        Ok(())
    }
    async fn get_diags(&self, req: GetDiagsReq) -> Result<Vec<SprfDiag>, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs.get(&req.uri).ok_or(SprfError::UnknownDoc(req.uri))?;
        let out: Vec<SprfDiag> = d
            .parse_diags
            .iter()
            .chain(d.walk_diags.iter())
            .chain(d.runtime_diags.iter())
            .map(SprfDiag::from)
            .collect();
        Ok(out)
    }
    async fn get_inlays(&self, req: GetInlaysReq) -> Result<Vec<InlayProbe>, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs.get(&req.uri).ok_or(SprfError::UnknownDoc(req.uri))?;
        Ok(d.probes.clone())
    }
    async fn lsp_locate_dsl(&self, req: LspLocateDslReq) -> Result<LspLocateDslResp, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs
            .get(&req.uri)
            .ok_or(SprfError::UnknownDoc(req.uri.clone()))?;
        let mut hit: Option<(OpCall, usize)> = None;
        for p in &d.program {
            walk_pipe_for_dsl(p, req.byte as usize, &mut hit);
        }
        Ok(match hit {
            Some((call, body_byte)) => match &call.dsl {
                Some(dsl) => LspLocateDslResp {
                    op_name: Some(call.name.to_string()),
                    body_raw: Some(dsl.raw.to_string()),
                    body_off: dsl.span.lo,
                    body_byte: body_byte as u32,
                },
                None => LspLocateDslResp {
                    op_name: None,
                    body_raw: None,
                    body_off: 0,
                    body_byte: 0,
                },
            },
            None => LspLocateDslResp {
                op_name: None,
                body_raw: None,
                body_off: 0,
                body_byte: 0,
            },
        })
    }

    async fn lsp_hover(&self, req: LspHoverReq) -> Result<LspHoverResp, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs
            .get(&req.uri)
            .ok_or(SprfError::UnknownDoc(req.uri.clone()))?;
        let mut hit: Option<(OpCall, usize)> = None;
        for p in &d.program {
            walk_pipe_for_dsl(p, req.byte as usize, &mut hit);
        }
        if let Some(contents) = hit.and_then(|(call, body_byte)| {
            let dsl = call.dsl.as_ref()?;
            dsl_hover_with_doc(
                d,
                self.facts.as_ref(),
                &self.root,
                call.name.as_ref(),
                dsl.raw.as_bytes(),
                body_byte,
            )
        }) {
            return Ok(LspHoverResp {
                contents: Some(contents),
            });
        }

        let host_byte = req.byte;
        if let Some(contents) = runtime_hover_at(&d.runtime_hovers, host_byte) {
            return Ok(LspHoverResp {
                contents: Some(contents),
            });
        }

        let Some(probe) = d
            .probes
            .iter()
            .filter(|p| p.lo <= host_byte && host_byte <= p.hi)
            .min_by_key(|p| (p.hi - p.lo, p.lo, p.hi))
        else {
            return Ok(LspHoverResp { contents: None });
        };
        let mut op: Option<OpCall> = None;
        for p in &d.program {
            walk_pipe_for_op(p, host_byte as usize, &mut op);
        }
        let op_name = op
            .map(|c| c.name.to_string())
            .unwrap_or_else(|| "<op>".to_string());
        Ok(LspHoverResp {
            contents: Some(host_hover(&op_name, probe)),
        })
    }

    async fn lsp_completion(
        &self,
        req: LspCompletionReq,
    ) -> Result<Vec<lsp_types::CompletionItem>, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs
            .get(&req.uri)
            .ok_or(SprfError::UnknownDoc(req.uri.clone()))?;
        Ok(lsp_dsl_completion(
            d,
            self.facts.as_ref(),
            req.byte as usize,
        ))
    }

    async fn lsp_definition(&self, req: LspDefinitionReq) -> Result<LspDefinitionResp, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs
            .get(&req.uri)
            .ok_or(SprfError::UnknownDoc(req.uri.clone()))?;
        let Some(range) = lsp_dsl_definition(d, self.facts.as_ref(), req.byte as usize) else {
            return Ok(LspDefinitionResp { lo: None, hi: None });
        };
        Ok(LspDefinitionResp {
            lo: Some(range.lo),
            hi: Some(range.hi),
        })
    }

    async fn run(&self, req: RunReq) -> Result<RunReport, SprfError> {
        let run_start = std::time::Instant::now();
        let mut phases = RunPhaseTelemetry::default();
        let t = std::time::Instant::now();
        let src = std::fs::read_to_string(&req.path).map_err(|e| SprfError::Io(e.to_string()))?;
        phases.read_sprf_ms = t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        let (program, parse_diags) = host_parse(&src);
        phases.parse_ms = t.elapsed().as_secs_f64() * 1000.0;
        let dir = req
            .root
            .clone()
            .or_else(|| req.path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| self.root.clone());
        // `fs`'s branch-3 fallback walks the SCRIPT's own directory,
        // independent of `--root`. `--root` retargets `ctx.root` (used
        // by path/ast/read for relative resolution) and reaches `fs` as
        // an input cursor value (branch 2) via the seed below.
        let sprf_dir = req
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| dir.clone());
        let telemetry = std::env::var_os("SPREFA_TELEMETRY")
            .is_some()
            .then(|| Arc::new(PipelineTelemetry::new()));
        // Corpus root resolves the git toplevel for the no-read oracle;
        // recorded `_memo_deps` paths are absolute under it.
        let dir_for_hint = dir.clone();
        // Seed the process-global `SourceIndex` OnceLock NOW, from the
        // corpus root, before anything else can win it. On a cold run
        // the `warm_*` predicates below early-return WITHOUT touching
        // the OnceLock (empty `_memo_deps`), so the first real init
        // would otherwise be the seam probe with `hint="."` (the daemon
        // CWD — the sprefa worktree, NOT the corpus). That resolves the
        // wrong git toplevel; every corpus file `strip_prefix`-misses
        // and falls back to `Stat{mtime,size}`, whose mtime is unstable
        // across runs (a checkout rewrites it) → the warm-slice treats
        // the whole corpus as changed every run → full recompute. With
        // the corpus git root, tracked-clean files resolve to a stable
        // `Git(oid)` and the slice shrinks to the edited file.
        let _ = self.runtime_graph.source_index(dir_for_hint.as_path());
        let mut ctx = LowerCtx::new(self.facts.clone(), dir)
            .with_sprf_dir(sprf_dir)
            .with_sprf_store(self.sprf_store.clone())
            .with_config(self.config.clone());
        if let Some(t) = &telemetry {
            ctx = ctx.with_telemetry(t.clone());
        }
        // Warm-skip: on a re-run sharing a persistent `--fact-db`, files
        // whose no-read identity matches the recorded `_memo_deps`
        // identity are UNCHANGED. `fs` must not emit them — their `hits`
        // rows are already materialized and retraction is owner-scoped,
        // so an un-rendered owner keeps its rows. Self-gating: a cold
        // run (`_memo_deps` empty, e.g. in-memory store) yields an
        // always-false predicate ⇒ full corpus walk, no behavior change.
        ctx = ctx.with_warm_skip(
            self.runtime_graph.warm_skip_predicate(dir_for_hint.as_path()),
        );
        // Warm dirty-slice: on a re-run, feed `fs` exactly the changed
        // paths (no 63k tree walk). `None` on a cold run ⇒ full walk.
        ctx = ctx.with_warm_slice(
            self.runtime_graph
                .warm_changed_paths(dir_for_hint.as_path())
                .map(std::sync::Arc::new),
        );
        let t = std::time::Instant::now();
        let (pipes, walk_diags) = walk_program(&program, &self.registry, &mut ctx);
        phases.lower_ms = t.elapsed().as_secs_f64() * 1000.0;

        // Smaller caps multiply per-batch lock + transaction overhead in
        // batched sinks (FactWrite, subscription compact). Default to an
        // insane cap so a whole-corpus run collapses to ~one transaction
        // per stage instead of hundreds. Env-overridable.
        let runtime_diags = Arc::new(BufferDiagSink::new());
        let batch_cap = std::env::var("SPREFA_BATCH_CAP")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(1 << 20);
        // The memo/replay/reconcile seam IS installed on the one-shot
        // `run` path. It is now SAFE: the seam's staleness
        // (`Memo::probe_ch` → `is_stale_content`) is decided by
        // re-hashing each recorded dep's bytes on disk vs the hash the
        // component recorded when it last read them. That decision is a
        // pure function of the file bytes and consults neither
        // `FactStoreClock` nor any event-layer `bump()`. So across two
        // `sprefa-run` invocations sharing `--fact-db`, an edited .c
        // file's recorded content hash no longer matches its on-disk
        // bytes → the seam probes STALE and the edit is detected
        // (linux `hits` 16628, correct); every unchanged file's hash
        // still matches → REPLAY, so its ~tree-sitter parse is skipped.
        // The ~7s re-read of 1.3GB to recompute hashes is expected and
        // dwarfed by the ~20s parse it lets us skip.
        let memo_seam = crate::memo_seam_impl::V4MemoSeam::new(
            self.runtime_graph.clone(),
        );
        let opts = ExpandOpts::default()
            .with_batch_cap(batch_cap)
            .with_bus(self.bus.clone())
            .with_diag(runtime_diags.clone())
            .with_runtime(self.runtime_graph.clone())
            .with_memo_seam::<crate::Cursor>(memo_seam);
        let opts = match telemetry.as_ref() {
            Some(t) => opts.with_telemetry(t.effect.clone()),
            None => opts,
        };
        // `--root` reaches `fs` as INPUT: seed the bootstrap cursor's
        // focal value with the root path so `fs`'s branch 2 (path-value
        // → fs-walk) fires. With no `--root` the seed stays the empty
        // default cursor and `fs` falls to branch 3 (script dir). A
        // value-bearing seed is inert for pipes that don't read it
        // (e.g. literal/`repo()` heads overwrite the focal cursor).
        let seed_cursor: Cursor = match req.root.as_ref() {
            Some(root) => {
                let mut c = Cursor::default();
                c.set_value(root.to_string_lossy().to_string());
                c
            }
            None => Cursor::default(),
        };
        let mut n = 0;
        let mut run_stats = effect_runtime::v2::ExpandStats::default();
        for (idx, fused) in pipes.into_iter().enumerate() {
            // Full-SQL fused rules execute their precomputed INSERT...SELECT
            // directly against the SQLite fact store. This bypasses the
            // per-cursor expand path entirely: zero cursors cross Rust for
            // the materialization. Falls through to legacy expand if the
            // backing store is not SQLite (e.g. MemFactStore).
            if fused.kind() == FusedKind::FullSql {
                let t = std::time::Instant::now();
                let executed = run_fused_sql(
                    self.facts.as_ref(),
                    self.runtime_graph.clock().as_ref(),
                    &fused,
                );
                phases.expand_ms += t.elapsed().as_secs_f64() * 1000.0;
                if executed {
                    n += 1;
                    continue;
                }
                // Store didn't support direct execution (MemFactStore) or
                // SQL execution failed (e.g. fuser emits structurally
                // invalid SQL today; see chat_log/20260514.7). Fall
                // through to the legacy expand path so the rule still
                // runs.
            }

            let t = std::time::Instant::now();
            let pipe = fused.into_pipe();
            let pipe = match telemetry.as_ref() {
                Some(t) => t.wrap_pipe(pipe),
                None => pipe,
            };
            let identity = program
                .get(idx)
                .map(|pipe_ast| stable_pipe_identity(&req.path, pipe_ast))
                .unwrap_or_else(|| fallback_pipe_identity(&req.path, idx));
            let inst = self.mount_pipe(pipe, identity);
            phases.wrap_mount_ms += t.elapsed().as_secs_f64() * 1000.0;

            let t = std::time::Instant::now();
            let stats = expand(
                inst.as_ref(),
                self.queue.clone(),
                vec![Arc::new(seed_cursor.clone())],
                opts.clone(),
            );
            phases.expand_ms += t.elapsed().as_secs_f64() * 1000.0;
            add_expand_stats(&mut run_stats, stats);
            n += 1;
        }
        let generation = *self.next_instance_id.lock().unwrap();
        let t = std::time::Instant::now();
        self.sprf_store.flush();
        self.facts.commit(generation, Some(&self.bus));
        phases.commit_ms = t.elapsed().as_secs_f64() * 1000.0;
        let t = std::time::Instant::now();
        self.drain_runtime_until_idle(opts);
        // Expand fully drained: persist the in-RAM memo + memo-deps in
        // ONE transaction each (cost ∝ changes, not corpus). This is
        // the single sqlite touch for the memo layer per run — no
        // per-row writes (the cold N+1 that was 63k transactions).
        self.runtime_graph.flush();
        self.sprf_store.flush();
        phases.resume_ms = t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        let mut tables: Vec<String> = Vec::new();
        collect_rule_tables(&program, &mut tables);
        phases.collect_tables_ms = t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        let parse_diags = parse_diags.iter().map(SprfDiag::from).collect();
        let walk_diags = walk_diags.iter().map(SprfDiag::from).collect();
        let runtime_diags = runtime_diags
            .snapshot()
            .iter()
            .map(SprfDiag::from)
            .collect();
        phases.report_ms = t.elapsed().as_secs_f64() * 1000.0;

        let telemetry = telemetry.map(|t| {
            let mut snapshot = t.snapshot_with_phases(run_start.elapsed(), run_stats, phases);
            snapshot.fact_store = self.facts.stats().map(Into::into);
            snapshot.runtime_graph = self.runtime_graph.stats();
            snapshot
        });

        Ok(RunReport {
            parse_diags,
            walk_diags,
            runtime_diags,
            pipes: n,
            tables,
            telemetry,
        })
    }

    async fn refs_at(&self, req: RefsAtReq) -> Result<RefsAtResp, SprfError> {
        let store = &self.sprf_store;
        let file = match store.find_file_by_path(&req.path) {
            Some(f) => f,
            None => return Ok(RefsAtResp { hits: Vec::new() }),
        };
        let refs = store.find_refs_in(file, req.byte);
        let mut hits = Vec::with_capacity(refs.len());
        for r in refs {
            let coord = match store.coord_of(r) {
                Some(c) => c,
                None => continue,
            };
            let fs_path = store.path_of(coord.fs).map(|a| a.to_string());
            hits.push(RefHit {
                ref_id: r.0,
                coord: CoordWire {
                    repo: coord.repo,
                    rev: coord.rev,
                    fs: coord.fs,
                    fs_path,
                    lo: coord.lo,
                    hi: coord.hi,
                },
            });
        }
        Ok(RefsAtResp { hits })
    }

    #[cfg(feature = "ghcache")]
    async fn notify_ghcache_change(
        &self,
        req: NotifyGhcacheChangeReq,
    ) -> Result<Vec<DirtyNotice>, SprfError> {
        let notices = self.dispatch_ghcache_change(&req.change);
        self.drain_ready();
        Ok(notices)
    }

    async fn get_fact_table(&self, req: GetFactTableReq) -> Result<FactTable, SprfError> {
        let total = self.facts.len(&req.name);
        let raw = self.facts.rows_of(&req.name);
        let take = req.limit.unwrap_or(usize::MAX).min(raw.len());
        let rows = raw
            .iter()
            .take(take)
            .map(|c| FactRow {
                fields: c
                    .terms
                    .iter()
                    .filter(|term| term.name.as_ref() != FOCAL_TERM)
                    .map(|term| (term.name.to_string(), term.value.to_string()))
                    .collect(),
            })
            .collect();
        Ok(FactTable {
            name: req.name,
            total,
            rows,
        })
    }
}

#[derive(Default)]
struct ProbeAgg {
    count: u32,
    sample_value: Option<String>,
    sample_terms: Vec<(String, String)>,
}

fn split_runtime_hovers(rows: Vec<Diag>) -> (Vec<RuntimeHover>, Vec<Diag>) {
    let mut hovers = Vec::new();
    let mut diags = Vec::new();
    for row in rows {
        if row.code.as_ref() == LSP_HOVER_CODE {
            if let Some(span) = row.span {
                hovers.push(RuntimeHover {
                    lo: span.lo,
                    hi: span.hi,
                    contents: row.message,
                });
            }
        } else {
            diags.push(row);
        }
    }
    hovers.sort_by_key(|h| (h.lo, h.hi, h.contents.clone()));
    (hovers, diags)
}

fn runtime_hover_at(hovers: &[RuntimeHover], host_byte: u32) -> Option<String> {
    hovers
        .iter()
        .filter(|h| h.lo <= host_byte && host_byte <= h.hi)
        .min_by_key(|h| (h.hi - h.lo, h.lo, h.hi))
        .map(|h| h.contents.clone())
}

fn host_hover(op_name: &str, probe: &InlayProbe) -> String {
    let mut lines = vec![
        format!("`{op_name}`"),
        format!("cursors: {}", probe.count),
        format!("span: {}..{}", probe.lo, probe.hi),
    ];
    if let Some(value) = &probe.sample_value {
        if !value.is_empty() {
            lines.push(format!("value: `{value}`"));
        }
    }
    if !probe.sample_terms.is_empty() {
        let terms = probe
            .sample_terms
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("terms: `{terms}`"));
    }
    lines.join("\n")
}

fn sample_text(s: &str, max: usize) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let compact = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max {
        return Some(compact);
    }
    let mut out = compact
        .chars()
        .take(max.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    Some(out)
}

fn stable_pipe_identity(path: &std::path::Path, pipe: &PipeAst) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(path.to_string_lossy().as_bytes());
    h.update(b"\0pipe\0");
    hash_pipe_ast(&mut h, pipe);
    hash_to_nonzero_u64(h.finalize().as_bytes())
}

fn fallback_pipe_identity(path: &std::path::Path, idx: usize) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(path.to_string_lossy().as_bytes());
    h.update(b"\0pipe-index\0");
    h.update(&idx.to_le_bytes());
    hash_to_nonzero_u64(h.finalize().as_bytes())
}

fn add_expand_stats(
    total: &mut effect_runtime::v2::ExpandStats,
    next: effect_runtime::v2::ExpandStats,
) {
    total.rendered += next.rendered;
    total.emitted += next.emitted;
    total.terminal += next.terminal;
    total.parked = total.parked.max(next.parked);
}

fn dsl_hover(root: &Path, op_name: &str, body: &[u8], body_byte: usize) -> Option<String> {
    if op_name == "path" {
        return Some(path_hover(root, body));
    }
    let dsl: Box<dyn Dsl> = match op_name {
        "sql" => Box::new(crate::cst::dsls::sql::SqlDsl::new()),
        "json" => Box::new(crate::cst::dsls::json::JsonDsl::new()),
        "render" | "render_markdown" | "render.markdown" => {
            Box::new(crate::cst::dsls::markdown::MarkdownDsl::new())
        }
        "re" => Box::new(crate::cst::dsls::re::ReDsl::new()),
        "glob" => Box::new(crate::cst::dsls::glob::GlobDsl::new()),
        _ => return None,
    };
    let hover = dsl.lsp()?.hover(body, body_byte)?;
    Some(hover_contents_to_string(hover.contents))
}

fn dsl_hover_with_doc(
    doc: &DocState,
    facts: &dyn FactStore<Cursor>,
    root: &Path,
    op_name: &str,
    body: &[u8],
    body_byte: usize,
) -> Option<String> {
    if op_name == "sql" {
        let ctx = sql_lsp_ctx(doc, facts, None);
        let hover = SqlDsl::new().hover_with_ctx(body, body_byte, &ctx)?;
        return Some(hover_contents_to_string(hover.contents));
    }
    dsl_hover(root, op_name, body, body_byte)
}

fn lsp_dsl_completion(
    doc: &DocState,
    facts: &dyn FactStore<Cursor>,
    host_byte: usize,
) -> Vec<lsp_types::CompletionItem> {
    let Some(hit) = find_dsl_hit(&doc.program, host_byte) else {
        return Vec::new();
    };
    let call = hit.call;
    let Some(dsl) = call.dsl.as_ref() else {
        return Vec::new();
    };
    match call.name.as_ref() {
        "sql" => SqlDsl::new().completions_with_ctx(
            dsl.raw.as_bytes(),
            hit.body_byte,
            &sql_lsp_ctx(doc, facts, Some(&hit.prefix)),
        ),
        _ => Vec::new(),
    }
}

fn lsp_dsl_definition(
    doc: &DocState,
    facts: &dyn FactStore<Cursor>,
    host_byte: usize,
) -> Option<effect_runtime::v2::ByteRange> {
    let hit = find_dsl_hit(&doc.program, host_byte)?;
    if hit.call.name.as_ref() != "sql" {
        return None;
    }
    let dsl = hit.call.dsl.as_ref()?;
    let ctx = sql_lsp_ctx(doc, facts, Some(&hit.prefix));
    let table = SqlDsl::new().table_name_at(dsl.raw.as_bytes(), hit.body_byte, &ctx)?;
    rule_def_span(&doc.program, &table)
}

fn sql_lsp_diagnostics(program: &[PipeAst], facts: &dyn FactStore<Cursor>) -> Vec<Diag> {
    let mut out = Vec::new();
    for hit in all_sql_hits(program) {
        let Some(dsl) = hit.call.dsl.as_ref() else {
            continue;
        };
        let ctx = sql_lsp_ctx_from_program(program, facts, Some(&hit.prefix));
        for diag in SqlDsl::new().diagnostics_with_ctx(dsl.raw.as_bytes(), &ctx) {
            let shifted = diag.byte_range.start + dsl.span.lo as usize
                ..diag.byte_range.end + dsl.span.lo as usize;
            out.push(cst_diag_to_runtime(diag, shifted));
        }
    }
    out
}

fn cst_diag_to_runtime(diag: crate::cst::diag::Diag, shifted: std::ops::Range<usize>) -> Diag {
    let base = match diag.severity {
        crate::cst::diag::Severity::Error => Diag::error(diag.code, diag.message),
        crate::cst::diag::Severity::Warning => Diag::warn(diag.code, diag.message),
        crate::cst::diag::Severity::Info => Diag::info(diag.code, diag.message),
        crate::cst::diag::Severity::Hint => Diag::hint(diag.code, diag.message),
    };
    base.with_span(shifted.start as u32, shifted.end as u32)
}

fn sql_lsp_ctx(
    doc: &DocState,
    facts: &dyn FactStore<Cursor>,
    prefix: Option<&[OpCall]>,
) -> SqlLspCtx {
    sql_lsp_ctx_from_program(&doc.program, facts, prefix)
}

fn sql_lsp_ctx_from_program(
    program: &[PipeAst],
    facts: &dyn FactStore<Cursor>,
    prefix: Option<&[OpCall]>,
) -> SqlLspCtx {
    let mut rule_names = Vec::new();
    collect_rule_tables(program, &mut rule_names);
    let rule_tables = rule_names
        .into_iter()
        .filter_map(|name| sql_table_from_store(facts, &name))
        .collect();
    let core_tables = [
        crate::store::STRINGS_TABLE,
        crate::store::FILES_TABLE,
        crate::store::WHERE_BYTES_TABLE,
        crate::store::REPOS_TABLE,
        crate::store::REVS_TABLE,
        crate::store::PATHS_TABLE,
        crate::store::STRING_OBSERVATIONS_TABLE,
    ]
    .into_iter()
    .filter_map(|name| sql_table_from_store(facts, name))
    .collect();
    let mut input_cols = vec![SqlCol::text("__cursor_idx"), SqlCol::text("value")];
    if let Some(prefix) = prefix {
        for term in input_terms_from_prefix(prefix) {
            if !input_cols.iter().any(|col| col.name == term) {
                input_cols.push(SqlCol::text(term));
            }
        }
    }
    SqlLspCtx {
        input_cols,
        rule_tables,
        core_tables,
    }
}

fn sql_table_from_store(facts: &dyn FactStore<Cursor>, name: &str) -> Option<SqlTable> {
    let cols = facts.declared_cols(name)?;
    Some(SqlTable {
        name: name.to_string(),
        cols: cols.into_iter().map(SqlCol::text).collect(),
    })
}

#[derive(Clone)]
struct DslHit {
    call: OpCall,
    body_byte: usize,
    prefix: Vec<OpCall>,
}

fn find_dsl_hit(program: &[PipeAst], host_byte: usize) -> Option<DslHit> {
    let mut hit = None;
    for pipe in program {
        walk_pipe_for_dsl_hit(pipe, host_byte, &mut Vec::new(), &mut hit);
    }
    hit
}

fn all_sql_hits(program: &[PipeAst]) -> Vec<DslHit> {
    let mut out = Vec::new();
    for pipe in program {
        collect_sql_hits(pipe, &mut Vec::new(), &mut out);
    }
    out
}

fn walk_pipe_for_dsl_hit(
    pipe: &PipeAst,
    host_byte: usize,
    prefix: &mut Vec<OpCall>,
    hit: &mut Option<DslHit>,
) {
    for step in &pipe.steps {
        if let Some(dsl) = &step.dsl {
            let lo = dsl.span.lo as usize;
            let hi = dsl.span.hi as usize;
            if host_byte >= lo && host_byte <= hi {
                *hit = Some(DslHit {
                    call: step.clone(),
                    body_byte: (host_byte - lo).min(hi - lo),
                    prefix: prefix.clone(),
                });
            }
        }
        if let Some(block) = &step.block {
            walk_pipe_for_dsl_hit(block, host_byte, &mut Vec::new(), hit);
        }
        prefix.push(step.clone());
    }
}

fn collect_sql_hits(pipe: &PipeAst, prefix: &mut Vec<OpCall>, out: &mut Vec<DslHit>) {
    for step in &pipe.steps {
        if step.name.as_ref() == "sql" && step.dsl.is_some() {
            out.push(DslHit {
                call: step.clone(),
                body_byte: 0,
                prefix: prefix.clone(),
            });
        }
        if let Some(block) = &step.block {
            collect_sql_hits(block, &mut Vec::new(), out);
        }
        prefix.push(step.clone());
    }
}

fn input_terms_from_prefix(prefix: &[OpCall]) -> Vec<String> {
    let mut out = Vec::new();
    for step in prefix {
        for arg in &step.args {
            collect_term_name_from_slot(arg.raw.as_ref(), &mut out);
        }
    }
    out
}

fn collect_term_name_from_slot(raw: &str, out: &mut Vec<String>) {
    let value = raw
        .split_once(':')
        .map(|(_, rhs)| rhs)
        .unwrap_or(raw)
        .trim()
        .trim_end_matches('?')
        .trim_start_matches(':')
        .trim();
    if value.is_empty() || value.contains('`') || value.contains('(') || value.contains('$') {
        return;
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        && !out.iter().any(|seen| seen == value)
    {
        out.push(value.to_string());
    }
}

fn rule_def_span(program: &[PipeAst], table: &str) -> Option<effect_runtime::v2::ByteRange> {
    for pipe in program {
        for step in &pipe.steps {
            if matches!(step.name.as_ref(), "rule" | "fact" | "fact_write") {
                if let Some(first) = step.args.first() {
                    let raw = first.raw.trim();
                    let name = raw.strip_prefix(':').unwrap_or(raw).trim();
                    if name == table {
                        return Some(first.span);
                    }
                }
            }
            if let Some(block) = &step.block {
                if let Some(span) = rule_def_span(std::slice::from_ref(block), table) {
                    return Some(span);
                }
            }
        }
    }
    None
}

fn path_hover(root: &Path, body: &[u8]) -> String {
    let Ok(raw) = std::str::from_utf8(body) else {
        return "path\ninvalid utf-8".to_string();
    };
    if raw.contains("${") {
        return "path template\nchecks existence at runtime".to_string();
    }
    let path = resolve_path_text(root, raw);
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_dir() => format!("path\ndirectory\n{}", path.display()),
        Ok(meta) if meta.is_file() => format!("path\nfile\n{}", path.display()),
        Ok(_) => format!("path\nexists\n{}", path.display()),
        Err(err) => format!("path\nmissing\n{} ({err})", path.display()),
    }
}

fn hover_contents_to_string(contents: lsp_types::HoverContents) -> String {
    match contents {
        lsp_types::HoverContents::Scalar(marked) => marked_string_to_string(marked),
        lsp_types::HoverContents::Array(items) => items
            .into_iter()
            .map(marked_string_to_string)
            .collect::<Vec<_>>()
            .join("\n"),
        lsp_types::HoverContents::Markup(markup) => markup.value,
    }
}

fn marked_string_to_string(marked: lsp_types::MarkedString) -> String {
    match marked {
        lsp_types::MarkedString::String(s) => s,
        lsp_types::MarkedString::LanguageString(s) => s.value,
    }
}

fn hash_to_nonzero_u64(bytes: &[u8; 32]) -> u64 {
    let mut out = [0u8; 8];
    out.copy_from_slice(&bytes[..8]);
    u64::from_le_bytes(out).max(1)
}

fn hash_pipe_ast(h: &mut blake3::Hasher, pipe: &PipeAst) {
    h.update(&(pipe.steps.len() as u64).to_le_bytes());
    for step in &pipe.steps {
        hash_op_call(h, step);
    }
}

fn hash_op_call(h: &mut blake3::Hasher, call: &OpCall) {
    h.update(call.name.as_bytes());
    h.update(&[call.force as u8, call.predicate as u8, call.apply as u8]);
    if let Some(flow) = &call.flow {
        h.update(b"\0flow\0");
        h.update(flow.raw.as_bytes());
    }
    h.update(&(call.args.len() as u64).to_le_bytes());
    for arg in &call.args {
        h.update(b"\0arg\0");
        h.update(arg.raw.as_bytes());
    }
    if let Some(dsl) = &call.dsl {
        h.update(b"\0dsl\0");
        h.update(dsl.raw.as_bytes());
    }
    if let Some(block) = &call.block {
        h.update(b"\0block\0");
        hash_pipe_ast(h, block);
    }
}

fn walk_pipe_for_dsl(p: &PipeAst, host_byte: usize, hit: &mut Option<(OpCall, usize)>) {
    for step in &p.steps {
        walk_step_for_dsl(step, host_byte, hit);
    }
}

fn walk_pipe_for_op(p: &PipeAst, host_byte: usize, hit: &mut Option<OpCall>) {
    for step in &p.steps {
        let lo = step.span.lo as usize;
        let hi = step.span.hi as usize;
        if lo <= host_byte && host_byte <= hi {
            match hit {
                Some(prev) if (prev.span.hi - prev.span.lo) <= (step.span.hi - step.span.lo) => {}
                _ => *hit = Some(step.clone()),
            }
        }
        if let Some(block) = &step.block {
            walk_pipe_for_op(block, host_byte, hit);
        }
    }
}

fn walk_step_for_dsl(call: &OpCall, host_byte: usize, hit: &mut Option<(OpCall, usize)>) {
    if let Some(dsl) = &call.dsl {
        let lo = dsl.span.lo as usize;
        let hi = dsl.span.hi as usize;
        if host_byte >= lo && host_byte <= hi {
            // Deepest containing body wins (overwrites outer hits).
            *hit = Some((call.clone(), (host_byte - lo).min(hi - lo)));
        }
    }
    if let Some(block) = &call.block {
        walk_pipe_for_dsl(block, host_byte, hit);
    }
}

/// Walk the AST scanning for `rule(:NAME, …)` ops; collects unique
/// table names. Recurses into block bodies because rules can nest.
fn collect_rule_tables(program: &[PipeAst], out: &mut Vec<String>) {
    for p in program {
        for op in &p.steps {
            if matches!(&*op.name, "rule" | "fact" | "fact_write") {
                if let Some(first) = op.args.first() {
                    let raw = first.raw.trim();
                    let name = raw.strip_prefix(':').unwrap_or(raw).trim();
                    if !name.is_empty() && !out.iter().any(|n| n == name) {
                        out.push(name.to_string());
                    }
                }
            }
            if let Some(block) = &op.block {
                collect_rule_tables(std::slice::from_ref(block), out);
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────
// Convenience constructor
// ───────────────────────────────────────────────────────────────────

pub fn build_in_process(root: PathBuf) -> (Arc<SprfState>, InProcessClient) {
    let state = Arc::new(SprfState::new(root));
    let router = build_router(state.clone());
    (state, InProcessClient::new(router))
}

// ───────────────────────────────────────────────────────────────────
// Smoke
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_get_diags_close_roundtrip() {
        let (_state, client) = build_in_process(std::env::temp_dir());

        client
            .lsp_open(LspOpenReq {
                uri: "file:///x.sprf".into(),
                text: "".into(),
                version: 1,
            })
            .await
            .unwrap();

        let diags = client
            .get_diags(GetDiagsReq {
                uri: "file:///x.sprf".into(),
            })
            .await
            .unwrap();
        assert_eq!(diags.len(), 0);

        let inlays = client
            .get_inlays(GetInlaysReq {
                uri: "file:///x.sprf".into(),
            })
            .await
            .unwrap();
        assert!(inlays.is_empty());

        client
            .lsp_close(LspCloseReq {
                uri: "file:///x.sprf".into(),
            })
            .await
            .unwrap();

        let err = client
            .get_diags(GetDiagsReq {
                uri: "file:///x.sprf".into(),
            })
            .await
            .unwrap_err();
        assert!(matches!(err, SprfError::UnknownDoc(_)));
    }

    #[tokio::test]
    async fn real_source_through_router() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "rule(:greet) { str `hello world` }";
        client
            .lsp_open(LspOpenReq {
                uri: "file:///hello.sprf".into(),
                text: src.into(),
                version: 1,
            })
            .await
            .unwrap();
        let _ = client
            .get_diags(GetDiagsReq {
                uri: "file:///hello.sprf".into(),
            })
            .await
            .unwrap();
        let _ = client
            .get_inlays(GetInlaysReq {
                uri: "file:///hello.sprf".into(),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn lsp_completion_inside_sql_body_returns_rule_tables() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "rule(:frontend_hooks, OP?, FILE?);\nsql`SELECT * FROM `;";
        client
            .lsp_open(LspOpenReq {
                uri: "file:///sql.sprf".into(),
                text: src.into(),
                version: 1,
            })
            .await
            .unwrap();

        let items = client
            .lsp_completion(LspCompletionReq {
                uri: "file:///sql.sprf".into(),
                byte: (src.find("FROM ").unwrap() + "FROM ".len()) as u32,
            })
            .await
            .unwrap();
        let labels: Vec<String> = items.into_iter().map(|item| item.label).collect();

        assert!(labels.contains(&"frontend_hooks".to_string()), "{labels:?}");
        assert!(labels.contains(&"_strings".to_string()), "{labels:?}");
    }

    #[tokio::test]
    async fn lsp_completion_inside_sql_body_returns_alias_columns() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "rule(:frontend_hooks, OP?, FILE?);\nsql`SELECT hooks. FROM input JOIN frontend_hooks AS hooks ON hooks.OP = input.OP`;";
        client
            .lsp_open(LspOpenReq {
                uri: "file:///sql.sprf".into(),
                text: src.into(),
                version: 1,
            })
            .await
            .unwrap();

        let items = client
            .lsp_completion(LspCompletionReq {
                uri: "file:///sql.sprf".into(),
                byte: (src.find("hooks.").unwrap() + "hooks.".len()) as u32,
            })
            .await
            .unwrap();
        let labels: Vec<String> = items.into_iter().map(|item| item.label).collect();

        assert_eq!(
            labels,
            vec!["hooks.OP".to_string(), "hooks.FILE".to_string()]
        );
    }

    #[tokio::test]
    async fn lsp_completion_inside_sql_body_returns_input_terms() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "rule(:openapi_ops, OP?);\nopenapi_ops(OP?) > sql`SELECT input.`;";
        client
            .lsp_open(LspOpenReq {
                uri: "file:///sql.sprf".into(),
                text: src.into(),
                version: 1,
            })
            .await
            .unwrap();

        let items = client
            .lsp_completion(LspCompletionReq {
                uri: "file:///sql.sprf".into(),
                byte: (src.find("input.").unwrap() + "input.".len()) as u32,
            })
            .await
            .unwrap();
        let labels: Vec<String> = items.into_iter().map(|item| item.label).collect();

        assert!(
            labels.contains(&"input.__cursor_idx".to_string()),
            "{labels:?}"
        );
        assert!(labels.contains(&"input.value".to_string()), "{labels:?}");
        assert!(labels.contains(&"input.OP".to_string()), "{labels:?}");
    }

    #[tokio::test]
    async fn lsp_diags_include_sql_unknown_table() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "sql`SELECT * FROM missing_hooks`;";
        client
            .lsp_open(LspOpenReq {
                uri: "file:///sql.sprf".into(),
                text: src.into(),
                version: 1,
            })
            .await
            .unwrap();

        let diags = client
            .get_diags(GetDiagsReq {
                uri: "file:///sql.sprf".into(),
            })
            .await
            .unwrap();

        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "sql/unknown-table");
        assert_eq!(diags[0].message, "unknown SQL table `missing_hooks`");
        assert_eq!(
            (diags[0].lo, diags[0].hi),
            (
                Some((src.find("missing_hooks").unwrap()) as u32),
                Some((src.find("missing_hooks").unwrap() + "missing_hooks".len()) as u32)
            )
        );
    }

    #[tokio::test]
    async fn lsp_hover_inside_sql_table_uses_schema_ctx() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "rule(:frontend_hooks, OP?, FILE?);\nsql`SELECT * FROM frontend_hooks`;";
        client
            .lsp_open(LspOpenReq {
                uri: "file:///sql.sprf".into(),
                text: src.into(),
                version: 1,
            })
            .await
            .unwrap();

        let hover = client
            .lsp_hover(LspHoverReq {
                uri: "file:///sql.sprf".into(),
                byte: src.find("frontend_hooks`").unwrap() as u32,
            })
            .await
            .unwrap();

        let contents = hover.contents.expect("hover contents");
        assert!(
            contents.contains("SQL relation `frontend_hooks`"),
            "{contents:?}"
        );
        assert!(contents.contains("OP, FILE"), "{contents:?}");
    }

    #[tokio::test]
    async fn lsp_definition_inside_sql_table_returns_rule_span() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "rule(:frontend_hooks, OP?, FILE?);\nsql`SELECT * FROM frontend_hooks`;";
        client
            .lsp_open(LspOpenReq {
                uri: "file:///sql.sprf".into(),
                text: src.into(),
                version: 1,
            })
            .await
            .unwrap();

        let def = client
            .lsp_definition(LspDefinitionReq {
                uri: "file:///sql.sprf".into(),
                byte: src.find("frontend_hooks`").unwrap() as u32,
            })
            .await
            .unwrap();

        assert_eq!(
            (def.lo, def.hi),
            (
                Some(src.find(":frontend_hooks").unwrap() as u32),
                Some((src.find(":frontend_hooks").unwrap() + ":frontend_hooks".len()) as u32)
            )
        );
    }

    #[tokio::test]
    async fn run_report_includes_runtime_diags() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("warn.sprf");
        std::fs::write(
            &path,
            "`hello` > term_bind(:WORD) > lsp_warn(:demo_warn)`word ${WORD}`;",
        )
        .unwrap();

        let (_state, client) = build_in_process(dir.path().to_path_buf());
        let report = client
            .run(RunReq {
                path,
                root: Some(dir.path().to_path_buf()),
            })
            .await
            .unwrap();

        assert_eq!(report.runtime_diags.len(), 1);
        assert_eq!(report.runtime_diags[0].severity, "warning");
        assert_eq!(report.runtime_diags[0].code, "demo_warn");
        assert_eq!(report.runtime_diags[0].message, "word hello");
    }
}
