//! sprefa-lsp — every LSP method shims into the unified `SprfClient`.
//!
//! Backend holds a `SprfClient`. By default this is the in-process
//! `axum::Router` used by `sprefa-run`; when `SPREFA_LSP_DAEMON_URL`
//! is set it becomes an HTTP client for `sprefa-daemon`. Every domain
//! operation goes through one of the registered RPCs in
//! `v4::app::sprf_rpc!`. Backend's only local state is a text cache so
//! byte-offset diags can be converted to LSP line/col.
//!
//! The shim is mechanical: LSP request → SprfClient method → LSP reply.
//! Adding a new sprf RPC adds zero LSP code unless a new LSP request
//! type wants to surface it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

/// Coalesce rapid keystrokes (`did_change`) before running the sprf
/// compile/expand pipeline. Each per-URI refresh waits this long; if a
/// newer version arrives in the meantime, the older call drops without
/// firing the RPC. 80ms is below human-noticeable LSP latency and well
/// above keyboard repeat rates.
const DID_CHANGE_DEBOUNCE: Duration = Duration::from_millis(80);

/// Hard cap on `last_published` map size. A buggy lint pointing diags
/// at fresh target URIs per row could otherwise grow the map without
/// bound. 4096 source URIs is well above any realistic editor session
/// and well below the OS file-handle ceiling.
const LAST_PUBLISHED_CAP: usize = 4096;
use tower_lsp::jsonrpc::Result as RpcResult;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, Location,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, InitializedParams, InlayHint, InlayHintLabel,
    InlayHintParams, MarkedString, MessageType, NumberOrString, OneOf,
    Position, Range, SemanticTokens, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams,
    SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, Url,
    WorkDoneProgressOptions,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tracing_subscriber::EnvFilter;

mod dsl_lookup;
mod inlay;
mod semantic;

use v4::app::{
    build_in_process, GetInlaysReq, HttpClient,
    LspChangeReq, LspCloseReq, LspCompletionReq, LspDefinitionReq, LspDiagsByUriReq, LspHoverReq,
    LspLocateDslReq, LspLocateDslResp, LspOpenReq, LspPreWarmReq, LspWakeFsReq, SprfClient, SprfDiag,
};

struct Backend {
    inner: Arc<BackendInner>,
}

struct BackendInner {
    client: Client,
    sprf:   Mutex<Arc<dyn SprfClient>>,
    docs:   Mutex<HashMap<Url, DocEntry>>,
    /// Most-recently-requested doc version per URI. Set BEFORE the
    /// debounce sleep and BEFORE the RPC; checked after both. A newer
    /// version arriving while an older refresh is in flight bumps this
    /// counter and the older call drops without publishing.
    pending: Mutex<HashMap<Url, i32>>,
    /// Target URIs we last published to under each source `.sprf` URI.
    /// Lets us clear stale squiggles on a non-`.sprf` file once the
    /// lint stops targeting it.
    last_published: Mutex<HashMap<Url, Vec<Url>>>,
    /// Handle to the background pre-warm task started during
    /// `initialize`. Aborted on `shutdown` so we never leak a walker
    /// thread or an in-flight RPC past LSP teardown.
    pre_warm_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Phase 6 (MVP-6a). Handle to the background Watchman subscription
    /// loop. Aborted on `shutdown`. None when Watchman was unreachable
    /// at initialize.
    watch_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Per-source-URI publish locks. Two refreshes racing on the same
    /// URI serialize through the per-key Mutex so the second sees the
    /// first's `last_published` writes. Different URIs never contend.
    publish_locks: dashmap::DashMap<Url, Arc<tokio::sync::Mutex<()>>>,
}

#[allow(dead_code)]
struct DocEntry {
    text:    String,
    version: i32,
}

impl Backend {
    fn new(client: Client) -> Self {
        let sprf = BackendInner::build_client(default_root());
        Self {
            inner: Arc::new(BackendInner {
                client,
                sprf: Mutex::new(sprf),
                docs: Mutex::new(HashMap::new()),
                pending: Mutex::new(HashMap::new()),
                last_published: Mutex::new(HashMap::new()),
                pre_warm_handle: Mutex::new(None),
                watch_handle: Mutex::new(None),
                publish_locks: dashmap::DashMap::new(),
            }),
        }
    }

    fn root_from_initialize(params: &InitializeParams) -> PathBuf {
        params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|folder| folder.uri.to_file_path().ok())
            .or_else(|| {
                params
                    .root_uri
                    .as_ref()
                    .and_then(|uri| uri.to_file_path().ok())
            })
            .unwrap_or_else(default_root)
    }
}

impl BackendInner {
    fn build_client(root: PathBuf) -> Arc<dyn SprfClient> {
        match std::env::var("SPREFA_LSP_DAEMON_URL") {
            Ok(base) if !base.trim().is_empty() => Arc::new(HttpClient::new(base)),
            _ => {
                let (_state, sprf) = build_in_process(root);
                Arc::new(sprf)
            }
        }
    }

    async fn sprf(&self) -> Arc<dyn SprfClient> {
        self.sprf.lock().await.clone()
    }

    async fn set_workspace_root(&self, root: PathBuf) {
        if std::env::var("SPREFA_LSP_DAEMON_URL")
            .map(|base| !base.trim().is_empty())
            .unwrap_or(false)
        {
            return;
        }
        *self.sprf.lock().await = Self::build_client(root);
    }

    /// Push doc through the sprf RPC, then publish converted diags.
    ///
    /// Two checkpoints drop stale work when newer keystrokes arrive:
    ///   1. After the debounce sleep (did_change only): if `pending`
    ///      has moved on, return without firing the RPC.
    ///   2. After the RPC: if `pending` has moved on, return without
    ///      publishing diags. The newer call will publish its own.
    /// `did_open` skips the debounce so the first file load is immediate.
    async fn refresh(&self, uri: Url, text: String, version: i32, is_open: bool) {
        // Mark this as the latest pending version BEFORE any sleep / RPC.
        // A newer call arriving from now on will overwrite this and
        // signal the checkpoints below to drop our work.
        {
            let mut p = self.pending.lock().await;
            p.insert(uri.clone(), version);
        }

        if !is_open {
            tokio::time::sleep(DID_CHANGE_DEBOUNCE).await;
            if !self.is_still_latest(&uri, version).await {
                return;
            }
        }

        // Per-URI publish lock. Concurrent refreshes on the same URI
        // serialize so the later one observes the earlier's writes to
        // `last_published`. Different URIs never contend (DashMap shard
        // keeps lookup O(1) and the lock itself is per-key). Acquired
        // BEFORE the RPC so the daemon-side ingest also serializes per
        // URI.
        let lock = self.publish_locks
            .entry(uri.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        let req_uri = uri.to_string();
        let sprf = self.sprf().await;
        let res = if is_open {
            sprf.lsp_open(LspOpenReq {
                uri: req_uri.clone(), text: text.clone(), version,
            }).await
        } else {
            sprf.lsp_change(LspChangeReq {
                uri: req_uri.clone(), text: text.clone(), version,
            }).await
        };
        if let Err(e) = res {
            tracing::error!(uri = %uri, %e, "sprf RPC failed");
            return;
        }

        if !self.is_still_latest(&uri, version).await {
            return;
        }

        let resp = match sprf
            .lsp_diags_by_uri(LspDiagsByUriReq { source_uri: req_uri })
            .await
        {
            Ok(v)  => v,
            Err(e) => {
                tracing::error!(uri = %uri, %e, "lsp_diags_by_uri failed");
                return;
            }
        };

        {
            let mut g = self.docs.lock().await;
            g.insert(uri.clone(), DocEntry { text: text.clone(), version });
        }

        // For diags whose target URI is the requesting .sprf URI we
        // already have the source text in hand. For cross-URI targets
        // we load the target file from disk so byte spans render at
        // correct line/col; on read failure we fall back to (0,0).
        let mut published_uris: Vec<Url> = Vec::new();
        for (target_uri_str, diags) in resp.by_uri.iter() {
            let target_url = match Url::parse(target_uri_str) {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!(target = %target_uri_str, %e, "invalid target URI; dropping diags");
                    continue;
                }
            };
            let text_for_target: Option<String> = if target_url == uri {
                Some(text.clone())
            } else {
                match target_url.to_file_path() {
                    Ok(p) => match tokio::fs::read_to_string(&p).await {
                        Ok(s) => Some(s),
                        Err(e) => {
                            tracing::warn!(target = %target_url, %e, "cross-URI target unreadable; skipping publish");
                            None
                        }
                    },
                    Err(()) => {
                        tracing::warn!(target = %target_url, "cross-URI target URI is not a file path; skipping publish");
                        None
                    }
                }
            };
            let Some(text_for_target) = text_for_target else {
                continue;
            };
            let lsp_diags: Vec<Diagnostic> = diags
                .iter()
                .map(|d| to_lsp_diag(&text_for_target, d))
                .collect();
            // Only echo `version` on the source `.sprf` URI; for
            // cross-URI publish we have no LSP-tracked version.
            let version_for_publish = if target_url == uri { Some(version) } else { None };
            self.client
                .publish_diagnostics(target_url.clone(), lsp_diags, version_for_publish)
                .await;
            published_uris.push(target_url);
        }

        // Clear stale squiggles on URIs we previously published to
        // under this source but that are absent from this round.
        let previous: Vec<Url> = {
            let mut g = self.last_published.lock().await;
            evict_last_published_to_cap(&mut g, &uri);
            g.insert(uri.clone(), published_uris.clone()).unwrap_or_default()
        };
        for prev in previous {
            if prev == uri {
                continue;
            }
            if !published_uris.iter().any(|u| u == &prev) {
                self.client
                    .publish_diagnostics(prev, Vec::new(), None)
                    .await;
            }
        }
    }

    async fn is_still_latest(&self, uri: &Url, version: i32) -> bool {
        self.pending.lock().await.get(uri).copied() == Some(version)
    }

    /// Pre-warm-only sibling of `refresh`. Opens the doc on the daemon,
    /// fetches diagnostics, publishes them, and tracks `last_published`
    /// exactly like `refresh`, but never touches `pending`. Pre-warm is
    /// not keystroke-driven, so it must not participate in the
    /// version-checkpoint dance; otherwise a `version=0` write here
    /// would clobber an editor's later `did_open` and cause its real
    /// publish to be dropped by `is_still_latest`.
    ///
    /// If the editor has already opened this URI at a higher version,
    /// `lsp_open` is idempotent on the daemon side (just re-ingests the
    /// same text), so this is safe to call concurrently.
    async fn refresh_from_pre_warm(&self, uri: Url, text: String) {
        // Serialize on the same per-URI lock `refresh` uses so a pre-warm
        // racing an editor `did_open` doesn't interleave their writes to
        // `last_published`.
        let lock = self.publish_locks
            .entry(uri.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        let req_uri = uri.to_string();
        let sprf = self.sprf().await;
        if let Err(e) = sprf
            .lsp_open(LspOpenReq {
                uri: req_uri.clone(),
                text: text.clone(),
                version: 0,
            })
            .await
        {
            tracing::error!(uri = %uri, %e, "pre-warm lsp_open failed");
            return;
        }

        let resp = match sprf
            .lsp_diags_by_uri(LspDiagsByUriReq { source_uri: req_uri })
            .await
        {
            Ok(v)  => v,
            Err(e) => {
                tracing::error!(uri = %uri, %e, "pre-warm lsp_diags_by_uri failed");
                return;
            }
        };

        // Only seed the text cache if no editor-driven version is
        // already there; we must not stomp a real `did_open`.
        {
            let mut g = self.docs.lock().await;
            g.entry(uri.clone()).or_insert(DocEntry { text: text.clone(), version: 0 });
        }

        let mut published_uris: Vec<Url> = Vec::new();
        for (target_uri_str, diags) in resp.by_uri.iter() {
            let target_url = match Url::parse(target_uri_str) {
                Ok(u) => u,
                Err(e) => {
                    tracing::warn!(target = %target_uri_str, %e, "invalid target URI; dropping diags");
                    continue;
                }
            };
            let text_for_target: Option<String> = if target_url == uri {
                Some(text.clone())
            } else {
                match target_url.to_file_path() {
                    Ok(p) => match tokio::fs::read_to_string(&p).await {
                        Ok(s) => Some(s),
                        Err(e) => {
                            tracing::warn!(target = %target_url, %e, "cross-URI target unreadable; skipping publish");
                            None
                        }
                    },
                    Err(()) => {
                        tracing::warn!(target = %target_url, "cross-URI target URI is not a file path; skipping publish");
                        None
                    }
                }
            };
            let Some(text_for_target) = text_for_target else {
                continue;
            };
            let lsp_diags: Vec<Diagnostic> = diags
                .iter()
                .map(|d| to_lsp_diag(&text_for_target, d))
                .collect();
            // Pre-warm never echoes an LSP version (we did not see a
            // did_open from the editor for this URI).
            self.client
                .publish_diagnostics(target_url.clone(), lsp_diags, None)
                .await;
            published_uris.push(target_url);
        }

        let previous: Vec<Url> = {
            let mut g = self.last_published.lock().await;
            evict_last_published_to_cap(&mut g, &uri);
            g.insert(uri.clone(), published_uris.clone()).unwrap_or_default()
        };
        for prev in previous {
            if prev == uri {
                continue;
            }
            if !published_uris.iter().any(|u| u == &prev) {
                self.client
                    .publish_diagnostics(prev, Vec::new(), None)
                    .await;
            }
        }
    }

    /// Phase 6 (MVP-6a). Background Watchman subscription loop. Connects
    /// to the local Watchman daemon, subscribes per workspace root for
    /// `.rs/.ts/.tsx/.js/.jsx/.py/.sprf` suffixes, then loops on
    /// `subscription.next().await`. On `FilesChanged`, joins relative
    /// paths against the resolved root and calls
    /// `lsp_wake_fs({paths})` on the sprf RPC; each returned woken
    /// `.sprf` URI is re-refreshed so its cross-URI diagnostics
    /// re-publish. Logs and exits on connect failure (Watchman not
    /// installed) and on `Canceled` (root deleted / daemon shut down).
    async fn run_watchman_loop(self: Arc<Self>, root: PathBuf) {
        use v4::watchman::{SubscriptionData, WatchmanIngress};

        let ingress = match WatchmanIngress::connect().await {
            Ok(w) => w,
            Err(e) => {
                tracing::info!(
                    %e,
                    "watchman unreachable; FS watch disabled (install: brew install watchman)",
                );
                return;
            }
        };
        let (mut sub, resolved) = match ingress
            .start_subscription(&root, &["rs", "ts", "tsx", "js", "jsx", "py", "sprf"])
            .await
        {
            Ok(pair) => pair,
            Err(e) => {
                tracing::warn!(%e, "watchman subscribe failed; FS watch disabled");
                return;
            }
        };
        let root_path = resolved.path().to_path_buf();
        tracing::info!(?root_path, "watchman subscription active");
        loop {
            match sub.next().await {
                Ok(SubscriptionData::FilesChanged(qr)) => {
                    let files = qr.files.unwrap_or_default();
                    if files.is_empty() {
                        continue;
                    }
                    let paths: Vec<PathBuf> = files
                        .into_iter()
                        .map(|nf| {
                            let rel: PathBuf = nf.name.into_inner();
                            if rel.is_absolute() {
                                rel
                            } else {
                                root_path.join(rel)
                            }
                        })
                        .collect();
                    let sprf = self.sprf().await;
                    let wake = match sprf.lsp_wake_fs(LspWakeFsReq { paths }).await {
                        Ok(w) => w,
                        Err(e) => {
                            tracing::warn!(%e, "lsp_wake_fs failed; skipping batch");
                            continue;
                        }
                    };
                    for uri_str in wake.woken_sprf_uris {
                        let Ok(uri) = Url::parse(&uri_str) else { continue };
                        let (text, version) = {
                            let docs = self.docs.lock().await;
                            match docs.get(&uri) {
                                Some(d) => (d.text.clone(), d.version),
                                None => continue,
                            }
                        };
                        self.republish_from_wake(uri, text, version).await;
                    }
                }
                Ok(SubscriptionData::Canceled) => {
                    tracing::warn!("watchman subscription canceled");
                    return;
                }
                Ok(SubscriptionData::StateEnter { state_name, .. }) => {
                    tracing::debug!(state = %state_name, "watchman state-enter");
                }
                Ok(SubscriptionData::StateLeave { state_name, .. }) => {
                    tracing::debug!(state = %state_name, "watchman state-leave");
                }
                Err(e) => {
                    tracing::warn!(%e, "watchman subscription error; exiting watch loop");
                    return;
                }
            }
        }
    }

    /// Phase 6 (MVP-6a). Re-publish a doc's diags after a watchman wake.
    /// The daemon's `lsp_wake_fs` already re-ingested; we just fetch
    /// fresh diags via `lsp_diags_by_uri` and run the same cross-URI
    /// publish path `refresh` uses. Holds the per-URI publish lock so a
    /// concurrent keystroke-driven `refresh` and a watchman-driven
    /// `republish_from_wake` linearize.
    async fn republish_from_wake(&self, uri: Url, text: String, version: i32) {
        let lock = self
            .publish_locks
            .entry(uri.clone())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let sprf = self.sprf().await;
        let resp = match sprf
            .lsp_diags_by_uri(LspDiagsByUriReq {
                source_uri: uri.to_string(),
            })
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(uri = %uri, %e, "lsp_diags_by_uri after wake failed");
                return;
            }
        };
        let mut published_uris: Vec<Url> = Vec::new();
        for (target_uri_str, diags) in resp.by_uri.iter() {
            let target_url = match Url::parse(target_uri_str) {
                Ok(u) => u,
                Err(_) => continue,
            };
            let text_for_target: Option<String> = if target_url == uri {
                Some(text.clone())
            } else {
                match target_url.to_file_path() {
                    Ok(p) => tokio::fs::read_to_string(&p).await.ok(),
                    Err(()) => None,
                }
            };
            let Some(text_for_target) = text_for_target else {
                continue;
            };
            let lsp_diags: Vec<Diagnostic> = diags
                .iter()
                .map(|d| to_lsp_diag(&text_for_target, d))
                .collect();
            let version_for_publish = if target_url == uri { Some(version) } else { None };
            self.client
                .publish_diagnostics(target_url.clone(), lsp_diags, version_for_publish)
                .await;
            published_uris.push(target_url);
        }
        let previous: Vec<Url> = {
            let mut g = self.last_published.lock().await;
            evict_last_published_to_cap(&mut g, &uri);
            g.insert(uri.clone(), published_uris.clone()).unwrap_or_default()
        };
        for prev in previous {
            if prev == uri {
                continue;
            }
            if !published_uris.iter().any(|u| u == &prev) {
                self.client
                    .publish_diagnostics(prev, Vec::new(), None)
                    .await;
            }
        }
    }
}

/// Hold `last_published` at or below `LAST_PUBLISHED_CAP`. Called
/// before each insert; if the incoming URI is already a key the map
/// won't grow, so we only evict for NEW keys at capacity. Eviction
/// picks an arbitrary entry (HashMap iteration order is randomized
/// per-process) — not LRU, but bounded.
fn evict_last_published_to_cap(g: &mut HashMap<Url, Vec<Url>>, incoming: &Url) {
    if g.contains_key(incoming) || g.len() < LAST_PUBLISHED_CAP {
        return;
    }
    if let Some(victim) = g.keys().next().cloned() {
        g.remove(&victim);
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> RpcResult<InitializeResult> {
        let root = Self::root_from_initialize(&params);
        self.inner.set_workspace_root(root.clone()).await;
        self.inner.client
            .log_message(MessageType::INFO, format!("sprefa-lsp root {}", root.display()))
            .await;

        // Phase 3: kick off SCM-aware pre-warm in the background so the
        // initialize response stays fast. The spawned task drives
        // `refresh_from_pre_warm` per warmed URI; that path never
        // touches `pending`, so a real editor `did_open` racing pre-warm
        // cannot have its version checkpoint clobbered.
        let inner = self.inner.clone();
        let handle = tokio::spawn(async move {
            let sprf = inner.sprf().await;
            let resp = match sprf
                .lsp_pre_warm(LspPreWarmReq {
                    root: root.clone(),
                    scm_branch: "main".to_string(),
                })
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(%e, "lsp_pre_warm failed");
                    return;
                }
            };
            tracing::info!(
                warmed = resp.source_uris.len(),
                walked = resp.walked,
                deadline_hit = resp.deadline_hit,
                watchman_ok = resp.watchman_ok,
                scm_changed = ?resp.scm_changed_count,
                "pre-warm complete",
            );
            for uri_str in resp.source_uris {
                let Ok(uri) = Url::parse(&uri_str) else { continue };
                let Ok(path) = uri.to_file_path() else { continue };
                let Ok(text) = tokio::fs::read_to_string(&path).await else { continue };
                inner.refresh_from_pre_warm(uri, text).await;
            }
        });
        *self.inner.pre_warm_handle.lock().await = Some(handle);

        // Phase 6 (MVP-6a). Background Watchman subscription. On each
        // FilesChanged batch, call `lsp_wake_fs(paths)` to drive the
        // daemon's wake (clock bump + dispatch_source_wake + drain_ready
        // + per-doc re-ingest), then refresh each returned `.sprf` URI
        // so its cross-URI diagnostics re-publish.
        let watch_inner = self.inner.clone();
        let watch_root = Self::root_from_initialize(&params);
        let watch_handle = tokio::spawn(async move {
            watch_inner.run_watchman_loop(watch_root).await;
        });
        *self.inner.watch_handle.lock().await = Some(watch_handle);

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: SemanticTokensLegend {
                                token_types:     semantic::legend_token_types(),
                                token_modifiers: semantic::legend_token_modifiers(),
                            },
                            range: Some(false),
                            full:  Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                inlay_hint_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    // `$` opens host pipe-holes; `:` opens atoms; `.` is
                    // reserved future "field access". Triggering on these
                    // keystrokes makes type-ahead feel native instead of
                    // ctrl-space-only.
                    trigger_characters: Some(vec![
                        "$".into(), ":".into(), ".".into(),
                    ]),
                    resolve_provider: Some(false),
                    all_commit_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    completion_item: None,
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name:    "sprefa-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.inner.client.log_message(MessageType::INFO, "sprefa-lsp ready").await;
    }

    async fn shutdown(&self) -> RpcResult<()> {
        if let Some(h) = self.inner.pre_warm_handle.lock().await.take() {
            h.abort();
        }
        if let Some(h) = self.inner.watch_handle.lock().await.take() {
            h.abort();
        }
        Ok(())
    }

    async fn semantic_tokens_full(
        &self, params: SemanticTokensParams,
    ) -> RpcResult<Option<SemanticTokensResult>> {
        // Tokens computed locally (Backend already has text cached);
        // adding a `/lsp/semantic_tokens` RPC is a one-liner the day
        // a non-LSP shell wants the same data.
        let uri = params.text_document.uri;
        let text = {
            let g = self.inner.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let data = semantic::tokens_for(&text);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None, data,
        })))
    }

    async fn inlay_hint(
        &self, params: InlayHintParams,
    ) -> RpcResult<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let text = {
            let g = self.inner.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let sprf = self.inner.sprf().await;
        let probes = match sprf.get_inlays(GetInlaysReq {
            uri: uri.to_string(),
        }).await {
            Ok(v)  => v,
            Err(_) => return Ok(Some(Vec::new())),
        };
        let hints: Vec<InlayHint> = probes.into_iter()
            .map(|p| {
                let pos = byte_to_position(&text, p.hi as usize);
                InlayHint {
                    position: pos,
                    label:    InlayHintLabel::String(format!("→ {} cursor(s)", p.count)),
                    kind:     None,
                    text_edits: None,
                    tooltip: None,
                    padding_left:  Some(true),
                    padding_right: None,
                    data: None,
                }
            }).collect();
        Ok(Some(hints))
    }

    async fn hover(&self, params: HoverParams) -> RpcResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let text = {
            let g = self.inner.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let host_byte = position_to_byte(&text, pos);
        let sprf = self.inner.sprf().await;
        let resp = match sprf.lsp_hover(LspHoverReq {
            uri:  uri.to_string(),
            byte: host_byte as u32,
        }).await {
            Ok(h)  => h,
            Err(_) => return Ok(None),
        };
        let Some(contents) = resp.contents else { return Ok(None); };
        Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(contents)),
            range: None,
        }))
    }

    async fn completion(
        &self, params: CompletionParams,
    ) -> RpcResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let text = {
            let g = self.inner.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let host_byte = position_to_byte(&text, pos);
        let sprf = self.inner.sprf().await;
        let hit: LspLocateDslResp = match sprf.lsp_locate_dsl(LspLocateDslReq {
            uri:  uri.to_string(),
            byte: host_byte as u32,
        }).await {
            Ok(h)  => h,
            // Outside any dsl body, or unopened uri — host-level
            // completion (op names, atoms, term refs) lives later.
            Err(_) => return Ok(Some(CompletionResponse::Array(Vec::new()))),
        };
        let v4_items: Vec<v4_lsp_types::CompletionItem> = match (hit.op_name, hit.body_raw) {
            (Some(op_name), Some(_body_raw)) if op_name == "sql" => {
                sprf.lsp_completion(LspCompletionReq {
                    uri: uri.to_string(),
                    byte: host_byte as u32,
                })
                .await
                .unwrap_or_default()
            }
            (Some(op_name), Some(body_raw)) => match dsl_lookup::provider_for(&op_name) {
                Some(handle) => match handle.lsp() {
                    Some(lsp) => lsp.completions(body_raw.as_bytes(), hit.body_byte as usize),
                    None      => Vec::new(),
                },
                None => Vec::new(),
            },
            _ => Vec::new(),
        };
        let items: Vec<CompletionItem> = v4_items.iter()
            .filter_map(|it| crosswalk::<_, CompletionItem>(it))
            .collect();
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> RpcResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        let text = {
            let g = self.inner.docs.lock().await;
            match g.get(&uri) { Some(e) => e.text.clone(), None => return Ok(None) }
        };
        let host_byte = position_to_byte(&text, pos);
        let sprf = self.inner.sprf().await;
        let resp = match sprf.lsp_definition(LspDefinitionReq {
            uri: uri.to_string(),
            byte: host_byte as u32,
        }).await {
            Ok(resp) => resp,
            Err(_) => return Ok(None),
        };
        let (Some(lo), Some(hi)) = (resp.lo, resp.hi) else {
            return Ok(None);
        };
        Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: Range::new(
                byte_to_position(&text, lo as usize),
                byte_to_position(&text, hi as usize),
            ),
        })))
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let td = params.text_document;
        self.inner.refresh(td.uri, td.text, td.version, true).await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let text = match params.content_changes.pop() {
            Some(c) => c.text,
            None    => return,
        };
        self.inner.refresh(uri, text, version, false).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let sprf = self.inner.sprf().await;
        let _ = sprf.lsp_close(LspCloseReq { uri: uri.to_string() }).await;
        { let mut g = self.inner.docs.lock().await; g.remove(&uri); }
        let cross_targets = {
            let mut g = self.inner.last_published.lock().await;
            g.remove(&uri).unwrap_or_default()
        };
        self.inner.client.publish_diagnostics(uri.clone(), Vec::new(), None).await;
        for prev in cross_targets {
            if prev != uri {
                self.inner.client.publish_diagnostics(prev, Vec::new(), None).await;
            }
        }
    }
}

// ── diag conversion ──────────────────────────────────────────────────

fn to_lsp_diag(src: &str, d: &SprfDiag) -> Diagnostic {
    let range = match (&d.position, d.lo, d.hi) {
        // Prefer the emit-time (T0) position when the runtime carried
        // one. Avoids the T0/T1 race where `src` here is the file at
        // publish time but the byte offsets were minted at expand time.
        (Some(p), _, _) => Range {
            start: Position { line: p.line_lo, character: p.col_lo },
            end:   Position { line: p.line_hi, character: p.col_hi },
        },
        (None, Some(lo), Some(hi)) => Range::new(
            byte_to_position(src, lo as usize),
            byte_to_position(src, hi as usize),
        ),
        _ => Range::new(Position::new(0, 0), Position::new(0, 0)),
    };
    Diagnostic {
        range,
        severity: Some(map_severity(&d.severity)),
        code:     Some(NumberOrString::String(d.code.clone())),
        code_description: None,
        source:   Some("sprefa".into()),
        message:  d.message.clone(),
        related_information: None,
        tags:     None,
        data:     None,
    }
}

fn map_severity(s: &str) -> DiagnosticSeverity {
    match s {
        "error"   => DiagnosticSeverity::ERROR,
        "warning" => DiagnosticSeverity::WARNING,
        "info"    => DiagnosticSeverity::INFORMATION,
        _         => DiagnosticSeverity::HINT,
    }
}

/// `lsp_types` v0.97 (used by v4 lib) ↔ v0.94 (used by tower-lsp 0.20).
/// They share JSON shape, so serde round-trip is the cheapest bridge.
/// Returns `None` only if the source value fails to serialize, which
/// is essentially never for the structures we cross.
fn crosswalk<S: serde::Serialize, D: serde::de::DeserializeOwned>(src: &S) -> Option<D> {
    serde_json::from_value(serde_json::to_value(src).ok()?).ok()
}

/// Re-export of v4 lib's lsp-types under a non-clashing name. Hover
/// and CompletionItem returned by `DslBodyLsp` are 0.97 types; main.rs
/// otherwise speaks 0.94 (tower-lsp's vendored version).
mod v4_lsp_types {
    pub use lsp_types::CompletionItem;
}

/// Convert an LSP (line, utf16-col) position into a host-source byte
/// offset. Inverse of `byte_to_position`. Saturates at end-of-line on
/// past-end columns and at end-of-document on past-end lines.
fn position_to_byte(src: &str, p: Position) -> usize {
    let bytes = src.as_bytes();
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    let mut i: usize = 0;
    while i < bytes.len() && line < p.line {
        if bytes[i] == b'\n' { line += 1; line_start = i + 1; }
        i += 1;
    }
    if line < p.line { return bytes.len(); }
    let line_end = (line_start..bytes.len()).find(|&j| bytes[j] == b'\n').unwrap_or(bytes.len());
    let line_str = &src[line_start..line_end];
    let mut col_left: u32 = p.character;
    let mut byte = line_start;
    if line_str.is_ascii() {
        byte += (col_left as usize).min(line_str.len());
    } else {
        for ch in line_str.chars() {
            let w = ch.len_utf16() as u32;
            if col_left < w { break; }
            col_left -= w;
            byte += ch.len_utf8();
        }
    }
    byte
}

fn byte_to_position(src: &str, off: usize) -> Position {
    let off = off.min(src.len());
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (i, b) in src.as_bytes().iter().enumerate() {
        if i == off { break; }
        if *b == b'\n' { line += 1; line_start = i + 1; }
    }
    let slice = &src[line_start..off];
    let col: u32 = if slice.is_ascii() {
        slice.len() as u32
    } else {
        slice.chars().map(|c| c.len_utf16() as u32).sum()
    };
    Position::new(line, col)
}

fn default_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{ClientCapabilities, WorkspaceFolder};

    #[allow(deprecated)]
    fn init_params(root_uri: Option<Url>, workspace_folders: Option<Vec<WorkspaceFolder>>) -> InitializeParams {
        InitializeParams {
            process_id: None,
            root_path: None,
            root_uri,
            initialization_options: None,
            capabilities: ClientCapabilities::default(),
            trace: None,
            workspace_folders,
            client_info: None,
            locale: None,
        }
    }

    #[test]
    fn initialize_root_prefers_workspace_folder() {
        let root_uri = Url::from_directory_path("/tmp/root-uri").unwrap();
        let workspace_uri = Url::from_directory_path("/tmp/workspace").unwrap();
        let params = init_params(
            Some(root_uri),
            Some(vec![WorkspaceFolder {
                uri: workspace_uri,
                name: "workspace".into(),
            }]),
        );

        assert_eq!(Backend::root_from_initialize(&params), PathBuf::from("/tmp/workspace"));
    }

    #[test]
    fn initialize_root_falls_back_to_root_uri() {
        let root_uri = Url::from_directory_path("/tmp/root-uri").unwrap();
        let params = init_params(Some(root_uri), None);

        assert_eq!(Backend::root_from_initialize(&params), PathBuf::from("/tmp/root-uri"));
    }
}

// ── entry point ──────────────────────────────────────────────────────

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .init();

    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
