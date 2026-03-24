use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub db: DbConfig,
    pub daemon: Option<DaemonConfig>,
    pub scan: Option<ScanConfig>,
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
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

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}{}", home.to_string_lossy(), &path[1..]);
        }
    }
    path.to_string()
}
