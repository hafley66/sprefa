use globset::{Glob, GlobMatcher};

use crate::types::FileSelector;

/// Compiled file selector for fast path matching.
pub struct CompiledFileSelector {
    matchers: Vec<GlobMatcher>,
}

impl CompiledFileSelector {
    pub fn compile(sel: &FileSelector) -> Result<Self, globset::Error> {
        let patterns = match sel {
            FileSelector::Single(p) => vec![p.as_str()],
            FileSelector::Multiple(ps) => ps.iter().map(|s| s.as_str()).collect(),
        };

        let matchers = patterns
            .into_iter()
            .map(|p| Glob::new(p).map(|g| g.compile_matcher()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { matchers })
    }

    /// Check if a repo-relative file path matches any of the globs.
    pub fn matches(&self, path: &str) -> bool {
        self.matchers.iter().any(|m| m.is_match(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_glob() {
        let sel = FileSelector::Single("**/*.json".into());
        let c = CompiledFileSelector::compile(&sel).unwrap();
        assert!(c.matches("foo/bar.json"));
        assert!(c.matches("bar.json"));
        assert!(!c.matches("bar.yaml"));
    }

    #[test]
    fn multiple_globs() {
        let sel = FileSelector::Multiple(vec![
            "*.yaml".into(),
            "*.yml".into(),
            "templates/**/*.yaml".into(),
        ]);
        let c = CompiledFileSelector::compile(&sel).unwrap();
        assert!(c.matches("values.yaml"));
        assert!(c.matches("config.yml"));
        assert!(c.matches("templates/deploy/service.yaml"));
        assert!(!c.matches("src/main.rs"));
    }

    #[test]
    fn exact_filename() {
        let sel = FileSelector::Single("Cargo.toml".into());
        let c = CompiledFileSelector::compile(&sel).unwrap();
        assert!(c.matches("Cargo.toml"));
        assert!(!c.matches("crates/config/Cargo.toml"));
    }

    #[test]
    fn recursive_exact() {
        let sel = FileSelector::Single("**/Cargo.toml".into());
        let c = CompiledFileSelector::compile(&sel).unwrap();
        assert!(c.matches("Cargo.toml"));
        assert!(c.matches("crates/config/Cargo.toml"));
    }
}
