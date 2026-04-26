//! sprefa-upb Phase D end-to-end:
//! Pipeline::Op consults `OpCache` with an L2 `DiskCache` attached. After
//! a first run inserts to L2, dropping the OpCache + RtCtx and reopening
//! a fresh OpCache.with_l2 against the same sqlite file produces a cache
//! hit and skips `op.pipe`.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use effect_runtime::{BoxFuture, RtCtx, RtCtxBuilder};
use futures::stream::{self, StreamExt};
use pipeline::cache_key::OpCache;
use pipeline::disk_cache::DiskCache;
use pipeline::{Cursor, Op, Pipeline};

#[derive(Debug)]
struct CountingCacheableOp {
    calls: Arc<AtomicUsize>,
}

impl Op for CountingCacheableOp {
    fn name(&self) -> &'static str { "counting_cacheable_l2" }

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
    let mut c = Cursor::new(Arc::from(b"l2-payload".as_slice()));
    c.content_hash = Some(Arc::new([0xAB; 32]));
    c.repo = Arc::from("org/repo");
    c.rev = Arc::from("HEAD");
    c
}

fn tempdb(label: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    p.push(format!("sprefa_disk_cache_e2e_{label}_{pid}_{nonce}.db"));
    let _ = std::fs::remove_file(&p);
    p
}

fn build_ctx(cache: Arc<OpCache>) -> RtCtx {
    RtCtxBuilder::new().with_store(cache).build()
}

fn drain(p: &Pipeline, ctx: &RtCtx, batch: Arc<[Cursor]>) -> usize {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let s = stream::iter(vec![batch]).boxed();
        let out: Vec<Arc<[Cursor]>> = p.run(ctx, s).collect().await;
        out.len()
    })
}

#[test]
fn l2_disk_cache_survives_drop_and_reopen() {
    let db_path = tempdb("survives");
    let calls = Arc::new(AtomicUsize::new(0));
    let batch: Arc<[Cursor]> = Arc::from(vec![fixed_cursor()]);

    // First run: cold L1 + cold L2 → op.pipe runs once, L2 row written.
    {
        let l2 = Arc::new(DiskCache::open(&db_path).unwrap());
        let cache = Arc::new(OpCache::new(true).with_l2(l2.clone()));
        let ctx = build_ctx(cache.clone());
        let pipe = Pipeline::Op(Arc::new(CountingCacheableOp { calls: calls.clone() }));
        assert_eq!(drain(&pipe, &ctx, batch.clone()), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(cache.len(), 1, "L1 populated");
        assert_eq!(l2.len(), 1, "L2 populated");
        // ctx and cache drop here, simulating process exit.
    }

    // Reopen fresh OpCache + DiskCache against the same file. L1 starts
    // empty; first call should hit L2 and short-circuit op.pipe.
    {
        let l2 = Arc::new(DiskCache::open(&db_path).unwrap());
        assert_eq!(l2.len(), 1, "L2 row persisted across reopen");
        let cache = Arc::new(OpCache::new(true).with_l2(l2));
        let ctx = build_ctx(cache.clone());
        let pipe = Pipeline::Op(Arc::new(CountingCacheableOp { calls: calls.clone() }));
        assert_eq!(drain(&pipe, &ctx, batch.clone()), 1);
        assert_eq!(
            calls.load(Ordering::SeqCst), 1,
            "second-process run must hit L2 (no op.pipe call)",
        );
        assert_eq!(cache.len(), 1, "L1 promoted from L2 hit");
    }

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn l2_absent_falls_back_to_recompute() {
    // Sanity: without L2, dropping the cache loses all state and op.pipe
    // runs again on the second invocation. Pairs with the survives test
    // to prove L2 is the load-bearing piece.
    let calls = Arc::new(AtomicUsize::new(0));
    let batch: Arc<[Cursor]> = Arc::from(vec![fixed_cursor()]);

    {
        let cache = Arc::new(OpCache::new(true));
        let ctx = build_ctx(cache.clone());
        let pipe = Pipeline::Op(Arc::new(CountingCacheableOp { calls: calls.clone() }));
        drain(&pipe, &ctx, batch.clone());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    {
        let cache = Arc::new(OpCache::new(true));
        let ctx = build_ctx(cache.clone());
        let pipe = Pipeline::Op(Arc::new(CountingCacheableOp { calls: calls.clone() }));
        drain(&pipe, &ctx, batch.clone());
        assert_eq!(calls.load(Ordering::SeqCst), 2, "no L2, must recompute");
    }
}
