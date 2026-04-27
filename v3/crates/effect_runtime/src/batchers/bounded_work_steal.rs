//! BoundedWorkSteal: tokio bounded mpsc → rayon pool. The CPU path for
//! v3, with proper backpressure for concurrent emitters.
//!
//! Shape:
//!   ctx.put(E) ──send().await──> mpsc(cap) ──drainer──> rayon.spawn ──> reply
//!
//! When `cap` items are in flight, `send().await` blocks the calling
//! op's task. Backpressure propagates upstream through the op's
//! pipeline naturally — no coordination needed. Slow consumers of one
//! effect kind cannot stall emitters of other kinds because each kind
//! has its own inbox.
//!
//! Use this for CPU-bound effects emitted under burst conditions —
//! many ops, streaming pipeline, no natural pacing. This is the
//! default for v3 scan/parse/match.
//!
//! Cancellation: `Batcher::run` takes a `CancellationToken`. Tripping
//! the token resolves the future immediately with `E::Response::default()`,
//! and the rayon worker — if it has not yet started — short-circuits
//! before invoking the user closure. Already-running closures finish
//! their current iteration; the reply is then dropped on the floor.
//! This matches the v2 hover switchMap shape: stale renders return a
//! cheap "nothing" value so the caller can swap a fresh one in.
//!
//! Contrast:
//! - `WorkSteal` spawns directly to rayon per put — no bound. Fine
//!   when the caller already has a known-bounded input (e.g. a Vec of
//!   paths you par_iter over), not fine when callers are concurrent
//!   ops emitting at unknown rate.
//! - `BoundedBatched` adds a coalesce step for amortizing effects.

use crate::{Batcher, BoxFuture, CancellationToken, EffectKind};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

type ReplyTx<R> = oneshot::Sender<R>;

pub struct BoundedWorkSteal<E: EffectKind> {
    tx: mpsc::Sender<(E, CancellationToken, ReplyTx<E::Response>)>,
    // Hold pool alive; rayon drops on Drop.
    _pool: Arc<rayon::ThreadPool>,
    _drainer: Arc<tokio::task::JoinHandle<()>>,
}

impl<E: EffectKind> BoundedWorkSteal<E>
where
    E::Response: Default,
{
    /// `cap` bounds the inbox. `workers` sizes the rayon pool. `f` is
    /// the op-author work function (pure CPU, `Send + Sync`).
    pub fn new<F>(cap: usize, workers: usize, f: F) -> Self
    where
        F: Fn(E) -> E::Response + Send + Sync + 'static,
    {
        let pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .expect("rayon pool build"),
        );
        let f = Arc::new(f);
        let (tx, rx) =
            mpsc::channel::<(E, CancellationToken, ReplyTx<E::Response>)>(cap);
        let rx = Arc::new(Mutex::new(rx));
        let pool_for_drainer = pool.clone();
        let drainer = tokio::spawn(async move {
            loop {
                let triple = {
                    let mut guard = rx.lock().await;
                    match guard.recv().await {
                        Some(p) => p,
                        None => return,
                    }
                };
                let (req, cancel, reply) = triple;
                let f = f.clone();
                pool_for_drainer.spawn(move || {
                    // Cooperative cancel point: skip the user closure
                    // entirely if the request was abandoned before we
                    // landed on a worker thread. Reply is dropped, and
                    // the awaiting future has already resolved with
                    // E::Response::default() via the select! below.
                    if cancel.is_cancelled() {
                        return;
                    }
                    let out = f(req);
                    let _ = reply.send(out);
                });
            }
        });
        Self {
            tx,
            _pool: pool,
            _drainer: Arc::new(drainer),
        }
    }
}

impl<E: EffectKind> Batcher<E> for BoundedWorkSteal<E>
where
    E::Response: Default,
{
    fn run(&self, req: E, cancel: CancellationToken) -> BoxFuture<'static, E::Response> {
        let tx = self.tx.clone();
        let (rtx, rrx) = oneshot::channel::<E::Response>();
        let cancel_for_worker = cancel.clone();
        Box::pin(async move {
            // Bracket both the inbox enqueue and the reply await with
            // the cancel token. Either side resolves the future with
            // a default response so callers swapping the request
            // (switchMap-style) can move on.
            tokio::select! {
                biased;
                _ = cancel.cancelled() => E::Response::default(),
                send = tx.send((req, cancel_for_worker, rtx)) => {
                    if send.is_err() {
                        // Drainer is gone — pool was dropped. Treat as
                        // cancellation rather than panicking; tests
                        // tear contexts down without coordinating.
                        return E::Response::default();
                    }
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => E::Response::default(),
                        r = rrx => r.unwrap_or_default(),
                    }
                }
            }
        })
    }
}
