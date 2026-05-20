//! VfsOverlay: buffer-text overlay consulted by SourceReader before
//! falling back to disk. Phase 4 of lsp-diags-to-claude-code-full.md.
//!
//! Canonicalization invariant: every PathBuf written into `buffers`
//! MUST go through `canon_path()`, and every read MUST too. Mismatched
//! normalization is the silent overlay-miss bug.
//!
//! On macOS, `tempfile::tempdir()` returns `/var/folders/...` but
//! `dunce::canonicalize` (and `std::fs::canonicalize`) resolves the
//! `/var` -> `/private/var` symlink. Routing both sides through this
//! function keeps overlay write and overlay read on the same key.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;

#[derive(Default)]
pub struct VfsOverlay {
    buffers: DashMap<PathBuf, Arc<str>>,
}

impl VfsOverlay {
    pub fn new() -> Self {
        Self {
            buffers: DashMap::new(),
        }
    }

    pub fn put(&self, canonical: PathBuf, text: Arc<str>) {
        self.buffers.insert(canonical, text);
    }

    pub fn remove(&self, canonical: &Path) -> Option<Arc<str>> {
        self.buffers.remove(canonical).map(|(_, v)| v)
    }

    pub fn get(&self, canonical: &Path) -> Option<Arc<str>> {
        self.buffers.get(canonical).map(|e| e.value().clone())
    }

    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

/// Canonical path for VfsOverlay keys. Single source of truth for both
/// `vfs.put` (LSP side) and `SourceReader` (expand side). Returns None
/// when the path cannot be canonicalized (does not exist, permissions),
/// in which case the overlay must NOT be consulted.
pub fn canon_path(p: &Path) -> Option<PathBuf> {
    // dunce on Windows to dodge \\?\ verbatim prefixes; on Unix this is
    // identical to std::fs::canonicalize.
    dunce::canonicalize(p).ok()
}
