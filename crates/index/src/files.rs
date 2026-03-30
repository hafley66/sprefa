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
