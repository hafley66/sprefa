//! LspLayer — per-URI DocSession state plus per-connection subscriber
//! fan-out for diagnostics and other unsolicited server→editor messages.
//!
//! One LspLayer per ServerState. Every LSP WebSocket connection registers a
//! subscriber sender for each URI it opens; `publish` iterates all subscribers
//! for a URI and prunes dead ones on send failure.
//!
//! D1 stubs: types compile; wiring with ServerState lands in D3.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::{mpsc, Mutex};
use tower_lsp::lsp_types::{Diagnostic, Url};

use crate::analysis::DocSession;
use crate::readers::_3_buffer::BufferKey;

use super::_0_state::ServerState;
use super::_1_workspace::WorkspaceCtx;

// ---------------------------------------------------------------------------
// Per-URI session
// ---------------------------------------------------------------------------

pub struct LspSession {
    pub session:   DocSession,
    pub source:    String,
    pub workspace: Arc<WorkspaceCtx>,
    /// Keys written into the BufferOverlay for this URI. Retained so `close`
    /// can clear exactly what `open_or_replace` wrote, without re-deriving.
    pub overlay_keys: HashSet<BufferKey>,
}

// ---------------------------------------------------------------------------
// Outbound messages to editor (per WebSocket connection)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum LspOutbound {
    Diagnostics { uri: Url, diags: Vec<Diagnostic> },
    ShowMessage(String),
}

// ---------------------------------------------------------------------------
// Layer
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct LspLayer {
    by_uri:      Mutex<HashMap<Url, LspSession>>,
    subscribers: Mutex<HashMap<Url, Vec<mpsc::Sender<LspOutbound>>>>,
}

impl LspLayer {
    pub async fn open_or_replace(
        &self,
        state: &ServerState,
        uri: &Url,
        source: String,
    ) {
        let hint = uri_to_hint(uri);
        let workspace = state.workspaces.write().await.resolve(&hint);

        let overlay_keys = shadow_into_overlay(&workspace, &source);

        let mut guard = self.by_uri.lock().await;
        match guard.get_mut(uri) {
            Some(entry) => {
                entry.session.on_source_change(source.clone());
                entry.session.set_workspace_root(Some(workspace.root.clone()));
                entry.source = source;
                entry.overlay_keys = overlay_keys;
                entry.workspace = workspace;
                entry.session.ensure_run();
            }
            None => {
                let mut session = DocSession::new(
                    source.clone(),
                    workspace.reader.clone(),
                    state.registry.clone(),
                );
                session.set_workspace_root(Some(workspace.root.clone()));
                session.ensure_run();
                guard.insert(
                    uri.clone(),
                    LspSession { session, source, workspace, overlay_keys },
                );
            }
        }
    }

    pub async fn close(&self, _state: &ServerState, uri: &Url) {
        let mut guard = self.by_uri.lock().await;
        if let Some(entry) = guard.remove(uri) {
            for key in &entry.overlay_keys {
                entry.workspace.reader.clear_buffer(key);
            }
        }
    }

    pub async fn doc_count(&self) -> usize {
        self.by_uri.lock().await.len()
    }

    // ----- subscriber fan-out -----

    pub async fn register_subscriber(&self, uri: &Url, tx: mpsc::Sender<LspOutbound>) {
        self.subscribers.lock().await.entry(uri.clone()).or_default().push(tx);
    }

    pub async fn unregister_subscribers_of(&self, target: &mpsc::Sender<LspOutbound>) {
        let mut guard = self.subscribers.lock().await;
        for senders in guard.values_mut() {
            senders.retain(|tx| !std::ptr::eq(
                tx as *const _ as *const (),
                target as *const _ as *const (),
            ));
        }
        guard.retain(|_, v| !v.is_empty());
    }

    pub async fn publish(&self, uri: &Url, diags: Vec<Diagnostic>) {
        let senders: Vec<mpsc::Sender<LspOutbound>> = {
            let guard = self.subscribers.lock().await;
            guard.get(uri).cloned().unwrap_or_default()
        };
        let msg = LspOutbound::Diagnostics { uri: uri.clone(), diags };
        for tx in senders {
            let _ = tx.try_send(msg.clone());
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn uri_to_hint(uri: &Url) -> std::path::PathBuf {
    uri.to_file_path()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")))
}

/// Push the `.sprf` source into the workspace overlay for every
/// `(repo, rev)` pair the workspace knows. That way any op inside this
/// workspace that reads the same path sees the unsaved bytes.
///
/// The `.sprf` file itself is rarely the target of reads; this is mostly
/// defensive. Non-sprf targets (the files the `.sprf` *matches against*) are
/// handled via direct overlay writes from editor text sync in their own URIs.
fn shadow_into_overlay(workspace: &Arc<WorkspaceCtx>, source: &str) -> HashSet<BufferKey> {
    // Full wiring lands in D3 where the caller has (repo, rev, path) from
    // the URI + workspace mapping.
    let _ = source;
    let _ = workspace;
    HashSet::new()
}

#[allow(dead_code)]
fn make_buffer_key(repo: &str, rev: &str, path: &Path) -> BufferKey {
    BufferKey {
        repo: Arc::from(repo),
        rev:  Arc::from(rev),
        path: Arc::from(path.to_string_lossy().as_ref()),
    }
}

#[allow(dead_code)]
fn bytes_from_source(source: &str) -> Arc<Bytes> {
    Arc::new(Bytes::copy_from_slice(source.as_bytes()))
}
