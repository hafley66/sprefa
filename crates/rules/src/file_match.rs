use globset::{Glob, GlobMatcher};
use regex::Regex;

/// A single compiled pattern: either a glob or a regex.
enum PatternMatcher {
    Glob(GlobMatcher),
    Regex(Regex),
}

impl PatternMatcher {
    fn is_match(&self, value: &str) -> bool {
        match self {
            Self::Glob(g) => g.is_match(value),
            Self::Regex(r) => r.is_match(value),
        }
    }
}

/// Compiled file/folder selector for fast path matching.
/// Built from File/Folder steps extracted from the select chain.
/// Supports glob patterns and `re:` prefixed regex patterns.
pub struct CompiledFileSelector {
    matchers: Vec<PatternMatcher>,
}

impl std::fmt::Debug for CompiledFileSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledFileSelector")
            .field("pattern_count", &self.matchers.len())
            .finish()
    }
}

impl CompiledFileSelector {
    /// Compile from pipe-delimited pattern strings.
    /// Patterns prefixed with `re:` are treated as regex, otherwise as glob.
    pub fn from_patterns(patterns: &[&str]) -> anyhow::Result<Self> {
        let mut matchers = Vec::new();
        for p in patterns {
            if let Some(re_pattern) = p.strip_prefix("re:") {
                matchers.push(PatternMatcher::Regex(Regex::new(re_pattern)?));
            } else {
                for segment in p.split('|') {
                    let segment = segment.trim();
                    matchers.push(PatternMatcher::Glob(
                        Glob::new(segment)?.compile_matcher(),
                    ));
                }
            }
        }
        Ok(Self { matchers })
    }

    /// Check if a repo-relative file path matches any pattern.
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
