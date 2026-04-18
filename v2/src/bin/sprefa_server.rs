//! sprefa-server — long-lived HTTP server.
//!
//! D2 slice: ServerState + /status over unix socket and/or TCP.
//! Writes $XDG_RUNTIME_DIR/sprefa/server.json on startup for shell auto-find.
//! Future slices add /run SSE, /lsp WebSocket, watcher, registered rules.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::Utc;
use clap::Parser;
use v2::server::{
    default_info_path, default_log_path, default_store_path, default_unix_socket_path,
    serve_http, HttpInfo, HttpOpts, ServerInfo, ServerOpts, ServerState,
};

// ---------------------------------------------------------------------------
// Argv
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "sprefa-server", about = "sprefa long-lived HTTP server")]
struct Cli {
    /// SQLite store path. Default: $XDG_STATE_HOME/sprefa/store.sqlite.
    #[arg(long)]
    store_path: Option<PathBuf>,

    /// Unix socket path. Default: $XDG_RUNTIME_DIR/sprefa/sprefa.sock.
    /// Use --no-unix to disable.
    #[arg(long)]
    http_unix: Option<PathBuf>,

    #[arg(long)]
    no_unix: bool,

    /// TCP bind address. Default: 127.0.0.1:7417.
    /// Use --no-tcp to disable.
    #[arg(long)]
    http_tcp: Option<SocketAddr>,

    #[arg(long)]
    no_tcp: bool,

    /// server.json path. Default: $XDG_RUNTIME_DIR/sprefa/server.json.
    #[arg(long)]
    info_path: Option<PathBuf>,

    /// Log file path. Default: $XDG_STATE_HOME/sprefa/server.log.
    #[arg(long)]
    log_path: Option<PathBuf>,

    /// Run attached (mirror tracing to stderr).
    #[arg(long)]
    foreground: bool,
}

const DEFAULT_TCP: &str = "127.0.0.1:7417";

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let _periscope = v2::_telemetry::init();
    let _ = cli.foreground;

    let opts = ServerOpts {
        store_path: cli.store_path.clone().or_else(|| Some(default_store_path())),
        http_unix:  if cli.no_unix { None } else {
            Some(cli.http_unix.clone().unwrap_or_else(default_unix_socket_path))
        },
        http_tcp: if cli.no_tcp { None } else {
            Some(cli.http_tcp.unwrap_or_else(|| DEFAULT_TCP.parse().unwrap()))
        },
        log_path:   cli.log_path.clone().or_else(|| Some(default_log_path())),
        info_path:  cli.info_path.clone().or_else(|| Some(default_info_path())),
        foreground: cli.foreground,
    };

    if opts.http_unix.is_none() && opts.http_tcp.is_none() {
        return Err(anyhow!("no listeners: both --no-unix and --no-tcp are set"));
    }

    let state = Arc::new(ServerState::new(&opts).await?);

    // Write server.json so shells can auto-find us.
    let info = ServerInfo {
        pid:        std::process::id(),
        version:    env!("CARGO_PKG_VERSION").to_string(),
        started_at: Utc::now().to_rfc3339(),
        http: HttpInfo {
            unix: opts.http_unix.clone(),
            tcp:  opts.http_tcp.map(|a| a.to_string()),
        },
        store_path: opts.store_path.clone(),
        log_path:   opts.log_path.clone(),
    };
    ServerInfo::write_atomic(&state.info_path, &info)?;
    tracing::info!(info_path = %state.info_path.display(), "server.json written");

    let http_opts = HttpOpts {
        unix: opts.http_unix.clone(),
        tcp:  opts.http_tcp,
    };

    tokio::select! {
        res = serve_http(state.clone(), http_opts) => {
            res?;
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("sigint received; shutting down");
        }
        _ = wait_sigterm() => {
            tracing::info!("sigterm received; shutting down");
        }
        _ = state.cancel_root.cancelled() => {
            tracing::info!("cancel_root tripped; shutting down");
        }
    }

    state.shutdown().await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tracing
// ---------------------------------------------------------------------------

async fn wait_sigterm() {
    use tokio::signal::unix::{signal, SignalKind};
    if let Ok(mut s) = signal(SignalKind::terminate()) {
        s.recv().await;
    } else {
        std::future::pending::<()>().await;
    }
}

