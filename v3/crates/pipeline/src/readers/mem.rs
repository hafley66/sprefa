//! In-memory [`FileSource`] for tests and fixtures. Mirrors v2
//! `readers::MemReader` at the slice needed for Stage A.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::FileSource;

#[derive(Default)]
pub struct MemFileSource {
    files: HashMap<(String, String), Vec<Arc<Path>>>,
    bytes: HashMap<(String, String, PathBuf), Arc<[u8]>>,
}

impl MemFileSource {
    pub fn new() -> Self { Self::default() }

    /// Register files under a `(repo, rev)`. Idempotent append.
    pub fn with_files(mut self, repo: &str, rev: &str, paths: &[&str]) -> Self {
        let entry = self
            .files
            .entry((repo.to_string(), rev.to_string()))
            .or_default();
        for p in paths {
            entry.push(Arc::from(PathBuf::from(p).as_path()));
        }
        self
    }

    /// Register file bytes under `(repo, rev, path)`. Also records the
    /// path in the listing if not already present.
    pub fn with_file_bytes(
        mut self,
        repo: &str,
        rev: &str,
        path: &str,
        bytes: &[u8],
    ) -> Self {
        let key = (repo.to_string(), rev.to_string(), PathBuf::from(path));
        self.bytes.insert(key, Arc::from(bytes));
        let listing = self
            .files
            .entry((repo.to_string(), rev.to_string()))
            .or_default();
        let p: Arc<Path> = Arc::from(PathBuf::from(path).as_path());
        if !listing.iter().any(|e| e.as_ref() == p.as_ref()) {
            listing.push(p);
        }
        self
    }
}

impl FileSource for MemFileSource {
    fn files(&self, repo: &str, rev: &str) -> Vec<Arc<Path>> {
        self.files
            .get(&(repo.to_string(), rev.to_string()))
            .cloned()
            .unwrap_or_default()
    }

    fn file_bytes(&self, repo: &str, rev: &str, path: &Path) -> Option<Arc<[u8]>> {
        self.bytes
            .get(&(repo.to_string(), rev.to_string(), path.to_path_buf()))
            .cloned()
    }
}
