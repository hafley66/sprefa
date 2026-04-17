//! Reader trait. All reads, stream-shaped.

use std::ops::Range;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::stream;

use crate::_0_types::{FileId, FilePath};
use crate::_16_pattern::CompiledPattern;
use crate::_2_config::Config;

// ---------------------------------------------------------------------------
// Return shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParserKind { Json, TsAst, RsAst, Toml, Yaml }

#[derive(Debug)]
pub struct ParsedTree {
    pub kind:    ParserKind,
    pub bytes:   Bytes,
    pub payload: Arc<dyn std::any::Any + Send + Sync>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanKind { Repo, Rev, File }

#[derive(Debug, Clone)]
pub struct ScanCombo {
    pub repo: Arc<str>,
    pub rev:  Arc<str>,
    pub fs:   Option<FilePath>,
}

#[derive(Debug, Clone)]
pub struct CrossRefHit {
    pub target_row:  u64,
    pub target_file: FileId,
    pub value:       Arc<str>,
}

#[derive(Debug, Clone)]
pub struct ViolationEntry {
    pub check:   Arc<str>,
    pub row_id:  u64,
    pub payload: Arc<str>,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait Reader: Send + Sync {
    fn files(&self, repo: &str, rev: &str, pattern: &CompiledPattern)
        -> BoxStream<'static, Vec<FilePath>>;

    fn bytes(&self, repo: &str, rev: &str, fs: &FilePath)
        -> BoxStream<'static, Bytes>;

    fn bytes_range(&self, repo: &str, rev: &str, fs: &FilePath, range: Range<usize>)
        -> BoxStream<'static, Bytes>;

    /// Content-addressed identity for `(repo, rev, fs)`. `Some(oid)` means
    /// the reader can name the blob without reading it; the parse cache
    /// uses this to skip bytes+hash for duplicates across revs. `None` is
    /// the default and makes callers fall back to bytes+hash. Readers that
    /// cannot answer (dirty buffer, in-memory, worktree overlay) return
    /// `None` so the fallback path stays correct.
    fn blob_oid(&self, _repo: &str, _rev: &str, _fs: &FilePath)
        -> BoxStream<'static, Option<[u8; 20]>>
    { Box::pin(stream::once(async { None })) }

    fn parsed(&self, repo: &str, rev: &str, fs: &FilePath, kind: ParserKind)
        -> BoxStream<'static, Arc<ParsedTree>>;

    fn repos(&self) -> BoxStream<'static, Vec<Arc<str>>>;
    fn revs (&self, repo: &str) -> BoxStream<'static, Vec<Arc<str>>>;

    fn cross_ref(&self, rule: &str, var: &str, repo: &str, rev: &str)
        -> BoxStream<'static, Vec<CrossRefHit>>;

    fn unscanned(&self, table: &str, column: &str, kind: ScanKind, norm: bool)
        -> BoxStream<'static, Vec<ScanCombo>>;

    fn violations(&self, check: Option<&str>) -> BoxStream<'static, Vec<ViolationEntry>>;

    fn run_visited(&self, run_id: u64, op_id: u64, cursor_hash: u64)
        -> BoxStream<'static, bool>;

    fn config(&self) -> BoxStream<'static, Arc<Config>>;
}
