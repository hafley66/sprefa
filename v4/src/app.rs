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
use std::path::PathBuf;
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
    attach_dirty_to_queue, expand, BufferProbeSink, Diag, DiagSink, EventBus, ExpandOpts,
    Component, FactStore, MemFactStore, MemQueue, Node, Pipe, PipeInstance, ProbeSink, Purity,
    QueueBackend, SqliteFactStore, SqliteQueue,
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

use crate::compile::ast::{OpCall, PipeAst};
use crate::compile::parse::host_parse;
use crate::compile::walk::walk_program;
use crate::cst::dsl::Dsl;
pub use crate::git_watch::{DirtyNotice, GhcacheChangeReq, NotifyGhcacheChangeReq};
use crate::git_watch::{dirty_notice, ghcache_dirty_notices};
use crate::lower::{default_registry, LowerCtx, Registry};
use crate::Cursor;

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
            SprfError::Io(_)         => StatusCode::INTERNAL_SERVER_ERROR,
            SprfError::Wire(_)       => StatusCode::BAD_REQUEST,
            SprfError::Internal(_)   => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (code, Json(self)).into_response()
    }
}

// ───────────────────────────────────────────────────────────────────
// Request / Response payloads
// ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspOpenReq   { pub uri: String, pub text: String, pub version: i32 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspChangeReq { pub uri: String, pub text: String, pub version: i32 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspCloseReq  { pub uri: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetDiagsReq  { pub uri: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetInlaysReq { pub uri: String }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReq       { pub path: PathBuf, pub root: Option<PathBuf> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlayProbe   { pub lo: u32, pub hi: u32, pub count: u32 }

/// Wire shape for `effect_runtime::v2::Diag` — that crate intentionally
/// has no serde dep, so we ferry diags through this DTO at the seam.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SprfDiag {
    pub lo:       Option<u32>,
    pub hi:       Option<u32>,
    pub severity: String,    // "error" | "warning" | "info" | "hint"
    pub code:     String,
    pub message:  String,
}

impl From<&effect_runtime::v2::Diag> for SprfDiag {
    fn from(d: &effect_runtime::v2::Diag) -> Self {
        use effect_runtime::v2::Severity;
        Self {
            lo: d.span.map(|s| s.lo),
            hi: d.span.map(|s| s.hi),
            severity: match d.severity {
                Severity::Error => "error",
                Severity::Warn  => "warning",
                Severity::Info  => "info",
                Severity::Hint  => "hint",
            }.into(),
            code:    d.code.to_string(),
            message: d.message.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport    {
    pub parse_diags: Vec<SprfDiag>,
    pub walk_diags:  Vec<SprfDiag>,
    pub runtime_diags: Vec<SprfDiag>,
    pub pipes:       usize,
    /// Rule-table names harvested from the AST (one per `rule(:NAME)`).
    pub tables:      Vec<String>,
}

struct BufferDiagSink {
    rows: Mutex<Vec<Diag>>,
}

impl BufferDiagSink {
    fn new() -> Self {
        Self { rows: Mutex::new(Vec::new()) }
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
    pub uri:  String,
    /// Host-source byte offset (the LSP server resolves line/utf16-col
    /// to bytes before crossing this boundary).
    pub byte: u32,
}

/// `op_name` is `None` when `byte` falls outside every dsl body in the
/// cached program — host position, not a dsl position.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspLocateDslResp {
    pub op_name:   Option<String>,
    pub body_raw:  Option<String>,
    pub body_off:  u32,
    pub body_byte: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspHoverReq {
    pub uri:  String,
    pub byte: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LspHoverResp {
    pub contents: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetFactTableReq {
    pub name:  String,
    /// Cap on rows returned. None = no cap (still bounded by store).
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactRow { pub fields: Vec<(String, String)> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FactTable {
    pub name:  String,
    /// Total rows in the store (pre-limit).
    pub total: usize,
    pub rows:  Vec<FactRow>,
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
pub struct RefsAtReq  { pub path: String, pub byte: u32 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefsAtResp { pub hits: Vec<RefHit> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RefHit {
    pub ref_id: u64,
    pub coord:  CoordWire,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CoordWire {
    pub repo:    u32,
    pub rev:     u32,
    pub fs:      u64,
    /// `path_of(fs)` resolution. None if the FileId has no `_files` row
    /// (e.g. the synthetic sentinel never gets surfaced here, but a
    /// foreign FileId in a stripped store would).
    pub fs_path: Option<String>,
    pub lo:      u32,
    pub hi:      u32,
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
            fn $method:ident ( $req:ty ) -> $resp:ty => $path:literal ;
        )*
    ) => {
        // ── handler trait — implement once on app state ───────────
        #[async_trait::async_trait]
        pub trait SprfHandlers: Send + Sync + 'static {
            $(
                async fn $method(&self, req: $req) -> Result<$resp, SprfError>;
            )*
        }

        // ── typed client trait ────────────────────────────────────
        #[async_trait::async_trait]
        pub trait SprfClient: Send + Sync {
            $(
                async fn $method(&self, req: $req) -> Result<$resp, SprfError>;
            )*
        }

        // ── per-route generic handler fns ─────────────────────────
        // Named (not closure) so axum's Handler trait inference is
        // unambiguous. `_route_*` keeps these out of the public API.
        pub mod _routes {
            use super::*;
            $(
                pub async fn $method<H: SprfHandlers>(
                    State(h):   State<Arc<H>>,
                    Json(req):  Json<$req>,
                ) -> Result<Json<$resp>, SprfError> {
                    h.$method(req).await.map(Json)
                }
            )*
        }

        pub fn build_router<H: SprfHandlers>(h: Arc<H>) -> Router {
            Router::new()
                $(
                    .route($path, post(_routes::$method::<H>))
                )*
                .with_state(h)
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
    fn run            (RunReq)          -> RunReport     => "/run";
    fn get_fact_table (GetFactTableReq) -> FactTable     => "/facts";
    fn refs_at        (RefsAtReq)       -> RefsAtResp    => "/refs-at";
    fn notify_ghcache_change (NotifyGhcacheChangeReq) -> Vec<DirtyNotice> => "/git/ghcache-change";
}

// ───────────────────────────────────────────────────────────────────
// State + Handlers impl
// ───────────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct DocState {
    pub text:        String,
    pub version:     i32,
    pub program:     Vec<PipeAst>,
    pub parse_diags: Vec<Diag>,
    pub walk_diags:  Vec<Diag>,
    pub runtime_diags: Vec<Diag>,
    pub probes:      Vec<InlayProbe>,
}

pub struct SprfState {
    pub docs:       Mutex<HashMap<String, DocState>>,
    pub facts:      Arc<dyn FactStore<Cursor>>,
    pub queue:      Arc<dyn QueueBackend<Cursor>>,
    pub bus:        Arc<EventBus>,
    instances:      Mutex<Vec<Arc<PipeInstance<Cursor>>>>,
    next_instance_id: Mutex<u64>,
    /// Layer 0c.2 — content-derived intern store wrapping `facts`.
    /// Threaded into `LowerCtx` so source/pattern emitters stamp the
    /// coord-space side of Cursor (`value_id`, `at`, `terms`) alongside
    /// legacy `raw_terms` writes.
    pub sprf_store: Arc<crate::store::SprfStore>,
    /// Layer 5a — XDG repos config. Bare `repo()` reads from this.
    /// `SprfState::new` loads from `~/.config/sprefa/repos.toml` via
    /// `SprfConfig::load_default`; tests inject explicit configs via
    /// `with_config`.
    pub config:     Arc<crate::config::SprfConfig>,
    pub registry:   Arc<Registry>,
    pub root:       PathBuf,
}

struct AnalysisPassThrough;

impl Component for AnalysisPassThrough {
    type Next = Cursor;

    fn render(&self, _ctx: &effect_runtime::v2::RenderCtx, c: &Cursor) -> Node<Cursor> {
        Node::Emit(Arc::new(c.clone()))
    }

    fn kind(&self) -> &'static str { "analysis_pass_through" }
}

impl SprfState {
    pub fn new(root: PathBuf) -> Self {
        Self::new_with_backends(
            root,
            Arc::new(MemFactStore::<Cursor>::new()),
            Arc::new(MemQueue::new()),
        )
    }

    pub fn new_with_sqlite_queue(
        root: PathBuf,
        queue_path: impl AsRef<std::path::Path>,
    ) -> Self {
        Self::new_with_backends(
            root,
            Arc::new(MemFactStore::<Cursor>::new()),
            Arc::new(SqliteQueue::<Cursor>::open_file(queue_path.as_ref())),
        )
    }

    pub fn new_with_sqlite_facts(
        root: PathBuf,
        fact_path: impl AsRef<std::path::Path>,
    ) -> Self {
        Self::new_with_backends(
            root,
            Arc::new(
                SqliteFactStore::<Cursor>::open_file(fact_path.as_ref())
                    .expect("open sqlite fact store")
            ),
            Arc::new(MemQueue::new()),
        )
    }

    pub fn new_with_sqlite_backends(
        root: PathBuf,
        fact_path: impl AsRef<std::path::Path>,
        queue_path: impl AsRef<std::path::Path>,
    ) -> Self {
        Self::new_with_backends(
            root,
            Arc::new(
                SqliteFactStore::<Cursor>::open_file(fact_path.as_ref())
                    .expect("open sqlite fact store")
            ),
            Arc::new(SqliteQueue::<Cursor>::open_file(queue_path.as_ref())),
        )
    }

    pub fn new_with_backends(
        root: PathBuf,
        facts: Arc<dyn FactStore<Cursor>>,
        queue: Arc<dyn QueueBackend<Cursor>>,
    ) -> Self {
        let bus = Arc::new(EventBus::new());
        attach_dirty_to_queue(&bus, queue.clone());
        let sprf_store = crate::store::SprfStore::new(facts.clone());
        let config = Arc::new(crate::config::SprfConfig::load_default());
        Self {
            docs: Mutex::new(HashMap::new()),
            facts,
            queue,
            bus,
            instances: Mutex::new(Vec::new()),
            next_instance_id: Mutex::new(1),
            sprf_store,
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

    fn resume_mounted(&self, opts: ExpandOpts) {
        for _ in 0..8 {
            let instances = self.instances.lock().unwrap().clone();
            let mut rendered = 0;
            for inst in instances {
                rendered += expand(
                    inst.as_ref(),
                    self.queue.clone(),
                    Vec::new(),
                    opts.clone(),
                ).rendered;
            }
            if rendered == 0 {
                break;
            }
            let generation = *self.next_instance_id.lock().unwrap();
            self.facts.commit(generation, Some(&self.bus));
        }
    }

    pub fn dispatch_ghcache_change(&self, change: &GhcacheChangeReq) -> Vec<DirtyNotice> {
        let mut notices = Vec::new();
        for (domain, key) in ghcache_dirty_notices(change) {
            self.bus.dispatch_dirty(domain.clone(), key);
            notices.push(dirty_notice(domain, key));
        }
        notices
    }

    pub fn drain_ready(&self) {
        let opts = ExpandOpts::default().with_bus(self.bus.clone());
        self.resume_mounted(opts);
    }

    fn ingest(&self, uri: String, text: String, version: i32) {
        let (program, parse_diags) = host_parse(&text);
        let probe_sink: Arc<BufferProbeSink<Cursor>> = Arc::new(BufferProbeSink::new());
        let runtime_diags = Arc::new(BufferDiagSink::new());
        let mut ctx = LowerCtx::new(self.facts.clone(), self.root.clone())
            .with_probe(probe_sink.clone() as Arc<dyn ProbeSink<Cursor>>)
            .with_sprf_store(self.sprf_store.clone())
            .with_config(self.config.clone());
        let (pipes, walk_diags) = walk_program(&program, &self.registry, &mut ctx);

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let opts = ExpandOpts::default().with_diag(runtime_diags.clone());
        for pipe in pipes {
            let inst = analysis_safe_pipe(pipe).into_instance();
            expand(&inst, queue.clone(), vec![Arc::new(Cursor::default())], opts.clone());
        }

        let raw = probe_sink.drain();
        let mut by_span: HashMap<(u32,u32), u32> = HashMap::new();
        for p in &raw { *by_span.entry((p.span.lo, p.span.hi)).or_insert(0) += 1; }
        let mut probes: Vec<InlayProbe> = by_span.into_iter()
            .map(|((lo,hi),count)| InlayProbe { lo, hi, count }).collect();
        probes.sort_by_key(|p| (p.lo, p.hi));

        self.docs.lock().unwrap().insert(uri, DocState {
            text,
            version,
            program,
            parse_diags,
            walk_diags,
            runtime_diags: runtime_diags.snapshot(),
            probes,
        });
    }
}

fn analysis_safe_pipe(pipe: Pipe<Cursor>) -> Pipe<Cursor> {
    let steps = pipe.steps.into_iter()
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
        let out: Vec<SprfDiag> = d.parse_diags.iter()
            .chain(d.walk_diags.iter())
            .chain(d.runtime_diags.iter())
            .map(SprfDiag::from).collect();
        Ok(out)
    }
    async fn get_inlays(&self, req: GetInlaysReq) -> Result<Vec<InlayProbe>, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs.get(&req.uri).ok_or(SprfError::UnknownDoc(req.uri))?;
        Ok(d.probes.clone())
    }
    async fn lsp_locate_dsl(&self, req: LspLocateDslReq) -> Result<LspLocateDslResp, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs.get(&req.uri).ok_or(SprfError::UnknownDoc(req.uri.clone()))?;
        let mut hit: Option<(OpCall, usize)> = None;
        for p in &d.program { walk_pipe_for_dsl(p, req.byte as usize, &mut hit); }
        Ok(match hit {
            Some((call, body_byte)) => match &call.dsl {
                Some(dsl) => LspLocateDslResp {
                    op_name:   Some(call.name.to_string()),
                    body_raw:  Some(dsl.raw.to_string()),
                    body_off:  dsl.span.lo,
                    body_byte: body_byte as u32,
                },
                None => LspLocateDslResp {
                    op_name: None, body_raw: None, body_off: 0, body_byte: 0,
                },
            },
            None => LspLocateDslResp {
                op_name: None, body_raw: None, body_off: 0, body_byte: 0,
            },
        })
    }

    async fn lsp_hover(&self, req: LspHoverReq) -> Result<LspHoverResp, SprfError> {
        let docs = self.docs.lock().unwrap();
        let d = docs.get(&req.uri).ok_or(SprfError::UnknownDoc(req.uri.clone()))?;
        let mut hit: Option<(OpCall, usize)> = None;
        for p in &d.program {
            walk_pipe_for_dsl(p, req.byte as usize, &mut hit);
        }
        let contents = hit.and_then(|(call, body_byte)| {
            let dsl = call.dsl.as_ref()?;
            dsl_hover(call.name.as_ref(), dsl.raw.as_bytes(), body_byte)
        });
        Ok(LspHoverResp { contents })
    }

    async fn run(&self, req: RunReq) -> Result<RunReport, SprfError> {
        let src = std::fs::read_to_string(&req.path)
            .map_err(|e| SprfError::Io(e.to_string()))?;
        let (program, parse_diags) = host_parse(&src);
        let dir = req.root.clone()
            .or_else(|| req.path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| self.root.clone());
        let mut ctx = LowerCtx::new(self.facts.clone(), dir)
            .with_sprf_store(self.sprf_store.clone())
            .with_config(self.config.clone());
        let (pipes, walk_diags) = walk_program(&program, &self.registry, &mut ctx);

        // 4096 matches v4-bench. Smaller caps multiply per-batch lock
        // overhead in batched sinks (FactWrite et al.).
        let runtime_diags = Arc::new(BufferDiagSink::new());
        let opts = ExpandOpts::default()
            .with_batch_cap(4096)
            .with_bus(self.bus.clone())
            .with_diag(runtime_diags.clone());
        let mut n = 0;
        for (idx, pipe) in pipes.into_iter().enumerate() {
            let identity = program
                .get(idx)
                .map(|pipe_ast| stable_pipe_identity(&req.path, pipe_ast))
                .unwrap_or_else(|| fallback_pipe_identity(&req.path, idx));
            let inst = self.mount_pipe(pipe, identity);
            expand(
                inst.as_ref(),
                self.queue.clone(),
                vec![Arc::new(Cursor::default())],
                opts.clone(),
            );
            n += 1;
        }
        let generation = *self.next_instance_id.lock().unwrap();
        self.facts.commit(generation, Some(&self.bus));
        self.resume_mounted(opts);

        let mut tables: Vec<String> = Vec::new();
        collect_rule_tables(&program, &mut tables);

        Ok(RunReport {
            parse_diags: parse_diags.iter().map(SprfDiag::from).collect(),
            walk_diags:  walk_diags.iter().map(SprfDiag::from).collect(),
            runtime_diags: runtime_diags.snapshot().iter().map(SprfDiag::from).collect(),
            pipes:       n,
            tables,
        })
    }

    async fn refs_at(&self, req: RefsAtReq) -> Result<RefsAtResp, SprfError> {
        let store = &self.sprf_store;
        let file = match store.find_file_by_path(&req.path) {
            Some(f) => f,
            None    => return Ok(RefsAtResp { hits: Vec::new() }),
        };
        let refs = store.find_refs_in(file, req.byte);
        let mut hits = Vec::with_capacity(refs.len());
        for r in refs {
            let coord = match store.coord_of(r) {
                Some(c) => c,
                None    => continue,
            };
            let fs_path = store.path_of(coord.fs).map(|a| a.to_string());
            hits.push(RefHit {
                ref_id: r.0,
                coord: CoordWire {
                    repo:    coord.repo,
                    rev:     coord.rev,
                    fs:      coord.fs,
                    fs_path,
                    lo:      coord.lo,
                    hi:      coord.hi,
                },
            });
        }
        Ok(RefsAtResp { hits })
    }

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
        let raw   = self.facts.rows_of(&req.name);
        let take  = req.limit.unwrap_or(usize::MAX).min(raw.len());
        let rows  = raw.iter().take(take).map(|c| FactRow {
            fields: c.raw_terms.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }).collect();
        Ok(FactTable { name: req.name, total, rows })
    }
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

fn dsl_hover(op_name: &str, body: &[u8], body_byte: usize) -> Option<String> {
    let dsl: Box<dyn Dsl> = match op_name {
        "sql"  => Box::new(crate::cst::dsls::sql::SqlDsl::new()),
        "json" => Box::new(crate::cst::dsls::json::JsonDsl::new()),
        "re"   => Box::new(crate::cst::dsls::re::ReDsl::new()),
        "glob" => Box::new(crate::cst::dsls::glob::GlobDsl::new()),
        _ => return None,
    };
    let hover = dsl.lsp()?.hover(body, body_byte)?;
    Some(hover_contents_to_string(hover.contents))
}

fn hover_contents_to_string(contents: lsp_types::HoverContents) -> String {
    match contents {
        lsp_types::HoverContents::Scalar(marked) => marked_string_to_string(marked),
        lsp_types::HoverContents::Array(items) => {
            items.into_iter().map(marked_string_to_string).collect::<Vec<_>>().join("\n")
        }
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
    for step in &p.steps { walk_step_for_dsl(step, host_byte, hit); }
}

fn walk_step_for_dsl(call: &OpCall, host_byte: usize, hit: &mut Option<(OpCall, usize)>) {
    if let Some(dsl) = &call.dsl {
        let lo = dsl.span.lo as usize;
        let hi = dsl.span.hi as usize;
        if host_byte >= lo && host_byte < hi {
            // Deepest containing body wins (overwrites outer hits).
            *hit = Some((call.clone(), host_byte - lo));
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
    let state  = Arc::new(SprfState::new(root));
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

        client.lsp_open(LspOpenReq {
            uri: "file:///x.sprf".into(), text: "".into(), version: 1,
        }).await.unwrap();

        let diags = client.get_diags(GetDiagsReq {
            uri: "file:///x.sprf".into(),
        }).await.unwrap();
        assert_eq!(diags.len(), 0);

        let inlays = client.get_inlays(GetInlaysReq {
            uri: "file:///x.sprf".into(),
        }).await.unwrap();
        assert!(inlays.is_empty());

        client.lsp_close(LspCloseReq { uri: "file:///x.sprf".into() }).await.unwrap();

        let err = client.get_diags(GetDiagsReq {
            uri: "file:///x.sprf".into(),
        }).await.unwrap_err();
        assert!(matches!(err, SprfError::UnknownDoc(_)));
    }

    #[tokio::test]
    async fn real_source_through_router() {
        let (_state, client) = build_in_process(std::env::temp_dir());
        let src = "rule(:greet) { str `hello world` }";
        client.lsp_open(LspOpenReq {
            uri: "file:///hello.sprf".into(), text: src.into(), version: 1,
        }).await.unwrap();
        let _ = client.get_diags(GetDiagsReq { uri: "file:///hello.sprf".into() })
            .await.unwrap();
        let _ = client.get_inlays(GetInlaysReq { uri: "file:///hello.sprf".into() })
            .await.unwrap();
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
        let report = client.run(RunReq {
            path,
            root: Some(dir.path().to_path_buf()),
        }).await.unwrap();

        assert_eq!(report.runtime_diags.len(), 1);
        assert_eq!(report.runtime_diags[0].severity, "warning");
        assert_eq!(report.runtime_diags[0].code, "demo_warn");
        assert_eq!(report.runtime_diags[0].message, "word hello");
    }
}
