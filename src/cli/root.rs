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

/// Root for a daemon-control op (`dl daemon <verb>`, and the back-compat
/// flags): the spawned daemon's `DL_DAEMON_ROOT` if set, else the nearest `.dl/`
/// ancestor of cwd (the same root a one-shot auto-attaches to), else cwd.
pub fn daemon_target() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("DL_DAEMON_ROOT") {
        return Ok(PathBuf::from(p).canonicalize()?);
    }
    let cwd = std::env::current_dir()?;
    let raw = cwd.canonicalize().unwrap_or(cwd);
    Ok(nearest_dl_ancestor(&raw).unwrap_or(raw))
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
