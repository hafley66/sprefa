//! DiskFileSource — walks the filesystem under `root` and serves
//! repo-relative paths. The `cursor.rev` field is currently IGNORED:
//! `:wt`, `:HEAD`, `:staged`, `:unstaged` all short-circuit to the
//! current worktree state. This source matches a single configured
//! `rev` string (so `(repo, rev)` keys still partition cleanly in the
//! reader cache); cursors carrying any other rev get an empty listing /
//! a read miss.
//!
//! A follow-up card will introduce a git-blob-backed source for
//! immutable rev reads (`:HEAD`, branches, tags, SHAs) and a separate
//! index reader for `:staged`. Until then this is the only file source
//! the CLI/LSP wires up.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::FileSource;

pub struct DiskFileSource {
    pub root: PathBuf,
    pub rev:  String,
}

impl DiskFileSource {
    pub fn new(root: PathBuf, rev: String) -> Self {
        Self { root, rev }
    }
}

impl FileSource for DiskFileSource {
    fn files(&self, _repo: &str, rev: &str) -> Vec<Arc<Path>> {
        if rev != self.rev {
            return Vec::new();
        }
        let _t0 = std::time::Instant::now();
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        tracing::info!(
            target: "sprefa::reader",
            root = %self.root.display(),
            rev = %rev,
            files = out.len(),
            elapsed_ms = _t0.elapsed().as_millis() as u64,
            "DiskFileSource.files"
        );
        out
    }

    fn file_bytes(&self, _repo: &str, rev: &str, path: &Path) -> Option<Arc<[u8]>> {
        if rev != self.rev {
            return None;
        }
        let _t0 = std::time::Instant::now();
        let bytes = std::fs::read(self.root.join(path)).ok().map(Arc::<[u8]>::from);
        let len = bytes.as_ref().map(|b| b.len()).unwrap_or(0);
        tracing::trace!(
            target: "sprefa::reader",
            path = %path.display(),
            bytes = len,
            elapsed_us = _t0.elapsed().as_micros() as u64,
            "DiskFileSource.file_bytes"
        );
        bytes
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<Arc<Path>>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        // Skip hidden dirs (`.git`, `.vscode`, …) and common heavy ones.
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(root, &path, out);
        } else if ft.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(Arc::from(rel));
            }
        }
    }
}
