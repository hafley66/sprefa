//! WorkspaceCtx + WorkspaceRegistry.
//!
//! A `WorkspaceCtx` bundles the resolved `.sprefa.toml`-driven Config and
//! a buffer-overlay-wrapped `DiskFileSource` for one workspace root.
//! LSP `did_change` writes bytes into `source` so subsequent pipes see
//! the editor buffer; `did_save` clears the overlay and disk reads
//! resume.
//!
//! Workspaces are an LSP/CLI-surface concept; nothing under `run.rs`
//! cares which workspace a request came from once the source has been
//! cloned out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use pipeline::readers::{BufferOverlay, DiskFileSource};

use crate::config::{self, Config};

pub struct WorkspaceCtx {
    pub root:   PathBuf,
    pub config: Arc<Config>,
    /// Editor-buffer-aware reader. Same Arc handed to every pipe drain
    /// running under this workspace. Mutated by LSP `did_change`.
    pub source: Arc<BufferOverlay<DiskFileSource>>,
    /// Per-workspace cancel; child of `ServerState.cancel_root`. Tripped
    /// on `.sprefa.toml` change to invalidate all derived state.
    pub cancel: CancellationToken,
}

#[derive(Default)]
pub struct WorkspaceRegistry {
    by_root: HashMap<PathBuf, Arc<WorkspaceCtx>>,
}

impl WorkspaceRegistry {
    pub async fn resolve(
        &mut self,
        hint: &Path,
        parent_cancel: &CancellationToken,
    ) -> Arc<WorkspaceCtx> {
        let root = resolve_root(hint);
        if let Some(ws) = self.by_root.get(&root) {
            return ws.clone();
        }
        let ctx = build_workspace_ctx(&root, parent_cancel.child_token());
        self.by_root.insert(root, ctx.clone());
        ctx
    }

    pub fn len(&self) -> usize { self.by_root.len() }
    pub fn is_empty(&self) -> bool { self.by_root.is_empty() }
}

fn resolve_root(hint: &Path) -> PathBuf {
    let start: PathBuf = if hint.is_file() {
        hint.parent().unwrap_or(hint).to_path_buf()
    } else {
        hint.to_path_buf()
    };
    walk_up_for_file(&start, ".sprefa.toml")
        .and_then(|p| p.parent().map(PathBuf::from))
        .or_else(|| walk_up_for_dir(&start, ".git"))
        .unwrap_or(start)
}

fn walk_up_for_file(start: &Path, name: &str) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(d) = cur {
        let c = d.join(name);
        if c.is_file() { return Some(c); }
        cur = d.parent();
    }
    None
}

fn walk_up_for_dir(start: &Path, name: &str) -> Option<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(d) = cur {
        if d.join(name).exists() { return Some(d.to_path_buf()); }
        cur = d.parent();
    }
    None
}

fn build_workspace_ctx(root: &Path, cancel: CancellationToken) -> Arc<WorkspaceCtx> {
    // Try `.sprefa.toml` first; fall back to a default cwd-rooted config.
    let cfg = walk_up_for_file(root, ".sprefa.toml")
        .and_then(|p| config::from_path(&p).ok())
        .unwrap_or_else(|| config::from_cli(root.to_path_buf(), "wt".to_string()));
    let inner = Arc::new(DiskFileSource::new(root.to_path_buf(), "wt".to_string()));
    let source = Arc::new(BufferOverlay::new(inner));
    Arc::new(WorkspaceCtx {
        root:   root.to_path_buf(),
        config: Arc::new(cfg),
        source,
        cancel,
    })
}
