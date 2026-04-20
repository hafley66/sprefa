//! BoundedBatched: crossbeam bounded channel + W worker threads coalescing
//! up to `max_batch` items per drain. Push-driven with backpressure.
//!
//! Equivalent to rxjs
//! `source.pipe(bufferTime(Nus) OR bufferCount(max_batch), mergeMap(handler, { concurrency: W }))`
//! sitting on top of a bounded subject that blocks emitters when full.
//!
//! Best for: effects that amortize — sqlite tx (one fsync per batch),
//! git ODB writes (one pack index lookup per batch), network RPC, any
//! mutex-guarded downstream. The throughput probe's `hopp-v2` topology
//! is this shape.
//!
//! Shape: ctx.put(E) → oneshot channel for reply → request pushed onto
//! crossbeam bounded(cap) → worker thread pops up to max_batch items →
//! runs `handler(batch)` → fires all oneshots with the corresponding
//! responses in order.

use crate::{Batcher, BoxFuture, CancellationToken, EffectKind};
use std::sync::Arc;
use tokio::sync::oneshot;

type ReplyTx<R> = oneshot::Sender<R>;

/// The op-author-facing work function. Takes a batch in, produces a
/// vector of responses in the same order.
pub type BatchFn<E> = dyn Fn(Vec<E>) -> Vec<<E as EffectKind>::Response>
    + Send
    + Sync
    + 'static;

pub struct BoundedBatched<E: EffectKind> {
    tx: crossbeam_channel::Sender<(E, ReplyTx<E::Response>)>,
    _workers: Arc<Vec<std::thread::JoinHandle<()>>>,
}

impl<E: EffectKind> BoundedBatched<E> {
    /// Spawn `workers` threads behind a bounded channel of `cap`. Each
    /// worker drains up to `max_batch` items opportunistically (drains
    /// all available, capped at max_batch) before calling `handler`.
    pub fn new<F>(
        cap: usize,
        max_batch: usize,
        workers: usize,
        handler: F,
    ) -> Self
    where
        F: Fn(Vec<E>) -> Vec<E::Response> + Send + Sync + 'static,
    {
        let (tx, rx) = crossbeam_channel::bounded::<(E, ReplyTx<E::Response>)>(cap);
        let handler: Arc<BatchFn<E>> = Arc::new(handler);
        let mut handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let rx = rx.clone();
            let handler = handler.clone();
            handles.push(std::thread::spawn(move || {
                let mut batch: Vec<(E, ReplyTx<E::Response>)> =
                    Vec::with_capacity(max_batch);
                loop {
                    // Block for the first item (drains-on-disconnect).
                    let first = match rx.recv() {
                        Ok(pair) => pair,
                        Err(_) => return,
                    };
                    batch.push(first);
                    // Opportunistic drain: pull everything else that's
                    // ready without blocking, up to max_batch total.
                    while batch.len() < max_batch {
                        match rx.try_recv() {
                            Ok(pair) => batch.push(pair),
                            Err(_) => break,
                        }
                    }
                    // Split reqs from replies, run handler, zip back.
                    let (reqs, replies): (Vec<E>, Vec<ReplyTx<E::Response>>) =
                        std::mem::take(&mut batch).into_iter().unzip();
                    let outs = handler(reqs);
                    assert_eq!(
                        outs.len(),
                        replies.len(),
                        "handler must return one response per request",
                    );
                    for (reply, out) in replies.into_iter().zip(outs) {
                        let _ = reply.send(out);
                    }
                }
            }));
        }
        Self { tx, _workers: Arc::new(handles) }
    }
}

impl<E: EffectKind> Batcher<E> for BoundedBatched<E> {
    fn run(&self, req: E, _cancel: CancellationToken) -> BoxFuture<'static, E::Response> {
        let (rtx, rrx) = oneshot::channel::<E::Response>();
        // Send on a blocking crossbeam channel from inside an async
        // context. For the prototype this is fine; real code would use
        // `spawn_blocking` to avoid starving the runtime when `cap` is
        // saturated.
        self.tx.send((req, rtx)).expect("BoundedBatched workers gone");
        Box::pin(async move {
            rrx.await.expect("batched reply dropped")
        })
    }
}
