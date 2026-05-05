//! Generic Provider / Service / Transport scaffold.
//!
//! No sprf-specific types. Three traits + two structs define the entire
//! shape that v4 shells (cli, lsp, http) plug into:
//!
//!   - `Store`     : the persistence surface (Mem | Sqlite | …)
//!   - `Dispatch`  : the typed action/reply surface (one impl per shell)
//!   - `Handler`   : the reducer (sees `&Store`, returns `Reply`)
//!   - `App<S,D>`  : owns the store + handler, dispatch entrypoint
//!   - `AppService`: tower::Service<Action, Response = Reply>
//!   - `Transport` : in-process or daemon-proxy adapter behind same Service
//!
//! tower::Service is the universal adapter. axum and lsp-server both
//! consume Service-shaped values directly; clap drives the call site
//! synchronously via `.ready().await?.call(action)`.

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::stream::BoxStream;
use tower::Service;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ───────────────────────────────────────────────────────────────────
// Store
// ───────────────────────────────────────────────────────────────────

/// Persistence surface. Real impls: Mem (volatile), Sqlite (durable).
/// `Diff` is whatever the store hands subscribers per change.
pub trait Store: Send + Sync + 'static {
    type Diff: Send + 'static;
    type Tx:   Send + 'static;
    type Row:  Send + 'static;
    type Query;
    type Write;
    type Slice: Copy;

    fn write(&self, w: Self::Write) -> Self::Tx;
    fn read(&self, q: Self::Query) -> Vec<Self::Row>;
    fn subscribe(&self, slice: Self::Slice) -> BoxStream<'static, Self::Diff>;
}

// ───────────────────────────────────────────────────────────────────
// Dispatch
// ───────────────────────────────────────────────────────────────────

/// Typed action/reply contract. One impl per shell:
/// `CliDispatch`, `LspDispatch`, `HttpDispatch`.
pub trait Dispatch: Send + Sync + 'static {
    type Action: Send + 'static;
    type Reply:  Send + 'static;
    type Error:  std::error::Error + Send + Sync + 'static;
}

// ───────────────────────────────────────────────────────────────────
// Handler — the reducer
// ───────────────────────────────────────────────────────────────────

/// Reducer surface. Sees a `&Store`, takes an `Action`, returns a `Reply`.
/// Pure as far as the host is concerned; impurity lives inside the store
/// or in side-channels the handler reaches.
pub trait Handler<S, D>: Send + Sync + 'static
where
    S: Store,
    D: Dispatch,
{
    fn dispatch<'a>(
        &'a self,
        store: &'a S,
        action: D::Action,
    ) -> BoxFuture<'a, Result<D::Reply, D::Error>>;
}

// ───────────────────────────────────────────────────────────────────
// App
// ───────────────────────────────────────────────────────────────────

pub struct App<S, D>
where
    S: Store,
    D: Dispatch,
{
    pub store:   S,
    pub handler: Arc<dyn Handler<S, D>>,
    _d: PhantomData<D>,
}

impl<S, D> App<S, D>
where
    S: Store,
    D: Dispatch,
{
    pub fn new(store: S, handler: Arc<dyn Handler<S, D>>) -> Arc<Self> {
        Arc::new(Self { store, handler, _d: PhantomData })
    }

    pub async fn dispatch(&self, action: D::Action) -> Result<D::Reply, D::Error> {
        self.handler.dispatch(&self.store, action).await
    }
}

// ───────────────────────────────────────────────────────────────────
// AppService — tower::Service adapter
// ───────────────────────────────────────────────────────────────────

pub struct AppService<S, D>
where
    S: Store,
    D: Dispatch,
{
    app: Arc<App<S, D>>,
}

impl<S, D> AppService<S, D>
where
    S: Store,
    D: Dispatch,
{
    pub fn new(app: Arc<App<S, D>>) -> Self { Self { app } }
}

impl<S, D> Clone for AppService<S, D>
where
    S: Store,
    D: Dispatch,
{
    fn clone(&self) -> Self { Self { app: self.app.clone() } }
}

impl<S, D> Service<D::Action> for AppService<S, D>
where
    S: Store,
    D: Dispatch,
{
    type Response = D::Reply;
    type Error    = D::Error;
    type Future   = BoxFuture<'static, Result<D::Reply, D::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, action: D::Action) -> Self::Future {
        let app = self.app.clone();
        Box::pin(async move { app.dispatch(action).await })
    }
}

// ───────────────────────────────────────────────────────────────────
// Transport — in-process or daemon proxy
// ───────────────────────────────────────────────────────────────────

/// Transport gives back a `tower::Service` that takes `Action` and yields
/// `Reply`. In-process holds an `Arc<App>` directly; a daemon proxy would
/// forward over a wire (UnixStream / TCP / WebSocket) to a remote App.
/// Shells switch between the two via a single `Box<dyn Service<…>>`.
pub trait Transport<D: Dispatch>: Send + Sync + 'static {
    type Service: Service<
            D::Action,
            Response = D::Reply,
            Error    = D::Error,
        > + Send + 'static;

    fn service(&self) -> Self::Service;
}

pub struct InProcess<S, D>
where
    S: Store,
    D: Dispatch,
{
    app: Arc<App<S, D>>,
}

impl<S, D> InProcess<S, D>
where
    S: Store,
    D: Dispatch,
{
    pub fn new(app: Arc<App<S, D>>) -> Self { Self { app } }
}

impl<S, D> Transport<D> for InProcess<S, D>
where
    S: Store,
    D: Dispatch,
{
    type Service = AppService<S, D>;
    fn service(&self) -> Self::Service { AppService::new(self.app.clone()) }
}

/// Stub for daemon-side. A real impl forwards `Action` over a socket to a
/// remote `App` running `sprefa serve`. Same `tower::Service` surface;
/// shells stay identical regardless of which transport they hold.
pub struct DaemonProxy<D> {
    pub addr: String,
    _d: PhantomData<D>,
}

impl<D> DaemonProxy<D> {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into(), _d: PhantomData }
    }
}

// (impl Transport<D> for DaemonProxy<D> intentionally omitted —
// requires wire codec + connection pool, lands when sprefa serve is built)

// ───────────────────────────────────────────────────────────────────
// Task-local context
// ───────────────────────────────────────────────────────────────────

// Ambient context that handler-internal code can read without threading
// it through every signature. `App::dispatch` should `CTX.scope(ctx, …)`
// around handler invocations once a sprf-specific context type exists.
tokio::task_local! {
    pub static CTX: Arc<dyn std::any::Any + Send + Sync>;
}
