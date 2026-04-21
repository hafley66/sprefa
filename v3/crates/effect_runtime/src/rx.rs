//! rxRust adapter (feature = "rx").
//!
//! The registry stays non-rx — its value is heterogeneous typed
//! dispatch by `TypeId`, which does not map to rxRust's homogeneous
//! `Observable<Item>` model. What *does* map cleanly is the per-kind
//! traffic stream: every `ctx.put::<E>()` produces a request and a
//! response that can be published on two `SharedSubject`s so userland
//! pipelines can compose rxRust operators around them.
//!
//! Shape: one `ObservableTap<E>` batcher wraps an inner `Batcher<E>`
//! and on every request/response pair, forwards to the inner AND emits
//! onto two `SharedSubject`s. `RtCtxBuilder::register_observable`
//! returns the two subject handles along with the builder so userland
//! can subscribe before the ctx is built.
//!
//! Both `E` and `E::Response` must be `Clone + Send + 'static` — a
//! tighter bound than `EffectKind` alone, required because Subject's
//! value broadcast path needs `Clone`.

use crate::{Batcher, BoxFuture, CancellationToken, EffectKind, RtCtxBuilder};
use rxrust::prelude::*;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

/// Shared handle to a `SharedSubject`. `next` requires `&mut self`
/// so we gate concurrent publishers through a small `Mutex`. Only
/// one publisher (the tap) writes; subscribers clone the subject
/// independently and do not contend here.
pub type SubjectHandle<T> = Arc<Mutex<SharedSubject<'static, T, Infallible>>>;

/// Batcher that taps request/response traffic into two subjects,
/// forwarding to an inner batcher unchanged.
pub struct ObservableTap<E: EffectKind>
where
    E: Clone + Send + 'static,
    E::Response: Clone + Send + 'static,
{
    inner: Arc<dyn Batcher<E>>,
    req_tx: SubjectHandle<E>,
    resp_tx: SubjectHandle<E::Response>,
}

impl<E: EffectKind> ObservableTap<E>
where
    E: Clone + Send + 'static,
    E::Response: Clone + Send + 'static,
{
    pub fn new<B: Batcher<E>>(
        inner: B,
    ) -> (Self, SubjectHandle<E>, SubjectHandle<E::Response>) {
        let req_tx: SubjectHandle<E> =
            Arc::new(Mutex::new(Shared::subject::<E, Infallible>()));
        let resp_tx: SubjectHandle<E::Response> =
            Arc::new(Mutex::new(Shared::subject::<E::Response, Infallible>()));
        let tap = Self {
            inner: Arc::new(inner),
            req_tx: req_tx.clone(),
            resp_tx: resp_tx.clone(),
        };
        (tap, req_tx, resp_tx)
    }
}

impl<E: EffectKind> Batcher<E> for ObservableTap<E>
where
    E: Clone + Send + 'static,
    E::Response: Clone + Send + 'static,
{
    fn run(&self, req: E, cancel: CancellationToken) -> BoxFuture<'static, E::Response> {
        let req_clone = req.clone();
        if let Ok(mut s) = self.req_tx.lock() {
            s.next(req_clone);
        }
        let inner = self.inner.clone();
        let resp_tx = self.resp_tx.clone();
        Box::pin(async move {
            let resp = inner.run(req, cancel).await;
            if let Ok(mut s) = resp_tx.lock() {
                s.next(resp.clone());
            }
            resp
        })
    }
}

/// Builder sugar: registers an `ObservableTap<E>` wrapping `inner`
/// and returns the two subject handles so userland can subscribe
/// before building the ctx.
pub fn register_observable<E, B>(
    builder: RtCtxBuilder,
    inner: B,
) -> (RtCtxBuilder, SubjectHandle<E>, SubjectHandle<E::Response>)
where
    E: EffectKind + Clone + Send + 'static,
    E::Response: Clone + Send + 'static,
    B: Batcher<E>,
{
    let (tap, req_tx, resp_tx) = ObservableTap::<E>::new(inner);
    let next_builder = builder.register::<E, _>(tap);
    (next_builder, req_tx, resp_tx)
}
