//! `~/.config/sprefa/repos.toml` — XDG-style config of known repos.
//!
//! v4 Layer 5a: a single canonical place for "the list of repos sprefa
//! knows about" so the bare `repo()` op generator emits one cursor per
//! configured repo without hard-coded slug/path pairs in the .sprf
//! source.
//!
//! Format:
//!
//! ```toml
//! [[repos]]
//! slug = "myorg/sprefa"
//! root = "/Users/me/projects/sprefa"
//!
//! [[repos]]
//! slug = "myorg/other"
//! root = "/Users/me/projects/other"
//! ```
//!
//! Resolution: `load_default()` reads `$HOME/.config/sprefa/repos.toml`.
//! Missing file = empty config. Tests inject explicit configs via the
//! `SprfState::with_config` builder. Layer 5b adds rev/git plumbing.

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
}

impl SprfConfig {
    /// Empty config; equivalent to `Default::default()`. Provided for
    /// readability at call sites.
    pub fn empty() -> Self { Self::default() }

    /// Load from `$HOME/.config/sprefa/repos.toml`. Missing file or
    /// missing `$HOME` returns an empty config. Parse errors return
    /// empty config silently — Layer 5a doesn't surface them.
    pub fn load_default() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let path = home.map(|h| h.join(".config/sprefa/repos.toml"));
        match path {
            Some(p) if p.exists() => Self::load_from_path(&p).unwrap_or_default(),
            _ => Self::default(),
        }
    }

    /// Read+parse a TOML file at `path`. Returns the formatted error
    /// string on either IO or parse failure so the caller can decide
    /// whether to surface or swallow.
    pub fn load_from_path(path: &std::path::Path) -> Result<Self, String> {
        let bytes = std::fs::read_to_string(path)
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        toml::from_str(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
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
}
