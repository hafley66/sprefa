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
//!
//! # A repo that may be absent: a missing root is non-fatal. scan yields zero
//! # rows; the engine prints one stderr line and proceeds.
//! [[repos]]
//! slug = "delta/four"
//! root = "/path/to/maybe-delta"
//! allow_missing = true
//!
//! # Top-level default_org prefixes any bare slug (one without a `/`). Lets a
//! # multi-repo config share one org without repeating it per entry.
//! default_org = "alpha"
//! [[repos]]
//! slug = "five"          # normalizes to "alpha/five"
//! root = "/path/to/five"
//!
//! # An [[org]] entry expands, at load, into one [[repos]] per git checkout
//! # found under `dir` — the multi-root shape for an org-of-repos folder.
//! # Each discovered repo's slug is `<dir-basename>/<path-under-dir>` (so two
//! # repos named `docs` under different org folders never collide), root is the
//! # checkout dir, and descent STOPS at each `.git` (submodules are not repos).
//! [[org]]
//! dir = "~/orgs/grafana"   # leading ~ expands to $HOME
//! # max_depth = 3          # directory levels below `dir` to search (default 3)
//! # foldername = "gf"      # override the slug prefix (default: `dir` basename)
//! # foldername = "."       # FLATTEN: drop the prefix, slug = bare path under dir
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// One configured repository: a logical `slug`, its on-disk `root`, and an
/// optional `url` to clone from when `root` does not yet exist.
///
/// `allow_missing = true` opts a repo into progressive analysis: a missing
/// `root` (after the optional clone fails) is no longer fatal. The repo row
/// stays absent; `scan` against the slug yields zero rows; the engine prints
/// one stderr line and proceeds. Lets a multi-repo program run to completion
/// before all clones exist.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RepoConfig {
    pub slug: String,
    pub root: PathBuf,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub allow_missing: bool,
}

/// A folder holding many git checkouts (an org clone). At load, `dir` is walked
/// and every checkout it contains is expanded into a `RepoConfig` — the
/// multi-root shape without listing each repo by hand. Descent stops at each
/// `.git` (a repo's own subdirs / submodules are not separate repos).
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OrgConfig {
    /// The parent folder to scan. A leading `~` expands to `$HOME`.
    pub dir: PathBuf,
    /// How many directory levels below `dir` to search before giving up on a
    /// branch (a git root found shallower still registers). Default 3, enough
    /// for `org/repo` and `org/subgroup/repo` layouts without walking a deep
    /// tree of non-repos.
    #[serde(default)]
    pub max_depth: Option<usize>,
    /// Override the slug prefix (default: the basename of `dir`). Set to `"."`
    /// to FLATTEN — drop the org prefix entirely so a repo addresses by its bare
    /// path under `dir` (`~/projects/my-long-ass-org-name/repo-a` → slug
    /// `repo-a`, not `my-long-ass-org-name/repo-a`). Flattening risks slug
    /// collisions between same-named repos under different subfolders; that is
    /// the caller's call. Written `foldername = "..."` in TOML (`folder` alias).
    #[serde(default, alias = "folder")]
    pub foldername: Option<String>,
}

/// The whole config. `repos` are listed explicitly; `orgs` expand into more
/// `repos` at load.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct SprfConfig {
    #[serde(default)]
    pub repos: Vec<RepoConfig>,
    /// Folders of checkouts to expand into `repos` at load (the multi-root
    /// entry point). Written `[[org]]` in TOML (the `orgs` alias also works).
    /// See [`OrgConfig`].
    #[serde(default, alias = "org")]
    pub orgs: Vec<OrgConfig>,
    /// Prefix applied to any bare slug (no `/`) at load time. `None` leaves
    /// slugs untouched. Lets a multi-repo config share one org without
    /// repeating it per `[[repos]]` entry.
    #[serde(default)]
    pub default_org: Option<String>,
}

/// Default directory levels to search under an `[[org]] dir` when `max_depth`
/// is omitted.
const ORG_DEFAULT_DEPTH: usize = 3;

/// Known keys per config table, for the unknown-key warning. Aliases included so
/// the accepted spelling never warns.
const TOP_KEYS: &[&str] = &["repos", "orgs", "org", "default_org"];
const REPO_KEYS: &[&str] = &["slug", "root", "url", "allow_missing"];
const ORG_KEYS: &[&str] = &["dir", "max_depth", "foldername", "folder"];

/// Every config key not in the known set for its table, as `(table_label, key)`.
/// A renamed or misspelled field otherwise deserializes to its default and is
/// silently dropped — the classic "I set it and nothing happened" config trap.
fn unknown_keys(raw: &toml::Value) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let toml::Value::Table(top) = raw else { return out };
    for (k, v) in top {
        if !TOP_KEYS.contains(&k.as_str()) { out.push(("the config root", k.clone())); }
        // [[repos]] / [[org]] / [[orgs]] are arrays of tables; check each entry.
        let (entry_keys, label) = match k.as_str() {
            "repos" => (REPO_KEYS, "a [[repos]] entry"),
            "org" | "orgs" => (ORG_KEYS, "an [[org]] entry"),
            _ => continue,
        };
        if let toml::Value::Array(entries) = v {
            for entry in entries {
                if let toml::Value::Table(t) = entry {
                    for ek in t.keys() {
                        if !entry_keys.contains(&ek.as_str()) { out.push((label, ek.clone())); }
                    }
                }
            }
        }
    }
    out
}

/// Warn (stderr) on each unknown config key, naming the table and key.
fn warn_unknown_keys(raw: &toml::Value, path: &Path) {
    for (table, key) in unknown_keys(raw) {
        eprintln!("[config] {}: unknown key `{key}` in {table} — ignored (typo or renamed field?)",
            path.display());
    }
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
        // Warn (don't fail) on unknown keys: a typo'd or renamed field
        // (`folder` → `foldername`, `depth` vs `max_depth`) otherwise deserializes
        // to the default and is silently ignored. serde's `deny_unknown_fields`
        // would hard-error and break forward-compat; a raw-key diff warns and
        // proceeds. Parsed separately from the typed load so the struct derives
        // (Eq — toml::Value is not Eq) stay intact.
        if let Ok(raw) = toml::from_str::<toml::Value>(&text) {
            warn_unknown_keys(&raw, path);
        }
        let mut cfg: SprfConfig =
            toml::from_str(&text).with_context(|| format!("parse config {}", path.display()))?;
        cfg.expand_orgs();
        cfg.normalize_slugs();
        Ok(cfg)
    }

    /// Expand each `[[org]]` into one `RepoConfig` per git checkout under its
    /// `dir`, appended to `repos`. An explicit `[[repos]]` at the same root wins
    /// (org-discovered duplicates are dropped); a missing / unreadable `dir`
    /// logs one stderr line and is skipped. Slug = `<prefix>/<rel-path>` where
    /// `prefix` is `foldername` (default: `dir` basename), so repos of the same
    /// name under different org folders never collide — unless `foldername = "."`
    /// FLATTENS the prefix away (slug = the bare rel-path).
    fn expand_orgs(&mut self) {
        use std::collections::HashSet;
        let mut seen: HashSet<PathBuf> = self.repos.iter().map(|r| r.root.clone()).collect();
        for org in &self.orgs {
            let dir = expand_tilde(&org.dir);
            let org_name = org.foldername.clone().filter(|s| !s.is_empty())
                .unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "org".to_string()));
            let flatten = org_name == ".";
            if !dir.is_dir() {
                eprintln!("[config] org dir {} is not a directory; skipping", dir.display());
                continue;
            }
            let depth = org.max_depth.unwrap_or(ORG_DEFAULT_DEPTH);
            let mut repos = find_git_repos(&dir, depth);
            repos.sort();
            for root in repos {
                if !seen.insert(root.clone()) { continue; }
                // rel path under `dir`, `/`-joined, as the slug suffix.
                let rel = root.strip_prefix(&dir).unwrap_or(&root);
                let suffix = rel.components()
                    .map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
                // `dir` itself is a repo (empty suffix) → fall back to its basename.
                let leaf = || root.file_name().map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "repo".to_string());
                let slug = if flatten {
                    if suffix.is_empty() { leaf() } else { suffix }
                } else {
                    let suffix = if suffix.is_empty() { org_name.clone() } else { suffix };
                    format!("{org_name}/{suffix}")
                };
                self.repos.push(RepoConfig { slug, root, url: None, allow_missing: false });
            }
        }
    }

    /// Prefix `default_org/` onto any bare slug. Idempotent: slugs already
    /// containing `/` are left alone. No-op when `default_org` is `None`.
    fn normalize_slugs(&mut self) {
        if let Some(org) = &self.default_org {
            for r in &mut self.repos {
                if !r.slug.contains('/') {
                    r.slug = format!("{org}/{}", r.slug);
                }
            }
        }
    }
}

/// Expand a leading `~` (or `~/...`) to `$HOME`. Other paths pass through.
fn expand_tilde(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    } else if s == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home);
        }
    }
    p.to_path_buf()
}

/// Every git checkout under `dir`, searching at most `max_depth` directory
/// levels below it. A directory containing a `.git` entry IS a checkout: it is
/// recorded and NOT descended into (a repo's own subdirs / submodules are not
/// separate repos). `dir` itself counts as level 0.
fn find_git_repos(dir: &Path, max_depth: usize) -> Vec<PathBuf> {
    fn walk(d: &Path, depth: usize, max: usize, out: &mut Vec<PathBuf>) {
        if d.join(".git").exists() {
            out.push(d.to_path_buf());
            return; // don't descend into a checkout
        }
        if depth >= max { return; }
        if let Ok(entries) = std::fs::read_dir(d) {
            for e in entries.flatten() {
                let p = e.path();
                // Skip symlinks to avoid cycles; only recurse real dirs.
                if p.is_dir() && !e.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
                    walk(&p, depth + 1, max, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, 0, max_depth, &mut out);
    out
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

    #[test]
    fn default_org_prefixes_bare_slugs() {
        let p = write_tmp("org", "\
            default_org = \"alpha\"\n\
            [[repos]]\n\
            slug = \"one\"\n\
            root = \"/tmp/one\"\n\
            [[repos]]\n\
            slug = \"beta/two\"\n\
            root = \"/tmp/two\"\n");
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        assert_eq!(cfg.repos[0].slug, "alpha/one");
        assert_eq!(cfg.repos[1].slug, "beta/two");
    }

    #[test]
    fn no_default_org_leaves_slugs_alone() {
        let p = write_tmp("no_org", "\
            [[repos]]\n\
            slug = \"one\"\n\
            root = \"/tmp/one\"\n");
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        assert_eq!(cfg.repos[0].slug, "one");
    }

    /// Build an org-of-repos tree: `<base>/<org>/<repo>/.git` plus a nested
    /// `<org>/group/svc/.git`, and a non-repo dir that must be ignored.
    fn org_fixture(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("sprf_org_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for repo in ["grafana/mimir", "grafana/loki", "grafana/group/svc"] {
            std::fs::create_dir_all(base.join(repo).join(".git")).unwrap();
        }
        std::fs::create_dir_all(base.join("grafana/not-a-repo/src")).unwrap();
        base
    }

    #[test]
    fn unknown_config_keys_are_flagged_not_silently_dropped() {
        // A typo'd top-level key, a renamed org field, and an unknown repo key.
        let raw: toml::Value = toml::from_str("\
            default_orgz = \"acme\"\n\
            [[org]]\n\
            dir = \"/tmp/x\"\n\
            depth = 5\n\
            [[repos]]\n\
            slug = \"a\"\n\
            root = \"/tmp/a\"\n\
            urls = \"x\"\n").unwrap();
        let mut found = unknown_keys(&raw);
        found.sort();
        assert_eq!(found, vec![
            ("a [[repos]] entry", "urls".to_string()),
            ("an [[org]] entry", "depth".to_string()),
            ("the config root", "default_orgz".to_string()),
        ], "each unknown key is reported with its table; known keys + `folder` alias never flagged");

        // The accepted spellings + aliases must NOT warn.
        let ok: toml::Value = toml::from_str("\
            default_org = \"acme\"\n\
            [[org]]\n\
            dir = \"/tmp/x\"\n\
            folder = \".\"\n\
            max_depth = 2\n").unwrap();
        assert!(unknown_keys(&ok).is_empty(), "known keys + folder alias are clean");
    }

    #[test]
    fn org_dir_expands_into_repos() {
        let base = org_fixture("expand");
        let p = write_tmp("orgdir", &format!("\
            [[org]]\n\
            dir = \"{}/grafana\"\n", base.display()));
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        let mut slugs: Vec<_> = cfg.repos.iter().map(|r| r.slug.clone()).collect();
        slugs.sort();
        assert_eq!(slugs, vec![
            "grafana/group/svc".to_string(),
            "grafana/loki".to_string(),
            "grafana/mimir".to_string(),
        ], "one repo per checkout, rel-path slug, non-repo dir skipped");
        // Roots point at the checkout dirs.
        let mimir = cfg.repos.iter().find(|r| r.slug == "grafana/mimir").unwrap();
        assert_eq!(mimir.root, base.join("grafana/mimir"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn org_foldername_dot_flattens_the_slug() {
        let base = org_fixture("flat");
        // foldername = "." drops the org prefix: slugs are the bare rel-path.
        let p = write_tmp("orgflat", &format!("\
            [[org]]\n\
            dir = \"{}/grafana\"\n\
            foldername = \".\"\n", base.display()));
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        let mut slugs: Vec<_> = cfg.repos.iter().map(|r| r.slug.clone()).collect();
        slugs.sort();
        assert_eq!(slugs, vec![
            "group/svc".to_string(), "loki".to_string(), "mimir".to_string(),
        ], "foldername=. flattens: no `grafana/` prefix");
        // Root is unchanged — only the slug (address) flattens, not the path.
        let mimir = cfg.repos.iter().find(|r| r.slug == "mimir").unwrap();
        assert_eq!(mimir.root, base.join("grafana/mimir"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn org_foldername_overrides_the_prefix() {
        let base = org_fixture("rename");
        let p = write_tmp("orgrename", &format!("\
            [[org]]\n\
            dir = \"{}/grafana\"\n\
            foldername = \"gf\"\n", base.display()));
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        assert!(cfg.repos.iter().any(|r| r.slug == "gf/mimir"),
            "foldername renames the prefix: {:?}", cfg.repos.iter().map(|r| &r.slug).collect::<Vec<_>>());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn org_max_depth_caps_the_walk() {
        let base = org_fixture("depth");
        // depth 1: only immediate children (mimir, loki), not group/svc (depth 2).
        let p = write_tmp("orgdepth", &format!("\
            [[org]]\n\
            dir = \"{}/grafana\"\n\
            max_depth = 1\n", base.display()));
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        let mut slugs: Vec<_> = cfg.repos.iter().map(|r| r.slug.clone()).collect();
        slugs.sort();
        assert_eq!(slugs, vec!["grafana/loki".to_string(), "grafana/mimir".to_string()]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn explicit_repo_wins_over_org_duplicate() {
        let base = org_fixture("dup");
        let p = write_tmp("orgdup", &format!("\
            [[repos]]\n\
            slug = \"pinned/mimir\"\n\
            root = \"{mimir}\"\n\
            [[org]]\n\
            dir = \"{base}/grafana\"\n",
            mimir = base.join("grafana/mimir").display(), base = base.display()));
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        // mimir's root appears once, under the explicit slug.
        let for_mimir: Vec<_> = cfg.repos.iter()
            .filter(|r| r.root == base.join("grafana/mimir")).collect();
        assert_eq!(for_mimir.len(), 1, "org duplicate dropped");
        assert_eq!(for_mimir[0].slug, "pinned/mimir", "explicit slug wins");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn missing_org_dir_is_skipped_not_fatal() {
        let p = write_tmp("orgmissing", "\
            [[org]]\n\
            dir = \"/no/such/org/dir/xyz\"\n");
        let cfg = SprfConfig::load_from_path(&p).unwrap();
        assert!(cfg.repos.is_empty(), "a missing org dir yields no repos, no error");
    }
}
