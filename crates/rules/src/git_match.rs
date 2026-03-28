use globset::{Glob, GlobMatcher};

/// Compiled git selector for fast matching.
/// Built from Repo/Branch/Tag steps extracted from the select chain.
pub struct CompiledGitSelector {
    repo: Vec<GlobMatcher>,
    branch: Vec<GlobMatcher>,
    tag: Vec<GlobMatcher>,
}

impl std::fmt::Debug for CompiledGitSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledGitSelector")
            .field("repo_count", &self.repo.len())
            .field("branch_count", &self.branch.len())
            .field("tag_count", &self.tag.len())
            .finish()
    }
}

impl CompiledGitSelector {
    pub fn from_patterns(
        repo_patterns: &[&str],
        branch_patterns: &[&str],
        tag_patterns: &[&str],
    ) -> Result<Self, globset::Error> {
        let repo = compile_patterns(repo_patterns)?;
        let branch = compile_patterns(branch_patterns)?;
        let tag = compile_patterns(tag_patterns)?;
        Ok(Self { repo, branch, tag })
    }

    pub fn matches(&self, repo_name: &str, branch: Option<&str>, tags: &[&str]) -> bool {
        if !self.repo.is_empty() && !self.repo.iter().any(|m| m.is_match(repo_name)) {
            return false;
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

fn compile_patterns(patterns: &[&str]) -> Result<Vec<GlobMatcher>, globset::Error> {
    patterns
        .iter()
        .flat_map(|p| p.split('|').map(|s| s.trim()))
        .map(|p| Glob::new(p).map(|g| g.compile_matcher()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_selector_matches_everything() {
        let c = CompiledGitSelector::from_patterns(&[], &[], &[]).unwrap();
        assert!(c.matches("anything", Some("main"), &[]));
        assert!(c.matches("anything", None, &[]));
    }

    #[test]
    fn repo_glob() {
        let c = CompiledGitSelector::from_patterns(&["*/helm-charts"], &[], &[]).unwrap();
        assert!(c.matches("org/helm-charts", Some("main"), &[]));
        assert!(!c.matches("org/backend", Some("main"), &[]));
    }

    #[test]
    fn branch_pipe_pattern() {
        let c = CompiledGitSelector::from_patterns(&[], &["main|release/*"], &[]).unwrap();
        assert!(c.matches("repo", Some("main"), &[]));
        assert!(c.matches("repo", Some("release/v3"), &[]));
        assert!(!c.matches("repo", Some("feature/foo"), &[]));
        assert!(!c.matches("repo", None, &[]));
    }

    #[test]
    fn tag_matching() {
        let c = CompiledGitSelector::from_patterns(&[], &[], &["v*"]).unwrap();
        assert!(c.matches("repo", None, &["v1.0.0"]));
        assert!(c.matches("repo", None, &["latest", "v2.0"]));
        assert!(!c.matches("repo", None, &["latest"]));
        assert!(!c.matches("repo", None, &[]));
    }

    #[test]
    fn all_fields_must_match() {
        let c = CompiledGitSelector::from_patterns(
            &["org/*"],
            &["main"],
            &["v*"],
        )
        .unwrap();
        assert!(c.matches("org/repo", Some("main"), &["v1.0"]));
        assert!(!c.matches("other/repo", Some("main"), &["v1.0"]));
        assert!(!c.matches("org/repo", Some("dev"), &["v1.0"]));
        assert!(!c.matches("org/repo", Some("main"), &["latest"]));
    }
}
