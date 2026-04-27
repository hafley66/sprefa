//! sprefa-run — thin HTTP CLI. POSTs `/run`, streams SSE, prints rows.
//!
//! Auto-spawns sprefa-server if `server.json` is missing. The actual
//! pipeline drain + SQLite drain runs inside the daemon (`server::run`).

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper::{Request, Uri};
use hyper_util::rt::TokioIo;
use serde::Deserialize;
use tokio::net::{TcpStream, UnixStream};
use tracing_subscriber::EnvFilter;

use server::{default_info_path, ServerInfo};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        eprintln!("usage: sprefa-run <file.sprf> [--root <dir>] [--out <db>] [--no-cache]");
        std::process::exit(2);
    }
    let file = PathBuf::from(&args[0]);
    let mut root: Option<PathBuf> = None;
    let mut out_db: Option<PathBuf> = None;
    let mut cache: bool = true;
    let mut info_path: Option<PathBuf> = None;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root"     => root = it.next().map(PathBuf::from),
            "--out"      => out_db = it.next().map(PathBuf::from),
            "--no-cache" => cache = false,
            "--info"     => info_path = it.next().map(PathBuf::from),
            other => { eprintln!("unknown flag: {other}"); std::process::exit(2); }
        }
    }

    let info_path = info_path.unwrap_or_else(default_info_path);
    let (info, conn, host) = connect_or_spawn(&info_path).await?;

    // Canonicalize `--root` (and the source path) against the CLI's CWD
    // before sending. Otherwise `--root .` flows literally to the daemon
    // and gets resolved against the daemon's CWD, which is wrong.
    let root = root
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p));
    let file_abs = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());

    let source = std::fs::read_to_string(&file)
        .with_context(|| format!("read {}", file.display()))?;
    let body = serde_json::json!({
        "source":         source,
        "source_path":    file_abs,
        "workspace_hint": root,
        "out_db":         out_db,
        "cache":          cache,
    }).to_string();

    post_sse(&info, conn, host, "/run", body, print_frame).await
}

#[derive(Default)]
struct RunState {
    rule_names:      Vec<String>,
    cursors_by_expr: HashMap<String, Vec<CursorDto>>,
    diags:           Vec<(String, String, String)>,
}

#[derive(Deserialize, Debug, Clone)]
struct CursorDto {
    repo:     String,
    rev:      String,
    fs:       Option<String>,
    captures: BTreeMap<String, String>,
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
        RunFrame::Meta { rule_names }            => s.rule_names = rule_names,
        RunFrame::Cursor { expr_name, cursor }   => {
            let key = expr_name.unwrap_or_default();
            s.cursors_by_expr.entry(key).or_default().push(cursor);
        }
        RunFrame::ExprDone { .. }                => {}
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
                        .map(|(k, v)| format!("{k}={v}")).collect();
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

enum Conn { Unix(UnixStream), Tcp(TcpStream) }

async fn try_dial(info_path: &std::path::Path) -> Option<(ServerInfo, Conn, String)> {
    let info = ServerInfo::read(info_path).ok()?;
    if let Some(sock) = &info.http.unix {
        if let Ok(s) = UnixStream::connect(sock).await {
            return Some((info, Conn::Unix(s), "localhost".into()));
        }
    }
    if let Some(addr) = &info.http.tcp {
        if let Ok(s) = TcpStream::connect(addr).await {
            let host = addr.clone();
            return Some((info, Conn::Tcp(s), host));
        }
    }
    None
}

async fn connect_or_spawn(info_path: &std::path::Path)
    -> Result<(ServerInfo, Conn, String)>
{
    if let Some(t) = try_dial(info_path).await { return Ok(t); }
    spawn_daemon(info_path)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(t) = try_dial(info_path).await { return Ok(t); }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(anyhow!("sprefa-server did not become reachable via {} within 5s", info_path.display()))
}

async fn post_sse(
    _info: &ServerInfo,
    conn: Conn,
    host: String,
    path: &str,
    body: String,
    mut cb: impl FnMut(&str) -> bool,
) -> Result<()> {
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
            while let Some(idx) = buf.find("\n\n") {
                let event: String = buf.drain(..=idx + 1).collect();
                for line in event.lines() {
                    if line.is_empty() { continue; }
                    if !cb(line) { return Ok(()); }
                }
            }
        }
    }
    for line in buf.lines() {
        if line.is_empty() { continue; }
        if !cb(line) { return Ok(()); }
    }
    Ok(())
}

async fn send_stream(conn: Conn, req: Request<Full<Bytes>>) -> Result<hyper::body::Incoming> {
    match conn {
        Conn::Unix(s) => {
            let (mut sender, c) = http1::handshake(TokioIo::new(s)).await?;
            tokio::spawn(c);
            Ok(sender.send_request(req).await?.into_body())
        }
        Conn::Tcp(s) => {
            let (mut sender, c) = http1::handshake(TokioIo::new(s)).await?;
            tokio::spawn(c);
            Ok(sender.send_request(req).await?.into_body())
        }
    }
}

fn spawn_daemon(_info_path: &std::path::Path) -> Result<()> {
    let exe = std::env::current_exe().context("locate sprefa-run")?;
    let server_bin = exe.parent()
        .map(|p| p.join("sprefa-server"))
        .filter(|p| p.exists())
        .or_else(|| which_in_path("sprefa-server"))
        .ok_or_else(|| anyhow!("sprefa-server not found"))?;

    eprintln!("auto-spawning sprefa-server ({})", server_bin.display());
    std::process::Command::new(&server_bin)
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().context("spawn sprefa-server")?;
    Ok(())
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let c = dir.join(name);
        if c.is_file() { return Some(c); }
    }
    None
}
