// sprefa-daemon — serves the same axum::Router that powers sprefa-run
// and sprefa-lsp, over a TCP socket. Remote shells (web, curl, scripts,
// other sprefa-run invocations with --remote URL) get the entire RPC
// surface for free.
//
// Usage:
//   sprefa-daemon [--bind 127.0.0.1:8787] [--root <dir>]
//                 [--fact-db <path>] [--queue-db <path>]
//
// The exposed endpoints are whatever is in `v4::app::sprf_rpc!{...}`.
// Adding routes here is zero work — the macro generates them.

use std::path::PathBuf;
use std::sync::Arc;

use v4::app::{build_router, SprfState};

#[derive(Debug)]
struct Args {
    bind: String,
    root: PathBuf,
    fact_db: Option<PathBuf>,
    queue_db: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut bind = "127.0.0.1:8787".to_string();
    let mut root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut fact_db: Option<PathBuf> = None;
    let mut queue_db: Option<PathBuf> = None;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "--bind" => {
                bind = raw.get(i+1).ok_or("--bind needs addr")?.clone();
                i += 2;
            }
            "--root" => {
                root = PathBuf::from(raw.get(i+1).ok_or("--root needs dir")?);
                i += 2;
            }
            "--fact-db" => {
                fact_db = Some(PathBuf::from(raw.get(i+1).ok_or("--fact-db needs path")?));
                i += 2;
            }
            "--queue-db" => {
                queue_db = Some(PathBuf::from(raw.get(i+1).ok_or("--queue-db needs path")?));
                i += 2;
            }
            "-h" | "--help" => {
                eprintln!("sprefa-daemon [--bind 127.0.0.1:8787] [--root DIR] [--fact-db PATH] [--queue-db PATH]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(Args { bind, root, fact_db, queue_db })
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = match parse_args() {
        Ok(a)  => a,
        Err(e) => { eprintln!("sprefa-daemon: {e}"); std::process::exit(2); }
    };

    let state = Arc::new(match (args.fact_db, args.queue_db) {
        (Some(fact_db), Some(queue_db)) => {
            SprfState::new_with_sqlite_backends(args.root, fact_db, queue_db)
        }
        (Some(fact_db), None) => SprfState::new_with_sqlite_facts(args.root, fact_db),
        (None, Some(queue_db)) => SprfState::new_with_sqlite_queue(args.root, queue_db),
        (None, None) => SprfState::new(args.root),
    });
    let router = build_router(state);

    let listener = match tokio::net::TcpListener::bind(&args.bind).await {
        Ok(l)  => l,
        Err(e) => { eprintln!("bind {}: {e}", args.bind); std::process::exit(1); }
    };
    eprintln!("sprefa-daemon listening on http://{}", args.bind);

    if let Err(e) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        eprintln!("serve error: {e}");
        std::process::exit(1);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    eprintln!("shutting down");
}
