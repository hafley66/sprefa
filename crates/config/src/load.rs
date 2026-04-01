use std::path::{Path, PathBuf};

use crate::Config;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no config file found (checked: $SPREFA_CONFIG, ./sprefa.toml, ~/.config/sprefa/sprefa.toml)")]
    NotFound,
    #[error("failed to read config at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
}

/// Discovers and loads the config file.
/// Priority: $SPREFA_CONFIG > ./sprefa.toml > ~/.config/sprefa/sprefa.toml
pub fn load_config() -> Result<(Config, PathBuf), ConfigError> {
    let path = discover_config_path()?;
    let config = load_config_from(&path)?;
    Ok((config, path))
}

/// Load config from a specific path.
pub fn load_config_from(path: &Path) -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Read {
        path: path.to_owned(),
        source: e,
    })?;
    let config: Config = toml::from_str(&content).map_err(|e| ConfigError::Parse {
        path: path.to_owned(),
        source: e,
    })?;
    Ok(config)
}

/// Find the config file path without loading it.
pub fn discover_config_path() -> Result<PathBuf, ConfigError> {
    // 1. $SPREFA_CONFIG
    if let Ok(env_path) = std::env::var("SPREFA_CONFIG") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. ./sprefa.toml
    let local = PathBuf::from("sprefa.toml");
    if local.exists() {
        return Ok(local);
    }

    // 3. ~/.config/sprefa/sprefa.toml
    if let Some(home) = std::env::var_os("HOME") {
        let xdg = PathBuf::from(home).join(".config/sprefa/sprefa.toml");
        if xdg.exists() {
            return Ok(xdg);
        }
    }

    Err(ConfigError::NotFound)
}

/// Generate a default config TOML string.
pub fn default_config_toml() -> String {
    r#"[db]
path = "~/.sprefa/index.db"

[daemon]
bind = "127.0.0.1:9400"

[scan]
# workers = 4

[scan.normalize]
strip_suffixes = ["-service", "-api", "-v2", "-client", "-server"]

[filter]
mode = "exclude"
exclude = [
  "node_modules/**",
  "vendor/**",
  "dist/**",
  "target/**",
  ".git/**",
  "*.min.js",
  "*.lock",
  "*.map",
]

# Auto-discover repos from a checkout root managed by an external tool.
# The layout pattern tells sprefa how the directory tree maps to org/repo/branch.
# sprefa does NOT clone or fetch -- it only reads what's already on disk.
#
# [[sources]]
# root = "~/checkouts"
# layout = "{org}/{branch}/{repo}"
# # default_org = "myco"          # used when layout has no {org}
# # default_branch = "main"       # used when layout has no {branch}

# Explicit repo entries (in addition to anything discovered via sources).
# [[repos]]
# name = "my-repo"
# path = "/path/to/repo"
# branches = ["main"]
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_yaml_snapshot;

    #[test]
    fn parse_default_config() {
        let config: Config = toml::from_str(&default_config_toml()).unwrap();
        assert_yaml_snapshot!(config);
    }

    #[test]
    fn parse_full_config_with_overrides() {
        let toml_str = r#"
[db]
path = "~/.sprefa/test.db"

[daemon]
url = "http://localhost:9400"
bind = "0.0.0.0:9400"

[scan]
workers = 8

[scan.normalize]
strip_suffixes = ["-svc", "-api"]

[[repos]]
name = "frontend"
path = "/home/me/frontend"
revs = ["main", "develop"]

[repos.filter]
mode = "include"
include = ["src/**"]

[[repos.branch_overrides]]
branch = "develop"

[repos.branch_overrides.filter]
mode = "exclude"
exclude = ["src/generated/**"]

[[repos]]
name = "backend"
path = "/home/me/backend"

[filter]
mode = "exclude"
exclude = ["node_modules/**", "target/**"]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_yaml_snapshot!(config);
    }

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[db]
path = "/tmp/sprefa.db"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_yaml_snapshot!(config);
    }
}
