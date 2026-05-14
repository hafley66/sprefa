//! CacheLayer: wraps an inner `Batcher<E>` with a moka keyed store.
//!
//! Only available for `E: PureEffect`. Keys come from `E::cache_key()`,
//! invalidation is bucketed by `E::DOMAIN`. A write op that mutates the
//! same domain calls `ctx.invalidate_domain(domain)`, which walks every
//! registered domain-aware entry and drops its cache.
//!
//! The inner batcher is the miss path — on a cache miss the layer
//! forwards the request, stores the response, and returns it. On hit,
//! the inner batcher is never consulted.

use crate::{Batcher, BoxFuture, CancellationToken, DomainInvalidate, PureEffect};
use moka::future::Cache;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Snapshot of a `CacheLayer`'s counters. `entries` is a moka-reported
/// approximation (lock-free read). Hits + misses are monotonically
/// increasing atomics — take differences across two snapshots to get a
/// per-window view.
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: u64,
}

impl CacheStats {
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

pub struct CacheLayer<E: PureEffect> {
    inner: Arc<dyn Batcher<E>>,
    cache: Cache<E::Key, Arc<E::Response>>,
    hits: Arc<AtomicU64>,
    misses: Arc<AtomicU64>,
}

impl<E: PureEffect> Clone for CacheLayer<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            cache: self.cache.clone(),
            hits: self.hits.clone(),
            misses: self.misses.clone(),
        }
    }
}

impl<E: PureEffect> CacheLayer<E>
where
    E::Response: Send + Sync + 'static,
{
    /// `capacity` is the moka max entry count. Inner batcher handles
    /// misses; its lifetime is owned by this layer.
    pub fn new<B: Batcher<E>>(capacity: u64, inner: B) -> Self {
        Self {
            inner: Arc::new(inner),
            cache: Cache::builder().max_capacity(capacity).build(),
            hits: Arc::new(AtomicU64::new(0)),
            misses: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries: self.cache.entry_count(),
        }
    }
}

impl<E: PureEffect> Batcher<E> for CacheLayer<E>
where
    E::Response: Send + Sync + 'static + Clone,
{
    fn run(&self, req: E, cancel: CancellationToken) -> BoxFuture<'static, E::Response> {
        let key = req.cache_key();
        let cache = self.cache.clone();
        let inner = self.inner.clone();
        let hits = self.hits.clone();
        let misses = self.misses.clone();
        Box::pin(async move {
            if let Some(hit) = cache.get(&key).await {
                hits.fetch_add(1, Ordering::Relaxed);
                return (*hit).clone();
            }
            misses.fetch_add(1, Ordering::Relaxed);
            let fresh = inner.run(req, cancel).await;
            let stored = Arc::new(fresh.clone());
            cache.insert(key, stored).await;
            fresh
        })
    }
}

impl<E: PureEffect> DomainInvalidate for CacheLayer<E>
where
    E::Response: Send + Sync + 'static + Clone,
{
    fn invalidate_domain(&self, domain: &str) {
        if domain == E::DOMAIN {
            self.cache.invalidate_all();
        }
    }
}
