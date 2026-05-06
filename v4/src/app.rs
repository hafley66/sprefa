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

use crate::compile::ast::PipeAst;
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
pub struct RunReq       { pub path: PathBuf }

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
    pub parse_diags: usize,
    pub walk_diags:  usize,
    pub pipes:       usize,
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
    fn run         (RunReq)       -> RunReport           => "/run";
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
    async fn run(&self, req: RunReq) -> Result<RunReport, SprfError> {
        let src = std::fs::read_to_string(&req.path)
            .map_err(|e| SprfError::Io(e.to_string()))?;
        let (program, parse_diags) = host_parse(&src);
        let dir = req.path.parent().map(|p| p.to_path_buf()).unwrap_or(self.root.clone());
        let mut ctx = LowerCtx::new(self.facts.clone(), dir);
        let (pipes, walk_diags) = walk_program(&program, &self.registry, &mut ctx);

        let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
        let opts = ExpandOpts::default();
        let mut n = 0;
        for pipe in pipes {
            let inst = pipe.into_instance();
            expand(&inst, queue.clone(), vec![Arc::new(Cursor::default())], opts.clone());
            n += 1;
        }
        self.facts.commit(1, None);
        Ok(RunReport {
            parse_diags: parse_diags.len(),
            walk_diags:  walk_diags.len(),
            pipes:       n,
        })
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
