use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Result;
use sprefa_config::CompiledFilter;

/// List all indexable files under `repo_path`.
/// Tries `git ls-files` first (respects .gitignore), falls back to walkdir.
/// Applies `filter` if provided.
pub fn list_files(repo_path: &Path, filter: Option<&CompiledFilter>) -> Result<Vec<PathBuf>> {
    let files = git_ls_files(repo_path).unwrap_or_else(|_| walkdir_files(repo_path));

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

fn git_ls_files(repo_path: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("git ls-files failed");
    }

    let paths = output.stdout
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|s| std::str::from_utf8(s).ok())
        .map(|s| repo_path.join(s))
        .collect();

    Ok(paths)
}

fn walkdir_files(repo_path: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(repo_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // skip .git and other hidden dirs at the root
            let name = e.file_name().to_string_lossy();
            !(e.depth() == 1 && name.starts_with('.'))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect()
}
