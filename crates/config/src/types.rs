use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub db: DbConfig,
    pub daemon: Option<DaemonConfig>,
    pub scan: Option<ScanConfig>,
    /// Connection to a ghcache instance for checkout event subscriptions.
    pub ghcache: Option<GhcacheConfig>,
    /// Auto-discovery from a checkout root managed by an external tool.
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    /// Explicit repo entries (in addition to anything discovered via sources).
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
    pub filter: Option<FilterConfig>,
}

/// Points to a directory tree of git checkouts managed by an external tool.
/// sprefa does not clone or fetch -- it reads what's already on disk.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SourceConfig {
    /// Root directory containing checkouts.
    pub root: String,
    /// Directory layout pattern using {org}, {branch}, {repo} placeholders.
    /// Sprefa walks `root` and matches directory structure against this pattern
    /// to discover org, repo name, and branch for each checkout.
    ///
    /// Examples:
    ///   "{org}/{branch}/{repo}"       ->  acme/main/frontend/
    ///   "{org}/{repo}/{branch}"       ->  acme/frontend/main/
    ///   "{org}/{repo}"                ->  acme/frontend/  (branch from git HEAD)
    ///   "{repo}"                      ->  frontend/       (flat, no org)
    pub layout: String,
    /// Default org if the layout has no {org} placeholder.
    pub default_org: Option<String>,
    /// Default branch if the layout has no {branch} placeholder.
    /// When absent and no {branch} in layout, branch is read from git HEAD.
    pub default_branch: Option<String>,
    /// Branch patterns to subscribe to via ghcache. When a checkout event
    /// arrives, the branch name is matched against these globs.
    /// e.g. ["main", "feature/3.2/*", "release/*"]
    /// If empty, all branches are accepted.
    #[serde(default)]
    pub branch_patterns: Vec<String>,
    /// Filter applied to repos discovered from this source.
    pub filter: Option<FilterConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DbConfig {
    /// Path to the SQLite database. Supports ~ expansion.
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DaemonConfig {
    /// If set, CLI delegates to this running daemon instead of opening DB directly.
    pub url: Option<String>,
    /// Address for the daemon to bind to.
    pub bind: Option<String>,
    /// If true, CLI starts daemon automatically if not running.
    pub auto_start: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScanConfig {
    /// Number of parallel scan threads.
    pub workers: Option<usize>,
    pub normalize: Option<NormalizeConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NormalizeConfig {
    /// Suffixes to strip during norm2 normalization (e.g., "-service", "-api").
    #[serde(default)]
    pub strip_suffixes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FilterConfig {
    pub mode: FilterMode,
    #[serde(default)]
    pub exclude: Option<Vec<String>>,
    #[serde(default)]
    pub include: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FilterMode {
    Exclude,
    Include,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RepoConfig {
    pub name: String,
    pub path: String,
    /// Branches to index. Defaults to ["main"].
    pub branches: Option<Vec<String>>,
    /// Per-repo filter, merged with global.
    pub filter: Option<FilterConfig>,
    /// Per-branch overrides, most specific wins.
    #[serde(default)]
    pub branch_overrides: Option<Vec<BranchOverride>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BranchOverride {
    /// Branch name (exact match or glob).
    pub branch: String,
    /// Replaces repo-level filter for this branch.
    pub filter: Option<FilterConfig>,
    /// Override scan settings per branch.
    pub scan: Option<ScanConfig>,
}

impl RepoConfig {
    pub fn branch_list(&self) -> Vec<String> {
        match &self.branches {
            Some(b) if !b.is_empty() => b.clone(),
            _ => vec!["main".to_string()],
        }
    }
}

impl Config {
    pub fn db_path(&self) -> String {
        expand_tilde(&self.db.path)
    }

    pub fn daemon_url(&self) -> Option<&str> {
        self.daemon.as_ref()?.url.as_deref()
    }

    pub fn daemon_bind(&self) -> &str {
        self.daemon
            .as_ref()
            .and_then(|d| d.bind.as_deref())
            .unwrap_or("127.0.0.1:9400")
    }
}

/// Connection to a ghcache SQLite database for checkout event subscriptions.
/// When configured, the daemon subscribes to change_log events and auto-scans
/// new or updated checkouts that match the configured source patterns.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GhcacheConfig {
    /// Path to the ghcache SQLite database.
    pub db: String,
    /// Poll interval in milliseconds (default 500).
    pub poll_ms: Option<u64>,
}

impl GhcacheConfig {
    pub fn db_path(&self) -> String {
        expand_tilde(&self.db)
    }

    pub fn poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.poll_ms.unwrap_or(500))
    }
}

pub(crate) fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}{}", home.to_string_lossy(), &path[1..]);
        }
    }
    path.to_string()
}
