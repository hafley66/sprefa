//! Working-root resolution. The root is the current directory, never a flag: a
//! client (the vscode ext, a test harness, a shell) points `dl` at a folder by
//! spawning it with that `cwd`. A spawned daemon carries its root in the
//! `DL_DAEMON_ROOT` env instead (there is no `--root`).

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Resolve the working root for a one-shot / mode run. In discovery mode (no
/// positional program) a cwd with no `.dl/` walks up to the nearest ancestor
/// that has one, so an editor opened on a subdir inherits the workspace rails
/// instead of failing "no .dl". With an explicit program, cwd is respected
/// exactly.
pub fn resolve(programs: &[String]) -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let raw = cwd.canonicalize().unwrap_or(cwd);
    if programs.is_empty() {
        return Ok(nearest_dl_ancestor(&raw).unwrap_or(raw));
    }
    Ok(raw)
}

/// Which root a daemon-control op / one-shot addresses on the singleton daemon.
/// There is one daemon and one socket; the root is an envelope KEY, not a socket
/// selector. `None` = the config-view engine (org/folders model, scans nothing).
#[derive(Debug, Clone)]
pub enum DaemonTarget {
    Singleton { root: Option<PathBuf> },
}

impl DaemonTarget {
    /// The root key (`None` = config view).
    pub fn root(&self) -> Option<&std::path::Path> {
        match self {
            DaemonTarget::Singleton { root } => root.as_deref(),
        }
    }
}

/// Resolve which root a daemon op addresses. `DL_DAEMON_ROOT` wins (spawned
/// children, tests); else the nearest `.dl/`-owning ancestor of cwd (cwd itself
/// if it owns one); else `None` — the rootless config view.
pub fn daemon_target() -> Result<DaemonTarget> {
    if let Some(p) = std::env::var_os("DL_DAEMON_ROOT") {
        let root = PathBuf::from(p).canonicalize()?;
        return Ok(DaemonTarget::Singleton { root: Some(root) });
    }
    let cwd = std::env::current_dir()?;
    let raw = cwd.canonicalize().unwrap_or(cwd);
    let root = if raw.join(".dl").is_dir() {
        Some(raw)
    } else {
        nearest_dl_ancestor(&raw)
    };
    Ok(DaemonTarget::Singleton { root })
}

/// If `dir` has no `.dl/`, the nearest ancestor that does. `None` when `dir`
/// itself owns a `.dl/` (use it as-is) or no ancestor has one.
pub fn nearest_dl_ancestor(dir: &Path) -> Option<PathBuf> {
    if dir.join(".dl").is_dir() {
        return None;
    }
    dir.ancestors()
        .find(|a| a.join(".dl").is_dir())
        .map(|a| a.to_path_buf())
}
