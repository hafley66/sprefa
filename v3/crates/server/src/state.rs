//! ServerState — the top-level Arc passed to every transport and handler.
//!
//! Single tokio runtime per process, multi-thread flavor. Every shared
//! resource that used to be threaded through Backend or rebuilt per
//! hover lives here exactly once: OpCache, fs watcher set, change
//! subject, the rayon-backed `BoundedWorkSteal<AstParseEffect>`, the
//! op registry, the workspace registry, and a root cancellation token.
//!
//! No SqliteStore — v3 surfaces SQLite as a per-run drain target, not a
//! persistent process-level store. No LspLayer — single-LSP-connection
//! is sufficient for v3 (cross-connection state is out of scope).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;

use effect_runtime::batchers::BoundedWorkSteal;
use effect_runtime::GenCounter;
use pipeline::effects::{ast_parse, AstParseEffect};
use pipeline::registry::Registry;
use pipeline::{spawn_fs_watcher, spawn_invalidator, ChangeSubject, FsWatcherGuard, OpCache};

use crate::workspace::WorkspaceRegistry;

/// Watcher debounce window. Mirrors the Backend constant before the
/// port; large enough to swallow editor save-burst patterns.
pub const WATCHER_DEBOUNCE: Duration = Duration::from_millis(200);

pub struct ServerState {
    /// Shared op cache. L1 only when an LSP buffer overlay is in the
    /// reader stack (see `feedback_buffer_no_l2`). The `/run` handler
    /// attaches an L2 disk cache per-request when `cfg.run.out_db` is set.
    pub cache: Arc<OpCache>,

    /// Op factory registry. Cloneable; ops are looked up by host CST
    /// node head names.
    pub registry: Registry,

    /// Workspace registry keyed by canonical root.
    pub workspaces: Arc<RwLock<WorkspaceRegistry>>,

    /// Side-band Subject<Change>. Producers (per-repo fs watchers) clone
    /// the inner sender via `subject.sender()`.
    pub subject: ChangeSubject,

    /// One watcher per (slug, canonical-root). Lazy-spawned on first
    /// touch by `ensure_watcher`.
    pub watchers: Arc<Mutex<HashMap<(Arc<str>, PathBuf), FsWatcherGuard>>>,

    /// One rayon pool for the life of the process. Cloned into every
    /// per-request RtCtx via the blanket `Batcher<E> for Arc<B>`.
    pub ast_parse_batcher: Arc<BoundedWorkSteal<AstParseEffect>>,

    /// Root cancel for shutdown. Per-doc / per-request tokens are
    /// children of this one.
    pub cancel_root: CancellationToken,

    /// Drain task draining `ChangeSubject` → `cache.evict_paths`. Held
    /// so its lifetime equals the ServerState's; aborted on shutdown.
    pub drain: tokio::task::JoinHandle<()>,

    /// Locator file path (`server.json`). Removed on shutdown.
    pub info_path: PathBuf,

    /// Fired by `serve_http` once every configured listener is bound and
    /// accepting. The daemon binary awaits this before writing
    /// `server.json` so clients (sprefa-lsp / sprefa-run) can't observe
    /// the locator before the socket is connectable.
    pub ready: Arc<Notify>,

    /// Process-wide monotonic round counter. Bumped at every input
    /// event that re-triggers pipeline evaluation (LSP did_change /
    /// did_open / did_save, fs-watcher events, manual reload). Read by
    /// op closures via `RtCtx::current_gen` to correlate effect rows
    /// back to the round that produced them. See `effect_runtime/src/
    /// generation.rs` and human-goals.md item 13.
    pub gen: Arc<GenCounter>,
}

impl ServerState {
    pub async fn new(opts: &ServerOpts) -> Result<Arc<Self>> {
        let cache = Arc::new(OpCache::new(true));
        let subject = ChangeSubject::new();
        let drain = spawn_invalidator(cache.clone(), &subject);

        let workers: usize = std::env::var("SPREFA_AST_WORKERS")
            .ok().and_then(|s| s.parse().ok()).unwrap_or(8);
        let cap: usize = std::env::var("SPREFA_AST_CAP")
            .ok().and_then(|s| s.parse().ok()).unwrap_or(256);
        let ast_parse_batcher = Arc::new(
            BoundedWorkSteal::<AstParseEffect>::new(cap, workers, ast_parse),
        );

        Ok(Arc::new(Self {
            cache,
            registry: Registry::with_stdlib(),
            workspaces: Arc::new(RwLock::new(WorkspaceRegistry::default())),
            subject,
            watchers: Arc::new(Mutex::new(HashMap::new())),
            ast_parse_batcher,
            cancel_root: CancellationToken::new(),
            drain,
            info_path: opts.info_path.clone().unwrap_or_else(default_info_path),
            ready: Arc::new(Notify::new()),
            gen: Arc::new(GenCounter::new()),
        }))
    }

    pub async fn shutdown(&self) {
        self.cancel_root.cancel();
        let _ = std::fs::remove_file(&self.info_path);
        self.drain.abort();
    }

    /// Lazy: ensure an FS watcher is running for `(slug, root)`. Called
    /// per seed before each pipe drain so newly configured seeds pick
    /// up live invalidation without restart.
    pub async fn ensure_watcher(&self, slug: Arc<str>, root: PathBuf) {
        let canonical = std::fs::canonicalize(&root).unwrap_or(root);
        let key = (slug.clone(), canonical.clone());
        let mut guard = self.watchers.lock().await;
        if guard.contains_key(&key) { return; }
        match spawn_fs_watcher(slug, canonical.clone(), self.subject.sender(), WATCHER_DEBOUNCE) {
            Ok(w) => {
                tracing::info!(
                    target: "sprefa::server",
                    slug = %key.0,
                    root = %canonical.display(),
                    "fs_watcher.spawned",
                );
                guard.insert(key, w);
            }
            Err(e) => {
                tracing::warn!(
                    target: "sprefa::server",
                    root = %canonical.display(),
                    err = %e,
                    "fs_watcher.spawn_failed",
                );
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ServerOpts {
    pub http_unix:  Option<PathBuf>,
    pub http_tcp:   Option<SocketAddr>,
    pub log_path:   Option<PathBuf>,
    pub info_path:  Option<PathBuf>,
    pub foreground: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerInfo {
    pub pid:        u32,
    pub version:    String,
    pub started_at: String,
    pub http:       HttpInfo,
    pub log_path:   Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HttpInfo {
    pub unix: Option<PathBuf>,
    pub tcp:  Option<String>,
}

impl ServerInfo {
    pub fn write_atomic(path: &Path, info: &ServerInfo) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(info)?;
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<ServerInfo> {
        let bytes = std::fs::read(path)?;
        let info: ServerInfo = serde_json::from_slice(&bytes)?;
        Ok(info)
    }
}

pub fn default_info_path() -> PathBuf {
    if let Ok(p) = std::env::var("SPREFA_SERVER_INFO") { return PathBuf::from(p); }
    if let Ok(r) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(r).join("sprefa").join("server.json");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".cache").join("sprefa").join("server.json");
    }
    PathBuf::from("/tmp/sprefa-server.json")
}

pub fn default_unix_socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("SPREFA_HTTP_UNIX") { return PathBuf::from(p); }
    if let Ok(r) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(r).join("sprefa").join("sprefa.sock");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".cache").join("sprefa").join("sprefa.sock");
    }
    PathBuf::from("/tmp/sprefa.sock")
}

pub fn default_log_path() -> PathBuf {
    if let Ok(p) = std::env::var("SPREFA_LOG_PATH") { return PathBuf::from(p); }
    if let Ok(s) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(s).join("sprefa").join("server.log");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".local").join("state").join("sprefa").join("server.log");
    }
    PathBuf::from("/tmp/sprefa-server.log")
}
