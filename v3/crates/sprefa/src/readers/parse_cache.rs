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
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use futures_core::stream::BoxStream;
use futures_util::{stream, StreamExt};
use tokio::sync::OnceCell;
use tracing::{info_span, Instrument};

use crate::types::FilePath;
use crate::config::Config;
use crate::reader::{
    CrossRefHit, ParsedTree, ParserKind, Reader, ScanCombo, ScanKind, ViolationEntry,
};
use crate::pattern::CompiledPattern;

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
type OidKey       = ([u8; 20], ParserKind);

pub struct ParseCacheReader {
    pub inner: Arc<dyn Reader + Send + Sync>,
    /// Per-(repo, rev, path, kind) slot. First caller reads bytes +
    /// computes hash; later concurrent callers await the same future.
    /// Handles the dag-level concurrent ast-op case within one run.
    cache:     Arc<RwLock<HashMap<CacheKey, Slot>>>,
    /// Content-addressed secondary cache by git blob oid. Populated when
    /// the underlying reader can name the blob without reading it
    /// (GitBlobReader). Fast path: duplicate oid across revs skips bytes
    /// read entirely. Collision-free by construction (oid is sha1 of
    /// blob content).
    by_oid:    Arc<RwLock<HashMap<OidKey, Slot>>>,
    /// Hash-addressed secondary cache for readers that cannot supply an
    /// oid (MemReader, buffered BufferOverlay). Two different
    /// (repo, rev, path) triples with identical bytes share one parsed
    /// tree. Costs a blake3 pass per first-read but orthogonal to the
    /// oid fast path.
    by_hash:   Arc<RwLock<HashMap<HashKey, Slot>>>,
    /// Optional on-disk cache of raw blob bytes keyed by (oid, kind).
    /// Only populated on the oid fast path — blake3 path does not
    /// persist because content hash lookups need the bytes in hand
    /// anyway. Layout: `$dir/<xx>/<rest>.<ext>` where `xx` = first two
    /// hex chars of oid for directory fanout, `rest` = remaining 38
    /// hex chars, `ext` = kind tag (`rs`, `ts`, etc). Disk miss falls
    /// through to `inner.bytes()`; disk hit skips the blob read and the
    /// git-repo mutex entirely.
    ///
    /// Opt-in via `SPREFA_PARSE_CACHE_DIR`. Default off because on small
    /// fixtures (g4/sprefa-self) a `tokio::fs::read` costs more than a
    /// git2 blob read from a warm packfile mmap (~400 us vs ~300 us) —
    /// net ~20 ms regression on 200-file runs. Designed for the
    /// workloads where it inverts:
    ///   1. Mutex-saturated git blob reads (500-repo scan; the single
    ///      `Mutex<Repository>` in GitBlobReader caps K=1 throughput
    ///      until the handle pool lands)
    ///   2. LSP reparses (same files touched 50x/day across process
    ///      restarts; in-mem `by_oid` dies on restart, disk survives)
    ///   3. Multi-process — concurrent CLI + LSP share one cache
    /// Leave enabled for those; disable for small-fixture benchmarking.
    disk_dir:  Option<Arc<Path>>,
    /// Shared interner — used to build cache keys without allocating a
    /// fresh `Arc<str>` per `parsed()` call.
    intern:    Arc<crate::str_intern::StrInterner>,
}

impl ParseCacheReader {
    pub fn new(
        inner:  Arc<dyn Reader + Send + Sync>,
        intern: Arc<crate::str_intern::StrInterner>,
    ) -> Self {
        Self {
            inner,
            cache:    Arc::new(RwLock::new(HashMap::new())),
            by_oid:   Arc::new(RwLock::new(HashMap::new())),
            by_hash:  Arc::new(RwLock::new(HashMap::new())),
            disk_dir: None,
            intern,
        }
    }

    pub fn with_disk(
        inner:  Arc<dyn Reader + Send + Sync>,
        intern: Arc<crate::str_intern::StrInterner>,
        dir:    PathBuf,
    ) -> Self {
        Self {
            inner,
            cache:    Arc::new(RwLock::new(HashMap::new())),
            by_oid:   Arc::new(RwLock::new(HashMap::new())),
            by_hash:  Arc::new(RwLock::new(HashMap::new())),
            disk_dir: Some(Arc::from(dir.as_path())),
            intern,
        }
    }
}

fn kind_ext(kind: ParserKind) -> &'static str {
    match kind {
        ParserKind::RsAst => "rs",
        ParserKind::TsAst => "ts",
        ParserKind::Json  => "json",
        ParserKind::Toml  => "toml",
        ParserKind::Yaml  => "yaml",
    }
}

fn disk_path(dir: &Path, oid: &[u8; 20], kind: ParserKind) -> PathBuf {
    let hex = hex_lower(oid);
    let (xx, rest) = hex.split_at(2);
    dir.join(xx).join(format!("{}.{}", rest, kind_ext(kind)))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn once<T: Send + 'static>(v: T) -> BoxStream<'static, T> {
    Box::pin(stream::iter(std::iter::once(v)))
}

/// Parse raw bytes into a kind-specific payload. Runs synchronously —
/// always called on the rayon pool via `rayon_one` so neither the tokio
/// async worker nor the tokio blocking pool is tied up by parse CPU.
/// Returns `()` payload for kinds outside RsAst/TsAst so the caller can
/// still read `tree.bytes`.
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
            self.intern.get(repo),
            self.intern.get(rev),
            fp.0.clone(),
            kind,
        );

        // Reserve or claim the OnceCell slot under a short write lock.
        let slot: Slot = {
            let mut m = self.cache.write().unwrap();
            m.entry(key.clone()).or_insert_with(|| Arc::new(OnceCell::new())).clone()
        };

        let inner    = self.inner.clone();
        let by_oid   = self.by_oid.clone();
        let by_hash  = self.by_hash.clone();
        let disk_dir = self.disk_dir.clone();
        let repo_s   = key.0.clone();
        let rev_s    = key.1.clone();
        let fp_cl    = fp.clone();

        let span = info_span!(
            "parse_cache.parsed",
            repo = %repo_s, rev = %rev_s, fp = %fp_cl.0.display(),
        );
        let fut = async move {
            let tree = slot.get_or_init(|| async move {
                // Try oid fast path first. Reader default returns None so
                // MemReader / dirty BufferOverlay skip this without a
                // spurious roundtrip; only GitBlobReader answers with a
                // real oid. Duplicate blob across revs is then a pure
                // hashmap lookup — no bytes read, no blake3.
                let mut oid_s = inner.blob_oid(&repo_s, &rev_s, &fp_cl);
                let oid = async {
                    oid_s.next().await.flatten()
                }.instrument(info_span!("parse_cache.oid_probe")).await;

                if let Some(oid_bytes) = oid {
                    let oid_key: OidKey = (oid_bytes, kind);
                    let oid_slot: Slot = {
                        let mut m = by_oid.write().unwrap();
                        m.entry(oid_key).or_insert_with(|| Arc::new(OnceCell::new())).clone()
                    };
                    let inner2    = inner.clone();
                    let repo2     = repo_s.clone();
                    let rev2      = rev_s.clone();
                    let fp2       = fp_cl.clone();
                    let disk_dir2 = disk_dir.clone();
                    let tree = oid_slot.get_or_init(|| async move {
                        // Disk cache first when configured. A hit skips
                        // the blob read (and the git-repo mutex), which
                        // is the dominant cost on cold-run LSP reparse.
                        let disk_path_opt = disk_dir2.as_ref()
                            .map(|d| disk_path(d, &oid_bytes, kind));
                        let mut from_disk = false;
                        let bytes: Bytes = if let Some(p) = disk_path_opt.as_ref() {
                            match tokio::fs::read(p)
                                .instrument(info_span!("parse_cache.disk.read"))
                                .await {
                                Ok(v) => { from_disk = true; Bytes::from(v) }
                                Err(_) => {
                                    let mut s = inner2.bytes(&repo2, &rev2, &fp2);
                                    async { s.next().await.unwrap_or_default() }
                                        .instrument(info_span!("parse_cache.bytes_read"))
                                        .await
                                }
                            }
                        } else {
                            let mut s = inner2.bytes(&repo2, &rev2, &fp2);
                            async { s.next().await.unwrap_or_default() }
                                .instrument(info_span!("parse_cache.bytes_read"))
                                .await
                        };

                        // On miss, fire-and-forget the disk write so the
                        // next run (or LSP reparse) hits the fast path.
                        // Swallow write failures — worst case is a future
                        // miss, never a wrong answer.
                        if !from_disk {
                            if let (Some(p), true) = (disk_path_opt, !bytes.is_empty()) {
                                let payload = bytes.clone();
                                tokio::spawn(async move {
                                    if let Some(parent) = p.parent() {
                                        let _ = tokio::fs::create_dir_all(parent).await;
                                    }
                                    let _ = tokio::fs::write(&p, &payload).await;
                                }.instrument(info_span!("parse_cache.disk.write_bg")));
                            }
                        }

                        let bytes_for_init = bytes.clone();
                        let payload = crate::parallel::rayon_one(move || {
                            let _s = info_span!("parse_cache.parse_payload_sync", bytes = bytes.len()).entered();
                            parse_payload_sync(kind, &bytes)
                        })
                        .instrument(info_span!("parse_cache.rayon_one.oid"))
                        .await
                        .unwrap_or_else(|| Arc::new(()) as _);
                        Arc::new(ParsedTree { kind, bytes: bytes_for_init, payload })
                    }.instrument(info_span!("parse_cache.by_oid.init"))).await;
                    return tree.clone();
                }

                // Fallback: no oid available. Read bytes, hash, dedup by
                // content hash. Covers MemReader and dirty buffers.
                let mut s = inner.bytes(&repo_s, &rev_s, &fp_cl);
                let bytes = async { s.next().await.unwrap_or_default() }
                    .instrument(info_span!("parse_cache.bytes_read.hash_path"))
                    .await;

                let hash_bytes: [u8; 32] = {
                    let _s = info_span!("parse_cache.blake3", bytes = bytes.len()).entered();
                    blake3::hash(&bytes).into()
                };
                let hash_key: HashKey = (hash_bytes, kind);

                let hash_slot: Slot = {
                    let mut m = by_hash.write().unwrap();
                    m.entry(hash_key).or_insert_with(|| Arc::new(OnceCell::new())).clone()
                };

                let bytes_for_init = bytes.clone();
                let tree = hash_slot.get_or_init(|| async move {
                    let bytes_for_parse = bytes_for_init.clone();
                    let payload = crate::parallel::rayon_one(move || {
                        let _s = info_span!("parse_cache.parse_payload_sync", bytes = bytes_for_parse.len()).entered();
                        parse_payload_sync(kind, &bytes_for_parse)
                    })
                    .instrument(info_span!("parse_cache.rayon_one.hash"))
                    .await
                    .unwrap_or_else(|| Arc::new(()) as _);
                    Arc::new(ParsedTree { kind, bytes: bytes_for_init, payload })
                }.instrument(info_span!("parse_cache.by_hash.init"))).await;
                tree.clone()
            }).await;
            tree.clone()
        };
        Box::pin(stream::once(fut.instrument(span)))
    }

    fn blob_oid(&self, repo: &str, rev: &str, fp: &FilePath)
        -> BoxStream<'static, Option<[u8; 20]>>
    { self.inner.blob_oid(repo, rev, fp) }

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
