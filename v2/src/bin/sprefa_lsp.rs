//! sprefa-lsp — stdio ↔ WebSocket proxy to sprefa-server's /lsp endpoint.
//!
//! Editors spawn this binary over stdio expecting a normal LSP stdio
//! server. Bytes are shuttled verbatim onto a WebSocket binary channel;
//! the server reassembles them into one stream and feeds tower-lsp's
//! Content-Length framer. No re-framing happens here.
//!
//! Transport priority: unix socket if server.json lists one and we can
//! connect, else TCP. Autospawning the server is D5.

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::protocol::Message;

use v2::server::{default_info_path, ServerInfo};

mod tokio_io {
    use tokio::io::{AsyncRead, AsyncWrite};
    pub trait AsyncIo: AsyncRead + AsyncWrite + Unpin + Send {}
    impl<T: AsyncRead + AsyncWrite + Unpin + Send + ?Sized> AsyncIo for T {}
}

#[derive(Parser)]
#[command(name = "sprefa-lsp", about = "stdio ↔ sprefa-server /lsp proxy")]
struct Cli {
    #[arg(long)]
    info_path: Option<PathBuf>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let info_path = cli.info_path.unwrap_or_else(default_info_path);
    let info = ServerInfo::read(&info_path)
        .with_context(|| format!("read server.json at {}", info_path.display()))?;

    // Build a WS upgrade request with a plausible Host + path.
    let req = Request::builder()
        .method("GET")
        .uri("ws://sprefa/lsp")
        .header("host",                  "sprefa")
        .header("upgrade",               "websocket")
        .header("connection",            "Upgrade")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key",     generate_key())
        .body(())?;

    // Try unix, fall back to TCP. Box the stream so both branches share a type.
    type Io = Box<dyn tokio_io::AsyncIo>;
    let stream: Io = if let Some(sock) = &info.http.unix {
        match UnixStream::connect(sock).await {
            Ok(s) => Box::new(s),
            Err(_) => if let Some(addr) = &info.http.tcp {
                Box::new(TcpStream::connect(addr).await?)
            } else {
                return Err(anyhow!("unix dial failed and no tcp fallback"));
            }
        }
    } else if let Some(addr) = &info.http.tcp {
        Box::new(TcpStream::connect(addr).await?)
    } else {
        return Err(anyhow!("server.json lists no transport"));
    };
    let (ws_stream, _resp) = tokio_tungstenite::client_async(req, stream).await?;

    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // stdin → ws.
    let inbound = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf   = vec![0u8; 16 << 10];
        loop {
            let n = match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n)          => n,
            };
            if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() { break; }
        }
        let _ = ws_tx.close().await;
    });

    // ws → stdout.
    let mut stdout = tokio::io::stdout();
    while let Some(msg) = ws_rx.next().await {
        let msg = msg?;
        match msg {
            Message::Binary(b) => { stdout.write_all(&b).await?; stdout.flush().await?; }
            Message::Text(t)   => { stdout.write_all(t.as_bytes()).await?; stdout.flush().await?; }
            Message::Close(_)  => break,
            _ => {}
        }
    }

    inbound.abort();
    Ok(())
}
