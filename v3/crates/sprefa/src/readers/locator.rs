//! CheckoutLocator — decouples GitBlobReader from any specific checkout
//! manager (ghcacher, manual config, test harness).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

pub trait CheckoutLocator: Send + Sync {
    /// On-disk path to a git repo whose object DB contains `rev`.
    /// `None` = this locator has no record of the (repo_slug, rev) pair.
    fn locate(&self, repo_slug: &str, rev: &str) -> Option<PathBuf>;

    /// All repo slugs known to this locator. Drives `Reader::repos`.
    fn repos(&self) -> Vec<Arc<str>>;

    /// Revs declared for a repo. Drives `Reader::revs`.
    /// Returning empty is valid — rules that name rev literals directly
    /// still work; `Reader::revs` just returns nothing useful.
    fn revs(&self, repo_slug: &str) -> Vec<Arc<str>>;
}

// ---------------------------------------------------------------------------
// ConfigLocator — reads a TOML config file
// ---------------------------------------------------------------------------

/// One entry in the TOML. All repos share a single path (the checkout).
/// Multiple rev strings are listed per repo so `Reader::revs` can enumerate.
///
/// ```toml
/// [[repo]]
/// slug = "myorg/sprefa"
/// path = "/abs/path/to/clone"
/// revs = ["main", "v1.2"]
/// ```
#[derive(Debug, Clone, serde::Deserialize)]
struct RepoEntry {
    slug: String,
    path: String,
    #[serde(default)]
    revs: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct LocatorConfig {
    #[serde(default)]
    repo: Vec<RepoEntry>,
}

pub struct ConfigLocator {
    /// slug → checkout path (one path per slug; all revs share the object DB)
    paths: HashMap<Arc<str>, PathBuf>,
    /// slug → declared revs
    revs:  HashMap<Arc<str>, Vec<Arc<str>>>,
    slugs: Vec<Arc<str>>,
}

impl ConfigLocator {
    pub fn from_toml_str(src: &str) -> Result<Self, toml::de::Error> {
        let cfg: LocatorConfig = toml::from_str(src)?;
        let mut paths  = HashMap::new();
        let mut revs   = HashMap::new();
        let mut slugs  = Vec::new();
        for e in cfg.repo {
            let slug: Arc<str> = Arc::from(e.slug.as_str());
            paths.insert(slug.clone(), PathBuf::from(e.path));
            revs.insert(slug.clone(), e.revs.iter().map(|r| Arc::<str>::from(r.as_str())).collect());
            slugs.push(slug);
        }
        Ok(Self { paths, revs, slugs })
    }

    pub fn from_toml_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let src = std::fs::read_to_string(path)?;
        Ok(Self::from_toml_str(&src)?)
    }
}

impl CheckoutLocator for ConfigLocator {
    fn locate(&self, repo_slug: &str, _rev: &str) -> Option<PathBuf> {
        // All revs for a slug live in the same object DB, so rev is
        // irrelevant for locating the repo dir.
        self.paths.get(repo_slug).cloned()
    }

    fn repos(&self) -> Vec<Arc<str>> {
        self.slugs.clone()
    }

    fn revs(&self, repo_slug: &str) -> Vec<Arc<str>> {
        self.revs.get(repo_slug).cloned().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// InMemoryLocator — test helper
// ---------------------------------------------------------------------------

/// Register in-memory (slug → PathBuf) + optional rev lists.
/// Primarily used in git_read tests where the path is a tmp git repo.
pub struct InMemoryLocator {
    paths: HashMap<Arc<str>, PathBuf>,
    revs:  HashMap<Arc<str>, Vec<Arc<str>>>,
    slugs: Vec<Arc<str>>,
}

impl InMemoryLocator {
    pub fn new() -> Self {
        Self { paths: HashMap::new(), revs: HashMap::new(), slugs: Vec::new() }
    }

    pub fn with_repo(mut self, slug: &str, path: PathBuf, revs: &[&str]) -> Self {
        let s: Arc<str> = Arc::from(slug);
        self.paths.insert(s.clone(), path);
        self.revs.insert(s.clone(), revs.iter().map(|r| Arc::<str>::from(*r)).collect());
        self.slugs.push(s);
        self
    }
}

impl CheckoutLocator for InMemoryLocator {
    fn locate(&self, repo_slug: &str, _rev: &str) -> Option<PathBuf> {
        self.paths.get(repo_slug).cloned()
    }
    fn repos(&self) -> Vec<Arc<str>> { self.slugs.clone() }
    fn revs(&self, repo_slug: &str) -> Vec<Arc<str>> {
        self.revs.get(repo_slug).cloned().unwrap_or_default()
    }
}
