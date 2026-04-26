//! sprefa-upb Phase E end-to-end:
//! Pipeline::Op consults `OpCache` on `RtCtx::store` and short-circuits
//! the second invocation of a cacheable op for the same input batch.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use effect_runtime::{BoxFuture, RtCtx, RtCtxBuilder};
use futures::stream::{self, StreamExt};
use pipeline::cache_key::OpCache;
use pipeline::{Cursor, Op, Pipeline};

#[derive(Debug)]
struct CountingCacheableOp {
    calls: Arc<AtomicUsize>,
}

impl Op for CountingCacheableOp {
    fn name(&self) -> &'static str { "counting_cacheable" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, batch: Arc<[Cursor]>) -> BoxFuture<'a, Arc<[Cursor]>> {
        let calls = self.calls.clone();
        Box::pin(async move {
            calls.fetch_add(1, Ordering::SeqCst);
            batch
        })
    }

    fn cache_key(&self, h: &mut blake3::Hasher) -> bool {
        h.update(self.name().as_bytes());
        true
    }
}

fn fixed_cursor() -> Cursor {
    let mut c = Cursor::new(Arc::from(b"abc".as_slice()));
    c.content_hash = Some(Arc::new([42u8; 32]));
    c
}

#[test]
fn pipeline_op_short_circuits_on_cache_hit() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let op_cache = Arc::new(OpCache::new(true));
    let ctx = RtCtxBuilder::new()
        .with_store(op_cache.clone())
        .build();
    let pipe = Pipeline::Op(Arc::new(CountingCacheableOp { calls: calls.clone() }));
    let batch: Arc<[Cursor]> = Arc::from(vec![fixed_cursor()]);

    let drain = |p: &Pipeline| {
        rt.block_on(async {
            let s = stream::iter(vec![batch.clone()]).boxed();
            let out: Vec<Arc<[Cursor]>> = p.run(&ctx, s).collect().await;
            out
        })
    };

    let first = drain(&pipe);
    let second = drain(&pipe);

    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "second run must hit cache");
    assert_eq!(op_cache.len(), 1);
}

#[test]
fn pipeline_op_disabled_cache_still_runs_op_each_time() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let op_cache = Arc::new(OpCache::new(false));
    let ctx = RtCtxBuilder::new()
        .with_store(op_cache.clone())
        .build();
    let pipe = Pipeline::Op(Arc::new(CountingCacheableOp { calls: calls.clone() }));
    let batch: Arc<[Cursor]> = Arc::from(vec![fixed_cursor()]);

    let drain = |p: &Pipeline| {
        rt.block_on(async {
            let s = stream::iter(vec![batch.clone()]).boxed();
            let _: Vec<Arc<[Cursor]>> = p.run(&ctx, s).collect().await;
        })
    };

    drain(&pipe);
    drain(&pipe);

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(op_cache.len(), 0);
}
