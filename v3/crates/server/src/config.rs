//! `.sprefa.toml` — workspace config for the CLI driver.
//!
//! Minimal v3 shape: a list of seeds, each naming one (repo slug, root
//! path, rev) triple. sprefa-run iterates each seed through the pipes
//! in the `.sprf` source. Missing fields fall back to CLI flags.
//!
//! Format:
//!
//! ```toml
//! [[seed]]
//! slug = "sprefa"
//! root = "."
//! rev  = "HEAD"
//!
//! [[seed]]
//! slug = "other"
//! root = "../other"
//! rev  = "main"
//! ```
//!
//! Resolution precedence (highest first):
//!   1. `--config <path>` flag.
//!   2. `.sprefa.toml` in CWD.
//!   3. CLI single-seed: `--root` + `--rev`.
//!
//! Step 2 is deliberately not an ancestor walk in this slice — an
//! ancestor walk adds edge cases (which dir is the workspace root?)
//! and the smoke run already lives in the repo root. Follow-up.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One seed cursor row in the config. `slug` becomes `cursor.repo`;
/// `root` is the fs base for the `DiskFileSource`; `rev` is
/// `cursor.rev` plus the key that gates file listings.
#[derive(Debug, Clone)]
pub struct Seed {
    pub slug: String,
    pub root: PathBuf,
    pub rev:  String,
}

/// Resolved workspace config used by sprefa-run.
#[derive(Debug, Clone)]
pub struct Config {
    pub seeds: Vec<Seed>,
}

/// TOML-wire shape. Intermediate, not exposed.
#[derive(Debug, Deserialize)]
struct ConfigWire {
    #[serde(default)]
    seed: Vec<SeedWire>,
}

#[derive(Debug, Deserialize)]
struct SeedWire {
    slug: String,
    root: String,
    #[serde(default = "default_rev")]
    rev:  String,
}

fn default_rev() -> String { "HEAD".to_string() }

/// Parse a TOML string into a `Config`. Paths in `root` are resolved
/// relative to `base_dir` (typically the directory containing the
/// config file). Returns the intermediate error shape on failure so
/// the caller can format it with context.
pub fn from_toml_str(src: &str, base_dir: &Path) -> Result<Config, String> {
    let wire: ConfigWire = toml::from_str(src)
        .map_err(|e| format!("config parse: {e}"))?;
    if wire.seed.is_empty() {
        return Err("config: [[seed]] table is required and must not be empty".into());
    }
    let seeds = wire.seed.into_iter().map(|s| {
        let root = if Path::new(&s.root).is_absolute() {
            PathBuf::from(&s.root)
        } else {
            base_dir.join(&s.root)
        };
        Seed { slug: s.slug, root, rev: s.rev }
    }).collect();
    Ok(Config { seeds })
}

/// Load from a file path. The parent directory anchors relative roots.
pub fn from_path(path: &Path) -> Result<Config, String> {
    let src = fs::read_to_string(path)
        .map_err(|e| format!("read {path:?}: {e}"))?;
    let base = path.parent().unwrap_or(Path::new("."));
    from_toml_str(&src, base)
}

/// CLI single-seed fallback when no config file is in play.
pub fn from_cli(root: PathBuf, rev: String) -> Config {
    let slug = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("root")
        .to_string();
    Config { seeds: vec![Seed { slug, root, rev }] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_seed() {
        let toml = r#"
            [[seed]]
            slug = "sprefa"
            root = "."
        "#;
        let cfg = from_toml_str(toml, Path::new("/work")).unwrap();
        assert_eq!(cfg.seeds.len(), 1);
        assert_eq!(cfg.seeds[0].slug, "sprefa");
        assert_eq!(cfg.seeds[0].root, PathBuf::from("/work/."));
        assert_eq!(cfg.seeds[0].rev, "HEAD");
    }

    #[test]
    fn parses_multiple_seeds_with_explicit_rev() {
        let toml = r#"
            [[seed]]
            slug = "a"
            root = "./a"
            rev  = "main"

            [[seed]]
            slug = "b"
            root = "/abs/b"
            rev  = "v1.0"
        "#;
        let cfg = from_toml_str(toml, Path::new("/base")).unwrap();
        assert_eq!(cfg.seeds.len(), 2);
        assert_eq!(cfg.seeds[0].root, PathBuf::from("/base/./a"));
        assert_eq!(cfg.seeds[0].rev, "main");
        assert_eq!(cfg.seeds[1].root, PathBuf::from("/abs/b"));
        assert_eq!(cfg.seeds[1].rev, "v1.0");
    }

    #[test]
    fn empty_seed_table_rejected() {
        let toml = "# no seeds\n";
        let err = from_toml_str(toml, Path::new(".")).unwrap_err();
        assert!(err.contains("seed"));
    }

    #[test]
    fn from_cli_uses_basename_as_slug() {
        let cfg = from_cli(PathBuf::from("/work/sprefa"), "HEAD".into());
        assert_eq!(cfg.seeds.len(), 1);
        assert_eq!(cfg.seeds[0].slug, "sprefa");
    }
}
