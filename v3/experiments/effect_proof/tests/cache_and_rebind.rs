//! Exercises the new framework surface:
//!   - `PureEffect` + `CacheLayer<E>` (hit counts, domain invalidation)
//!   - `ctx.rebind::<E>()` atomic handler swap
//!   - `ctx.cancel_all()` propagates through `root_cancel`

use effect_runtime::batchers::{CacheLayer, Passthrough};
use effect_runtime::{
    Batcher, BoxFuture, CancellationToken, EffectKind, PureEffect, RtCtxBuilder,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Clone)]
struct LookupName {
    id: u64,
}
impl EffectKind for LookupName {
    type Response = String;
}
impl PureEffect for LookupName {
    type Key = u64;
    const DOMAIN: &'static str = "names";
    fn cache_key(&self) -> u64 { self.id }
}

struct CountingSource {
    hits: Arc<AtomicUsize>,
}
impl Batcher<LookupName> for CountingSource {
    fn run(&self, req: LookupName, _cancel: CancellationToken) -> BoxFuture<'static, String> {
        let hits = self.hits.clone();
        Box::pin(async move {
            hits.fetch_add(1, Ordering::SeqCst);
            format!("name-{}", req.id)
        })
    }
}

#[tokio::test]
async fn cache_layer_deduplicates_and_invalidates_on_domain() {
    let source_hits = Arc::new(AtomicUsize::new(0));
    let source = CountingSource { hits: source_hits.clone() };
    let cached = CacheLayer::<LookupName>::new(64, source);

    let ctx = RtCtxBuilder::new()
        .register_domain_aware::<LookupName, _>(cached)
        .build();

    assert_eq!(ctx.put(LookupName { id: 1 }).await, "name-1");
    assert_eq!(ctx.put(LookupName { id: 1 }).await, "name-1");
    assert_eq!(ctx.put(LookupName { id: 2 }).await, "name-2");
    assert_eq!(source_hits.load(Ordering::SeqCst), 2, "two unique keys = two misses");

    ctx.invalidate_domain("names");
    assert_eq!(ctx.put(LookupName { id: 1 }).await, "name-1");
    assert_eq!(source_hits.load(Ordering::SeqCst), 3);

    ctx.invalidate_domain("unrelated_domain");
    assert_eq!(ctx.put(LookupName { id: 1 }).await, "name-1");
    assert_eq!(source_hits.load(Ordering::SeqCst), 3, "unrelated invalidation is a no-op");
}

#[tokio::test]
async fn rebind_atomically_swaps_handler() {
    let ctx = RtCtxBuilder::new()
        .register::<LookupName, _>(Passthrough::<LookupName, _>::new(|r: LookupName| {
            format!("first-{}", r.id)
        }))
        .build();

    assert_eq!(ctx.put(LookupName { id: 7 }).await, "first-7");

    ctx.rebind::<LookupName, _>(Passthrough::<LookupName, _>::new(|r: LookupName| {
        format!("second-{}", r.id)
    }));

    assert_eq!(ctx.put(LookupName { id: 7 }).await, "second-7");
}

#[tokio::test]
async fn register_pure_sugar_equivalent_to_manual_cachelayer_wrap() {
    let source_hits = Arc::new(AtomicUsize::new(0));
    let source = CountingSource { hits: source_hits.clone() };

    let ctx = RtCtxBuilder::new()
        .register_pure::<LookupName, _>(64, source)
        .build();

    assert_eq!(ctx.put(LookupName { id: 9 }).await, "name-9");
    assert_eq!(ctx.put(LookupName { id: 9 }).await, "name-9");
    assert_eq!(source_hits.load(Ordering::SeqCst), 1, "second put is a hit");

    ctx.invalidate_domain("names");
    assert_eq!(ctx.put(LookupName { id: 9 }).await, "name-9");
    assert_eq!(source_hits.load(Ordering::SeqCst), 2, "invalidation forced miss");
}

#[tokio::test]
async fn cancel_all_marks_root_cancellation() {
    let ctx = RtCtxBuilder::new().build();
    let token = ctx.root_cancel();
    assert!(!token.is_cancelled());
    ctx.cancel_all();
    assert!(token.is_cancelled());
}
