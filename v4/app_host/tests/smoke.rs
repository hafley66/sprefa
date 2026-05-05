//! End-to-end: build an App with a noop store + echo handler, run an
//! Action through the in-process transport's Service. Verifies the full
//! generic shape compiles and the tower::Service plumbing works.

use std::sync::Arc;

use app_host::{
    App, AppService, BoxFuture, Dispatch, Handler, InProcess, Store, Transport,
};
use futures::stream::{self, BoxStream};
use tower::{Service, ServiceExt};

// ─── store ─────────────────────────────────────────────────────────

struct NoopStore;

impl Store for NoopStore {
    type Diff  = ();
    type Tx    = ();
    type Row   = ();
    type Query = ();
    type Write = ();
    type Slice = u64;

    fn write(&self, _: ()) {}
    fn read(&self, _: ()) -> Vec<()> { vec![] }
    fn subscribe(&self, _: u64) -> BoxStream<'static, ()> {
        Box::pin(stream::empty())
    }
}

// ─── dispatch ──────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
#[error("echo error")]
struct EchoError;

struct EchoDispatch;

impl Dispatch for EchoDispatch {
    type Action = String;
    type Reply  = String;
    type Error  = EchoError;
}

// ─── handler ───────────────────────────────────────────────────────

struct EchoHandler;

impl Handler<NoopStore, EchoDispatch> for EchoHandler {
    fn dispatch<'a>(
        &'a self,
        _store:  &'a NoopStore,
        action:  String,
    ) -> BoxFuture<'a, Result<String, EchoError>> {
        Box::pin(async move { Ok(format!("echo:{action}")) })
    }
}

// ─── tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn inprocess_roundtrip() {
    let app = App::<NoopStore, EchoDispatch>::new(NoopStore, Arc::new(EchoHandler));
    let svc = InProcess::new(app).service();
    let reply = svc.oneshot("hi".to_string()).await.unwrap();
    assert_eq!(reply, "echo:hi");
}

#[tokio::test]
async fn service_clones_share_app() {
    let app = App::<NoopStore, EchoDispatch>::new(NoopStore, Arc::new(EchoHandler));
    let transport = InProcess::new(app);

    let s1 = transport.service();
    let s2 = transport.service();

    let r1 = s1.oneshot("a".to_string()).await.unwrap();
    let r2 = s2.oneshot("b".to_string()).await.unwrap();
    assert_eq!(r1, "echo:a");
    assert_eq!(r2, "echo:b");
}

#[tokio::test]
async fn boxed_service_unifies_transport_choice() {
    // Demonstrates the shell-side branch: pick transport at boot, then
    // store as Box<dyn Service<…>>; rest of shell code is transport-blind.
    let app = App::<NoopStore, EchoDispatch>::new(NoopStore, Arc::new(EchoHandler));
    let mut svc: AppService<NoopStore, EchoDispatch> =
        InProcess::new(app).service();
    let reply = ServiceExt::ready(&mut svc)
        .await
        .unwrap()
        .call("boxed".to_string())
        .await
        .unwrap();
    assert_eq!(reply, "echo:boxed");
}
