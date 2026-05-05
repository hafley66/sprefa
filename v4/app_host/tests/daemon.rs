//! Daemon round-trip: spin up `serve_unix`, connect via `DaemonProxy`,
//! verify Action → Reply over the wire and that server-side errors
//! propagate as WireError::Server.

use std::sync::Arc;
use std::time::Duration;

use app_host::{
    daemon::{serve_unix, DaemonProxy},
    App, BoxFuture, Dispatch, Handler, Store, Transport, WireError,
};
use futures::stream::{self, BoxStream};
use tower::ServiceExt;

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
enum EchoError {
    #[error("wire: {0}")]
    Wire(#[from] WireError),
    #[error("bad action")]
    Bad,
}

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
        _store: &'a NoopStore,
        action: String,
    ) -> BoxFuture<'a, Result<String, EchoError>> {
        Box::pin(async move {
            if action == "BAD" { Err(EchoError::Bad) }
            else                { Ok(format!("echo:{action}")) }
        })
    }
}

// ─── helpers ───────────────────────────────────────────────────────

async fn await_socket(p: &std::path::Path) {
    for _ in 0..200 {
        if p.exists() { return; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("socket never appeared at {p:?}");
}

fn spawn_server(sock: std::path::PathBuf) -> tokio::task::JoinHandle<()> {
    let app = App::<NoopStore, EchoDispatch>::new(NoopStore, Arc::new(EchoHandler));
    tokio::spawn(async move {
        serve_unix(app, sock).await.unwrap();
    })
}

// ─── tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn daemon_roundtrip() {
    let dir  = tempfile::tempdir().unwrap();
    let sock = dir.path().join("echo.sock");

    let server = spawn_server(sock.clone());
    await_socket(&sock).await;

    let svc = DaemonProxy::<EchoDispatch>::new(sock).service();
    let reply = svc.oneshot("over wire".to_string()).await.unwrap();
    assert_eq!(reply, "echo:over wire");

    server.abort();
}

#[tokio::test]
async fn server_error_propagates_as_wire_server() {
    let dir  = tempfile::tempdir().unwrap();
    let sock = dir.path().join("err.sock");

    let server = spawn_server(sock.clone());
    await_socket(&sock).await;

    let svc = DaemonProxy::<EchoDispatch>::new(sock).service();
    let err = svc.oneshot("BAD".to_string()).await.unwrap_err();

    match err {
        EchoError::Wire(WireError::Server(msg)) => {
            assert!(msg.contains("bad action"), "got: {msg}");
        }
        other => panic!("expected Wire(Server), got {other:?}"),
    }

    server.abort();
}

#[tokio::test]
async fn many_calls_one_proxy() {
    let dir  = tempfile::tempdir().unwrap();
    let sock = dir.path().join("many.sock");

    let server = spawn_server(sock.clone());
    await_socket(&sock).await;

    let proxy = DaemonProxy::<EchoDispatch>::new(sock);

    for i in 0..10 {
        let svc = proxy.service();
        let r = svc.oneshot(format!("n{i}")).await.unwrap();
        assert_eq!(r, format!("echo:n{i}"));
    }

    server.abort();
}
