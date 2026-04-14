//! In-memory Reader. Implements repos/revs/files against canned maps.
//! Everything file-content-related is `unimplemented!()` until the first op
//! that needs it lands.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream;

use crate::_0_types::FilePath;
use crate::_2_config::Config;
use crate::_3_reader::{
    CrossRefHit, ParsedTree, ParserKind, Reader, ScanCombo, ScanKind, ViolationEntry,
};
use crate::_8_parse::glob_match;

pub struct MemReader {
    pub repos:   Vec<Arc<str>>,
    pub revs:    HashMap<Arc<str>, Vec<Arc<str>>>,
    pub files:   HashMap<(Arc<str>, Arc<str>), Vec<FilePath>>,
    pub content: HashMap<(Arc<str>, Arc<str>, Arc<std::path::Path>), Bytes>,
    pub config:  Arc<Config>,
}

impl MemReader {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            repos: vec![],
            revs: HashMap::new(),
            files: HashMap::new(),
            content: HashMap::new(),
            config,
        }
    }

    pub fn with_repo(mut self, repo: &str, revs: &[&str]) -> Self {
        let r: Arc<str> = Arc::from(repo);
        self.repos.push(r.clone());
        self.revs.insert(r, revs.iter().map(|s| Arc::<str>::from(*s)).collect());
        self
    }

    pub fn with_files(mut self, repo: &str, rev: &str, paths: &[&str]) -> Self {
        let fps: Vec<FilePath> = paths.iter()
            .map(|p| FilePath(Arc::from(std::path::Path::new(p))))
            .collect();
        self.files.insert((Arc::from(repo), Arc::from(rev)), fps);
        self
    }

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
    fn files(&self, repo: &str, rev: &str, pattern: &str)
        -> BoxStream<'static, Vec<FilePath>>
    {
        let key = (Arc::<str>::from(repo), Arc::<str>::from(rev));
        let all = self.files.get(&key).cloned().unwrap_or_default();
        let filtered = all.into_iter()
            .filter(|fp| glob_match(pattern, &fp.0.to_string_lossy()))
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

    fn parsed(&self, _: &str, _: &str, _: &FilePath, _: ParserKind)
        -> BoxStream<'static, Arc<ParsedTree>>
    { unimplemented!() }

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
    use crate::_3_reader::Reader;
    use futures::executor::block_on;
    use futures_util::stream::StreamExt;

    fn cfg() -> Arc<Config> {
        Arc::new(Config {
            repos: vec![], revs: vec![], fs_exclude: vec![], sprf_files: vec![],
            shell_allow: vec![],
            runtime: crate::_2_config::RuntimeConfig {
                worker_threads: 1, buffer_size: 256, flush_interval_ms: 100,
                collect_witnesses: false,
            xref_cartesian_limit: 10_000,
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
