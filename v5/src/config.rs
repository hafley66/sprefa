//! Turnkey multi-repo config. A single TOML file lists the checkouts to analyze
//! together; each becomes a `repo` row. Ported from v4 (`v4/src/config.rs`),
//! trimmed to the `repos` table v5 needs.
//!
//! Search order (first existing wins):
//!   1. `$SPREFA_CONFIG`                      (explicit override)
//!   2. `$XDG_CONFIG_HOME/sprefa/config.toml` (XDG standard)
//!   3. `~/.config/sprefa/config.toml`        (XDG default)
//!
//! ```toml
//! [[repos]]
//! slug = "alpha/one"
//! root = "/path/to/checkout-a"
//!
//! [[repos]]
//! slug = "beta/two"
//! root = "/path/to/checkout-b"
//!
//! # A repo not yet on disk: give a `url` and the engine clones it into `root`
//! # on first scan (full clone).
//! [[repos]]
//! slug = "gamma/three"
//! root = "/path/to/cache/gamma"
//! url  = "git@github.com:org/gamma.git"
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// One configured repository: a logical `slug`, its on-disk `root`, and an
/// optional `url` to clone from when `root` does not yet exist.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RepoConfig {
    pub slug: String,
    pub root: PathBuf,
    #[serde(default)]
    pub url: Option<String>,
}

/// The whole config. Only `repos` for now; add sections as v5 needs them.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct SprfConfig {
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
}

impl SprfConfig {
    /// The search paths, highest precedence first. `$SPREFA_CONFIG` is taken
    /// verbatim; otherwise the XDG config dir, then `~/.config`.
    pub fn search_paths() -> Vec<PathBuf> {
        if let Some(p) = std::env::var_os("SPREFA_CONFIG") {
            return vec![PathBuf::from(p)];
        }
        let mut out = Vec::new();
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            out.push(PathBuf::from(xdg).join("sprefa/config.toml"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            out.push(PathBuf::from(home).join(".config/sprefa/config.toml"));
        }
        out
    }

    /// The config file to load/watch: the first search path that exists, else
    /// the first search path (so a watcher can wait for it to be created).
    /// `None` only when neither `$SPREFA_CONFIG`, `$XDG_CONFIG_HOME`, nor `$HOME`
    /// is set.
    pub fn config_path() -> Option<PathBuf> {
        let paths = Self::search_paths();
        paths.iter().find(|p| p.exists()).cloned().or_else(|| paths.into_iter().next())
    }

    /// Load from the first existing search path. Missing file (or no path at
    /// all) yields an empty config; a present-but-malformed file is an error so
    /// the caller can surface it rather than silently analyzing nothing.
    pub fn load_default() -> Result<Self> {
        match Self::search_paths().into_iter().find(|p| p.exists()) {
            Some(path) => Self::load_from_path(&path),
            None => Ok(Self::default()),
        }
    }

    pub fn load_from_path(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse config {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(name: &str, body: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("sprf_cfg_{name}.toml"));
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn parses_repos() {
        let p = write_tmp("repos", "\
            [[repos]]\n\
            slug = \"alpha/one\"\n\
            root = \"/tmp/alpha\"\n\
            [[repos]]\n\
            slug = \"beta/two\"\n\
            root = \"/tmp/beta\"\n");
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        assert_eq!(cfg.repos.len(), 2);
        assert_eq!(cfg.repos[0].slug, "alpha/one");
        assert_eq!(cfg.repos[1].root, PathBuf::from("/tmp/beta"));
    }

    #[test]
    fn empty_file_is_empty_config() {
        let p = write_tmp("empty", "");
        assert_eq!(SprfConfig::load_from_path(&p).unwrap(), SprfConfig::default());
    }

    #[test]
    fn malformed_is_an_error_not_a_silent_empty() {
        let p = write_tmp("bad", "this is not = valid = toml [[[");
        assert!(SprfConfig::load_from_path(&p).is_err());
    }

    #[test]
    fn spefa_config_env_takes_precedence() {
        // Set the explicit override; it must be the sole search path.
        std::env::set_var("SPREFA_CONFIG", "/custom/path.toml");
        let paths = SprfConfig::search_paths();
        std::env::remove_var("SPREFA_CONFIG");
        assert_eq!(paths, vec![PathBuf::from("/custom/path.toml")]);
    }
}
