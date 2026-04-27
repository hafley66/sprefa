//! sprefa-server — single binary, two modes.
//!
//! Default (HTTP daemon): owns `ServerState`, serves `/status`,
//! `/run` (SSE), `/lsp` (WS), `/shutdown` over unix socket + TCP.
//! Writes `server.json` on startup so `sprefa-run` can auto-find it.
//!
//! `--lsp-stdio`: editor-spawned mode. Builds `ServerState`, runs
//! `tower_lsp::Server` directly on stdin/stdout. No HTTP, no
//! `server.json`, no WS round-trip. The previous separate
//! `sprefa-lsp` proxy binary was redundant — same code path, same
//! crate; running it as a dedicated bin only created profile-skew
//! ("debug sprefa-lsp talking to release sprefa-server" hover
//! timeouts and similar).

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use chrono::Utc;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use server::{
    default_info_path, default_log_path, default_unix_socket_path, serve_http,
    Backend, HttpInfo, HttpOpts, ServerInfo, ServerOpts, ServerState,
};
use tower_lsp::{LspService, Server as LspServer};

const DEFAULT_TCP: &str = "127.0.0.1:7417";

#[derive(Parser, Debug)]
#[command(name = "sprefa-server", about = "sprefa long-lived HTTP daemon")]
struct Cli {
    /// Unix socket path. Default: $XDG_RUNTIME_DIR/sprefa/sprefa.sock.
    #[arg(long)]
    http_unix: Option<PathBuf>,

    #[arg(long)]
    no_unix: bool,

    /// TCP bind address. Default: 127.0.0.1:7417.
    #[arg(long)]
    http_tcp: Option<SocketAddr>,

    #[arg(long)]
    no_tcp: bool,

    /// server.json path.
    #[arg(long)]
    info_path: Option<PathBuf>,

    /// Log file path.
    #[arg(long)]
    log_path: Option<PathBuf>,

    /// Mirror tracing to stderr (else daemonized-quiet).
    #[arg(long)]
    foreground: bool,

    /// Run as an LSP server on stdio. Skips HTTP, server.json, and
    /// the daemon listen loop. Editors should spawn this mode.
    #[arg(long)]
    lsp_stdio: bool,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    if cli.lsp_stdio {
        let http_unix = if cli.no_unix { None } else {
            Some(cli.http_unix.clone().unwrap_or_else(default_unix_socket_path))
        };
        let http_tcp = if cli.no_tcp { None } else {
            Some(cli.http_tcp.unwrap_or_else(|| DEFAULT_TCP.parse().unwrap()))
        };
        let info_path = cli.info_path.clone().unwrap_or_else(default_info_path);
        return run_lsp_stdio(http_unix, http_tcp, info_path).await;
    }

    let opts = ServerOpts {
        http_unix:  if cli.no_unix { None } else {
            Some(cli.http_unix.unwrap_or_else(default_unix_socket_path))
        },
        http_tcp:   if cli.no_tcp  { None } else {
            Some(cli.http_tcp.unwrap_or_else(|| DEFAULT_TCP.parse().unwrap()))
        },
        log_path:   cli.log_path.or_else(|| Some(default_log_path())),
        info_path:  cli.info_path.or_else(|| Some(default_info_path())),
        foreground: cli.foreground,
    };

    if opts.http_unix.is_none() && opts.http_tcp.is_none() {
        return Err(anyhow!("no listeners: both --no-unix and --no-tcp are set"));
    }

    let state = ServerState::new(&opts).await?;

    let info = ServerInfo {
        pid:        std::process::id(),
        version:    env!("CARGO_PKG_VERSION").to_string(),
        started_at: Utc::now().to_rfc3339(),
        http: HttpInfo {
            unix: opts.http_unix.clone(),
            tcp:  opts.http_tcp.map(|a| a.to_string()),
        },
        log_path: opts.log_path.clone(),
    };

    let http_opts = HttpOpts { unix: opts.http_unix.clone(), tcp: opts.http_tcp };

    // Spawn the server first; only write server.json once every listener
    // is bound and accepting. Otherwise sprefa-lsp/sprefa-run can observe
    // the locator before the socket exists and hit ECONNREFUSED.
    let serve_state = state.clone();
    let mut serve = tokio::spawn(async move { serve_http(serve_state, http_opts).await });

    let ready = state.ready.clone();
    tokio::select! {
        _ = ready.notified() => {
            ServerInfo::write_atomic(&state.info_path, &info)?;
            tracing::info!(info_path = %state.info_path.display(), "server.json written");
        }
        res = &mut serve => {
            // serve_http exited before signaling ready: bind failure.
            res??;
            anyhow::bail!("serve_http exited before becoming ready");
        }
    }

    tokio::select! {
        res = &mut serve => { res??; }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("sigint received");
        }
        _ = wait_sigterm() => {
            tracing::info!("sigterm received");
        }
        _ = state.cancel_root.cancelled() => {
            tracing::info!("cancel_root tripped");
        }
    }

    state.shutdown().await;
    Ok(())
}

async fn wait_sigterm() {
    use tokio::signal::unix::{signal, SignalKind};
    if let Ok(mut s) = signal(SignalKind::terminate()) { s.recv().await; }
    else { std::future::pending::<()>().await; }
}

async fn run_lsp_stdio(
    http_unix: Option<PathBuf>,
    http_tcp:  Option<SocketAddr>,
    info_path: PathBuf,
) -> Result<()> {
    // Editor-owned process: stdio LSP is the primary job. We also try
    // to bind HTTP so the same process serves /run for sprefa-run —
    // best-effort. If another daemon already holds the socket/port,
    // serve_http exits early and we keep going LSP-only.
    let opts = ServerOpts {
        http_unix:  http_unix.clone(),
        http_tcp,
        log_path:   None,
        info_path:  Some(info_path.clone()),
        foreground: true,
    };
    let state = ServerState::new(&opts).await?;

    if http_unix.is_some() || http_tcp.is_some() {
        let serve_state = state.clone();
        let serve_opts  = HttpOpts { unix: http_unix.clone(), tcp: http_tcp };
        let info = ServerInfo {
            pid:        std::process::id(),
            version:    env!("CARGO_PKG_VERSION").to_string(),
            started_at: Utc::now().to_rfc3339(),
            http: HttpInfo {
                unix: http_unix.clone(),
                tcp:  http_tcp.map(|a| a.to_string()),
            },
            log_path: None,
        };
        let ready = state.ready.clone();
        let info_for_writer = info_path.clone();
        tokio::spawn(async move {
            let mut serve = tokio::spawn(async move {
                serve_http(serve_state, serve_opts).await
            });
            tokio::select! {
                _ = ready.notified() => {
                    if let Err(e) = ServerInfo::write_atomic(&info_for_writer, &info) {
                        tracing::warn!(err = %e, "lsp_stdio: server.json write failed");
                    } else {
                        tracing::info!(
                            info_path = %info_for_writer.display(),
                            "lsp_stdio: HTTP also serving; server.json written"
                        );
                    }
                    // Keep serve future alive; pin it to this task.
                    let _ = serve.await;
                }
                res = &mut serve => {
                    match res {
                        Ok(Err(e)) => tracing::info!(
                            err = %e,
                            "lsp_stdio: HTTP not bound (likely another daemon); LSP-only"
                        ),
                        Ok(Ok(())) => {}
                        Err(e) => tracing::warn!(err = %e, "lsp_stdio: serve_http join error"),
                    }
                }
            }
        });
    }

    let stdin  = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) =
        LspService::new(move |client| Backend::with_state(client, state.clone()));
    LspServer::new(stdin, stdout, socket).serve(service).await;
    Ok(())
}
