//! In-memory Reader. Implements repos/revs/files against canned maps.
//! Everything file-content-related is `unimplemented!()` until the first op
//! that needs it lands.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream;

use crate::types::FilePath;
use crate::config::Config;
use crate::reader::{
    CrossRefHit, ParsedTree, ParserKind, Reader, ScanCombo, ScanKind, ViolationEntry,
};
use crate::pattern::CompiledPattern;

/// `Reader` impl backed by in-memory maps. Used by tests and by the
/// hover-side DocSession when no git backend is wired. All state is
/// seeded via the `with_*` builder methods; no mutation after construction.
pub struct MemReader {
    pub repos:   Vec<Arc<str>>,
    pub revs:    HashMap<Arc<str>, Vec<Arc<str>>>,
    pub files:   HashMap<(Arc<str>, Arc<str>), Vec<FilePath>>,
    pub content: HashMap<(Arc<str>, Arc<str>, Arc<std::path::Path>), Bytes>,
    pub config:  Arc<Config>,
}

impl MemReader {
    /// Construct an empty MemReader bound to `config`. Use `with_repo`,
    /// `with_files`, `with_content` to populate.
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            repos: vec![],
            revs: HashMap::new(),
            files: HashMap::new(),
            content: HashMap::new(),
            config,
        }
    }

    /// Register `repo` with the given rev list.
    pub fn with_repo(mut self, repo: &str, revs: &[&str]) -> Self {
        let r: Arc<str> = Arc::from(repo);
        self.repos.push(r.clone());
        self.revs.insert(r, revs.iter().map(|s| Arc::<str>::from(*s)).collect());
        self
    }

    /// Register the file listing for `(repo, rev)`.
    pub fn with_files(mut self, repo: &str, rev: &str, paths: &[&str]) -> Self {
        let fps: Vec<FilePath> = paths.iter()
            .map(|p| FilePath(Arc::from(std::path::Path::new(p))))
            .collect();
        self.files.insert((Arc::from(repo), Arc::from(rev)), fps);
        self
    }

    /// Register blob bytes for `(repo, rev, path)`.
    pub fn with_content(mut self, repo: &str, rev: &str, path: &str, bytes: &[u8]) -> Self {
        let key = (
            Arc::<str>::from(repo),
            Arc::<str>::from(rev),
            Arc::<std::path::Path>::from(std::path::Path::new(path)),
        );
        self.content.insert(key, Bytes::copy_from_slice(bytes));
        self
    }
}

fn once<T: Send + 'static>(v: T) -> BoxStream<'static, T> {
    Box::pin(stream::iter(std::iter::once(v)))
}

impl Reader for MemReader {
    fn files(&self, repo: &str, rev: &str, pattern: &CompiledPattern)
        -> BoxStream<'static, Vec<FilePath>>
    {
        let key = (Arc::<str>::from(repo), Arc::<str>::from(rev));
        let all = self.files.get(&key).cloned().unwrap_or_default();
        let filtered = all.into_iter()
            .filter(|fp| pattern.is_match(&fp.0.to_string_lossy()))
            .collect();
        once(filtered)
    }

    fn bytes(&self, repo: &str, rev: &str, fp: &FilePath) -> BoxStream<'static, Bytes> {
        let key = (
            Arc::<str>::from(repo),
            Arc::<str>::from(rev),
            fp.0.clone(),
        );
        let data = self.content.get(&key).cloned().unwrap_or_default();
        once(data)
    }

    fn bytes_range(&self, _: &str, _: &str, _: &FilePath, _: Range<usize>)
        -> BoxStream<'static, Bytes>
    { unimplemented!() }

    fn parsed(&self, repo: &str, rev: &str, fp: &FilePath, kind: ParserKind)
        -> BoxStream<'static, Arc<ParsedTree>>
    {
        let key = (Arc::<str>::from(repo), Arc::<str>::from(rev), fp.0.clone());
        let bytes = self.content.get(&key).cloned().unwrap_or_default();
        let payload = crate::readers::parse_cache::parse_payload_sync(kind, &bytes);
        once(Arc::new(ParsedTree { kind, bytes, payload }))
    }

    fn repos(&self) -> BoxStream<'static, Vec<Arc<str>>> {
        once(self.repos.clone())
    }

    fn revs(&self, repo: &str) -> BoxStream<'static, Vec<Arc<str>>> {
        once(self.revs.get(repo).cloned().unwrap_or_default())
    }

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
        once(self.config.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::Reader;
    use futures::executor::block_on;
    use futures_util::stream::StreamExt;

    fn cfg() -> Arc<Config> {
        Arc::new(Config {
            repos: vec![], revs: vec![], fs_exclude: vec![], sprf_files: vec![],
            shell_allow: vec![],
            runtime: crate::config::RuntimeConfig {
                worker_threads: 1, buffer_size: 256, flush_interval_ms: 100,
                collect_witnesses: false,
            xref_cartesian_limit: 10_000,
            max_passes:           8,
            max_claims_per_pass:  10_000,
            max_cursors_per_root: 1_000_000,
            max_file_bytes:       1_048_576,
            },
            content_hash: 0,
        })
    }

    #[test]
    fn bytes_roundtrip() {
        let reader = MemReader::new(cfg())
            .with_repo("r", &["main"])
            .with_files("r", "main", &["pkg.json"])
            .with_content("r", "main", "pkg.json", b"hello");
        let fp = FilePath(Arc::from(std::path::Path::new("pkg.json")));
        let data = block_on(reader.bytes("r", "main", &fp).next()).unwrap();
        assert_eq!(&*data, b"hello");
    }
}
