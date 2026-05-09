//! `~/.config/sprefa/config.toml` — XDG-style config for known repos
//! plus default app driver settings.
//!
//! v4 Layer 5a: a single canonical place for "the list of repos sprefa
//! knows about" so the bare `repo()` op generator emits one cursor per
//! configured repo without hard-coded slug/path pairs in the .sprf
//! source.
//!
//! Format:
//!
//! ```toml
//! [store]
//! fact_db = "/Users/me/.local/share/sprefa/facts.db"
//! queue_db = "/Users/me/.local/share/sprefa/queue.db"
//!
//! [run]
//! root = "/Users/me/projects"
//! show_rows = true
//! max_diags = 50
//!
//! [daemon]
//! bind = "127.0.0.1:8787"
//! root = "/Users/me/projects"
//! ghcache_db = "/Users/me/.local/share/ghcache/gh.db"
//! ghcache_interval_ms = 500
//!
//! [[repos]]
//! slug = "myorg/sprefa"
//! root = "/Users/me/projects/sprefa"
//!
//! [[repos]]
//! slug = "myorg/other"
//! root = "/Users/me/projects/other"
//! ```
//!
//! Resolution: `load_default()` reads `$SPREFA_CONFIG` when set, then
//! `$HOME/.config/sprefa/config.toml`, then legacy
//! `$HOME/.config/sprefa/repos.toml`. Missing file = empty config.
//! Tests inject explicit configs via the `SprfState::with_config`
//! builder. CLI flags override these defaults.

use std::path::PathBuf;

use serde::Deserialize;

/// One configured repository: slug + on-disk root.
#[derive(Debug, Clone, Deserialize)]
pub struct RepoConfig {
    pub slug: String,
    pub root: PathBuf,
}

/// Top-level config blob.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SprfConfig {
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
    #[serde(default)]
    pub store: StoreConfig,
    #[serde(default)]
    pub run: RunConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StoreConfig {
    pub fact_db: Option<PathBuf>,
    pub queue_db: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RunConfig {
    pub root: Option<PathBuf>,
    pub remote: Option<String>,
    pub show_rows: Option<bool>,
    pub max_diags: Option<usize>,
    pub fact_db: Option<PathBuf>,
    pub queue_db: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DaemonConfig {
    pub bind: Option<String>,
    pub root: Option<PathBuf>,
    pub fact_db: Option<PathBuf>,
    pub queue_db: Option<PathBuf>,
    pub ghcache_db: Option<PathBuf>,
    pub ghcache_interval_ms: Option<u64>,
}

impl SprfConfig {
    /// Empty config; equivalent to `Default::default()`. Provided for
    /// readability at call sites.
    pub fn empty() -> Self { Self::default() }

    /// Load from `$SPREFA_CONFIG`, `$HOME/.config/sprefa/config.toml`,
    /// or legacy `$HOME/.config/sprefa/repos.toml`. Missing file or
    /// missing `$HOME` returns an empty config. Parse errors return
    /// empty config silently because callers currently do not have a
    /// diagnostics channel during construction.
    pub fn load_default() -> Self {
        if let Some(path) = std::env::var_os("SPREFA_CONFIG").map(PathBuf::from) {
            return Self::load_from_path(&path).unwrap_or_default();
        }

        for path in Self::default_paths() {
            if path.exists() {
                return Self::load_from_path(&path).unwrap_or_default();
            }
        }
        Self::default()
    }

    pub fn default_paths() -> Vec<PathBuf> {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return Vec::new();
        };
        vec![
            home.join(".config/sprefa/config.toml"),
            home.join(".config/sprefa/repos.toml"),
        ]
    }

    /// Read+parse a TOML file at `path`. Returns the formatted error
    /// string on either IO or parse failure so the caller can decide
    /// whether to surface or swallow.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        toml::from_str(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
    }

    pub fn run_fact_db(&self) -> Option<PathBuf> {
        self.run.fact_db.clone().or_else(|| self.store.fact_db.clone())
    }

    pub fn run_queue_db(&self) -> Option<PathBuf> {
        self.run.queue_db.clone().or_else(|| self.store.queue_db.clone())
    }

    pub fn daemon_fact_db(&self) -> Option<PathBuf> {
        self.daemon.fact_db.clone().or_else(|| self.store.fact_db.clone())
    }

    pub fn daemon_queue_db(&self) -> Option<PathBuf> {
        self.daemon.queue_db.clone().or_else(|| self.store.queue_db.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_repos_toml() {
        let src = r#"
            [[repos]]
            slug = "alpha/one"
            root = "/tmp/alpha"

            [[repos]]
            slug = "beta/two"
            root = "/tmp/beta"
        "#;
        let cfg: SprfConfig = toml::from_str(src).unwrap();
        assert_eq!(cfg.repos.len(), 2);
        assert_eq!(cfg.repos[0].slug, "alpha/one");
        assert_eq!(cfg.repos[1].root, PathBuf::from("/tmp/beta"));
    }

    #[test]
    fn empty_string_parses_to_empty_config() {
        let cfg: SprfConfig = toml::from_str("").unwrap();
        assert!(cfg.repos.is_empty());
    }

    #[test]
    fn parses_unified_config_defaults() {
        let src = r#"
            [store]
            fact_db = "/tmp/facts.db"
            queue_db = "/tmp/queue.db"

            [run]
            root = "/tmp/run-root"
            remote = "http://127.0.0.1:8787"
            show_rows = false
            max_diags = 9

            [daemon]
            bind = "127.0.0.1:9999"
            root = "/tmp/daemon-root"
            ghcache_db = "/tmp/gh.db"
            ghcache_interval_ms = 250

            [[repos]]
            slug = "alpha/one"
            root = "/tmp/alpha"
        "#;
        let cfg: SprfConfig = toml::from_str(src).unwrap();
        assert_eq!(cfg.run_fact_db(), Some(PathBuf::from("/tmp/facts.db")));
        assert_eq!(cfg.run_queue_db(), Some(PathBuf::from("/tmp/queue.db")));
        assert_eq!(cfg.run.show_rows, Some(false));
        assert_eq!(cfg.run.max_diags, Some(9));
        assert_eq!(cfg.daemon.bind.as_deref(), Some("127.0.0.1:9999"));
        assert_eq!(cfg.daemon.ghcache_db, Some(PathBuf::from("/tmp/gh.db")));
        assert_eq!(cfg.daemon.ghcache_interval_ms, Some(250));
        assert_eq!(cfg.repos[0].slug, "alpha/one");
    }

    #[test]
    fn run_and_daemon_store_defaults_can_be_overridden() {
        let src = r#"
            [store]
            fact_db = "/tmp/store-facts.db"
            queue_db = "/tmp/store-queue.db"

            [run]
            fact_db = "/tmp/run-facts.db"

            [daemon]
            queue_db = "/tmp/daemon-queue.db"
        "#;
        let cfg: SprfConfig = toml::from_str(src).unwrap();
        assert_eq!(cfg.run_fact_db(), Some(PathBuf::from("/tmp/run-facts.db")));
        assert_eq!(cfg.run_queue_db(), Some(PathBuf::from("/tmp/store-queue.db")));
        assert_eq!(cfg.daemon_fact_db(), Some(PathBuf::from("/tmp/store-facts.db")));
        assert_eq!(cfg.daemon_queue_db(), Some(PathBuf::from("/tmp/daemon-queue.db")));
    }
}
