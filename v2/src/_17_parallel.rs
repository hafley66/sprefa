//! rayon_map — async -> rayon -> async bridge.
//!
//! Shape:
//!   async prefetch (tokio) -> rayon::spawn + into_par_iter -> oneshot -> await
//!
//! Used by ast / fs ops for CPU-bound batch work after I/O fan-out. All
//! captured state must be Send + Sync; `f` runs on the rayon pool, never on a
//! tokio worker.

use rayon::prelude::*;
use tracing::{info_span, Instrument};

/// Run `f` over `items` on the rayon pool, bridging back into async via a
/// oneshot channel. Order-preserving (`into_par_iter().map().collect()`).
pub async fn rayon_map<T, U, F>(items: Vec<T>, f: F) -> Vec<U>
where
    T: Send + 'static,
    U: Send + 'static,
    F: Fn(T) -> U + Send + Sync + 'static,
{
    let n = items.len();
    let span = info_span!("rayon.map", n = n);
    async move {
        let (tx, rx) = tokio::sync::oneshot::channel::<Vec<U>>();
        rayon::spawn(move || {
            let _s = info_span!("rayon.map.pool", n = n).entered();
            let out: Vec<U> = items.into_par_iter().map(f).collect();
            let _ = tx.send(out);
        });
        rx.await.unwrap_or_default()
    }.instrument(span).await
}

/// Single-item variant — dispatch one CPU unit onto the rayon pool and
/// await the result. Used when the caller already has per-item async
/// plumbing (e.g. OnceCell init inside ParseCacheReader) and only needs
/// the CPU off tokio's blocking pool.
pub async fn rayon_one<U, F>(f: F) -> Option<U>
where
    U: Send + 'static,
    F: FnOnce() -> U + Send + 'static,
{
    let span = info_span!("rayon.one");
    async move {
        let (tx, rx) = tokio::sync::oneshot::channel::<U>();
        rayon::spawn(move || {
            let _s = info_span!("rayon.one.pool").entered();
            let _ = tx.send(f());
        });
        rx.await.ok()
    }.instrument(span).await
}
