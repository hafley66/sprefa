//! The content-keyed extraction cache. Weight-bounded and concurrent (sharded
//! `&self` methods), so the rayon workers all hit it. Weight is an output byte
//! estimate, never 1-per-entry.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use quick_cache::sync::Cache;
use quick_cache::Weighter;

use crate::source::FamilyMask;
use crate::{CallF, CstF, DfF, Edge, ExtractOutput, Node, TypeF};

/// Default weight capacity in MiB, when `SPREFA_EXTRACT_BLOB_CACHE_MB` is
/// unset, unparseable, or zero.
const DEFAULT_CAPACITY_MB: u64 = 512;
/// Per-file byte estimate used only to size the items capacity. The 400 MB
/// measured to hold all 2343 files resident works out to ~170 KB per file.
const REPRESENTATIVE_ENTRY_BYTES: usize = 170 * 1024;

/// Number of extractions actually performed (cache misses). Read by the blob
/// cache tests to assert that a hit skips the parse.
pub static EXTRACTIONS: AtomicUsize = AtomicUsize::new(0);

/// The cache key: blob identity + matched `Source` name + the folded mask.
/// Distinct bytes, languages, or mask selections are distinct entries.
#[derive(Clone, Hash, Eq, PartialEq)]
pub struct CacheKey {
    blob: crate::ContentId,
    lang: &'static str,
    mask_bits: u8,
}

impl CacheKey {
    pub fn new(blob: crate::ContentId, lang: &'static str, mask: FamilyMask) -> Self {
        let mask_bits = (mask.cst as u8)
            | ((mask.types as u8) << 1)
            | ((mask.call as u8) << 2)
            | ((mask.df as u8) << 3)
            | ((mask.data as u8) << 4);
        Self {
            blob,
            lang,
            mask_bits,
        }
    }
}

/// Byte estimate of one output: the interner's heap plus, per present family,
/// `nodes.len() * size_of::<Node<F>>()` and the same for edges, not exact.
pub fn estimate_bytes(out: &ExtractOutput) -> usize {
    let mut bytes = out.strings.heap_bytes();
    if let Some(bundle) = &out.cst {
        bytes += bundle.nodes.len() * size_of::<Node<CstF>>();
        bytes += bundle.edges.len() * size_of::<Edge<CstF>>();
    }
    if let Some(bundle) = &out.types {
        bytes += bundle.nodes.len() * size_of::<Node<TypeF>>();
        bytes += bundle.edges.len() * size_of::<Edge<TypeF>>();
    }
    if let Some(bundle) = &out.call {
        bytes += bundle.nodes.len() * size_of::<Node<CallF>>();
        bytes += bundle.edges.len() * size_of::<Edge<CallF>>();
    }
    if let Some(bundle) = &out.df {
        bytes += bundle.nodes.len() * size_of::<Node<DfF>>();
        bytes += bundle.edges.len() * size_of::<Edge<DfF>>();
    }
    bytes
}

/// The weight rule: an output's byte estimate.
#[derive(Clone)]
pub struct BlobWeigher;

impl Weighter<CacheKey, Arc<ExtractOutput>> for BlobWeigher {
    fn weight(&self, _key: &CacheKey, value: &Arc<ExtractOutput>) -> u64 {
        estimate_bytes(value) as u64
    }
}

/// The cache type the crate holds. `Arc` value makes `get_or_insert_with`'s
/// clone cheap.
pub type BlobCache = Cache<CacheKey, Arc<ExtractOutput>, BlobWeigher>;

/// The env override, in MiB, defaulting to `DEFAULT_CAPACITY_MB`.
fn capacity_mb_from_env() -> u64 {
    std::env::var("SPREFA_EXTRACT_BLOB_CACHE_MB")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .filter(|&value| value > 0)
        .unwrap_or(DEFAULT_CAPACITY_MB)
}

/// A cache bounded to `weight_capacity_bytes`. The items capacity is derived
/// from the cap and the representative entry size; it is a preallocation hint,
/// not a bound.
pub fn cache_with_capacity(weight_capacity_bytes: u64) -> BlobCache {
    let items_capacity =
        (weight_capacity_bytes / REPRESENTATIVE_ENTRY_BYTES as u64).max(1) as usize;
    BlobCache::with_weighter(items_capacity, weight_capacity_bytes, BlobWeigher)
}

fn cache() -> &'static BlobCache {
    static CACHE: OnceLock<BlobCache> = OnceLock::new();
    CACHE.get_or_init(|| {
        let mb = capacity_mb_from_env();
        cache_with_capacity(mb.saturating_mul(1024 * 1024))
    })
}

/// Look up `key`, extracting on a miss and counting the extraction. Concurrent
/// misses on one key coalesce into one compute inside `quick_cache`.
pub fn get_or_extract(
    key: CacheKey,
    compute: impl FnOnce() -> Arc<ExtractOutput>,
) -> Arc<ExtractOutput> {
    // The miss flag rides the closure, never a delta on the global counter: the
    // rayon workers share that counter and a concurrent miss would read as this
    // call's own.
    let mut missed = false;
    let span = tracing::debug_span!(
        "cache",
        lang = key.lang,
        hit = tracing::field::Empty,
        key = ?key.blob
    );
    let entered = span.enter();
    let out = cache()
        .get_or_insert_with(&key, || {
            missed = true;
            EXTRACTIONS.fetch_add(1, Ordering::Relaxed);
            Ok::<_, std::convert::Infallible>(compute())
        })
        .expect("extraction is infallible");
    span.record("hit", !missed);
    drop(entered);
    out
}
