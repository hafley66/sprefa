//! ServerState — the top-level Arc passed to every transport and handler.
//!
//! One tokio runtime per process, multi-thread flavor (DocSession.run_sync
//! relies on `block_in_place`). All other state hangs off this struct.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::ops::default_registry;
use crate::store::{SqliteStore, Store};
use crate::{OperatorRegistry, ResultStore, RuntimeConfig};

use super::_1_workspace::WorkspaceRegistry;
use super::_2_lsp_layer::LspLayer;

// ---------------------------------------------------------------------------
// ServerState
// ---------------------------------------------------------------------------

pub struct ServerState {
    pub runtime_cfg: Arc<RuntimeConfig>,
    pub store:       Arc<dyn Store>,
    pub results:     Arc<ResultStore>,
    pub registry:    Arc<OperatorRegistry>,
    pub workspaces:  Arc<RwLock<WorkspaceRegistry>>,
    pub lsp:         Arc<LspLayer>,
    pub cancel_root: CancellationToken,
    pub info_path:   PathBuf,
}

impl ServerState {
    pub async fn new(opts: &ServerOpts) -> Result<Self> {
        let store: Arc<dyn Store> = match &opts.store_path {
            Some(p) => SqliteStore::open(p).await
                .map_err(|e| anyhow::anyhow!("open store {}: {e:?}", p.display()))?,
            None => SqliteStore::open_memory().await
                .map_err(|e| anyhow::anyhow!("open in-memory store: {e:?}"))?,
        };
        Ok(Self {
            runtime_cfg: Arc::new(RuntimeConfig::test_default()),
            store,
            results:     Arc::new(ResultStore::default()),
            registry:    Arc::new(default_registry()),
            workspaces:  Arc::new(RwLock::new(WorkspaceRegistry::default())),
            lsp:         Arc::new(LspLayer::default()),
            cancel_root: CancellationToken::new(),
            info_path:   opts.info_path.clone().unwrap_or_else(default_info_path),
        })
    }

    pub async fn shutdown(&self) {
        self.cancel_root.cancel();
        let _ = std::fs::remove_file(&self.info_path);
    }
}

// ---------------------------------------------------------------------------
// Startup knobs
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct ServerOpts {
    pub store_path: Option<PathBuf>,
    pub http_unix:  Option<PathBuf>,
    pub http_tcp:   Option<SocketAddr>,
    pub log_path:   Option<PathBuf>,
    pub info_path:  Option<PathBuf>,
    pub foreground: bool,
}

// ---------------------------------------------------------------------------
// server.json — the locator file shells use to find a running server
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerInfo {
    pub pid:        u32,
    pub version:    String,
    pub started_at: String,
    pub http:       HttpInfo,
    pub store_path: Option<PathBuf>,
    pub log_path:   Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HttpInfo {
    pub unix: Option<PathBuf>,
    pub tcp:  Option<String>,        // "127.0.0.1:7417"; String avoids serde gymnastics on SocketAddr
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

// ---------------------------------------------------------------------------
// Mutation approval strategy per request origin
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug)]
pub enum HandlerProto {
    AutoApprove,
    InteractiveLsp,
}

// ---------------------------------------------------------------------------
// Default paths
// ---------------------------------------------------------------------------

pub fn default_info_path() -> PathBuf {
    if let Ok(p) = std::env::var("SPREFA_SERVER_INFO") {
        return PathBuf::from(p);
    }
    if let Ok(r) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(r).join("sprefa").join("server.json");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".cache").join("sprefa").join("server.json");
    }
    PathBuf::from("/tmp/sprefa-server.json")
}

pub fn default_unix_socket_path() -> PathBuf {
    if let Ok(p) = std::env::var("SPREFA_HTTP_UNIX") {
        return PathBuf::from(p);
    }
    if let Ok(r) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(r).join("sprefa").join("sprefa.sock");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".cache").join("sprefa").join("sprefa.sock");
    }
    PathBuf::from("/tmp/sprefa.sock")
}

pub fn default_log_path() -> PathBuf {
    if let Ok(p) = std::env::var("SPREFA_LOG_PATH") {
        return PathBuf::from(p);
    }
    if let Ok(s) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(s).join("sprefa").join("server.log");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".local").join("state").join("sprefa").join("server.log");
    }
    PathBuf::from("/tmp/sprefa-server.log")
}

pub fn default_store_path() -> PathBuf {
    if let Ok(p) = std::env::var("SPREFA_STORE_PATH") {
        return PathBuf::from(p);
    }
    if let Ok(s) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(s).join("sprefa").join("store.sqlite");
    }
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h).join(".local").join("state").join("sprefa").join("store.sqlite");
    }
    PathBuf::from("/tmp/sprefa-store.sqlite")
}
