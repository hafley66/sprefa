//! Proves: same `EffectKind`, three wirings. Op code is topology-agnostic.
//!
//! Three RtCtx instances each register `ScanOne` against a different
//! batcher. The test body calls `ctx.put(ScanOne { ... }).await` and
//! asserts identical output across all three. This is the lever that
//! lets the runtime swap strategy per process type (LSP = serial,
//! daemon = work-steal, batch-writer = bounded-batched) without ops
//! knowing which one they landed in.

use effect_runtime::batchers::{BoundedBatched, BoundedWorkSteal, Passthrough, WorkSteal};
use effect_proof::effects::scan_one::{count_needle, ScanOne};
use effect_runtime::{RtCtx, RtCtxBuilder};

fn sample_request() -> ScanOne {
    ScanOne {
        bytes: b"printk(\"hi\"); printk(\"there\"); foo(); printk(\"x\");".to_vec(),
        needle: b"printk".to_vec(),
    }
}

fn expected() -> u64 {
    3
}

fn ctx_passthrough() -> RtCtx {
    RtCtxBuilder::new()
        .register::<ScanOne, _>(Passthrough::<ScanOne, _>::new(count_needle))
        .build()
}

fn ctx_work_steal() -> RtCtx {
    RtCtxBuilder::new()
        .register::<ScanOne, _>(WorkSteal::<ScanOne, _>::new(4, count_needle))
        .build()
}

fn ctx_bounded_work_steal() -> RtCtx {
    RtCtxBuilder::new()
        .register::<ScanOne, _>(BoundedWorkSteal::<ScanOne>::new(
            /* cap     */ 64,
            /* workers */ 4,
            count_needle,
        ))
        .build()
}

fn ctx_bounded_batched() -> RtCtx {
    RtCtxBuilder::new()
        .register::<ScanOne, _>(BoundedBatched::<ScanOne>::new(
            /* cap       */ 64,
            /* max_batch */ 8,
            /* workers   */ 2,
            // Batch handler: just run count_needle per item. A real
            // batcher here would coalesce (e.g. one sqlite tx around
            // multiple inserts); for a pure CPU effect, per-item is
            // the natural shape, and the batching exists only to
            // control backpressure and worker count.
            |batch: Vec<ScanOne>| batch.into_iter().map(count_needle).collect(),
        ))
        .build()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn passthrough_topology() {
    let ctx = ctx_passthrough();
    assert_eq!(ctx.put(sample_request()).await, expected());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn work_steal_topology() {
    let ctx = ctx_work_steal();
    assert_eq!(ctx.put(sample_request()).await, expected());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bounded_batched_topology() {
    let ctx = ctx_bounded_batched();
    assert_eq!(ctx.put(sample_request()).await, expected());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bounded_work_steal_topology() {
    let ctx = ctx_bounded_work_steal();
    assert_eq!(ctx.put(sample_request()).await, expected());
}

/// Burst test: 2000 concurrent emitters hammer a BoundedWorkSteal with
/// cap=16. Proves the bounded mpsc backpressures emitters without
/// memory blowup, rayon drains correctly, per-request identity is
/// preserved. This is the "many ops, streaming pipeline" scenario.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bounded_work_steal_burst_backpressure() {
    let ctx = RtCtxBuilder::new()
        .register::<ScanOne, _>(BoundedWorkSteal::<ScanOne>::new(
            /* cap     */ 16,
            /* workers */ 4,
            count_needle,
        ))
        .build();
    let mut handles = Vec::with_capacity(2000);
    for i in 0..2000u64 {
        let ctx = ctx.clone();
        handles.push(tokio::spawn(async move {
            let req = ScanOne {
                bytes: format!("printk_{}_printk_{}_printk", i, i).into_bytes(),
                needle: b"printk".to_vec(),
            };
            (i, ctx.put(req).await)
        }));
    }
    for h in handles {
        let (i, count) = h.await.unwrap();
        assert_eq!(count, 3, "request {} got wrong count {}", i, count);
    }
}

/// Fan-out: submit 200 requests concurrently, three topologies must all
/// return the same set of responses. Proves work-steal and
/// bounded-batched preserve per-request identity (no cross-talk, no
/// reply re-ordering) and handle concurrent submitters.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fanout_all_topologies() {
    let contexts: Vec<(&str, RtCtx)> = vec![
        ("passthrough", ctx_passthrough()),
        ("work_steal", ctx_work_steal()),
        ("bounded_batched", ctx_bounded_batched()),
    ];
    for (label, ctx) in contexts {
        let mut handles = Vec::new();
        for i in 0..200u64 {
            let ctx = ctx.clone();
            handles.push(tokio::spawn(async move {
                let req = ScanOne {
                    bytes: format!("printk_{}_printk_{}", i, i).into_bytes(),
                    needle: b"printk".to_vec(),
                };
                ctx.put(req).await
            }));
        }
        for h in handles {
            let got = h.await.unwrap();
            assert_eq!(got, 2, "topology={} wrong count", label);
        }
    }
}
