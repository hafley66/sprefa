//! ParseCacheReader — memoizes `Reader::parsed()` across ops inside a
//! single pipeline run. One cache per run: fresh on every `run_pipeline`
//! (and every LSP reparse), so there is no invalidation story yet. The
//! DashMap/LRU upgrade lands when the cache needs to outlive one run.
//!
//! Scope: the only kinds that touch the cache today are `RsAst` / `TsAst`,
//! produced by the `ast` op. Other kinds fall through to the inner reader
//! (which presently panics — no consumer exists yet).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::{stream, StreamExt};
use tokio::sync::OnceCell;

use crate::_0_types::FilePath;
use crate::_2_config::Config;
use crate::_3_reader::{
    CrossRefHit, ParsedTree, ParserKind, Reader, ScanCombo, ScanKind, ViolationEntry,
};
use crate::_16_pattern::CompiledPattern;

use ast_grep_core::{AstGrep, Language, source::StrDoc};
use ast_grep_language::SupportLang;

/// Parsed Rust/TypeScript tree payload. Downcast target for
/// `ParsedTree.payload` when the kind is `RsAst` / `TsAst`.
pub type RsAstPayload = AstGrep<StrDoc<SupportLang>>;

// ---------------------------------------------------------------------------
// Parse tic-toc (always on; snapshot + reset at run boundary)
// ---------------------------------------------------------------------------

use std::sync::atomic::{AtomicU64, Ordering};
pub static PARSE_COUNT: AtomicU64 = AtomicU64::new(0);
pub static PARSE_NS:    AtomicU64 = AtomicU64::new(0);
pub static PARSE_BYTES: AtomicU64 = AtomicU64::new(0);

pub fn parse_stats_snapshot() -> (u64, u64, u64) {
    (
        PARSE_COUNT.swap(0, Ordering::Relaxed),
        PARSE_NS.swap(0, Ordering::Relaxed),
        PARSE_BYTES.swap(0, Ordering::Relaxed),
    )
}

#[inline]
pub fn record_parse(bytes_len: usize, t0: std::time::Instant) {
    PARSE_COUNT.fetch_add(1, Ordering::Relaxed);
    PARSE_NS.fetch_add(t0.elapsed().as_nanos() as u64, Ordering::Relaxed);
    PARSE_BYTES.fetch_add(bytes_len as u64, Ordering::Relaxed);
}

type CacheKey     = (Arc<str>, Arc<str>, Arc<std::path::Path>, ParserKind);
type Slot         = Arc<OnceCell<Arc<ParsedTree>>>;
type HashKey      = ([u8; 32], ParserKind);

pub struct ParseCacheReader {
    pub inner: Arc<dyn Reader + Send + Sync>,
    /// Per-(repo, rev, path, kind) slot. First caller reads bytes +
    /// computes hash; later concurrent callers await the same future.
    /// Handles the dag-level concurrent ast-op case within one run.
    cache:     Arc<RwLock<HashMap<CacheKey, Slot>>>,
    /// Secondary cache keyed by content hash. Two different
    /// (repo, rev, path) triples with identical bytes (common across
    /// revs or after a merge) share one parsed tree. Coalesced via
    /// OnceCell so only one of the concurrent first-hitters actually
    /// parses.
    by_hash:   Arc<RwLock<HashMap<HashKey, Slot>>>,
}

impl ParseCacheReader {
    pub fn new(inner: Arc<dyn Reader + Send + Sync>) -> Self {
        Self {
            inner,
            cache:   Arc::new(RwLock::new(HashMap::new())),
            by_hash: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

fn once<T: Send + 'static>(v: T) -> BoxStream<'static, T> {
    Box::pin(stream::iter(std::iter::once(v)))
}

/// Parse raw bytes into a kind-specific payload. Runs synchronously —
/// always called inside `spawn_blocking` so the tokio worker thread is
/// free. Returns `()` payload for kinds outside RsAst/TsAst so the
/// caller can still read `tree.bytes`.
pub(crate) fn parse_payload_sync(kind: ParserKind, bytes: &Bytes)
    -> Arc<dyn std::any::Any + Send + Sync>
{
    let lang = match kind {
        ParserKind::RsAst => SupportLang::Rust,
        ParserKind::TsAst => SupportLang::TypeScript,
        _ => return Arc::new(()) as _,
    };
    let Ok(s) = std::str::from_utf8(bytes.as_ref()) else {
        return Arc::new(()) as _;
    };
    let t0 = std::time::Instant::now();
    let grep: RsAstPayload = lang.ast_grep(s);
    record_parse(bytes.len(), t0);
    Arc::new(grep)
}

impl Reader for ParseCacheReader {
    fn parsed(&self, repo: &str, rev: &str, fp: &FilePath, kind: ParserKind)
        -> BoxStream<'static, Arc<ParsedTree>>
    {
        let key: CacheKey = (
            Arc::from(repo),
            Arc::from(rev),
            fp.0.clone(),
            kind,
        );

        // Reserve or claim the OnceCell slot under a short write lock.
        let slot: Slot = {
            let mut m = self.cache.write().unwrap();
            m.entry(key.clone()).or_insert_with(|| Arc::new(OnceCell::new())).clone()
        };

        let inner   = self.inner.clone();
        let by_hash = self.by_hash.clone();
        let repo_s  = key.0.clone();
        let rev_s   = key.1.clone();
        let fp_cl   = fp.clone();

        let fut = async move {
            let tree = slot.get_or_init(|| async move {
                // Read bytes once per (repo, rev, path). If another path
                // in the run already parsed identical bytes (same blob
                // across revs), the by-hash slot returns the cached tree
                // without reparse.
                let mut s = inner.bytes(&repo_s, &rev_s, &fp_cl);
                let bytes = s.next().await.unwrap_or_default();

                // blake3 of bytes — ~GB/s, cheaper than even a tiny parse.
                let hash_bytes: [u8; 32] = blake3::hash(&bytes).into();
                let hash_key: HashKey = (hash_bytes, kind);

                let hash_slot: Slot = {
                    let mut m = by_hash.write().unwrap();
                    m.entry(hash_key).or_insert_with(|| Arc::new(OnceCell::new())).clone()
                };

                let bytes_for_init = bytes.clone();
                let tree = hash_slot.get_or_init(|| async move {
                    let bytes_for_parse = bytes_for_init.clone();
                    let payload = tokio::task::spawn_blocking(move || {
                        parse_payload_sync(kind, &bytes_for_parse)
                    }).await.unwrap_or_else(|_| Arc::new(()) as _);
                    Arc::new(ParsedTree { kind, bytes: bytes_for_init, payload })
                }).await;
                tree.clone()
            }).await;
            tree.clone()
        };
        Box::pin(stream::once(fut))
    }

    // ----------------------------- forwards -------------------------------

    fn files(&self, repo: &str, rev: &str, pattern: &CompiledPattern)
        -> BoxStream<'static, Vec<FilePath>>
    { self.inner.files(repo, rev, pattern) }

    fn bytes(&self, repo: &str, rev: &str, fp: &FilePath) -> BoxStream<'static, Bytes> {
        self.inner.bytes(repo, rev, fp)
    }

    fn bytes_range(&self, repo: &str, rev: &str, fp: &FilePath, range: Range<usize>)
        -> BoxStream<'static, Bytes>
    { self.inner.bytes_range(repo, rev, fp, range) }

    fn repos(&self) -> BoxStream<'static, Vec<Arc<str>>> { self.inner.repos() }
    fn revs (&self, repo: &str) -> BoxStream<'static, Vec<Arc<str>>> { self.inner.revs(repo) }

    fn cross_ref(&self, rule: &str, var: &str, repo: &str, rev: &str)
        -> BoxStream<'static, Vec<CrossRefHit>>
    { self.inner.cross_ref(rule, var, repo, rev) }

    fn unscanned(&self, table: &str, column: &str, kind: ScanKind, norm: bool)
        -> BoxStream<'static, Vec<ScanCombo>>
    { self.inner.unscanned(table, column, kind, norm) }

    fn violations(&self, check: Option<&str>) -> BoxStream<'static, Vec<ViolationEntry>> {
        self.inner.violations(check)
    }

    fn run_visited(&self, run_id: u64, op_id: u64, cursor_hash: u64)
        -> BoxStream<'static, bool>
    { self.inner.run_visited(run_id, op_id, cursor_hash) }

    fn config(&self) -> BoxStream<'static, Arc<Config>> { self.inner.config() }
}
