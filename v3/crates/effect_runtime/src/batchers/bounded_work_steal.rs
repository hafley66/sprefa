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
    tx: mpsc::Sender<(E, ReplyTx<E::Response>)>,
    // Hold pool alive; rayon drops on Drop.
    _pool: Arc<rayon::ThreadPool>,
    _drainer: Arc<tokio::task::JoinHandle<()>>,
}

impl<E: EffectKind> BoundedWorkSteal<E> {
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
        let (tx, rx) = mpsc::channel::<(E, ReplyTx<E::Response>)>(cap);
        let rx = Arc::new(Mutex::new(rx));
        let pool_for_drainer = pool.clone();
        let drainer = tokio::spawn(async move {
            loop {
                let pair = {
                    let mut guard = rx.lock().await;
                    match guard.recv().await {
                        Some(p) => p,
                        None => return,
                    }
                };
                let (req, reply) = pair;
                let f = f.clone();
                pool_for_drainer.spawn(move || {
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

impl<E: EffectKind> Batcher<E> for BoundedWorkSteal<E> {
    fn run(&self, req: E, _cancel: CancellationToken) -> BoxFuture<'static, E::Response> {
        let tx = self.tx.clone();
        let (rtx, rrx) = oneshot::channel::<E::Response>();
        Box::pin(async move {
            // send().await is the backpressure point. When the mpsc is
            // at cap, this future yields until a slot frees. The
            // calling op's task is parked, which propagates upstream.
            tx.send((req, rtx))
                .await
                .expect("BoundedWorkSteal drainer gone");
            rrx.await.expect("reply dropped")
        })
    }
}
