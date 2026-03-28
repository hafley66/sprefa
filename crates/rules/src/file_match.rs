use globset::{Glob, GlobMatcher};

/// Compiled file/folder selector for fast path matching.
/// Built from File/Folder steps extracted from the select chain.
pub struct CompiledFileSelector {
    matchers: Vec<GlobMatcher>,
}

impl std::fmt::Debug for CompiledFileSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledFileSelector")
            .field("pattern_count", &self.matchers.len())
            .finish()
    }
}

impl CompiledFileSelector {
    /// Compile from pipe-delimited glob pattern strings.
    pub fn from_patterns(patterns: &[&str]) -> Result<Self, globset::Error> {
        let matchers = patterns
            .iter()
            .flat_map(|p| p.split('|').map(|s| s.trim()))
            .map(|p| Glob::new(p).map(|g| g.compile_matcher()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { matchers })
    }

    /// Check if a repo-relative file path matches any of the globs.
    pub fn matches(&self, path: &str) -> bool {
        self.matchers.iter().any(|m| m.is_match(path))
    }

    /// Returns true if no patterns were configured (match everything).
    pub fn is_empty(&self) -> bool {
        self.matchers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_glob() {
        let c = CompiledFileSelector::from_patterns(&["**/*.json"]).unwrap();
        assert!(c.matches("foo/bar.json"));
        assert!(c.matches("bar.json"));
        assert!(!c.matches("bar.yaml"));
    }

    #[test]
    fn multiple_patterns() {
        let c = CompiledFileSelector::from_patterns(&[
            "*.yaml|*.yml",
            "templates/**/*.yaml",
        ]).unwrap();
        assert!(c.matches("values.yaml"));
        assert!(c.matches("config.yml"));
        assert!(c.matches("templates/deploy/service.yaml"));
        assert!(!c.matches("src/main.rs"));
    }

    #[test]
    fn exact_filename() {
        let c = CompiledFileSelector::from_patterns(&["Cargo.toml"]).unwrap();
        assert!(c.matches("Cargo.toml"));
        assert!(!c.matches("crates/config/Cargo.toml"));
    }

    #[test]
    fn recursive_exact() {
        let c = CompiledFileSelector::from_patterns(&["**/Cargo.toml"]).unwrap();
        assert!(c.matches("Cargo.toml"));
        assert!(c.matches("crates/config/Cargo.toml"));
    }
}
