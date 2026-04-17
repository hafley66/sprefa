//! GitBlobReader — reads files and bytes from git object DB.
//! No working-tree checkout required; works with bare clones, mirrors,
//! and user checkouts alike.

use std::ops::Range;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream;

use crate::_0_types::FilePath;
use crate::_2_config::Config;
use crate::_3_reader::{
    CrossRefHit, ParsedTree, ParserKind, Reader, ScanCombo, ScanKind, ViolationEntry,
};
use crate::_16_pattern::CompiledPattern;
use super::_1_locator::CheckoutLocator;

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

pub struct GitBlobReader {
    locator: Arc<dyn CheckoutLocator>,
    /// Lazily opened repo handles. git2::Repository is Send but not Sync;
    /// each entry is behind its own Mutex for &self method access.
    repos:   Mutex<HashMap<String, Arc<Mutex<git2::Repository>>>>,
    /// Tree-oid cache per (repo, rev). Without this, every bytes() call
    /// reruns `revparse_single` + `peel_to_commit` — N-files sequential
    /// revparses on a single repo. With it, one revparse per (repo,rev);
    /// later calls take `find_tree(oid)` directly.
    trees:   Mutex<HashMap<(String, String), git2::Oid>>,
    config:  Arc<Config>,
}

impl GitBlobReader {
    pub fn new(locator: Arc<dyn CheckoutLocator>, config: Arc<Config>) -> Self {
        Self {
            locator,
            repos:  Mutex::new(HashMap::new()),
            trees:  Mutex::new(HashMap::new()),
            config,
        }
    }

    fn open_repo(&self, slug: &str) -> Option<Arc<Mutex<git2::Repository>>> {
        let mut cache = self.repos.lock().unwrap();
        if let Some(r) = cache.get(slug) {
            return Some(r.clone());
        }
        let path = self.locator.locate(slug, "")?;
        let repo = git2::Repository::open(&path).ok()?;
        let entry = Arc::new(Mutex::new(repo));
        cache.insert(slug.to_string(), entry.clone());
        Some(entry)
    }

    /// Tree Oid for (repo, rev). Cached after first revparse.
    fn tree_oid(&self, repo_slug: &str, rev: &str, repo: &git2::Repository)
        -> Result<git2::Oid, git2::Error>
    {
        {
            let g = self.trees.lock().unwrap();
            if let Some(oid) = g.get(&(repo_slug.to_string(), rev.to_string())) {
                return Ok(*oid);
            }
        }
        let obj = repo.revparse_single(rev)?;
        let commit = obj.peel_to_commit()?;
        let oid = commit.tree_id();
        self.trees.lock().unwrap()
            .insert((repo_slug.to_string(), rev.to_string()), oid);
        Ok(oid)
    }

    fn walk_tree(tree: &git2::Tree, repo: &git2::Repository, pattern: &CompiledPattern) -> Vec<FilePath> {
        let mut paths = Vec::new();
        let _ = tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                let name = entry.name().unwrap_or("");
                let full = if root.is_empty() {
                    name.to_owned()
                } else {
                    format!("{}{}", root, name)
                };
                if pattern.is_match(&full) {
                    paths.push(FilePath(Arc::from(Path::new(&full))));
                }
            }
            git2::TreeWalkResult::Ok
        });
        let _ = repo; // suppress unused warning; may need repo for subtree walks later
        paths
    }
}

fn once_val<T: Send + 'static>(v: T) -> BoxStream<'static, T> {
    Box::pin(stream::once(async move { v }))
}

impl Reader for GitBlobReader {
    fn files(&self, repo: &str, rev: &str, pattern: &CompiledPattern) -> BoxStream<'static, Vec<FilePath>> {
        let Some(repo_arc) = self.open_repo(repo) else {
            return once_val(vec![]);
        };
        let result = {
            let guard = repo_arc.lock().unwrap();
            match self.tree_oid(repo, rev, &guard) {
                Ok(oid) => guard.find_tree(oid)
                    .map(|tree| Self::walk_tree(&tree, &guard, pattern))
                    .unwrap_or_default(),
                Err(_) => Vec::new(),
            }
        };
        once_val(result)
    }

    fn bytes(&self, repo: &str, rev: &str, fs: &FilePath) -> BoxStream<'static, Bytes> {
        let Some(repo_arc) = self.open_repo(repo) else {
            return once_val(Bytes::new());
        };
        let rel = fs.0.to_string_lossy().into_owned();
        let result = {
            let guard = repo_arc.lock().unwrap();
            (|| -> Option<Bytes> {
                let oid = self.tree_oid(repo, rev, &guard).ok()?;
                let tree = guard.find_tree(oid).ok()?;
                let entry = tree.get_path(Path::new(&rel)).ok()?;
                let obj = entry.to_object(&guard).ok()?;
                let blob = obj.peel_to_blob().ok()?;
                Some(Bytes::copy_from_slice(blob.content()))
            })().unwrap_or_default()
        };
        once_val(result)
    }

    fn bytes_range(&self, repo: &str, rev: &str, fs: &FilePath, range: Range<usize>)
        -> BoxStream<'static, Bytes>
    {
        let Some(repo_arc) = self.open_repo(repo) else {
            return once_val(Bytes::new());
        };
        let rel = fs.0.to_string_lossy().into_owned();
        let result = {
            let guard = repo_arc.lock().unwrap();
            (|| -> Option<Bytes> {
                let oid = self.tree_oid(repo, rev, &guard).ok()?;
                let tree = guard.find_tree(oid).ok()?;
                let entry = tree.get_path(Path::new(&rel)).ok()?;
                let obj = entry.to_object(&guard).ok()?;
                let blob = obj.peel_to_blob().ok()?;
                let content = blob.content();
                let start = range.start.min(content.len());
                let end   = range.end.min(content.len());
                Some(Bytes::copy_from_slice(&content[start..end]))
            })().unwrap_or_default()
        };
        once_val(result)
    }

    fn repos(&self) -> BoxStream<'static, Vec<Arc<str>>> {
        once_val(self.locator.repos())
    }

    fn revs(&self, repo: &str) -> BoxStream<'static, Vec<Arc<str>>> {
        once_val(self.locator.revs(repo))
    }

    fn parsed(&self, _: &str, _: &str, _: &FilePath, _: ParserKind)
        -> BoxStream<'static, Arc<ParsedTree>>
    { unimplemented!("stage C+: parsed trees") }

    fn cross_ref(&self, _: &str, _: &str, _: &str, _: &str)
        -> BoxStream<'static, Vec<CrossRefHit>>
    { unimplemented!() }

    fn unscanned(&self, _: &str, _: &str, _: ScanKind, _: bool)
        -> BoxStream<'static, Vec<ScanCombo>>
    { unimplemented!() }

    fn violations(&self, _: Option<&str>) -> BoxStream<'static, Vec<ViolationEntry>>
    { unimplemented!() }

    fn run_visited(&self, _: u64, _: u64, _: u64) -> BoxStream<'static, bool>
    { unimplemented!() }

    fn config(&self) -> BoxStream<'static, Arc<Config>> {
        once_val(self.config.clone())
    }
}
