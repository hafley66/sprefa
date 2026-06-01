//! Repo-root resolution for `.dl` programs.
//!
//! Two entry points:
//!   - `nearest_git`: walk up from a path to find the enclosing git root.
//!   - `resolve_self_root`: map a `repo` directive to an on-disk path.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::ast::RepoSpec;
use crate::config::RepoConfig;

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

/// Resolve the repo source root for a `.dl` program's `repo` directive.
///
/// `dl_dir`   - directory containing the `.dl` file (base for relative paths
///              and for `Nearest` discovery).
/// `_cli_root` - `--root` CLI fallback; accepted for API symmetry but unused
///              here (relative `Path` variants join `dl_dir`, not CLI root).
/// `repos`    - configured repo list.
pub fn resolve_self_root(
    spec: &RepoSpec,
    dl_dir: &Path,
    _cli_root: &Path,
    repos: &[RepoConfig],
) -> Result<PathBuf> {
    match spec {
        RepoSpec::Nearest => {
            nearest_git(dl_dir).ok_or_else(|| {
                anyhow::anyhow!(
                    "no .git ancestor found starting from {}",
                    dl_dir.display()
                )
            })
        }
        RepoSpec::Slug(s) => repos
            .iter()
            .find(|r| &r.slug == s)
            .map(|r| r.root.clone())
            .ok_or_else(|| anyhow::anyhow!("no repo configured with slug {:?}", s)),
        RepoSpec::Path(p) => {
            if p.is_absolute() {
                Ok(p.clone())
            } else {
                Ok(dl_dir.join(p))
            }
        }
    }
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

    #[test]
    fn resolve_slug() {
        let repos = vec![
            RepoConfig { slug: "alpha/one".into(), root: PathBuf::from("/tmp/alpha") },
            RepoConfig { slug: "beta/two".into(), root: PathBuf::from("/tmp/beta") },
        ];
        let spec = RepoSpec::Slug("beta/two".into());
        let result =
            resolve_self_root(&spec, Path::new("/some/dl/dir"), Path::new("/cli"), &repos)
                .unwrap();
        assert_eq!(result, PathBuf::from("/tmp/beta"));
    }

    #[test]
    fn resolve_slug_missing_is_error() {
        let repos = vec![RepoConfig { slug: "alpha/one".into(), root: PathBuf::from("/tmp/alpha") }];
        let spec = RepoSpec::Slug("ghost".into());
        let err =
            resolve_self_root(&spec, Path::new("/dl"), Path::new("/cli"), &repos).unwrap_err();
        assert!(err.to_string().contains("ghost"));
    }

    #[test]
    fn resolve_path_absolute() {
        let spec = RepoSpec::Path(PathBuf::from("/absolute/root"));
        let result =
            resolve_self_root(&spec, Path::new("/dl/dir"), Path::new("/cli"), &[]).unwrap();
        assert_eq!(result, PathBuf::from("/absolute/root"));
    }

    #[test]
    fn resolve_path_relative() {
        let spec = RepoSpec::Path(PathBuf::from("../sibling"));
        let result =
            resolve_self_root(&spec, Path::new("/dl/dir"), Path::new("/cli"), &[]).unwrap();
        assert_eq!(result, PathBuf::from("/dl/dir/../sibling"));
    }

    #[test]
    fn resolve_nearest() {
        let base = tmp_subdir("resolve_nearest");
        let git_root = base.join("proj");
        let dl_dir = git_root.join("src");
        std::fs::create_dir_all(&dl_dir).unwrap();
        std::fs::create_dir_all(git_root.join(".git")).unwrap();

        let spec = RepoSpec::Nearest;
        let result = resolve_self_root(&spec, &dl_dir, Path::new("/cli"), &[]).unwrap();
        assert_eq!(result, git_root);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_nearest_no_git_is_error() {
        let base = tmp_subdir("nearest_no_git_error");
        let dl_dir = base.join("orphan");
        std::fs::create_dir_all(&dl_dir).unwrap();

        // Only meaningful if no ancestor of dl_dir has .git (true in practice
        // for a fresh temp subdir on a typical machine).
        let has_git_above =
            dl_dir.ancestors().skip(1).any(|d| d.join(".git").exists());
        if !has_git_above {
            let spec = RepoSpec::Nearest;
            let err =
                resolve_self_root(&spec, &dl_dir, Path::new("/cli"), &[]).unwrap_err();
            assert!(err.to_string().contains(".git"));
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}
