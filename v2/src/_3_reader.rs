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

/// Discriminant for a parsed payload. Drives parser selection inside
/// `Reader::parsed` and the downcast target inside `ParsedTree.payload`.
/// Bounded set: Json / Toml / Yaml parse to `crate::data::*DataNode`
/// structures; TsAst / RsAst parse to `ast_grep_core::AstGrep<StrDoc<SupportLang>>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParserKind { Json, TsAst, RsAst, Toml, Yaml }

/// Output of `Reader::parsed`. Holds the raw blob bytes alongside the
/// parser-specific tree. `payload` is kept as `Arc<dyn Any>` so a single
/// `ParsedTree` type serves every `ParserKind`; callers downcast against
/// the variant implied by `kind`.
#[derive(Debug)]
pub struct ParsedTree {
    pub kind:    ParserKind,
    pub bytes:   Bytes,
    pub payload: Arc<dyn std::any::Any + Send + Sync>,
}

/// Granularity of an `unscanned` query. `Repo` = distinct repo slugs,
/// `Rev` = distinct (repo, rev), `File` = distinct (repo, rev, fs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanKind { Repo, Rev, File }

/// A tuple of (repo, rev, optional fs) returned by `Reader::unscanned` to
/// drive the demand walker toward work that has yet to be captured.
#[derive(Debug, Clone)]
pub struct ScanCombo {
    pub repo: Arc<str>,
    pub rev:  Arc<str>,
    pub fs:   Option<FilePath>,
}

/// Hit returned by `Reader::cross_ref` when an xref pattern resolves.
/// `target_row` is the id of the referenced row in its owning expr table;
/// `target_file` pins the file the referenced row lives in; `value` is
/// the captured string used to join.
#[derive(Debug, Clone)]
pub struct CrossRefHit {
    pub target_row:  u64,
    pub target_file: FileId,
    pub value:       Arc<str>,
}

/// One row yielded by `Reader::violations`, a check-backed view over
/// rows that failed user-defined assertions.
#[derive(Debug, Clone)]
pub struct ViolationEntry {
    pub check:   Arc<str>,
    pub row_id:  u64,
    pub payload: Arc<str>,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Read-side boundary: blob bytes, parsed trees, path listings, and the
/// query-log surfaces (cross_ref, unscanned, violations, run_visited).
///
/// Impls are composed as a stack in `OpCtx.reader` — today
/// `ParseCacheReader` wraps either `GitBlobReader`, `MemReader`, or
/// `BufferOverlay`. Implementations that have no answer for a method
/// (in-memory store, dirty buffer, not-wired-yet) return an empty
/// `BoxStream` or the documented "unknown" sentinel (`None` for
/// `blob_oid`).
///
/// Most methods yield exactly one value and wrap it in
/// `BoxStream<'static, T>` to keep one trait signature across streaming
/// (`files`, `repos`, `revs`) and one-shot (`bytes`, `parsed`, `config`)
/// cases. Callers read the first item and drop the stream.
///
/// ## Batching
///
/// Methods keyed on `(repo, rev, fs)` are called per-file today; see the
/// top-level batching-candidate table in the repo's ongoing refactor
/// notes. The trait keeps scalar shape intact so Phase 1 can land
/// batched forms additively without breaking existing callsites.
pub trait Reader: Send + Sync {
    /// Path listing for `(repo, rev)` filtered by `pattern`. One yield.
    /// GitBlobReader's three-phase flow (path_index hit / worktree
    /// materialize / libgit2 walk) lives here.
    fn files(&self, repo: &str, rev: &str, pattern: &CompiledPattern)
        -> BoxStream<'static, Vec<FilePath>>;

    /// Full blob bytes for `(repo, rev, fs)`. One yield.
    /// GitBlobReader's fast path reads from a provisioned worktree on
    /// disk; the slow path acquires `Mutex<git2::Repository>` and walks
    /// the tree. Empty `Bytes` on miss.
    fn bytes(&self, repo: &str, rev: &str, fs: &FilePath)
        -> BoxStream<'static, Bytes>;

    /// Byte subrange of `(repo, rev, fs)`. Same fast/slow path as
    /// `bytes`. No callers outside the impl today; kept for partial
    /// reads in future ops.
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

    /// Parsed tree for `(repo, rev, fs)` of parser `kind`. One yield.
    /// `ParseCacheReader` memoizes by oid first and by content hash as
    /// fallback; underlying readers typically `unimplemented!()` since
    /// callers always go through the cache layer.
    fn parsed(&self, repo: &str, rev: &str, fs: &FilePath, kind: ParserKind)
        -> BoxStream<'static, Arc<ParsedTree>>;

    /// Repo-slug catalog for this reader. One yield.
    fn repos(&self) -> BoxStream<'static, Vec<Arc<str>>>;

    /// Revs known for `repo`. One yield; empty list is valid — rules
    /// that name rev literals directly still work.
    fn revs (&self, repo: &str) -> BoxStream<'static, Vec<Arc<str>>>;

    /// Xref lookup: rows in `rule` that have a value matching `var`
    /// against `(repo, rev)`. Backed by query-log; today only wired
    /// when the scan-loop fills in cross-rule edges.
    fn cross_ref(&self, rule: &str, var: &str, repo: &str, rev: &str)
        -> BoxStream<'static, Vec<CrossRefHit>>;

    /// Scan-plan query: `(repo, rev, fs)` combos in `table.column` that
    /// have yet to be scanned at granularity `kind`. `norm` toggles
    /// value normalization. Drives the demand walker between passes.
    fn unscanned(&self, table: &str, column: &str, kind: ScanKind, norm: bool)
        -> BoxStream<'static, Vec<ScanCombo>>;

    /// Assertion failures, optionally filtered by check name. One yield.
    fn violations(&self, check: Option<&str>) -> BoxStream<'static, Vec<ViolationEntry>>;

    /// Has `(run_id, op_id, cursor_hash)` been seen in this run?
    /// Short-circuits fan-out re-walks of stable sub-trees.
    fn run_visited(&self, run_id: u64, op_id: u64, cursor_hash: u64)
        -> BoxStream<'static, bool>;

    /// Shared config handle. One yield. Emitted as a stream for
    /// signature uniformity; holders typically cache the first yield.
    fn config(&self) -> BoxStream<'static, Arc<Config>>;
}
