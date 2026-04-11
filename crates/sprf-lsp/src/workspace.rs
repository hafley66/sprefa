//! Workspace discovery for LSP completions.
//!
//! Loads sprefa.toml config with priority: $SPREFA_CONFIG > ./sprefa.toml > ~/.config/sprefa/sprefa.toml
//! Discovers repos from explicit config entries and source-based auto-discovery.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sprefa_config::{discover_repos, load_config, Config, ConfigError, DiscoveredRepo, RepoConfig};

/// Discovered workspace info for LSP operations.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Workspace root directory (config dir > git root > cwd)
    pub root: PathBuf,
    /// Repos by name (from explicit config + discovered sources)
    pub repos: HashMap<String, RepoInfo>,
    /// Path to loaded config file
    pub config_path: Option<PathBuf>,
    /// Database path from config (tilde-expanded)
    pub db_path: Option<PathBuf>,
}

/// Repo info for completions and lookups.
#[derive(Debug, Clone)]
pub struct RepoInfo {
    pub name: String,
    pub path: PathBuf,
    /// Revs configured for this repo (branches/tags to scan)
    pub revs: Vec<String>,
}

impl Workspace {
    /// Load workspace by discovering and loading sprefa.toml config.
    ///
    /// Config search priority:
    /// 1. $SPREFA_CONFIG environment variable
    /// 2. ./sprefa.toml (local directory)
    /// 3. ~/.config/sprefa/sprefa.toml (XDG config)
    ///
    /// Returns `None` if no config found and no git root can be determined.
    pub fn load() -> Option<Self> {
        match load_config() {
            Ok((config, config_path)) => Self::from_config(config, config_path),
            Err(_) => Self::from_git_root(),
        }
    }

    /// Load workspace from a specific config path.
    ///
    /// Returns `Err` if config cannot be loaded or parsed.
    pub fn from_path(config_path: &Path) -> Result<Self, ConfigError> {
        let config = sprefa_config::load_config_from(config_path)?;
        Self::from_config(config, config_path.to_path_buf())
            .ok_or(ConfigError::NotFound)
    }

    /// Create a minimal workspace from a directory (no config).
    ///
    /// Used when LSP client provides a rootUri but no sprefa.toml exists.
    pub fn from_directory(root: PathBuf) -> Self {
        Self {
            root,
            repos: HashMap::new(),
            config_path: None,
            db_path: None,
        }
    }

    /// Build workspace from loaded config.
    fn from_config(config: Config, config_path: PathBuf) -> Option<Self> {
        let root = config_path
            .parent()
            .map(|p| p.to_path_buf())
            .or_else(find_git_root)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let db_path = Some(PathBuf::from(config.db_path()));
        let repos = collect_repos(&config, &root);

        Some(Self {
            root,
            repos,
            config_path: Some(config_path),
            db_path,
        })
    }

    /// Build workspace from git root when no config exists.
    fn from_git_root() -> Option<Self> {
        let root = find_git_root()?;

        Some(Self {
            root,
            repos: HashMap::new(),
            config_path: None,
            db_path: None,
        })
    }

    /// Get repo info by name.
    pub fn repo(&self, name: &str) -> Option<&RepoInfo> {
        self.repos.get(name)
    }

    /// Get all repo names.
    pub fn repo_names(&self) -> Vec<String> {
        self.repos.keys().cloned().collect()
    }

    /// Check if workspace has any repos configured.
    pub fn is_empty(&self) -> bool {
        self.repos.is_empty()
    }

    /// Number of repos in workspace.
    pub fn len(&self) -> usize {
        self.repos.len()
    }
}

/// Collect repos from config: explicit entries + discovered sources.
fn collect_repos(config: &Config, _root: &Path) -> HashMap<String, RepoInfo> {
    let mut repos = HashMap::new();

    // Add explicit repos from config
    for repo_config in &config.repos {
        if let Some(info) = repo_from_config(repo_config) {
            repos.insert(info.name.clone(), info);
        }
    }

    // Discover repos from sources
    for source in &config.sources {
        match discover_repos(source) {
            Ok(discovered) => {
                for d in discovered {
                    let info = repo_from_discovered(&d, source.branch_patterns.clone());
                    repos.insert(info.name.clone(), info);
                }
            }
            Err(e) => {
                // Log discovery errors but continue with other sources
                eprintln!("Failed to discover repos from source '{}': {}", source.root, e);
            }
        }
    }

    repos
}

/// Convert explicit RepoConfig to RepoInfo.
fn repo_from_config(config: &RepoConfig) -> Option<RepoInfo> {
    // Include repo even if path doesn't exist - we still need the name for completions
    let path = PathBuf::from(&config.path);

    Some(RepoInfo {
        name: config.name.clone(),
        path,
        revs: config.rev_list(),
    })
}

/// Convert discovered repo to RepoInfo.
fn repo_from_discovered(d: &DiscoveredRepo, branch_patterns: Vec<String>) -> RepoInfo {
    // Use branch from discovery, or extract from patterns, or default to "main"
    let revs = if branch_patterns.is_empty() {
        vec![d.branch.clone().unwrap_or_else(|| "main".to_string())]
    } else {
        branch_patterns
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect()
    };

    RepoInfo {
        name: d.name.clone(),
        path: d.path.clone(),
        revs,
    }
}

/// Find git repository root from current directory.
fn find_git_root() -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .ok()
            .map(|s| PathBuf::from(s.trim()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_git_root() {
        // This test assumes we're running in a git repo
        let root = find_git_root();
        assert!(root.is_some(), "Should find git root when in a git repo");
    }

    #[test]
    fn test_workspace_empty() {
        let workspace = Workspace {
            root: PathBuf::from("/tmp"),
            repos: HashMap::new(),
            config_path: None,
            db_path: None,
        };
        assert!(workspace.is_empty());
        assert_eq!(workspace.len(), 0);
    }
}
