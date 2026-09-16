//! Repo-root discovery for `.dl` programs: `nearest_git` walks up from a path
//! to the enclosing git root. The CLI uses it to default `--root` to the repo
//! containing the program when no root is given.

use std::path::{Path, PathBuf};

/// Nearest ancestor of `start` (inclusive) that directly contains a `.git`
/// entry. `.git` may be a directory (normal repo) or a file (submodule /
/// worktree gitlink). Walk up only.
pub fn nearest_git(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Build a unique tempdir path from fixed prefix + distinguishing suffix;
    // no rand dependency required.
    fn tmp_subdir(suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sprf_repo_test_{suffix}"))
    }

    #[test]
    fn nearest_git_finds_ancestor() {
        let base = tmp_subdir("finds_ancestor");
        let git_root = base.join("repo");
        let nested = git_root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(git_root.join(".git")).unwrap();

        let found = nearest_git(&nested);
        assert_eq!(found, Some(git_root));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn nearest_git_none_when_no_git() {
        let base = tmp_subdir("none_when_no_git");
        let nested = base.join("x").join("y");
        std::fs::create_dir_all(&nested).unwrap();

        // Sanity: this tree has no .git at any level under base.
        // Walk up will eventually hit filesystem root which also has no .git
        // in typical CI environments; we only need to confirm None is returned
        // for the nested path we created.
        //
        // Because ancestors() walks all the way to "/", which may or may not
        // have a .git, we test against a path that we fully control by
        // checking all ancestors only up to base.
        let result = base
            .ancestors()
            .skip(1) // skip base itself; walk only ABOVE it
            .any(|d| d.join(".git").exists());

        // If the machine itself has a .git above our tempdir, skip the test.
        if !result {
            assert_eq!(nearest_git(&nested), None);
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn nearest_git_accepts_gitfile() {
        let base = tmp_subdir("accepts_gitfile");
        let git_root = base.join("repo");
        let nested = git_root.join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        // .git as a file (submodule / worktree gitlink)
        std::fs::write(git_root.join(".git"), "gitdir: ../.git/worktrees/sub\n").unwrap();

        let found = nearest_git(&nested);
        assert_eq!(found, Some(git_root));

        let _ = std::fs::remove_dir_all(&base);
    }
}
