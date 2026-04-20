//! WorkSteal: rayon thread pool, per-request spawn, oneshot reply.
//!
//! Pull-driven: each rayon worker steals the next task when idle. No
//! queue, no batching, no backpressure (rayon's internal deques are
//! bounded but producers block, not drop). Equivalent to rxjs
//! `mergeMap(fn, { concurrency: N })`.
//!
//! Best for: homogeneous CPU-bound effects (parse, match, fixed-string
//! scan, hash). The throughput probe's `walk-parallel` topology is this
//! shape, and measured within 5% of ast-grep CLI on the linux kernel.

use crate::{Batcher, BoxFuture, EffectKind};
use std::sync::Arc;

pub struct WorkSteal<E, F>
where
    E: EffectKind,
    F: Fn(E) -> E::Response + Send + Sync + 'static,
{
    pool: Arc<rayon::ThreadPool>,
    f: Arc<F>,
    _marker: std::marker::PhantomData<fn() -> E>,
}

impl<E, F> WorkSteal<E, F>
where
    E: EffectKind,
    F: Fn(E) -> E::Response + Send + Sync + 'static,
{
    /// Build a rayon pool owned by this batcher. `workers=0` → rayon
    /// default (num_cpus).
    pub fn new(workers: usize, f: F) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .expect("rayon pool build");
        Self {
            pool: Arc::new(pool),
            f: Arc::new(f),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<E, F> Batcher<E> for WorkSteal<E, F>
where
    E: EffectKind,
    F: Fn(E) -> E::Response + Send + Sync + 'static,
{
    fn run(&self, req: E) -> BoxFuture<'static, E::Response> {
        let pool = self.pool.clone();
        let f = self.f.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<E::Response>();
        pool.spawn(move || {
            let out = f(req);
            let _ = tx.send(out);
        });
        Box::pin(async move { rx.await.expect("work-steal reply dropped") })
    }
}
