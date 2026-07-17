//! HTTP-transport discovery helpers: where the daemon's `http.json` lives and how
//! a client reads the base URL from it.
//!
//! The HTTP SERVER itself moved to `crate::daemon_shell::http` (axum on the shell
//! runtime) when the daemon's serving shell went tokio; the old `tiny_http`
//! thread-per-request server is gone. Only these two path/URL helpers stayed here
//! because they are shared by `shutdown_cleanup` (which deletes the file) and the
//! `dl daemon url` verb (which reads it) — neither of which touches the runtime.
//!
//! This lives in a flat top-level module (not `crate::daemon::http`) because the
//! crate already carries a `crate::daemon` vs `crate::cli::daemon` name clash;
//! `daemon_http` sidesteps growing that ambiguity.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::daemon;

/// `<home>/http.json` — the HTTP-endpoint discovery file (sibling of
/// `roots.json`/`daemon.pid`). Written by `daemon_shell::http::spawn` on serve,
/// deleted in `shutdown_cleanup`.
pub fn http_json_path() -> PathBuf {
    daemon::daemon_home().join("http.json")
}

/// Read the published base URL (`http://127.0.0.1:<port>`) from `http.json`.
/// Errors cleanly when the file is absent (daemon down or too old to publish it).
pub fn base_url() -> Result<String> {
    let path = http_json_path();
    let text = std::fs::read_to_string(&path).map_err(|_| {
        anyhow::anyhow!(
            "no HTTP endpoint published at {} — is the daemon running? `dl daemon start`",
            path.display()
        )
    })?;
    let info: Value = serde_json::from_str(&text)
        .with_context(|| format!("parse {}", path.display()))?;
    let port = info.get("port").and_then(|v| v.as_u64())
        .ok_or_else(|| anyhow::anyhow!("{} has no port field", path.display()))?;
    Ok(format!("http://127.0.0.1:{port}"))
}
