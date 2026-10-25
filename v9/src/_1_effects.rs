//! Every effect of the crate: the `ryi` child process and the SQLite file. Nothing else spawns,
//! reads files or opens databases (`clippy.toml` bans it elsewhere).
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
