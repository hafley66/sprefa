//! Yield / Next / Unsubscribe — the bidirectional coroutine primitive.
//!
//! Vocabulary (locked 2026-04-26):
//!
//! - **Yield**: op-side suspend point. Op calls `cx.put(Yield::<S> {..})`
//!   and awaits a `Result<Arc<S::Payload>, Unsubscribed>`.
//! - **Next**: outside-world resume. Outside code calls
//!   `registry.next(key, value)` to deliver a value to the awaiting op.
//! - **Unsubscribe**: outside-world cancel. Outside code calls
//!   `registry.unsubscribe(key)` (or bulk via `unsubscribe_where`).
//!   The op's await resolves `Err(Unsubscribed)`.
//!
//! Each `SubjectKind` instantiation is its own `EffectKind`, its own
//! `TypeMap` slot, its own `SubjectRegistry`. Payload type is statically
//! known at the call site; no `Any` downcast at the API boundary.
//!
//! Three planned consumers (parked write effects, tag-subscribe, Pending
//! captures) declare three `SubjectKind` impls and live side by side.
//!
//! ## Cancellation paths
//!
//! Every path GCs the subject entry:
//!
//! 1. External `next(key, v)`: entry removed, op resolves `Ok(v)`.
//! 2. External `unsubscribe(key)`: entry removed, op resolves `Err`.
//! 3. Bulk `unsubscribe_where(pred)`: matching entries removed, ops resolve `Err`.
//! 4. Runtime cancel token (`cancel_all` or per-put): batcher selects on
//!    `cancel.cancelled()`, removes its own entry, op resolves `Err`.
//! 5. Future drop (op-side abort, e.g. stream consumer dropped): a Drop
//!    guard inside the future calls `unsubscribe` on the registry.
//!
//! ## Lineage
//!
//! `Yield` carries an opaque `Option<Lineage>` (boxed `Any`). Lineage
//! erasure is independent of payload typing — different consumers attach
//! their own struct (e.g. `CursorLineage`) and use `unsubscribe_where`
//! with a downcast predicate to invalidate by upstream-cursor identity.

use std::any::Any;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

use crate::{Batcher, BoxFuture, CancellationToken, EffectKind, Store};

/// Marker trait identifying a subject channel and its payload type.
/// Implementors are zero-sized markers (`struct TagWake;`); the only
/// associated item is the typed payload that wakes carry.
pub trait SubjectKind: Send + Sync + 'static {
    type Payload: Send + Sync + 'static;
}

/// Identifier for a pending subject in registry `S`. Allocated by
/// `SubjectRegistry::<S>::fresh_key`; opaque to consumers. The phantom
/// `S` keeps keys from one registry from being passed to another.
pub struct SubjectKey<S: SubjectKind> {
    id: u64,
    _s: PhantomData<fn() -> S>,
}

impl<S: SubjectKind> SubjectKey<S> {
    pub fn raw(self) -> u64 { self.id }
}

impl<S: SubjectKind> Clone for SubjectKey<S> {
    fn clone(&self) -> Self { *self }
}
impl<S: SubjectKind> Copy for SubjectKey<S> {}
impl<S: SubjectKind> PartialEq for SubjectKey<S> {
    fn eq(&self, other: &Self) -> bool { self.id == other.id }
}
impl<S: SubjectKind> Eq for SubjectKey<S> {}
impl<S: SubjectKind> Hash for SubjectKey<S> {
    fn hash<H: Hasher>(&self, state: &mut H) { self.id.hash(state); }
}
impl<S: SubjectKind> std::fmt::Debug for SubjectKey<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubjectKey<{}>({})", std::any::type_name::<S>(), self.id)
    }
}

/// Sentinel error: the subject was unsubscribed before a value arrived.
#[derive(Debug, PartialEq, Eq)]
pub struct Unsubscribed;

/// Optional lineage tag attached at register time. Used by
/// `unsubscribe_where` for bulk teardown when an upstream invariant
/// changes. Consumers define their own struct and attach it as
/// `Arc<MyLineage>`.
pub type Lineage = Arc<dyn Any + Send + Sync>;

/// Effect kind. Op author writes
/// `cx.put(Yield::<S> { key, lineage }).await`.
pub struct Yield<S: SubjectKind> {
    pub key: SubjectKey<S>,
    pub lineage: Option<Lineage>,
}

impl<S: SubjectKind> EffectKind for Yield<S> {
    type Response = Result<Arc<S::Payload>, Unsubscribed>;
}

struct Entry<S: SubjectKind> {
    sender: oneshot::Sender<Result<Arc<S::Payload>, Unsubscribed>>,
    lineage: Option<Lineage>,
}

/// Keyed registry of pending Yield calls for subject kind `S`. Held as
/// a `Store` on `RtCtx`. Outside code (LSP server, CLI runner, write
/// batcher) gets it via `cx.store::<SubjectRegistry<S>>()` and drives
/// `next` / `unsubscribe`.
pub struct SubjectRegistry<S: SubjectKind> {
    next_id: AtomicU64,
    pending: Mutex<HashMap<SubjectKey<S>, Entry<S>>>,
}

impl<S: SubjectKind> Default for SubjectRegistry<S> {
    fn default() -> Self { Self::new() }
}

impl<S: SubjectKind> Store for SubjectRegistry<S> {}

impl<S: SubjectKind> SubjectRegistry<S> {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// Allocate a fresh subject key. Op authors call this before
    /// dispatching `Yield` so the same key is reachable to whichever
    /// outside path will resume them.
    pub fn fresh_key(&self) -> SubjectKey<S> {
        SubjectKey {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            _s: PhantomData,
        }
    }

    /// Number of currently-pending subjects. Useful for tests + debug.
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }

    /// Resume the op awaiting `key` with `value`. Returns true if a
    /// pending subject was found and woken; false if no such key.
    /// Concurrent `next` calls on the same key: first wins.
    pub fn next(&self, key: SubjectKey<S>, value: Arc<S::Payload>) -> bool {
        let entry = self.pending.lock().unwrap().remove(&key);
        match entry {
            Some(e) => {
                let _ = e.sender.send(Ok(value));
                true
            }
            None => false,
        }
    }

    /// Cancel `key`. Returns true if a pending subject was found.
    pub fn unsubscribe(&self, key: SubjectKey<S>) -> bool {
        let entry = self.pending.lock().unwrap().remove(&key);
        match entry {
            Some(e) => {
                let _ = e.sender.send(Err(Unsubscribed));
                true
            }
            None => false,
        }
    }

    /// Cancel every subject whose lineage matches `pred`. Subjects with
    /// no lineage are ignored. Returns the count cancelled.
    pub fn unsubscribe_where<F>(&self, mut pred: F) -> usize
    where
        F: FnMut(&dyn Any) -> bool,
    {
        let mut guard = self.pending.lock().unwrap();
        let to_remove: Vec<SubjectKey<S>> = guard
            .iter()
            .filter_map(|(k, e)| match e.lineage.as_ref() {
                Some(lin) if pred(lin.as_ref()) => Some(*k),
                _ => None,
            })
            .collect();
        let n = to_remove.len();
        for k in to_remove {
            if let Some(e) = guard.remove(&k) {
                let _ = e.sender.send(Err(Unsubscribed));
            }
        }
        n
    }

    /// Cancel every pending subject. Used by `RtCtx::cancel_all` paths
    /// that want to drain the runtime; not called from the framework
    /// directly (root cancel token already wakes each YieldBatcher
    /// future, which then GCs its own entry).
    pub fn unsubscribe_all(&self) -> usize {
        let mut guard = self.pending.lock().unwrap();
        let n = guard.len();
        for (_, e) in guard.drain() {
            let _ = e.sender.send(Err(Unsubscribed));
        }
        n
    }

    fn register(
        &self,
        key: SubjectKey<S>,
        lineage: Option<Lineage>,
    ) -> oneshot::Receiver<Result<Arc<S::Payload>, Unsubscribed>> {
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(key, Entry { sender: tx, lineage });
        rx
    }

    /// Synchronous subscribe: insert a pending entry for `key` BEFORE
    /// returning, then hand back a future that awaits the wake.
    ///
    /// Producers holding `key` can call [`next`] / [`unsubscribe`]
    /// immediately after this returns and the wake is guaranteed to
    /// land — there is no batcher-poll window during which the entry is
    /// missing.
    ///
    /// The returned future is `'static`, carries an internal Drop guard
    /// that calls [`unsubscribe`] when the future is dropped before
    /// resolution.
    pub fn subscribe(
        self: &Arc<Self>,
        key: SubjectKey<S>,
        lineage: Option<Lineage>,
    ) -> BoxFuture<'static, Result<Arc<S::Payload>, Unsubscribed>> {
        let rx = self.register(key, lineage);
        let guard = YieldGuard { registry: self.clone(), key };
        Box::pin(async move {
            let _g = guard;
            match rx.await {
                Ok(r) => r,
                Err(_) => Err(Unsubscribed),
            }
        })
    }
}

/// Batcher for `Yield<S>`. Construct once with an `Arc<SubjectRegistry<S>>`
/// shared with whoever drives `next` / `unsubscribe`. Register on the
/// builder alongside the store:
///
/// ```ignore
/// struct TagWake;
/// impl SubjectKind for TagWake { type Payload = Vec<Arc<str>>; }
///
/// let registry = Arc::new(SubjectRegistry::<TagWake>::new());
/// let cx = RtCtxBuilder::new()
///     .with_store(registry.clone())
///     .register::<Yield<TagWake>, _>(YieldBatcher::new(registry.clone()))
///     .build();
/// ```
pub struct YieldBatcher<S: SubjectKind> {
    registry: Arc<SubjectRegistry<S>>,
}

impl<S: SubjectKind> YieldBatcher<S> {
    pub fn new(registry: Arc<SubjectRegistry<S>>) -> Self {
        Self { registry }
    }
}

/// RAII guard that cleans up the registry entry on future-drop. Any
/// path that exits the `select!` (Next, Unsubscribe, cancel-token,
/// future-drop from outside) hits this Drop. Already-resolved entries
/// are no-ops because `unsubscribe` returns false.
struct YieldGuard<S: SubjectKind> {
    registry: Arc<SubjectRegistry<S>>,
    key: SubjectKey<S>,
}

impl<S: SubjectKind> Drop for YieldGuard<S> {
    fn drop(&mut self) {
        self.registry.unsubscribe(self.key);
    }
}

impl<S: SubjectKind> Batcher<Yield<S>> for YieldBatcher<S> {
    fn run(
        &self,
        req: Yield<S>,
        cancel: CancellationToken,
    ) -> BoxFuture<'static, Result<Arc<S::Payload>, Unsubscribed>> {
        let fut = self.registry.subscribe(req.key, req.lineage);
        Box::pin(async move {
            tokio::select! {
                resolved = fut => resolved,
                _ = cancel.cancelled() => Err(Unsubscribed),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RtCtx, RtCtxBuilder};
    use std::time::Duration;

    /// Test subject: payload is a String so wakes carry typed values.
    pub struct TestSubject;
    impl SubjectKind for TestSubject {
        type Payload = String;
    }

    type TestKey = SubjectKey<TestSubject>;
    type TestRegistry = SubjectRegistry<TestSubject>;

    fn build_cx() -> (RtCtx, Arc<TestRegistry>) {
        let registry = Arc::new(TestRegistry::new());
        let cx = RtCtxBuilder::new()
            .with_store(registry.clone())
            .register::<Yield<TestSubject>, _>(YieldBatcher::new(registry.clone()))
            .build();
        (cx, registry)
    }

    fn val(s: &str) -> Arc<String> {
        Arc::new(s.to_string())
    }

    #[tokio::test]
    async fn roundtrip_next_resolves_yield() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let yield_fut = cx.put(Yield::<TestSubject> { key, lineage: None });

        let driver = {
            let reg = reg.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                assert!(reg.next(key, val("hello")));
            })
        };

        let v = yield_fut.await.expect("Ok");
        assert_eq!(&*v, "hello");
        assert_eq!(reg.pending_count(), 0);
        driver.await.unwrap();
    }

    fn assert_unsub<T>(r: Result<T, Unsubscribed>) {
        match r {
            Err(Unsubscribed) => {}
            Ok(_) => panic!("expected Unsubscribed, got Ok"),
        }
    }

    #[tokio::test]
    async fn unsubscribe_resolves_with_err() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let yield_fut = cx.put(Yield::<TestSubject> { key, lineage: None });

        let driver = {
            let reg = reg.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                assert!(reg.unsubscribe(key));
            })
        };

        assert_unsub(yield_fut.await);
        assert_eq!(reg.pending_count(), 0);
        driver.await.unwrap();
    }

    #[tokio::test]
    async fn future_drop_gcs_entry() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();

        let cx_clone = cx.clone();
        let handle = tokio::spawn(async move {
            let _ = cx_clone.put(Yield::<TestSubject> { key, lineage: None }).await;
        });
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(reg.pending_count(), 1, "registered while awaited");

        handle.abort();
        let _ = handle.await;
        tokio::task::yield_now().await;
        assert_eq!(reg.pending_count(), 0, "guard cleaned up on abort");
    }

    #[tokio::test]
    async fn runtime_cancel_resolves_err_and_gcs() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let yield_fut = cx.put(Yield::<TestSubject> { key, lineage: None });

        let driver = {
            let cx = cx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(10)).await;
                cx.cancel_all();
            })
        };

        assert_unsub(yield_fut.await);
        tokio::task::yield_now().await;
        assert_eq!(reg.pending_count(), 0);
        driver.await.unwrap();
    }

    #[derive(Debug)]
    struct Lin { tag: u32 }

    fn spawn_yield(
        cx: &RtCtx,
        key: TestKey,
        lineage: Option<Lineage>,
    ) -> tokio::task::JoinHandle<Result<Arc<String>, Unsubscribed>> {
        let cx = cx.clone();
        tokio::spawn(async move { cx.put(Yield::<TestSubject> { key, lineage }).await })
    }

    #[tokio::test]
    async fn unsubscribe_where_predicate_matches_lineage() {
        let (cx, reg) = build_cx();
        let k1 = reg.fresh_key();
        let k2 = reg.fresh_key();
        let k3 = reg.fresh_key();

        let h1 = spawn_yield(&cx, k1, Some(Arc::new(Lin { tag: 1 })));
        let h2 = spawn_yield(&cx, k2, Some(Arc::new(Lin { tag: 2 })));
        let h3 = spawn_yield(&cx, k3, None);

        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(reg.pending_count(), 3);

        let n = reg.unsubscribe_where(|any| {
            matches!(any.downcast_ref::<Lin>(), Some(l) if l.tag == 1)
        });
        assert_eq!(n, 1);
        assert_unsub(h1.await.unwrap());

        assert!(reg.next(k2, val("two")));
        assert!(reg.unsubscribe(k3));
        assert_eq!(&*h2.await.unwrap().unwrap(), "two");
        assert_unsub(h3.await.unwrap());
        assert_eq!(reg.pending_count(), 0);
    }

    #[tokio::test]
    async fn concurrent_next_first_wins() {
        let (cx, reg) = build_cx();
        let key = reg.fresh_key();
        let h = spawn_yield(&cx, key, None);

        tokio::time::sleep(Duration::from_millis(10)).await;

        assert!(reg.next(key, val("first")));
        assert!(!reg.next(key, val("second")), "second next is no-op");
        assert!(!reg.unsubscribe(key), "unsubscribe after next is no-op");

        let v = h.await.unwrap().unwrap();
        assert_eq!(&*v, "first");
    }

    #[tokio::test]
    async fn registry_reachable_via_store_typemap() {
        let (cx, reg) = build_cx();
        let from_store: Arc<TestRegistry> =
            cx.store::<TestRegistry>().expect("registered");
        let k = from_store.fresh_key();
        assert_eq!(k.raw(), reg.fresh_key().raw() - 1);
    }

    /// The race the original tests papered over: when the SubjectKey
    /// is shared with a producer, the producer can call `next(key)`
    /// BEFORE the awaiting code has any chance to register the pending
    /// entry. With `ctx.put(Yield)` the registration only happens when
    /// `YieldBatcher::run` is polled — a window during which `next`
    /// silently misses and the await blocks forever.
    ///
    /// `subscribe` closes the window by inserting the entry before
    /// returning. This test fires `next` synchronously after subscribe
    /// (no spawn, no sleep) and expects it to resolve.
    #[tokio::test]
    async fn subscribe_then_synchronous_next_resolves() {
        let reg = Arc::new(TestRegistry::new());
        let key = reg.fresh_key();
        let fut = reg.subscribe(key, None);
        assert!(reg.next(key, val("eager")));
        let v = fut.await.expect("Ok");
        assert_eq!(&*v, "eager");
        assert_eq!(reg.pending_count(), 0);
    }

    #[tokio::test]
    async fn subscribe_drop_unsubscribes() {
        let reg = Arc::new(TestRegistry::new());
        let key = reg.fresh_key();
        let fut = reg.subscribe(key, None);
        assert_eq!(reg.pending_count(), 1);
        drop(fut);
        assert_eq!(reg.pending_count(), 0);
    }

    #[tokio::test]
    async fn unsubscribe_all_drains_registry() {
        let (cx, reg) = build_cx();
        let h1 = spawn_yield(&cx, reg.fresh_key(), None);
        let h2 = spawn_yield(&cx, reg.fresh_key(), None);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(reg.pending_count(), 2);
        let n = reg.unsubscribe_all();
        assert_eq!(n, 2);
        assert_unsub(h1.await.unwrap());
        assert_unsub(h2.await.unwrap());
    }
}
