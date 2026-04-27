//! sprefa-lsp — stdio ↔ WebSocket proxy to sprefa-server's /lsp endpoint.
//!
//! Editors spawn this binary expecting stdio LSP. Bytes shuttle
//! verbatim onto a WS binary channel; the server reassembles them
//! into one stream for tower-lsp's Content-Length framer. No
//! re-framing here.
//!
//! Transport: unix socket if `server.json` lists one and we can
//! connect, else TCP. Auto-spawns sprefa-server if `server.json` is
//! missing.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UnixStream};
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::http::Request;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing_subscriber::EnvFilter;

use server::{default_info_path, ServerInfo};

mod io_dyn {
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
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let info_path = cli.info_path.unwrap_or_else(default_info_path);

    let info = match ServerInfo::read(&info_path) {
        Ok(i) => i,
        Err(_) => spawn_daemon_and_wait(&info_path)?,
    };

    let req = Request::builder()
        .method("GET").uri("ws://sprefa/lsp")
        .header("host", "sprefa")
        .header("upgrade", "websocket")
        .header("connection", "Upgrade")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", generate_key())
        .body(())?;

    type Io = Box<dyn io_dyn::AsyncIo>;
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

    let inbound = tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let mut buf = vec![0u8; 16 << 10];
        loop {
            let n = match stdin.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if ws_tx.send(Message::Binary(buf[..n].to_vec().into())).await.is_err() { break; }
        }
        let _ = ws_tx.close().await;
    });

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

fn spawn_daemon_and_wait(info_path: &std::path::Path) -> Result<ServerInfo> {
    let exe = std::env::current_exe()
        .context("locate sprefa-lsp executable")?;
    let server_bin = exe.parent()
        .map(|p| p.join("sprefa-server"))
        .filter(|p| p.exists())
        .or_else(|| which_in_path("sprefa-server"))
        .ok_or_else(|| anyhow!("sprefa-server not found alongside sprefa-lsp or in PATH"))?;

    tracing::info!(server = %server_bin.display(), "auto-spawning sprefa-server");
    std::process::Command::new(&server_bin)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().context("spawn sprefa-server")?;

    // Poll for server.json up to 5s.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Ok(info) = ServerInfo::read(info_path) {
            return Ok(info);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(anyhow!("sprefa-server did not write {} within 5s", info_path.display()))
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() { return Some(candidate); }
    }
    None
}
