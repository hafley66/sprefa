//! rayon_map — async -> rayon -> async bridge.
//!
//! Shape:
//!   async prefetch (tokio) -> rayon::spawn + into_par_iter -> oneshot -> await
//!
//! Used by ast / fs ops for CPU-bound batch work after I/O fan-out. All
//! captured state must be Send + Sync; `f` runs on the rayon pool, never on a
//! tokio worker.

use rayon::prelude::*;

/// Run `f` over `items` on the rayon pool, bridging back into async via a
/// oneshot channel. Order-preserving (`into_par_iter().map().collect()`).
pub async fn rayon_map<T, U, F>(items: Vec<T>, f: F) -> Vec<U>
where
    T: Send + 'static,
    U: Send + 'static,
    F: Fn(T) -> U + Send + Sync + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel::<Vec<U>>();
    rayon::spawn(move || {
        let out: Vec<U> = items.into_par_iter().map(f).collect();
        let _ = tx.send(out);
    });
    rx.await.unwrap_or_default()
}
