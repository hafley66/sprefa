//! Daemon transport. Forwards `D::Action` from a client to a remote
//! `App<S, D>` over a UnixStream and returns `D::Reply`. Same
//! `tower::Service<Action>` surface as `InProcess`, so the caller stays
//! transport-blind.
//!
//! Wire format (one round-trip per Service::call):
//!
//! ```text
//! ┌────────────┬──────────────────────────────────┐
//! │  u32 LE    │  bincode body                    │
//! │  length    │  Action  (request)               │
//! │            │  WireResponse<Reply> (response)  │
//! └────────────┴──────────────────────────────────┘
//! ```
//!
//! No pooling, no reconnect. Simplest path.
//!
//! Bounds note: the `Dispatch` assoc-type bounds (Serialize, etc.) are
//! repeated at each use site. Rust's where-clauses on a supertrait do
//! not propagate to user code through `D: DaemonDispatch` alone, so a
//! marker trait would buy nothing on stable. The bounds live inline.

use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;
use std::task::{Context, Poll};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tower::Service;

use crate::{App, BoxFuture, Dispatch, Store, Transport};

// ───────────────────────────────────────────────────────────────────
// Errors
// ───────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("codec: {0}")]
    Codec(String),
    #[error("server: {0}")]
    Server(String),
}

#[derive(Serialize, Deserialize)]
enum WireResponse<R> {
    Ok(R),
    Err(String),
}

/// Documentation alias. Marks a Dispatch whose Action/Reply/Error
/// satisfy the daemon-side bounds. Use sites still inline the bounds.
pub trait DaemonDispatch:
    Dispatch<
        Action = <Self as DaemonDispatch>::Act,
        Reply  = <Self as DaemonDispatch>::Rep,
    >
{
    type Act: Serialize + DeserializeOwned + Send + 'static;
    type Rep: Serialize + DeserializeOwned + Send + 'static;
}

// ───────────────────────────────────────────────────────────────────
// Framing
// ───────────────────────────────────────────────────────────────────

async fn read_framed(s: &mut UnixStream) -> Result<Vec<u8>, std::io::Error> {
    let mut len_buf = [0u8; 4];
    s.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    s.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_framed(s: &mut UnixStream, bytes: &[u8]) -> Result<(), std::io::Error> {
    let len = (bytes.len() as u32).to_le_bytes();
    s.write_all(&len).await?;
    s.write_all(bytes).await?;
    s.flush().await?;
    Ok(())
}

// ───────────────────────────────────────────────────────────────────
// Proxy — client side
// ───────────────────────────────────────────────────────────────────

pub struct DaemonProxy<D> {
    socket: PathBuf,
    _d: PhantomData<D>,
}

impl<D> DaemonProxy<D> {
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self { socket: socket.into(), _d: PhantomData }
    }
}

impl<D> Transport<D> for DaemonProxy<D>
where
    D: Dispatch + 'static,
    D::Action: Serialize + DeserializeOwned + Send + 'static,
    D::Reply:  Serialize + DeserializeOwned + Send + 'static,
    D::Error:  From<WireError>,
{
    type Service = DaemonProxyService<D>;
    fn service(&self) -> Self::Service {
        DaemonProxyService {
            socket: self.socket.clone(),
            _d: PhantomData,
        }
    }
}

pub struct DaemonProxyService<D> {
    socket: PathBuf,
    _d: PhantomData<D>,
}

impl<D> Clone for DaemonProxyService<D> {
    fn clone(&self) -> Self {
        Self { socket: self.socket.clone(), _d: PhantomData }
    }
}

impl<D> Service<D::Action> for DaemonProxyService<D>
where
    D: Dispatch + 'static,
    D::Action: Serialize + DeserializeOwned + Send + 'static,
    D::Reply:  Serialize + DeserializeOwned + Send + 'static,
    D::Error:  From<WireError>,
{
    type Response = D::Reply;
    type Error    = D::Error;
    type Future   = BoxFuture<'static, Result<D::Reply, D::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, action: D::Action) -> Self::Future {
        let socket = self.socket.clone();
        Box::pin(async move {
            let mut conn = UnixStream::connect(&socket)
                .await
                .map_err(|e| D::Error::from(WireError::Io(e)))?;

            let req = bincode::serialize(&action)
                .map_err(|e| D::Error::from(WireError::Codec(e.to_string())))?;
            write_framed(&mut conn, &req)
                .await
                .map_err(|e| D::Error::from(WireError::Io(e)))?;

            let resp_bytes = read_framed(&mut conn)
                .await
                .map_err(|e| D::Error::from(WireError::Io(e)))?;
            let resp: WireResponse<D::Reply> = bincode::deserialize(&resp_bytes)
                .map_err(|e| D::Error::from(WireError::Codec(e.to_string())))?;

            match resp {
                WireResponse::Ok(r)  => Ok(r),
                WireResponse::Err(s) => Err(D::Error::from(WireError::Server(s))),
            }
        })
    }
}

// ───────────────────────────────────────────────────────────────────
// Serve — server side
// ───────────────────────────────────────────────────────────────────

/// Bind a UnixListener at `socket`, accept connections, dispatch each
/// inbound Action through the App, write the Reply back. Loops until
/// listener errors. One spawned task per connection.
pub async fn serve_unix<S, D>(
    app: Arc<App<S, D>>,
    socket: PathBuf,
) -> std::io::Result<()>
where
    S: Store,
    D: Dispatch + 'static,
    D::Action: Serialize + DeserializeOwned + Send + 'static,
    D::Reply:  Serialize + DeserializeOwned + Send + 'static,
{
    if socket.exists() {
        let _ = std::fs::remove_file(&socket);
    }
    let listener = UnixListener::bind(&socket)?;
    loop {
        let (mut conn, _) = listener.accept().await?;
        let app = app.clone();
        tokio::spawn(async move {
            let req_bytes = match read_framed(&mut conn).await {
                Ok(b) => b,
                Err(_) => return,
            };
            let action: D::Action = match bincode::deserialize(&req_bytes) {
                Ok(a) => a,
                Err(e) => {
                    let resp = WireResponse::<D::Reply>::Err(format!("decode: {e}"));
                    let bytes = bincode::serialize(&resp).unwrap_or_default();
                    let _ = write_framed(&mut conn, &bytes).await;
                    return;
                }
            };
            let resp: WireResponse<D::Reply> = match app.dispatch(action).await {
                Ok(r)  => WireResponse::Ok(r),
                Err(e) => WireResponse::Err(format!("{e}")),
            };
            let bytes = match bincode::serialize(&resp) {
                Ok(b) => b,
                Err(_) => return,
            };
            let _ = write_framed(&mut conn, &bytes).await;
        });
    }
}
