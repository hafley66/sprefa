//! Classify a rev spec into mutable / immutable. Drives whether the future
//! watcher should call `drop_rev` on a ref-move event.
//!
//! Pure probe: no I/O beyond the locator's existing `locate()` and shelled
//! `git show-ref`. Standalone module; nothing in production calls it yet.

use super::_1_locator::CheckoutLocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevKind {
    /// 40-hex SHA. Immutable by construction.
    Sha,
    /// Resolved via `refs/tags/`. Treated as immutable (opportunistic;
    /// `git tag -f` is rare and requires explicit admin drop_rev).
    Tag,
    /// Resolved via `refs/heads/`. Mutable; drop on ref-move.
    Branch,
    /// Literal `HEAD`. Mutable; drop on ref-move.
    Head,
    /// Not resolvable via the locator's repo. Conservatively mutable.
    Unknown,
}

impl RevKind {
    pub fn is_mutable(self) -> bool {
        matches!(self, RevKind::Branch | RevKind::Head | RevKind::Unknown)
    }
}

/// Classify `rev` against `locator`'s repo. Cheap: hex-check + at most two
/// `git show-ref --verify` probes when the rev is a symbolic name.
pub fn classify(locator: &dyn CheckoutLocator, repo: &str, rev: &str) -> RevKind {
    if rev == "HEAD" { return RevKind::Head; }
    if is_hex_40(rev) { return RevKind::Sha; }

    let Some(repo_path) = locator.locate(repo, "HEAD") else {
        return RevKind::Unknown;
    };

    let is_branch = std::process::Command::new("git")
        .args(["show-ref", "--verify", "--quiet",
               &format!("refs/heads/{rev}")])
        .current_dir(&repo_path)
        .status().map(|s| s.success()).unwrap_or(false);
    if is_branch { return RevKind::Branch; }

    let is_tag = std::process::Command::new("git")
        .args(["show-ref", "--verify", "--quiet",
               &format!("refs/tags/{rev}")])
        .current_dir(&repo_path)
        .status().map(|s| s.success()).unwrap_or(false);
    if is_tag { return RevKind::Tag; }

    RevKind::Unknown
}

fn is_hex_40(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b|
        matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command as StdCmd;
    use std::sync::Arc;
    use tempfile::TempDir;

    struct NoopLocator;
    impl CheckoutLocator for NoopLocator {
        fn locate(&self, _: &str, _: &str) -> Option<PathBuf> { None }
        fn repos(&self) -> Vec<Arc<str>> { vec![] }
        fn revs(&self, _: &str) -> Vec<Arc<str>> { vec![] }
    }

    struct SingleRepoLocator(PathBuf);
    impl CheckoutLocator for SingleRepoLocator {
        fn locate(&self, _: &str, _: &str) -> Option<PathBuf> { Some(self.0.clone()) }
        fn repos(&self) -> Vec<Arc<str>> { vec![Arc::from("r")] }
        fn revs(&self, _: &str) -> Vec<Arc<str>> { vec![Arc::from("main")] }
    }

    fn make_fixture_with_branch_and_tag() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        StdCmd::new("git").args(["init", "-b", "main"]).current_dir(p).status().unwrap();
        StdCmd::new("git").args(["config", "user.email", "t@t.com"]).current_dir(p).status().unwrap();
        StdCmd::new("git").args(["config", "user.name", "T"]).current_dir(p).status().unwrap();
        std::fs::write(p.join("a.txt"), "hello").unwrap();
        StdCmd::new("git").args(["add", "."]).current_dir(p).status().unwrap();
        StdCmd::new("git").args(["commit", "-m", "init"]).current_dir(p).status().unwrap();
        StdCmd::new("git").args(["tag", "v1.0.0"]).current_dir(p).status().unwrap();
        dir
    }

    #[test]
    fn sha_is_immutable() {
        assert_eq!(
            classify(&NoopLocator, "r", "0123456789abcdef0123456789abcdef01234567"),
            RevKind::Sha,
        );
        assert!(!RevKind::Sha.is_mutable());
    }

    #[test]
    fn head_is_mutable() {
        assert_eq!(classify(&NoopLocator, "r", "HEAD"), RevKind::Head);
        assert!(RevKind::Head.is_mutable());
    }

    #[test]
    fn unknown_is_mutable_when_locator_empty() {
        assert_eq!(classify(&NoopLocator, "r", "something-weird"), RevKind::Unknown);
        assert!(RevKind::Unknown.is_mutable());
    }

    #[test]
    fn branch_and_tag_via_fixture() {
        let fixture = make_fixture_with_branch_and_tag();
        let loc = SingleRepoLocator(fixture.path().to_path_buf());
        assert_eq!(classify(&loc, "r", "main"), RevKind::Branch);
        assert!(RevKind::Branch.is_mutable());
        assert_eq!(classify(&loc, "r", "v1.0.0"), RevKind::Tag);
        assert!(!RevKind::Tag.is_mutable());
    }
}
