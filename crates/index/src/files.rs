use std::path::{Path, PathBuf};

use anyhow::Result;
use sprefa_config::CompiledFilter;

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
