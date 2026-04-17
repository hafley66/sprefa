//! sprefa — thin HTTP CLI. Dials sprefa-server (unix socket preferred),
//! POSTs /run, streams SSE, prints `rule NAME — N rows` + captures.
//!
//! Auto-spawn of the server is a future hook (D5). Today: error if no
//! server.json or dial fails.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper::{Request, Uri};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::net::{TcpStream, UnixStream};

use v2::server::{default_info_path, ServerInfo};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "sprefa", about = "sprefa HTTP CLI")]
struct Cli {
    /// Override server.json location (otherwise default_info_path()).
    #[arg(long)]
    info_path: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Run {
        file: PathBuf,
        /// Workspace hint. Defaults to file.parent().
        #[arg(long)]
        root: Option<PathBuf>,
    },
    Status,
    Stop,
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let info_path = cli.info_path.unwrap_or_else(default_info_path);
    let info = ServerInfo::read(&info_path)
        .with_context(|| format!("read server.json at {}", info_path.display()))?;

    match cli.cmd {
        Cmd::Status => {
            let body = get(&info, "/status").await?;
            println!("{body}");
        }
        Cmd::Stop => {
            let body = post_json(&info, "/shutdown", "{}".to_string()).await?;
            println!("{body}");
        }
        Cmd::Run { file, root } => {
            let source = std::fs::read_to_string(&file)
                .with_context(|| format!("read {}", file.display()))?;
            let req = serde_json::json!({
                "source":         source,
                "source_path":    file,
                "workspace_hint": root,
            });
            post_sse(&info, "/run", req.to_string(), print_frame).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SSE frame handling
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct RunState {
    rule_names:      Vec<String>,
    cursors_by_expr: HashMap<String, Vec<CursorDto>>,
    diags:           Vec<(String, String, String)>,
}

#[derive(Deserialize, Debug)]
struct CursorDto {
    repo:     String,
    rev:      String,
    fs:       Option<String>,
    captures: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RunFrame {
    Meta     { rule_names: Vec<String> },
    Cursor   { expr_name: Option<String>, cursor: CursorDto },
    ExprDone { expr_name: Option<String> },
    Diag     { code: String, severity: String, message: String },
    Done,
}

static STATE: std::sync::LazyLock<std::sync::Mutex<RunState>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(RunState::default()));

fn print_frame(line: &str) -> bool {
    // SSE lines start with "data: "; strip the prefix, decode JSON, accumulate.
    let Some(json) = line.strip_prefix("data: ") else { return true; };
    let frame: RunFrame = match serde_json::from_str(json) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[sprefa] decode error: {e}; line={json}");
            return true;
        }
    };
    let mut s = STATE.lock().unwrap();
    match frame {
        RunFrame::Meta { rule_names }           => s.rule_names = rule_names,
        RunFrame::Cursor { expr_name, cursor }  => {
            let key = expr_name.unwrap_or_default();
            s.cursors_by_expr.entry(key).or_default().push(cursor);
        }
        RunFrame::ExprDone { .. }               => {}
        RunFrame::Diag { code, severity, message } => {
            s.diags.push((severity, code, message));
        }
        RunFrame::Done => {
            let names = std::mem::take(&mut s.rule_names);
            let by_expr = std::mem::take(&mut s.cursors_by_expr);
            let diags = std::mem::take(&mut s.diags);
            drop(s);
            for (sev, code, msg) in &diags {
                eprintln!("[{sev} {code}] {msg}");
            }
            for name in &names {
                let cursors = by_expr.get(name).map(|v| v.as_slice()).unwrap_or(&[]);
                let display = if name.is_empty() { "?" } else { name.as_str() };
                println!("rule {display} — {} rows", cursors.len());
                for c in cursors {
                    let pairs: Vec<String> = c.captures.iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect();
                    let kv = pairs.join(", ");
                    let loc = c.fs.as_deref().map(|p| format!(" {p}")).unwrap_or_default();
                    println!("  {}@{}{}: {kv}", c.repo, c.rev, loc);
                }
            }
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Transport — dial unix if available, fall back to TCP
// ---------------------------------------------------------------------------

enum Conn {
    Unix(UnixStream),
    Tcp(TcpStream),
}

async fn dial(info: &ServerInfo) -> Result<(Conn, String)> {
    if let Some(sock) = &info.http.unix {
        if let Ok(s) = UnixStream::connect(sock).await {
            return Ok((Conn::Unix(s), "localhost".to_string()));
        }
    }
    if let Some(addr) = &info.http.tcp {
        let s = TcpStream::connect(addr).await
            .with_context(|| format!("tcp connect {addr}"))?;
        return Ok((Conn::Tcp(s), addr.clone()));
    }
    Err(anyhow!("server.json lists no reachable transport"))
}

async fn get(info: &ServerInfo, path: &str) -> Result<String> {
    let (conn, host) = dial(info).await?;
    let req = Request::builder()
        .method("GET")
        .uri(path.parse::<Uri>()?)
        .header("host", host)
        .body(Full::<Bytes>::new(Bytes::new()))?;
    send_collect(conn, req).await
}

async fn post_sse(
    info:    &ServerInfo,
    path:    &str,
    body:    String,
    mut cb:  impl FnMut(&str) -> bool,
) -> Result<()> {
    let (conn, host) = dial(info).await?;
    let req = Request::builder()
        .method("POST")
        .uri(path.parse::<Uri>()?)
        .header("host", host)
        .header("content-type", "application/json")
        .header("accept", "text/event-stream")
        .body(Full::<Bytes>::new(Bytes::from(body)))?;

    let mut resp = send_stream(conn, req).await?;
    let mut buf = String::new();
    while let Some(frame) = resp.frame().await {
        let frame = frame?;
        if let Some(data) = frame.data_ref() {
            buf.push_str(std::str::from_utf8(data)?);
            // Split on blank-line SSE event boundary.
            while let Some(idx) = buf.find("\n\n") {
                let event: String = buf.drain(..=idx + 1).collect();
                for line in event.lines() {
                    if line.is_empty() { continue; }
                    if !cb(line) { return Ok(()); }
                }
            }
        }
    }
    // Flush any tail.
    for line in buf.lines() {
        if line.is_empty() { continue; }
        if !cb(line) { return Ok(()); }
    }
    Ok(())
}

async fn post_json(info: &ServerInfo, path: &str, body: String) -> Result<String> {
    let (conn, host) = dial(info).await?;
    let req = Request::builder()
        .method("POST")
        .uri(path.parse::<Uri>()?)
        .header("host", host)
        .header("content-type", "application/json")
        .body(Full::<Bytes>::new(Bytes::from(body)))?;
    send_collect(conn, req).await
}

async fn send_collect(conn: Conn, req: Request<Full<Bytes>>) -> Result<String> {
    let resp = match conn {
        Conn::Unix(s) => {
            let (mut sender, c) = http1::handshake(TokioIo::new(s)).await?;
            tokio::spawn(c);
            sender.send_request(req).await?
        }
        Conn::Tcp(s) => {
            let (mut sender, c) = http1::handshake(TokioIo::new(s)).await?;
            tokio::spawn(c);
            sender.send_request(req).await?
        }
    };
    let bytes = resp.into_body().collect().await?.to_bytes();
    Ok(String::from_utf8(bytes.to_vec())?)
}

async fn send_stream(conn: Conn, req: Request<Full<Bytes>>) -> Result<hyper::body::Incoming> {
    match conn {
        Conn::Unix(s) => {
            let (mut sender, c) = http1::handshake(TokioIo::new(s)).await?;
            tokio::spawn(c);
            let resp = sender.send_request(req).await?;
            Ok(resp.into_body())
        }
        Conn::Tcp(s) => {
            let (mut sender, c) = http1::handshake(TokioIo::new(s)).await?;
            tokio::spawn(c);
            let resp = sender.send_request(req).await?;
            Ok(resp.into_body())
        }
    }
}

// Arc parameter is unused; kept so future hot-reload logic can cache a
// shared ServerInfo without touching call sites.
#[allow(dead_code)]
fn _shared_info_stub(_: Arc<ServerInfo>) {}
