//! Pipeline envelope. Framework owns fan-out semantics on a stream-to-
//! stream surface (session 20260426.4).
//!
//! `Pipeline::run` takes `BoxStream<Arc<[Cursor]>>` and returns one.
//!   - `Op(op)`  threads upstream through the op's `pipe_flat_map`
//!     (default = mergeMap(64) over `pipe`) and tags emitted cursors
//!     with `PathSeg::Op { name, step }` per emitted batch.
//!   - `Seq(xs)` folds upstream through stages: `xs.iter().fold(s, |s, st| st.run(ctx, s))`.
//!   - `Fork(arms)` broadcasts upstream to each arm and merges arm
//!     outputs. Each arm's emitted cursors are tagged with
//!     `PathSeg::ForkArm { index }`.
//!
//! Backpressure: native via `futures::Stream` pull semantics. Each arm
//! and stage is polled by downstream demand. `buffer_unordered(N)` inside
//! `pipe_flat_map` defaults bounds in-flight `pipe` calls per op.
//!
//! PAIN POINT — Fork broadcast cost:
//!
//! Broadcast over a stream requires either replay buffering (record
//! upstream, replay to each arm) or a fan-out channel (forward each
//! emit to N receivers). This slice picks the channel path via
//! `tokio::sync::broadcast`. Lossy by default if an arm lags; capacity
//! is intentionally large for this slice. The push/pull dam pattern
//! (theory:push-pull-dam) would replace this with a bounded mpsc per
//! arm; deferred.

use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::_0_cursor::{Cursor, PathSeg};
use crate::_1_op::Op;
use crate::cache_key::{batch_fingerprint, OpCache};
use effect_runtime::RtCtx;

pub enum Pipeline {
    /// `Arc` so a single lowered op can back both the LSP hover render
    /// path and the pipeline run path without re-lowering.
    Op(Arc<dyn Op>),
    Seq(Vec<Pipeline>),
    Fork(Vec<Pipeline>),
}

impl Pipeline {
    pub fn run<'a>(
        &'a self,
        ctx: &'a RtCtx,
        upstream: BoxStream<'a, Arc<[Cursor]>>,
    ) -> BoxStream<'a, Arc<[Cursor]>> {
        match self {
            Pipeline::Op(op) => {
                let name = op.name();
                // Probe op cacheability once. If the op declares itself
                // cacheable AND an OpCache store is bound on the ctx, we
                // route per-batch through a cached `pipe` call. Otherwise
                // fall through to the standard `pipe_flat_map`.
                let cacheable = {
                    let mut probe = blake3::Hasher::new();
                    op.cache_key(&mut probe)
                };
                let cache: Option<Arc<OpCache>> = ctx.store::<OpCache>();
                let inner: BoxStream<'a, Arc<[Cursor]>> = match (cacheable, cache) {
                    (true, Some(cache)) if cache.enabled => {
                        let op_arc = op.clone();
                        Box::pin(upstream.then(move |batch| {
                            let cache = cache.clone();
                            let op = op_arc.clone();
                            async move {
                                let in_len = batch.len();
                                let t0 = std::time::Instant::now();
                                let mut h = blake3::Hasher::new();
                                h.update(&batch_fingerprint(&batch));
                                let _ = op.cache_key(&mut h);
                                let key = *h.finalize().as_bytes();
                                if let Some(hit) = cache.get(&key) {
                                    tracing::info!(
                                        target: "sprefa::pipeline",
                                        op = name,
                                        in_cursors = in_len,
                                        out_cursors = hit.len(),
                                        elapsed_ms = t0.elapsed().as_millis() as u64,
                                        cache = "hit",
                                        "op.batch"
                                    );
                                    return hit;
                                }
                                let out = op.pipe(ctx, batch).await;
                                cache.insert(key, out.clone());
                                let elapsed_ms = t0.elapsed().as_millis() as u64;
                                tracing::info!(
                                    target: "sprefa::pipeline",
                                    op = name,
                                    in_cursors = in_len,
                                    out_cursors = out.len(),
                                    elapsed_ms,
                                    cache = "miss",
                                    "op.batch"
                                );
                                if elapsed_ms >= 2_000 {
                                    tracing::warn!(
                                        target: "sprefa::pipeline",
                                        op = name,
                                        in_cursors = in_len,
                                        out_cursors = out.len(),
                                        elapsed_ms,
                                        "op.slow"
                                    );
                                }
                                out
                            }
                        }))
                    }
                    _ => op.pipe_flat_map(ctx, upstream),
                };
                let stream = inner
                    .enumerate()
                    .map(move |(step, batch)| {
                        let mut v: Vec<Cursor> = batch.iter().cloned().collect();
                        for c in &mut v {
                            c.path.push(PathSeg::Op { name, step });
                        }
                        Arc::<[Cursor]>::from(v)
                    });
                Box::pin(stream)
            }
            Pipeline::Seq(stages) => {
                let mut s: BoxStream<'a, Arc<[Cursor]>> = upstream;
                for stage in stages {
                    s = stage.run(ctx, s);
                }
                s
            }
            Pipeline::Fork(arms) => fork_broadcast(ctx, upstream, arms),
        }
    }
}

/// Fork: broadcast upstream batches to every arm, run each arm, merge
/// outputs. Each arm tags its emitted cursors with
/// `PathSeg::ForkArm { index }`.
///
/// Implementation detail: a `tokio::sync::broadcast` channel fans
/// upstream batches to each arm's input stream. Upstream is *not*
/// spawned — it can borrow `'a` data, so cannot be `'static`. Instead
/// the upstream pull is driven inline by a sentinel stream that yields
/// no items but advances the producer. `stream::select` interleaves
/// that producer-driver with the merged arm outputs, so polling the
/// returned stream pulls upstream as needed.
fn fork_broadcast<'a>(
    ctx: &'a RtCtx,
    upstream: BoxStream<'a, Arc<[Cursor]>>,
    arms: &'a [Pipeline],
) -> BoxStream<'a, Arc<[Cursor]>> {
    // Capacity sized to absorb fork fan-out without blocking. If an
    // arm lags past this, BroadcastStream surfaces Lagged which we map
    // to an empty batch. The bounded mpsc-per-arm dam shape is the
    // follow-up.
    const FORK_CAP: usize = 1024;
    let n = arms.len();
    if n == 0 {
        return Box::pin(stream::empty());
    }

    let (tx, _rx0) = broadcast::channel::<Arc<[Cursor]>>(FORK_CAP);

    let mut arm_streams: Vec<BoxStream<'a, Arc<[Cursor]>>> = Vec::with_capacity(n);
    for (index, arm) in arms.iter().enumerate() {
        let rx = tx.subscribe();
        let arm_input: BoxStream<'a, Arc<[Cursor]>> = Box::pin(
            BroadcastStream::new(rx).filter_map(|r| async move {
                match r {
                    Ok(b) => Some(b),
                    Err(_lagged) => None,
                }
            }),
        );
        let tagged = arm.run(ctx, arm_input).map(move |batch| {
            let mut v: Vec<Cursor> = batch.iter().cloned().collect();
            for c in &mut v {
                c.path.push(PathSeg::ForkArm { index });
            }
            Arc::<[Cursor]>::from(v)
        });
        arm_streams.push(Box::pin(tagged));
    }

    // Producer-driver: an inline stream that drives upstream → broadcast.
    // It yields no items (Stream<Item=Arc<[Cursor]>>) but each .await on
    // upstream.next() advances the producer side of the fork. When
    // upstream completes, drop the broadcast sender so subscribers see
    // close.
    let producer_driver = {
        let tx_for_producer = tx.clone();
        let driver = stream::unfold(
            (Some(upstream), Some(tx_for_producer)),
            |(mut up_opt, mut tx_opt)| async move {
                let Some(up) = up_opt.as_mut() else { return None; };
                match up.next().await {
                    Some(batch) => {
                        if let Some(tx) = tx_opt.as_ref() {
                            let _ = tx.send(batch);
                        }
                        // Re-park: continue, no item emitted.
                        // Recurse via a fresh state.
                        Some((Arc::<[Cursor]>::from(Vec::<Cursor>::new()), (up_opt, tx_opt)))
                    }
                    None => {
                        // Drop the sender so subscribers see close.
                        tx_opt = None;
                        up_opt = None;
                        // Yield one final empty so downstream advances
                        // its select; subsequent polls return None.
                        Some((Arc::<[Cursor]>::from(Vec::<Cursor>::new()), (up_opt, tx_opt)))
                    }
                }
            },
        )
        .filter(|b| {
            let keep = !b.is_empty();
            async move { keep }
        });
        // Concrete type erasure to a BoxStream<'a, ...>.
        let s: BoxStream<'a, Arc<[Cursor]>> = Box::pin(driver);
        s
    };

    // Drop the original sender; the broadcast closes when every send
    // half drops (the producer_driver retains the only remaining one).
    drop(tx);

    let merged = stream::select_all(arm_streams);
    Box::pin(stream::select(producer_driver, merged))
}
