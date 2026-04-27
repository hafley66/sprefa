//! BufferOverlay — wraps an inner FileSource with an in-memory overlay
//! keyed by repo-relative path. LSP did_change populates it; pipes
//! scanning the same paths see the editor buffer instead of disk bytes.
//!
//! `content_hash` is recomputed on overlay write (blake3 of new bytes)
//! so the OpCache `batch_fingerprint` distinguishes overlay entries
//! from disk entries automatically — no separate cache keying.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use super::FileSource;

pub struct BufferOverlay<R: FileSource> {
    inner:    Arc<R>,
    overlays: RwLock<HashMap<PathBuf, (Arc<[u8]>, Arc<[u8; 32]>)>>,
}

impl<R: FileSource> BufferOverlay<R> {
    pub fn new(inner: Arc<R>) -> Self {
        Self { inner, overlays: RwLock::new(HashMap::new()) }
    }

    /// Install or replace the overlay at `path`. Hash is blake3 of
    /// `bytes`, computed eagerly so reads stay lock-light.
    pub fn set(&self, path: PathBuf, bytes: Arc<[u8]>) {
        let hash = Arc::new(*blake3::hash(&bytes).as_bytes());
        self.overlays.write().unwrap().insert(path, (bytes, hash));
    }

    /// Drop the overlay at `path`. Called on LSP didSave (LSP client
    /// signals the disk has caught up).
    pub fn clear(&self, path: &Path) {
        self.overlays.write().unwrap().remove(path);
    }

    pub fn has(&self, path: &Path) -> bool {
        self.overlays.read().unwrap().contains_key(path)
    }

    pub fn inner(&self) -> &Arc<R> { &self.inner }
}

impl<R: FileSource> FileSource for BufferOverlay<R> {
    fn files(&self, repo: &str, rev: &str) -> Vec<Arc<Path>> {
        // Listing comes from inner; overlays do not invent new paths.
        // An editor cannot create-and-edit a file that disk has never
        // seen — `did_open` would have triggered a save round-trip.
        self.inner.files(repo, rev)
    }

    fn file_bytes(&self, repo: &str, rev: &str, path: &Path) -> Option<Arc<[u8]>> {
        if let Some((b, _)) = self.overlays.read().unwrap().get(path) {
            return Some(b.clone());
        }
        self.inner.file_bytes(repo, rev, path)
    }

    fn file_bytes_with_hash(
        &self,
        repo: &str,
        rev:  &str,
        path: &Path,
    ) -> Option<(Arc<[u8]>, Option<Arc<[u8; 32]>>)> {
        if let Some((b, h)) = self.overlays.read().unwrap().get(path) {
            return Some((b.clone(), Some(h.clone())));
        }
        self.inner.file_bytes_with_hash(repo, rev, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readers::MemFileSource;

    fn mem_with(files: &[(&str, &[u8])]) -> Arc<MemFileSource> {
        let mut m = MemFileSource::new();
        for (p, b) in files {
            m = m.with_file_bytes("repo", "wt", p, b);
        }
        Arc::new(m)
    }

    #[test]
    fn delegates_when_no_overlay() {
        let inner = mem_with(&[("a.rs", b"disk-a")]);
        let ov = BufferOverlay::new(inner);
        let got = ov.file_bytes("repo", "wt", Path::new("a.rs")).unwrap();
        assert_eq!(&*got, b"disk-a");
    }

    #[test]
    fn overlay_shadows_inner() {
        let inner = mem_with(&[("a.rs", b"disk-a")]);
        let ov = BufferOverlay::new(inner);
        ov.set(PathBuf::from("a.rs"), Arc::from(b"buf-a".to_vec()));
        let got = ov.file_bytes("repo", "wt", Path::new("a.rs")).unwrap();
        assert_eq!(&*got, b"buf-a");
    }

    #[test]
    fn clear_restores_disk_view() {
        let inner = mem_with(&[("a.rs", b"disk-a")]);
        let ov = BufferOverlay::new(inner);
        ov.set(PathBuf::from("a.rs"), Arc::from(b"buf-a".to_vec()));
        ov.clear(Path::new("a.rs"));
        let got = ov.file_bytes("repo", "wt", Path::new("a.rs")).unwrap();
        assert_eq!(&*got, b"disk-a");
    }

    #[test]
    fn overlay_hash_is_blake3_of_bytes() {
        let inner = mem_with(&[]);
        let ov = BufferOverlay::new(inner);
        ov.set(PathBuf::from("a.rs"), Arc::from(b"hello".to_vec()));
        let (bytes, hash) =
            ov.file_bytes_with_hash("repo", "wt", Path::new("a.rs")).unwrap();
        assert_eq!(&*bytes, b"hello");
        let expect = *blake3::hash(b"hello").as_bytes();
        assert_eq!(*hash.unwrap(), expect);
    }

    #[test]
    fn files_listing_unchanged_by_overlay() {
        let inner = mem_with(&[("a.rs", b"disk-a")]);
        let ov = BufferOverlay::new(inner);
        ov.set(PathBuf::from("phantom.rs"), Arc::from(b"never on disk".to_vec()));
        let listing = ov.files("repo", "wt");
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].as_ref(), Path::new("a.rs"));
    }
}
