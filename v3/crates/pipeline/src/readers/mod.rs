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
}

pub mod mem;

pub use mem::MemFileSource;
