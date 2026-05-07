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
    expand, BufferProbeSink, Diag, ExpandOpts, FactStore, MemFactStore, MemQueue, ProbeSink,
    QueueBackend,
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

use crate::compile::ast::{OpCall, PipeAst};
use crate::compile::parse::host_parse;
use crate::compile::walk::walk_program;
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
    pub pipes:       usize,
    /// Rule-table names harvested from the AST (one per `rule(:NAME)`).
    pub tables:      Vec<String>,
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
    fn run            (RunReq)          -> RunReport     => "/run";
    fn get_fact_table (GetFactTableReq) -> FactTable     => "/facts";
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
    pub probes:      Vec<InlayProbe>,
}

pub struct SprfState {
    pub docs:     Mutex<HashMap<String, DocState>>,
    pub facts:    Arc<dyn FactStore<Cursor>>,
    pub registry: Arc<Registry>,
    pub root:     PathBuf,
}

impl SprfState {
    pub fn new(root: PathBuf) -> Self {
        Self {
            docs:     Mutex::new(HashMap::new()),
            facts:    Arc::new(MemFactStore::<Cursor>::new()),
            registry: Arc::new(default_registry()),
            root,
        }
    }

    fn ingest(&self, uri: String, text: String, version: i32) {
        let (program, parse_diags) = host_parse(&text);
        let probe_sink: Arc<BufferProbeSink<Cursor>> = Arc::new(BufferProbeSink::new());
        let mut ctx = LowerCtx::new(self.facts.clone(), self.root.clone())
            .with_probe(probe_sink.clone() as Arc<dyn ProbeSink<Cursor>>);
        let (pipes, walk_diags) = walk_program(&program, &self.registry, &mut ctx);

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let opts = ExpandOpts::default();
        for pipe in pipes {
            let inst = pipe.into_instance();
            expand(&inst, queue.clone(), vec![Arc::new(Cursor::default())], opts.clone());
        }

        let raw = probe_sink.drain();
        let mut by_span: HashMap<(u32,u32), u32> = HashMap::new();
        for p in &raw { *by_span.entry((p.span.lo, p.span.hi)).or_insert(0) += 1; }
        let mut probes: Vec<InlayProbe> = by_span.into_iter()
            .map(|((lo,hi),count)| InlayProbe { lo, hi, count }).collect();
        probes.sort_by_key(|p| (p.lo, p.hi));

        self.docs.lock().unwrap().insert(uri, DocState {
            text, version, program, parse_diags, walk_diags, probes,
        });
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
        let out: Vec<SprfDiag> = d.parse_diags.iter().chain(d.walk_diags.iter())
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

    async fn run(&self, req: RunReq) -> Result<RunReport, SprfError> {
        let src = std::fs::read_to_string(&req.path)
            .map_err(|e| SprfError::Io(e.to_string()))?;
        let (program, parse_diags) = host_parse(&src);
        let dir = req.root.clone()
            .or_else(|| req.path.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| self.root.clone());
        let mut ctx = LowerCtx::new(self.facts.clone(), dir);
        let (pipes, walk_diags) = walk_program(&program, &self.registry, &mut ctx);

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        // 4096 matches v4-bench. Smaller caps multiply per-batch lock
        // overhead in batched sinks (FactWrite et al.).
        let opts = ExpandOpts::default().with_batch_cap(4096);
        let mut n = 0;
        for pipe in pipes {
            let inst = pipe.into_instance();
            expand(&inst, queue.clone(), vec![Arc::new(Cursor::default())], opts.clone());
            n += 1;
        }
        self.facts.commit(1, None);

        let mut tables: Vec<String> = Vec::new();
        collect_rule_tables(&program, &mut tables);

        Ok(RunReport {
            parse_diags: parse_diags.iter().map(SprfDiag::from).collect(),
            walk_diags:  walk_diags.iter().map(SprfDiag::from).collect(),
            pipes:       n,
            tables,
        })
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
}
