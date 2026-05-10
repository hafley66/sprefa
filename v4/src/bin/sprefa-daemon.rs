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
#[cfg(feature = "ghcache")]
use std::time::Duration;

use v4::app::{build_router, SprfState};
use v4::config::SprfConfig;
#[cfg(feature = "ghcache")]
use v4::git_watch::{latest_ghcache_change_id, poll_ghcache_changes};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[derive(Debug)]
struct Args {
    bind: String,
    root: PathBuf,
    fact_db: Option<PathBuf>,
    queue_db: Option<PathBuf>,
    #[cfg(feature = "ghcache")]
    ghcache_db: Option<PathBuf>,
    #[cfg(feature = "ghcache")]
    ghcache_interval_ms: u64,
}

fn parse_args() -> Result<Args, String> {
    parse_args_from(std::env::args().skip(1), &SprfConfig::load_default())
}

fn parse_args_from<I, S>(raw: I, cfg: &SprfConfig) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let raw: Vec<String> = raw.into_iter().map(Into::into).collect();
    let mut bind = cfg.daemon.bind.clone()
        .unwrap_or_else(|| "127.0.0.1:8787".to_string());
    let mut root = cfg.daemon.root.clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut fact_db: Option<PathBuf> = cfg.daemon_fact_db();
    let mut queue_db: Option<PathBuf> = cfg.daemon_queue_db();
    #[cfg(feature = "ghcache")]
    let mut ghcache_db: Option<PathBuf> = cfg.daemon.ghcache_db.clone();
    #[cfg(feature = "ghcache")]
    let mut ghcache_interval_ms = cfg.daemon.ghcache_interval_ms.unwrap_or(500);

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
            #[cfg(feature = "ghcache")]
            "--ghcache-db" => {
                ghcache_db = Some(PathBuf::from(raw.get(i+1).ok_or("--ghcache-db needs path")?));
                i += 2;
            }
            #[cfg(feature = "ghcache")]
            "--ghcache-interval-ms" => {
                ghcache_interval_ms = raw
                    .get(i+1)
                    .ok_or("--ghcache-interval-ms needs milliseconds")?
                    .parse()
                    .map_err(|_| "--ghcache-interval-ms must be an integer")?;
                i += 2;
            }
            #[cfg(not(feature = "ghcache"))]
            "--ghcache-db" | "--ghcache-interval-ms" => {
                return Err("ghcache support was compiled out".into());
            }
            "-h" | "--help" => {
                eprintln!("{}", daemon_help());
                std::process::exit(0);
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(Args {
        bind,
        root,
        fact_db,
        queue_db,
        #[cfg(feature = "ghcache")]
        ghcache_db,
        #[cfg(feature = "ghcache")]
        ghcache_interval_ms,
    })
}

fn daemon_help() -> &'static str {
    #[cfg(feature = "ghcache")]
    {
        "sprefa-daemon [--bind 127.0.0.1:8787] [--root DIR] [--fact-db PATH] [--queue-db PATH] [--ghcache-db PATH]"
    }
    #[cfg(not(feature = "ghcache"))]
    {
        "sprefa-daemon [--bind 127.0.0.1:8787] [--root DIR] [--fact-db PATH] [--queue-db PATH]"
    }
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
    #[cfg(feature = "ghcache")]
    if let Some(db_path) = args.ghcache_db {
        spawn_ghcache_watcher(
            state.clone(),
            db_path,
            Duration::from_millis(args.ghcache_interval_ms),
        );
    }
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

#[cfg(feature = "ghcache")]
fn spawn_ghcache_watcher(state: Arc<SprfState>, db_path: PathBuf, interval: Duration) {
    tokio::spawn(async move {
        let mut last_id = match tokio::task::spawn_blocking({
            let db_path = db_path.clone();
            move || latest_ghcache_change_id(&db_path)
        }).await {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => {
                eprintln!("ghcache watch disabled: {e}");
                return;
            }
            Err(e) => {
                eprintln!("ghcache watch task failed: {e}");
                return;
            }
        };

        loop {
            tokio::time::sleep(interval).await;
            let result = tokio::task::spawn_blocking({
                let db_path = db_path.clone();
                move || poll_ghcache_changes(&db_path, last_id)
            }).await;
            let changes = match result {
                Ok(Ok(changes)) => changes,
                Ok(Err(e)) => {
                    eprintln!("ghcache poll error: {e}");
                    continue;
                }
                Err(e) => {
                    eprintln!("ghcache poll task failed: {e}");
                    continue;
                }
            };
            for change in changes {
                last_id = change.id;
                state.dispatch_ghcache_change(&change);
            }
            state.drain_ready();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use v4::config::{DaemonConfig, StoreConfig};

    #[test]
    fn parse_args_uses_config_defaults() {
        let cfg = SprfConfig {
            store: StoreConfig {
                fact_db: Some(PathBuf::from("/tmp/facts.db")),
                queue_db: Some(PathBuf::from("/tmp/queue.db")),
            },
            daemon: DaemonConfig {
                bind: Some("127.0.0.1:9999".into()),
                root: Some(PathBuf::from("/tmp/root")),
                #[cfg(feature = "ghcache")]
                ghcache_db: Some(PathBuf::from("/tmp/gh.db")),
                #[cfg(feature = "ghcache")]
                ghcache_interval_ms: Some(250),
                ..Default::default()
            },
            ..Default::default()
        };

        let args = parse_args_from(std::iter::empty::<&str>(), &cfg).unwrap();
        assert_eq!(args.bind, "127.0.0.1:9999");
        assert_eq!(args.root, PathBuf::from("/tmp/root"));
        assert_eq!(args.fact_db, Some(PathBuf::from("/tmp/facts.db")));
        assert_eq!(args.queue_db, Some(PathBuf::from("/tmp/queue.db")));
        #[cfg(feature = "ghcache")]
        {
            assert_eq!(args.ghcache_db, Some(PathBuf::from("/tmp/gh.db")));
            assert_eq!(args.ghcache_interval_ms, 250);
        }
    }

    #[test]
    fn parse_args_cli_overrides_config_defaults() {
        let cfg = SprfConfig {
            store: StoreConfig {
                fact_db: Some(PathBuf::from("/tmp/facts.db")),
                queue_db: Some(PathBuf::from("/tmp/queue.db")),
            },
            daemon: DaemonConfig {
                bind: Some("127.0.0.1:9999".into()),
                root: Some(PathBuf::from("/tmp/root")),
                ..Default::default()
            },
            ..Default::default()
        };

        let args = parse_args_from([
            "--bind", "127.0.0.1:7777",
            "--root", "/tmp/cli-root",
            "--fact-db", "/tmp/cli-facts.db",
        ], &cfg).unwrap();
        assert_eq!(args.bind, "127.0.0.1:7777");
        assert_eq!(args.root, PathBuf::from("/tmp/cli-root"));
        assert_eq!(args.fact_db, Some(PathBuf::from("/tmp/cli-facts.db")));
        assert_eq!(args.queue_db, Some(PathBuf::from("/tmp/queue.db")));
    }
}
