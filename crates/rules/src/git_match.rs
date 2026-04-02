use std::collections::HashMap;

use crate::pattern::{compile_patterns, PatternMatcher};

/// Compiled git selector for fast matching.
/// Built from Repo/Rev steps extracted from the select chain.
pub struct CompiledGitSelector {
    repo: Vec<PatternMatcher>,
    rev: Vec<PatternMatcher>,
}

impl std::fmt::Debug for CompiledGitSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompiledGitSelector")
            .field("repo_count", &self.repo.len())
            .field("rev_count", &self.rev.len())
            .finish()
    }
}

impl CompiledGitSelector {
    pub fn from_patterns(
        repo_patterns: &[&str],
        rev_patterns: &[&str],
    ) -> anyhow::Result<Self> {
        let repo = compile_patterns(repo_patterns)?;
        let rev = compile_patterns(rev_patterns)?;
        Ok(Self { repo, rev })
    }

    pub fn matches(&self, repo_name: &str, branch: Option<&str>, tags: &[&str]) -> bool {
        if !self.repo.is_empty() && !self.repo.iter().any(|m| m.is_match(repo_name)) {
            return false;
        }

        if !self.rev.is_empty() {
            // Rev matches against branch OR any tag
            let branch_hit = branch.map_or(false, |b| self.rev.iter().any(|m| m.is_match(b)));
            let tag_hit = tags.iter().any(|t| self.rev.iter().any(|m| m.is_match(t)));
            if !branch_hit && !tag_hit {
                return false;
            }
        }

        true
    }

    /// Like `matches`, but also returns segment/regex captures from patterns.
    /// Captures from repo and rev patterns are merged into one map.
    pub fn matches_with_captures(
        &self,
        repo_name: &str,
        branch: Option<&str>,
        tags: &[&str],
    ) -> Option<HashMap<String, String>> {
        let mut caps = HashMap::new();

        if !self.repo.is_empty() {
            let mut hit = false;
            for m in &self.repo {
                if m.is_match(repo_name) {
                    hit = true;
                    if let Some(c) = m.captures(repo_name) {
                        caps.extend(c);
                    }
                    break;
                }
            }
            if !hit {
                return None;
            }
        }

        if !self.rev.is_empty() {
            let mut hit = false;
            // Try branch first
            if let Some(b) = branch {
                for m in &self.rev {
                    if m.is_match(b) {
                        hit = true;
                        if let Some(c) = m.captures(b) {
                            caps.extend(c);
                        }
                        break;
                    }
                }
            }
            // Try tags if branch didn't hit
            if !hit {
                for t in tags {
                    for m in &self.rev {
                        if m.is_match(t) {
                            hit = true;
                            if let Some(c) = m.captures(t) {
                                caps.extend(c);
                            }
                            break;
                        }
                    }
                    if hit {
                        break;
                    }
                }
            }
            if !hit {
                return None;
            }
        }

        Some(caps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_selector_matches_everything() {
        let c = CompiledGitSelector::from_patterns(&[], &[]).unwrap();
        assert!(c.matches("anything", Some("main"), &[]));
        assert!(c.matches("anything", None, &[]));
    }

    #[test]
    fn repo_glob() {
        let c = CompiledGitSelector::from_patterns(&["*/deploy-charts"], &[]).unwrap();
        assert!(c.matches("org/deploy-charts", Some("main"), &[]));
        assert!(!c.matches("org/backend", Some("main"), &[]));
    }

    #[test]
    fn rev_matches_branch() {
        let c = CompiledGitSelector::from_patterns(&[], &["main|release/*"]).unwrap();
        assert!(c.matches("repo", Some("main"), &[]));
        assert!(c.matches("repo", Some("release/v3"), &[]));
        assert!(!c.matches("repo", Some("feature/foo"), &[]));
        assert!(!c.matches("repo", None, &[]));
    }

    #[test]
    fn rev_matches_tag() {
        let c = CompiledGitSelector::from_patterns(&[], &["v*"]).unwrap();
        assert!(c.matches("repo", None, &["v1.0.0"]));
        assert!(c.matches("repo", None, &["latest", "v2.0"]));
        assert!(!c.matches("repo", None, &["latest"]));
        assert!(!c.matches("repo", None, &[]));
    }

    #[test]
    fn rev_matches_branch_or_tag() {
        let c = CompiledGitSelector::from_patterns(&[], &["main|v*"]).unwrap();
        assert!(c.matches("repo", Some("main"), &[]));
        assert!(c.matches("repo", None, &["v1.0"]));
        assert!(c.matches("repo", Some("dev"), &["v2.0"]));
        assert!(!c.matches("repo", Some("dev"), &["latest"]));
    }

    #[test]
    fn all_fields_must_match() {
        let c = CompiledGitSelector::from_patterns(
            &["org/*"],
            &["main"],
        )
        .unwrap();
        assert!(c.matches("org/repo", Some("main"), &[]));
        assert!(!c.matches("other/repo", Some("main"), &[]));
        assert!(!c.matches("org/repo", Some("dev"), &[]));
    }
}
