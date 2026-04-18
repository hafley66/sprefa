//! WorkspaceRegistry — path hint → Arc<WorkspaceCtx>.
//!
//! A WorkspaceCtx bundles the resolved Config, CheckoutLocator, Reader stack,
//! and BufferOverlay for one workspace root. Workspaces are an LSP/CLI-surface
//! concept; nothing under run_pipeline cares which workspace it came from.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info_span, Instrument};

use crate::readers::{
    build_index_stack, BufferOverlay, CheckoutLocator, ConfigLocator, GitBlobReader,
    InMemoryLocator, MemReader,
};
use crate::{Config, Reader};

// ---------------------------------------------------------------------------
// WorkspaceCtx
// ---------------------------------------------------------------------------

pub struct WorkspaceCtx {
    pub root:    Arc<Path>,
    pub config:  Arc<Config>,
    pub locator: Arc<dyn CheckoutLocator>,
    pub reader:  Arc<BufferOverlay>,
    /// Per-workspace string interner. Readers and ops look up repo /
    /// rev / path strings here; repeat lookups return the same
    /// `Arc<str>` so cursor clones only bump refcounts. Scoped to
    /// workspace reload; drops with `WorkspaceCtx`.
    pub intern:  Arc<crate::_18_str_intern::StrInterner>,
}

impl WorkspaceCtx {
    /// The underlying buffer-mutable overlay. Same Arc as `reader`; exposed as
    /// a distinct accessor so LSP code that only writes buffers does not pass
    /// around a full Reader.
    pub fn overlay(&self) -> Arc<BufferOverlay> {
        self.reader.clone()
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct WorkspaceRegistry {
    by_root: HashMap<PathBuf, Arc<WorkspaceCtx>>,
}

impl WorkspaceRegistry {
    pub async fn resolve(&mut self, hint: &Path) -> Arc<WorkspaceCtx> {
        let root = info_span!("workspace.resolve_root")
            .in_scope(|| resolve_root(hint));
        if let Some(ws) = self.by_root.get(&root) {
            return ws.clone();
        }
        let ws = build_workspace_ctx(&root)
            .instrument(info_span!("workspace.build_ctx", root = ?root))
            .await;
        self.by_root.insert(root, ws.clone());
        ws
    }

    pub fn get(&self, root: &Path) -> Option<Arc<WorkspaceCtx>> {
        self.by_root.get(root).cloned()
    }

    pub fn len(&self) -> usize {
        self.by_root.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_root.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Root resolution
// ---------------------------------------------------------------------------

fn resolve_root(hint: &Path) -> PathBuf {
    let start: &Path = if hint.is_file() {
        hint.parent().unwrap_or(hint)
    } else {
        hint
    };
    walk_up_for_file(start, ".sprefa.toml")
        .and_then(|p| p.parent().map(PathBuf::from))
        .or_else(|| walk_up_for_git_root(start))
        .unwrap_or_else(|| start.to_path_buf())
}

pub fn walk_up_for_file(start: &Path, name: &str) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(d) = cur {
        let c = d.join(name);
        if c.is_file() {
            return Some(c);
        }
        cur = d.parent();
    }
    None
}

pub fn walk_up_for_git_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(d) = cur {
        if d.join(".git").exists() {
            return Some(d.to_path_buf());
        }
        cur = d.parent();
    }
    None
}

// ---------------------------------------------------------------------------
// Build
// ---------------------------------------------------------------------------

/// Priority:
///   1. `.sprefa.toml` walk-up → `ConfigLocator` + `GitBlobReader`
///   2. `.git` walk-up → auto single-repo `InMemoryLocator` (slug = basename,
///      revs = `["HEAD"]`) + `GitBlobReader`
///   3. Fallback: empty `MemReader`
///
/// Every branch wraps the inner reader in a `BufferOverlay` so LSP edits
/// shadow on-disk bytes transparently.
async fn build_workspace_ctx(root: &Path) -> Arc<WorkspaceCtx> {
    // Branch 1 — .sprefa.toml at or under root
    if let Some(toml) = walk_up_for_file(root, ".sprefa.toml") {
        if let Some(loc) = ConfigLocator::from_toml_file(&toml).ok() {
            let loc_arc: Arc<dyn CheckoutLocator> = Arc::new(loc);
            let cfg = make_config_from_locator(&*loc_arc);
            let (prov, idx) = build_index_stack().await;
            let inner: Arc<dyn Reader + Send + Sync> =
                Arc::new(GitBlobReader::new_with_index(loc_arc.clone(), cfg.clone(), prov, idx));
            let intern = Arc::new(crate::_18_str_intern::StrInterner::new());
            let overlay = Arc::new(BufferOverlay::with_intern(inner, intern.clone()));
            let actual_root: Arc<Path> = loc_arc
                .repos()
                .first()
                .and_then(|slug| loc_arc.locate(slug, "HEAD"))
                .map(|p| Arc::from(p.as_path()))
                .unwrap_or_else(|| Arc::from(root));
            return Arc::new(WorkspaceCtx {
                root:    actual_root,
                config:  cfg,
                locator: loc_arc,
                reader:  overlay,
                intern,
            });
        }
    }

    // Branch 2 — auto git root
    if let Some(git_root) = walk_up_for_git_root(root) {
        let slug = git_root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("repo")
            .to_string();
        let loc = InMemoryLocator::new().with_repo(&slug, git_root.clone(), &["HEAD"]);
        let loc_arc: Arc<dyn CheckoutLocator> = Arc::new(loc);
        let cfg = make_config_from_locator(&*loc_arc);
        let (prov, idx) = build_index_stack().await;
        let inner: Arc<dyn Reader + Send + Sync> =
            Arc::new(GitBlobReader::new_with_index(loc_arc.clone(), cfg.clone(), prov, idx));
        let intern = Arc::new(crate::_18_str_intern::StrInterner::new());
        let overlay = Arc::new(BufferOverlay::with_intern(inner, intern.clone()));
        return Arc::new(WorkspaceCtx {
            root:    Arc::from(git_root.as_path()),
            config:  cfg,
            locator: loc_arc,
            reader:  overlay,
            intern,
        });
    }

    // Branch 3 — fallback
    let cfg = empty_config();
    let inner: Arc<dyn Reader + Send + Sync> = Arc::new(MemReader::new(cfg.clone()));
    let intern = Arc::new(crate::_18_str_intern::StrInterner::new());
    let overlay = Arc::new(BufferOverlay::with_intern(inner, intern.clone()));
    // Empty locator; any Op that needs one will short-circuit on the empty repos list.
    let loc: Arc<dyn CheckoutLocator> = Arc::new(InMemoryLocator::new());
    Arc::new(WorkspaceCtx {
        root:    Arc::from(root),
        config:  cfg,
        locator: loc,
        reader:  overlay,
        intern,
    })
}

fn make_config_from_locator(loc: &dyn CheckoutLocator) -> Arc<Config> {
    let repos = loc.repos();
    let mut revs: Vec<Arc<str>> = Vec::new();
    for r in &repos {
        for rv in loc.revs(r) {
            if !revs.contains(&rv) {
                revs.push(rv);
            }
        }
    }
    Arc::new(Config { repos, revs, ..Config::test_default() })
}

fn empty_config() -> Arc<Config> {
    Arc::new(Config::test_default())
}
