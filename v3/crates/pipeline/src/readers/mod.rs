//! Readers — abstractions for pulling repo/rev/file metadata out of an
//! external substrate (git blob tree, mem fixture, working-copy overlay).
//!
//! The only piece needed for Stage A is [`FileSource`]: list all files
//! registered under `(repo, rev)`. FsOp applies its glob regex to the
//! full listing client-side; the reader does no path-matching so it can
//! cache the full listing per `(repo, rev)` and re-use it across
//! multiple `fs` call-sites in the same run.
//!
//! Bytes + structured reads are follow-ups; see parse.md §14.5c.

use std::path::Path;
use std::sync::Arc;

/// Lists file paths inside a `(repo, rev)` coordinate. Paths are
/// repo-relative. `file_bytes` serves the bytes for a single path. The
/// default returns `None` so sources that only enumerate listings
/// (MemFileSource for path-only tests) stay compilable; the `read` op
/// treats `None` as a read-miss.
pub trait FileSource: Send + Sync + 'static {
    fn files(&self, repo: &str, rev: &str) -> Vec<Arc<Path>>;

    fn file_bytes(&self, _repo: &str, _rev: &str, _path: &Path) -> Option<Arc<[u8]>> {
        None
    }

    /// Bytes + a precomputed content hash, when the source already has
    /// one (e.g., a git-backed source returns the blob OID). The default
    /// impl calls `file_bytes` and returns `None` for the hash, so
    /// `ensure_content_loaded` falls back to blake3-of-bytes. Sources
    /// with a free hash (git blob ID) override this to skip the
    /// blake3 pass on large files.
    ///
    /// Hash space is 32 bytes; sources whose native hash is shorter
    /// (e.g. git SHA-1 = 20 bytes) should pad or rehash into 32. Treat
    /// the hash as opaque; equality is the only operation downstream
    /// performs on it (Phase C cache-key composition).
    fn file_bytes_with_hash(
        &self,
        repo: &str,
        rev:  &str,
        path: &Path,
    ) -> Option<(Arc<[u8]>, Option<Arc<[u8; 32]>>)> {
        self.file_bytes(repo, rev, path).map(|b| (b, None))
    }
}

pub mod buffer_overlay;
pub mod disk;
pub mod git;
pub mod mem;

pub use buffer_overlay::BufferOverlay;
pub use disk::DiskFileSource;
pub use git::{GitFileSource, resolve_rev};
pub use mem::MemFileSource;
