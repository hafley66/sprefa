//! sprefa-upb Phase F end-to-end:
//! Pipeline::Op caches a batch keyed on its (repo, rev, fs)+content_hash
//! input. After invalidation via apply_change, a re-run misses the cache
//! and op.pipe runs again. Without invalidation, a re-run hits and skips.
//!
//! This is the golden "the loop closes" test for Phase F. The previous
//! pipeline-level e2e tests (cache_key_e2e, disk_cache_e2e) prove the
//! cache reads/writes; this one proves the *eviction path* threads
//! through the same Pipeline::Op runner.

use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use effect_runtime::{BoxFuture, RtCtx, RtCtxBuilder};
use futures::stream::{self, StreamExt};
use pipeline::cache_key::OpCache;
use pipeline::invalidate::{apply_change, Change};
use pipeline::{Cursor, Op, Pipeline};

/// Cacheable identity op that increments a shared counter on each
/// invocation of `pipe`. The counter is the test's ground truth for
/// "did we hit the cache or recompute?"
#[derive(Debug)]
struct CountingOp {
    calls: Arc<AtomicUsize>,
}

impl Op for CountingOp {
    fn name(&self) -> &'static str { "counting_invalidate" }
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

fn cursor_for(repo: &str, rev: &str, path: &str, content_hash: [u8; 32]) -> Cursor {
    let mut c = Cursor::new(Arc::from(b"placeholder".as_slice()));
    c.repo = Arc::from(repo);
    c.rev = Arc::from(rev);
    c.fs = Some(Arc::from(std::path::Path::new(path)));
    c.content_hash = Some(Arc::new(content_hash));
    c
}

fn drain(p: &Pipeline, ctx: &RtCtx, batch: Arc<[Cursor]>) -> usize {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let s = stream::iter(vec![batch]).boxed();
        let out: Vec<Arc<[Cursor]>> = p.run(ctx, s).collect().await;
        out.iter().map(|b| b.len()).sum()
    })
}

#[test]
fn evict_paths_forces_recompute_on_next_run() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cache = Arc::new(OpCache::new(true));
    let ctx = RtCtxBuilder::new().with_store(cache.clone()).build();
    let pipe = Pipeline::Op(Arc::new(CountingOp { calls: calls.clone() }));
    let batch: Arc<[Cursor]> = Arc::from(vec![cursor_for("r", "wt", "src/a.rs", [1u8; 32])]);

    // run 1: cold cache, op fires once.
    assert_eq!(drain(&pipe, &ctx, batch.clone()), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.len(), 1);

    // run 2: warm cache, op skipped (counter unchanged).
    assert_eq!(drain(&pipe, &ctx, batch.clone()), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "run 2 must hit cache");
    assert_eq!(cache.len(), 1);

    // simulate the file changing on disk.
    let evicted = apply_change(
        &cache,
        &Change::FileContent {
            repo: Arc::from("r"),
            rev: Arc::from("wt"),
            path: PathBuf::from("src/a.rs"),
        },
    );
    assert_eq!(evicted, 1, "exactly one entry should evict");
    assert_eq!(cache.len(), 0, "cache empty after eviction");

    // run 3: cache was evicted, op MUST recompute. The same input batch
    // is reused — content_hash is unchanged in this hand-crafted test —
    // so what's being asserted is purely that eviction wiped the entry.
    assert_eq!(drain(&pipe, &ctx, batch.clone()), 1);
    assert_eq!(
        calls.load(Ordering::SeqCst), 2,
        "run 3 must recompute after eviction"
    );
}

#[test]
fn unrelated_path_does_not_evict() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cache = Arc::new(OpCache::new(true));
    let ctx = RtCtxBuilder::new().with_store(cache.clone()).build();
    let pipe = Pipeline::Op(Arc::new(CountingOp { calls: calls.clone() }));
    let batch: Arc<[Cursor]> = Arc::from(vec![cursor_for("r", "wt", "src/a.rs", [9u8; 32])]);

    drain(&pipe, &ctx, batch.clone());
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Evict a sibling path; the indexed entry must survive.
    let evicted = apply_change(
        &cache,
        &Change::FileContent {
            repo: Arc::from("r"),
            rev: Arc::from("wt"),
            path: PathBuf::from("src/b.rs"),
        },
    );
    assert_eq!(evicted, 0);
    assert_eq!(cache.len(), 1);

    drain(&pipe, &ctx, batch.clone());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "unrelated eviction must not impact cache hit");
}

#[test]
fn rev_hop_evicts_every_entry_under_rev() {
    let calls = Arc::new(AtomicUsize::new(0));
    let cache = Arc::new(OpCache::new(true));
    let ctx = RtCtxBuilder::new().with_store(cache.clone()).build();
    let pipe = Pipeline::Op(Arc::new(CountingOp { calls: calls.clone() }));

    // Three batches under (r, wt) at different paths plus one under HEAD.
    let batches: Vec<Arc<[Cursor]>> = vec![
        Arc::from(vec![cursor_for("r", "wt",   "src/a.rs", [1u8; 32])]),
        Arc::from(vec![cursor_for("r", "wt",   "src/b.rs", [2u8; 32])]),
        Arc::from(vec![cursor_for("r", "wt",   "src/c.rs", [3u8; 32])]),
        Arc::from(vec![cursor_for("r", "HEAD", "src/a.rs", [4u8; 32])]),
    ];
    for b in &batches {
        drain(&pipe, &ctx, b.clone());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(cache.len(), 4);

    let evicted = apply_change(
        &cache,
        &Change::RevHead { repo: Arc::from("r"), rev: Arc::from("wt") },
    );
    assert_eq!(evicted, 3, "only wt entries evict; HEAD survives");
    assert_eq!(cache.len(), 1);

    // Replay every batch: 3 misses (recompute) + 1 hit (HEAD).
    for b in &batches {
        drain(&pipe, &ctx, b.clone());
    }
    assert_eq!(
        calls.load(Ordering::SeqCst), 7,
        "wt batches recomputed (+3); HEAD still cached (+0)"
    );
}
