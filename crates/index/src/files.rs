use std::path::{Path, PathBuf};

use anyhow::Result;
use sprefa_config::CompiledFilter;

/// Result of diffing two git commits.
pub struct DiffResult {
    /// Absolute paths of files added or modified (need extraction).
    pub changed: Vec<PathBuf>,
    /// Relative paths of files deleted (need branch_files cleanup).
    pub deleted: Vec<String>,
    /// Pure renames where content is identical (old_rel_path, new_rel_path).
    /// These only need a path update on the files row, no re-extraction.
    pub renamed: Vec<(String, String)>,
    /// HEAD sha after the diff.
    pub new_sha: String,
}

/// Compute files changed between `old_sha` and current HEAD.
/// Returns absolute paths for changed files, relative paths for deleted.
/// Errors if `old_sha` cannot be resolved (e.g. garbage collected after force push).
pub fn diff_files(
    repo_path: &Path,
    old_sha: &str,
    filter: Option<&CompiledFilter>,
) -> Result<DiffResult> {
    let repo = git2::Repository::open(repo_path)?;
    let old_oid = git2::Oid::from_str(old_sha)?;
    let old_commit = repo.find_commit(old_oid)?;
    let old_tree = old_commit.tree()?;

    let head = repo.head()?.peel_to_commit()?;
    let new_tree = head.tree()?;
    let new_sha = head.id().to_string();

    let diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;

    let mut changed = Vec::new();
    let mut deleted = Vec::new();
    let mut renamed = Vec::new();

    for delta in diff.deltas() {
        match delta.status() {
            git2::Delta::Added | git2::Delta::Modified | git2::Delta::Copied => {
                if let Some(path) = delta.new_file().path() {
                    let rel = path.to_string_lossy();
                    if filter.map(|f| f.allows(&rel)).unwrap_or(true) {
                        changed.push(repo_path.join(path));
                    }
                }
            }
            git2::Delta::Deleted => {
                if let Some(path) = delta.old_file().path() {
                    let rel = path.to_string_lossy().to_string();
                    if filter.map(|f| f.allows(&rel)).unwrap_or(true) {
                        deleted.push(rel);
                    }
                }
            }
            git2::Delta::Renamed => {
                let old_rel = delta.old_file().path().map(|p| p.to_string_lossy().to_string());
                let new_rel = delta.new_file().path().map(|p| p.to_string_lossy().to_string());
                let same_content = delta.old_file().id() == delta.new_file().id();

                match (old_rel, new_rel) {
                    (Some(old), Some(new)) if same_content => {
                        let old_ok = filter.map(|f| f.allows(&old)).unwrap_or(true);
                        let new_ok = filter.map(|f| f.allows(&new)).unwrap_or(true);
                        if old_ok && new_ok {
                            renamed.push((old, new));
                        } else {
                            // Filter mismatch: treat as delete + add independently.
                            if old_ok { deleted.push(old); }
                            if new_ok { changed.push(repo_path.join(&new)); }
                        }
                    }
                    (Some(old), Some(new)) => {
                        // Content changed during rename: delete old, extract new.
                        if filter.map(|f| f.allows(&old)).unwrap_or(true) {
                            deleted.push(old);
                        }
                        if filter.map(|f| f.allows(&new)).unwrap_or(true) {
                            changed.push(repo_path.join(&new));
                        }
                    }
                    (Some(old), None) => {
                        if filter.map(|f| f.allows(&old)).unwrap_or(true) {
                            deleted.push(old);
                        }
                    }
                    (None, Some(new)) => {
                        if filter.map(|f| f.allows(&new)).unwrap_or(true) {
                            changed.push(repo_path.join(&new));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    Ok(DiffResult { changed, deleted, renamed, new_sha })
}

/// List all indexable files under `repo_path`.
/// Uses git2 to enumerate committed files (respects .gitignore, works on any branch).
/// Falls back to walkdir if the directory is not a git repo.
/// Applies `filter` if provided.
pub fn list_files(repo_path: &Path, filter: Option<&CompiledFilter>) -> Result<Vec<PathBuf>> {
    let files = git2_list_files(repo_path).unwrap_or_else(|_| walkdir_files(repo_path));

    let filtered = files
        .into_iter()
        .filter(|p| {
            let rel = p.strip_prefix(repo_path).unwrap_or(p);
            let rel_str = rel.to_string_lossy();
            filter.map(|f: &CompiledFilter| f.allows(&rel_str)).unwrap_or(true)
        })
        .collect();

    Ok(filtered)
}

fn git2_list_files(repo_path: &Path) -> Result<Vec<PathBuf>> {
    let repo = git2::Repository::open(repo_path)?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    let tree = commit.tree()?;

    let mut paths = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            let rel = if root.is_empty() {
                PathBuf::from(entry.name().unwrap_or(""))
            } else {
                PathBuf::from(root).join(entry.name().unwrap_or(""))
            };
            paths.push(repo_path.join(rel));
        }
        git2::TreeWalkResult::Ok
    })?;

    Ok(paths)
}

/// Read all tags from a git repository.
/// Returns (tag_name, commit_hash) pairs. Annotated tags are peeled to their commit.
pub fn read_git_tags(repo_path: &Path) -> Result<Vec<(String, Option<String>)>> {
    let repo = git2::Repository::open(repo_path)?;
    let tag_names = repo.tag_names(None)?;
    let mut tags = Vec::with_capacity(tag_names.len());

    for name in tag_names.iter().flatten() {
        let refname = format!("refs/tags/{name}");
        let commit_hash = repo.find_reference(&refname).ok()
            .and_then(|r| r.peel_to_commit().ok())
            .map(|c| c.id().to_string());
        tags.push((name.to_string(), commit_hash));
    }

    Ok(tags)
}

/// Check whether a tag name looks like semver (v?MAJOR.MINOR.PATCH with optional suffix).
pub fn is_semver(name: &str) -> bool {
    let s = name.strip_prefix('v').unwrap_or(name);
    let mut parts = s.splitn(4, |c: char| c == '.' || c == '-' || c == '+');
    let major = parts.next().and_then(|p| p.parse::<u64>().ok());
    let minor = parts.next().and_then(|p| p.parse::<u64>().ok());
    let patch = parts.next().and_then(|p| p.parse::<u64>().ok());
    major.is_some() && minor.is_some() && patch.is_some()
}

fn walkdir_files(repo_path: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(repo_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.depth() == 1 && name.starts_with('.'))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{Oid, Repository, Signature};
    use std::fs;

    fn make_git_repo(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        // Need an initial commit so HEAD exists.
        let sig = Signature::now("test", "test@test.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        {
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        }
        repo
    }

    fn commit_file(repo: &Repository, path: &str, content: &[u8]) -> Oid {
        let root = repo.workdir().unwrap();
        let abs = root.join(path);
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&abs, content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("test", "test@test.com").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, &format!("add {path}"), &tree, &[&parent]).unwrap()
    }

    fn delete_file(repo: &Repository, path: &str) -> Oid {
        let root = repo.workdir().unwrap();
        fs::remove_file(root.join(path)).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("test", "test@test.com").unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, &format!("rm {path}"), &tree, &[&parent]).unwrap()
    }

    #[test]
    fn diff_detects_added_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        let old_sha = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        commit_file(&repo, "src/a.ts", b"hello");

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert_eq!(result.changed.len(), 1);
        assert!(result.changed[0].ends_with("src/a.ts"));
        assert!(result.deleted.is_empty());
        assert!(result.renamed.is_empty());
    }

    #[test]
    fn diff_detects_deleted_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "src/a.ts", b"hello");
        let old_sha = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        delete_file(&repo, "src/a.ts");

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert!(result.changed.is_empty());
        assert_eq!(result.deleted, vec!["src/a.ts"]);
    }

    #[test]
    fn diff_detects_modified_file() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "src/a.ts", b"v1");
        let old_sha = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        commit_file(&repo, "src/a.ts", b"v2");

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert_eq!(result.changed.len(), 1);
        assert!(result.changed[0].ends_with("src/a.ts"));
    }

    #[test]
    fn diff_bad_sha_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        make_git_repo(tmp.path());
        assert!(diff_files(tmp.path(), "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", None).is_err());
    }

    #[test]
    fn is_semver_valid() {
        assert!(is_semver("1.0.0"));
        assert!(is_semver("v1.0.0"));
        assert!(is_semver("v2.3.4-rc1"));
        assert!(is_semver("0.1.0+build.123"));
    }

    #[test]
    fn is_semver_invalid() {
        assert!(!is_semver("latest"));
        assert!(!is_semver("v1"));
        assert!(!is_semver("1.0"));
        assert!(!is_semver("release-2024"));
    }

    #[test]
    fn read_tags_from_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        commit_file(&repo, "a.txt", b"data");

        // Create a lightweight tag.
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.tag_lightweight("v1.0.0", head.as_object(), false).unwrap();

        let tags = read_git_tags(tmp.path()).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].0, "v1.0.0");
        assert!(tags[0].1.is_some());
    }

    #[test]
    fn diff_returns_new_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = make_git_repo(tmp.path());
        let old_sha = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
        commit_file(&repo, "x.txt", b"data");
        let expected = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

        let result = diff_files(tmp.path(), &old_sha, None).unwrap();
        assert_eq!(result.new_sha, expected);
    }
}
