use globset::{Glob, GlobMatcher};

use crate::types::GitSelector;

/// Compiled git selector for fast matching.
pub struct CompiledGitSelector {
    repo: Option<GlobMatcher>,
    branch: Vec<GlobMatcher>,
    tag: Vec<GlobMatcher>,
}

impl CompiledGitSelector {
    pub fn compile(sel: &GitSelector) -> Result<Self, globset::Error> {
        let repo = sel
            .repo
            .as_deref()
            .map(|p| Glob::new(p).map(|g| g.compile_matcher()))
            .transpose()?;

        let branch = compile_pipe_pattern(sel.branch.as_deref())?;
        let tag = compile_pipe_pattern(sel.tag.as_deref())?;

        Ok(Self { repo, branch, tag })
    }

    /// Check if this selector matches the given git context.
    /// None fields in the context are treated as "not available" and
    /// will fail to match if the selector requires them.
    pub fn matches(&self, repo_name: &str, branch: Option<&str>, tags: &[&str]) -> bool {
        if let Some(ref m) = self.repo {
            if !m.is_match(repo_name) {
                return false;
            }
        }

        if !self.branch.is_empty() {
            match branch {
                Some(b) => {
                    if !self.branch.iter().any(|m| m.is_match(b)) {
                        return false;
                    }
                }
                None => return false,
            }
        }

        if !self.tag.is_empty() {
            if !tags.iter().any(|t| self.tag.iter().any(|m| m.is_match(t))) {
                return false;
            }
        }

        true
    }
}

/// Parse pipe-delimited glob alternatives: `"main|release/*"` -> two matchers.
fn compile_pipe_pattern(pattern: Option<&str>) -> Result<Vec<GlobMatcher>, globset::Error> {
    match pattern {
        None => Ok(vec![]),
        Some(p) => p
            .split('|')
            .map(|seg| Glob::new(seg.trim()).map(|g| g.compile_matcher()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(repo: Option<&str>, branch: Option<&str>, tag: Option<&str>) -> GitSelector {
        GitSelector {
            repo: repo.map(String::from),
            branch: branch.map(String::from),
            tag: tag.map(String::from),
        }
    }

    #[test]
    fn empty_selector_matches_everything() {
        let c = CompiledGitSelector::compile(&sel(None, None, None)).unwrap();
        assert!(c.matches("anything", Some("main"), &[]));
        assert!(c.matches("anything", None, &[]));
    }

    #[test]
    fn repo_glob() {
        let c = CompiledGitSelector::compile(&sel(Some("*/helm-charts"), None, None)).unwrap();
        assert!(c.matches("org/helm-charts", Some("main"), &[]));
        assert!(!c.matches("org/backend", Some("main"), &[]));
    }

    #[test]
    fn branch_pipe_pattern() {
        let c =
            CompiledGitSelector::compile(&sel(None, Some("main|release/*"), None)).unwrap();
        assert!(c.matches("repo", Some("main"), &[]));
        assert!(c.matches("repo", Some("release/v3"), &[]));
        assert!(!c.matches("repo", Some("feature/foo"), &[]));
        assert!(!c.matches("repo", None, &[]));
    }

    #[test]
    fn tag_matching() {
        let c = CompiledGitSelector::compile(&sel(None, None, Some("v*"))).unwrap();
        assert!(c.matches("repo", None, &["v1.0.0"]));
        assert!(c.matches("repo", None, &["latest", "v2.0"]));
        assert!(!c.matches("repo", None, &["latest"]));
        assert!(!c.matches("repo", None, &[]));
    }

    #[test]
    fn all_fields_must_match() {
        let c = CompiledGitSelector::compile(&sel(
            Some("org/*"),
            Some("main"),
            Some("v*"),
        ))
        .unwrap();
        assert!(c.matches("org/repo", Some("main"), &["v1.0"]));
        assert!(!c.matches("other/repo", Some("main"), &["v1.0"]));
        assert!(!c.matches("org/repo", Some("dev"), &["v1.0"]));
        assert!(!c.matches("org/repo", Some("main"), &["latest"]));
    }
}
