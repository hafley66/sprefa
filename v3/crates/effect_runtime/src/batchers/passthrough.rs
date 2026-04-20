//! Passthrough: direct compute, no pool, no queue. The control shape.
//!
//! Use when the effect is cheap, rare, or must not overlap (single global
//! mutex held, ordered side-effects). Equivalent to rxjs `concatMap`.

use crate::{Batcher, BoxFuture, EffectKind};
use std::sync::Arc;

/// Wrap any `Fn(E) -> E::Response` that is cheap/synchronous.
pub struct Passthrough<E, F>
where
    E: EffectKind,
    F: Fn(E) -> E::Response + Send + Sync + 'static,
{
    f: Arc<F>,
    _marker: std::marker::PhantomData<fn() -> E>,
}

impl<E, F> Passthrough<E, F>
where
    E: EffectKind,
    F: Fn(E) -> E::Response + Send + Sync + 'static,
{
    pub fn new(f: F) -> Self {
        Self { f: Arc::new(f), _marker: std::marker::PhantomData }
    }
}

impl<E, F> Batcher<E> for Passthrough<E, F>
where
    E: EffectKind,
    F: Fn(E) -> E::Response + Send + Sync + 'static,
{
    fn run(&self, req: E) -> BoxFuture<'static, E::Response> {
        let f = self.f.clone();
        Box::pin(async move { f(req) })
    }
}
