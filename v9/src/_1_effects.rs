//! Every effect of the crate: the `ryi` child process, the SQLite file, threads, the listening
//! socket, the process args, the clock. Nothing else spawns, reads files, opens databases, binds,
//! reads args or reads the clock (`clippy.toml` bans it elsewhere).
#![allow(clippy::disallowed_methods)]

use std::path::Path;

/// One call edge as ryi names it: `path#name` of caller and callee.
pub struct CallEdge {
    pub caller: String,
    pub callee: String,
}

/// `ryi fast .` inside `root`: written cross-file call facts, `resolved_edge` records only.
pub fn ryi_call_edges(ryi: &str, root: &Path) -> Result<Vec<CallEdge>, String> {
    let output = std::process::Command::new(ryi)
        .args(["fast", "."])
        .current_dir(root)
        .output()
        .map_err(|error| format!("spawn {ryi}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{ryi} fast exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut edges = Vec::new();
    for line in output.stdout.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let record: serde_json::Value =
            serde_json::from_slice(line).map_err(|error| format!("ryi line: {error}"))?;
        if record["record"] != "resolved_edge" {
            continue;
        }
        let part = |key: &str| record[key].as_str().unwrap_or_default().to_string();
        edges.push(CallEdge {
            caller: format!("{}#{}", part("caller_path"), part("caller_name")),
            callee: format!("{}#{}", part("callee_path"), part("callee_name")),
        });
    }
    Ok(edges)
}

pub fn open_db(path: &Path) -> rusqlite::Result<rusqlite::Connection> {
    rusqlite::Connection::open(path)
}

/// `V9_SYSTEM`, the recipe text; `None` when unset. Non-UTF-8 bytes are replaced so the recipe
/// parse names them.
pub fn v9_system() -> Option<String> {
    std::env::var_os("V9_SYSTEM").map(|value| value.to_string_lossy().into_owned())
}

/// `V9_RYI`, the ryi binary; `ryi` on PATH when unset.
pub fn v9_ryi() -> String {
    std::env::var("V9_RYI").unwrap_or_else(|_| "ryi".to_string())
}

/// The process args, the program name first.
pub fn args() -> Vec<String> {
    std::env::args().collect()
}

/// The monotonic clock now.
pub fn now() -> std::time::Instant {
    std::time::Instant::now()
}

/// A named thread running `body`.
pub fn spawn<T: Send + 'static>(
    name: &str,
    body: impl FnOnce() -> T + Send + 'static,
) -> std::io::Result<std::thread::JoinHandle<T>> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
}

/// A localhost listener; port 0 takes a free port.
pub fn bind(port: u16) -> std::io::Result<std::net::TcpListener> {
    std::net::TcpListener::bind(("127.0.0.1", port))
}

/// Serves `router` on `listener` until the process ends; the tokio runtime lives in this call.
pub fn serve_http(listener: std::net::TcpListener, router: axum::Router) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    tokio::runtime::Runtime::new()?.block_on(async move {
        axum::serve(tokio::net::TcpListener::from_std(listener)?, router).await
    })
}
