//! BufferOverlay reader. Layer 3 of the Reader stack: blob → worktree → buffer.
//! Live editor buffers shadow inner for matching (repo, rev, path) triples.
//! All other reads delegate unchanged.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream;

use crate::_0_types::FilePath;
use crate::_2_config::Config;
use crate::_3_reader::{
    CrossRefHit, ParsedTree, ParserKind, Reader, ScanCombo, ScanKind, ViolationEntry,
};
use crate::_8_parse::glob_match;

// ---------------------------------------------------------------------------
// Key
// ---------------------------------------------------------------------------

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub struct BufferKey {
    pub repo: Arc<str>,
    pub rev:  Arc<str>,
    pub path: Arc<str>,
}

// ---------------------------------------------------------------------------
// Overlay
// ---------------------------------------------------------------------------

pub struct BufferOverlay {
    pub inner:   Arc<dyn Reader + Send + Sync>,
    pub buffers: Arc<RwLock<HashMap<BufferKey, Arc<Bytes>>>>,
}

impl BufferOverlay {
    pub fn new(inner: Arc<dyn Reader + Send + Sync>) -> Self {
        Self {
            inner,
            buffers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_buffer(&self, key: BufferKey, bytes: Arc<Bytes>) {
        self.buffers.write().unwrap().insert(key, bytes);
    }

    pub fn clear_buffer(&self, key: &BufferKey) {
        self.buffers.write().unwrap().remove(key);
    }

    pub fn has_buffer(&self, key: &BufferKey) -> bool {
        self.buffers.read().unwrap().contains_key(key)
    }
}

fn once<T: Send + 'static>(v: T) -> BoxStream<'static, T> {
    Box::pin(stream::iter(std::iter::once(v)))
}

impl Reader for BufferOverlay {
    fn files(&self, repo: &str, rev: &str, pattern: &str) -> BoxStream<'static, Vec<FilePath>> {
        // Pre-collect buffer paths that match this (repo, rev, pattern). The
        // buffer map is tiny; snapshot sync, then union async with inner.
        let extra: Vec<FilePath> = {
            let guard = self.buffers.read().unwrap();
            guard
                .keys()
                .filter(|k| k.repo.as_ref() == repo && k.rev.as_ref() == rev)
                .filter(|k| glob_match(pattern, k.path.as_ref()))
                .map(|k| FilePath(Arc::from(std::path::Path::new(k.path.as_ref()))))
                .collect()
        };

        // Build the union inside the returned stream — consumer drives the
        // inner.files() await. No block_on anywhere in this Reader impl.
        use futures_util::StreamExt;
        let inner_stream = self.inner.files(repo, rev, pattern);
        Box::pin(inner_stream.map(move |mut inner_files| {
            for fp in extra.iter() {
                if !inner_files.contains(fp) {
                    inner_files.push(fp.clone());
                }
            }
            inner_files
        }))
    }

    fn bytes(&self, repo: &str, rev: &str, fp: &FilePath) -> BoxStream<'static, Bytes> {
        let path_str = fp.0.to_string_lossy();
        let key = BufferKey {
            repo: Arc::from(repo),
            rev:  Arc::from(rev),
            path: Arc::from(path_str.as_ref()),
        };
        if let Some(arc_bytes) = self.buffers.read().unwrap().get(&key).cloned() {
            return once((*arc_bytes).clone());
        }
        self.inner.bytes(repo, rev, fp)
    }

    fn bytes_range(
        &self,
        repo: &str,
        rev: &str,
        fp: &FilePath,
        range: Range<usize>,
    ) -> BoxStream<'static, Bytes> {
        let path_str = fp.0.to_string_lossy();
        let key = BufferKey {
            repo: Arc::from(repo),
            rev:  Arc::from(rev),
            path: Arc::from(path_str.as_ref()),
        };
        if let Some(arc_bytes) = self.buffers.read().unwrap().get(&key).cloned() {
            let sliced = arc_bytes.slice(range);
            return once(sliced);
        }
        self.inner.bytes_range(repo, rev, fp, range)
    }

    fn parsed(
        &self,
        repo: &str,
        rev: &str,
        fp: &FilePath,
        kind: ParserKind,
    ) -> BoxStream<'static, Arc<ParsedTree>> {
        // Buffer overlay has no parser; delegate unconditionally.
        self.inner.parsed(repo, rev, fp, kind)
    }

    fn repos(&self) -> BoxStream<'static, Vec<Arc<str>>> {
        self.inner.repos()
    }

    fn revs(&self, repo: &str) -> BoxStream<'static, Vec<Arc<str>>> {
        self.inner.revs(repo)
    }

    fn cross_ref(
        &self,
        rule: &str,
        var: &str,
        repo: &str,
        rev: &str,
    ) -> BoxStream<'static, Vec<CrossRefHit>> {
        self.inner.cross_ref(rule, var, repo, rev)
    }

    fn unscanned(
        &self,
        table: &str,
        column: &str,
        kind: ScanKind,
        norm: bool,
    ) -> BoxStream<'static, Vec<ScanCombo>> {
        self.inner.unscanned(table, column, kind, norm)
    }

    fn violations(&self, check: Option<&str>) -> BoxStream<'static, Vec<ViolationEntry>> {
        self.inner.violations(check)
    }

    fn run_visited(&self, run_id: u64, op_id: u64, cursor_hash: u64) -> BoxStream<'static, bool> {
        self.inner.run_visited(run_id, op_id, cursor_hash)
    }

    fn config(&self) -> BoxStream<'static, Arc<Config>> {
        self.inner.config()
    }
}
