//! Completion providers using ripgrep and git.

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;
use tokio::time::timeout;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

/// Complete file paths using ripgrep with fallback to find.
///
/// Requires 2+ characters in partial, returns empty if < 2.
/// Times out after 500ms.
pub async fn complete_files(root: &Path, partial: &str) -> Vec<CompletionItem> {
    if partial.len() < 2 {
        return vec![];
    }

    let glob_pattern = format!("{}*", partial);

    // Try ripgrep first (case-insensitive glob)
    log::debug!("running ripgrep: rg --files --iglob '{}' in {}", glob_pattern, root.display());
    let rg_result = timeout(
        Duration::from_millis(500),
        Command::new("rg")
            .args(["--files", "--iglob", &glob_pattern])
            .current_dir(root)
            .output(),
    )
    .await;

    let output = match rg_result {
        Ok(Ok(output)) if output.status.success() => {
            log::debug!("ripgrep succeeded: {} bytes", output.stdout.len());
            output
        }
        Ok(Ok(output)) => {
            log::debug!("ripgrep failed: {}", String::from_utf8_lossy(&output.stderr));
            // Fallback to find
            log::debug!("falling back to find: find . -name '{}' -type f", glob_pattern);
            match timeout(
                Duration::from_millis(500),
                Command::new("find")
                    .args([".", "-name", &glob_pattern, "-type", "f"])
                    .current_dir(root)
                    .output(),
            )
            .await
            {
                Ok(Ok(output)) if output.status.success() => {
                    log::debug!("find succeeded: {} bytes", output.stdout.len());
                    output
                }
                Ok(Ok(output)) => {
                    log::debug!("find failed: {}", String::from_utf8_lossy(&output.stderr));
                    return vec![];
                }
                Ok(Err(e)) => {
                    log::debug!("find error: {}", e);
                    return vec![];
                }
                Err(_) => {
                    log::debug!("find timed out");
                    return vec![];
                }
            }
        }
        Ok(Err(e)) => {
            log::debug!("ripgrep error: {}", e);
            // Fallback to find
            log::debug!("falling back to find: find . -name '{}' -type f", glob_pattern);
            match timeout(
                Duration::from_millis(500),
                Command::new("find")
                    .args([".", "-name", &glob_pattern, "-type", "f"])
                    .current_dir(root)
                    .output(),
            )
            .await
            {
                Ok(Ok(output)) if output.status.success() => output,
                _ => return vec![],
            }
        }
        Err(_) => {
            log::debug!("ripgrep timed out");
            return vec![];
        }
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|path| CompletionItem {
            label: path.to_string(),
            kind: Some(CompletionItemKind::FILE),
            ..Default::default()
        })
        .collect()
}

/// Complete directory paths using find.
pub async fn complete_dirs(root: &Path, partial: &str) -> Vec<CompletionItem> {
    let glob_pattern = format!("{}*", partial);

    log::debug!("running find for dirs: find . -type d -name '{}' in {}", glob_pattern, root.display());
    let output = match timeout(
        Duration::from_millis(500),
        Command::new("find")
            .args([".", "-type", "d", "-name", &glob_pattern])
            .current_dir(root)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => {
            log::debug!("find dirs succeeded: {} bytes", output.stdout.len());
            output
        }
        Ok(Ok(output)) => {
            log::debug!("find dirs failed: {}", String::from_utf8_lossy(&output.stderr));
            return vec![];
        }
        Ok(Err(e)) => {
            log::debug!("find dirs error: {}", e);
            return vec![];
        }
        Err(_) => {
            log::debug!("find dirs timed out");
            return vec![];
        }
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|path| CompletionItem {
            label: path.to_string(),
            kind: Some(CompletionItemKind::FOLDER),
            ..Default::default()
        })
        .collect()
}

/// Complete git refs (branches and tags) for a repository.
pub async fn complete_git_refs(repo_path: &Path, partial: &str) -> Vec<CompletionItem> {
    let mut items = vec![];

    // Get branches
    log::debug!("running git branch in {}", repo_path.display());
    let branch_output = timeout(
        Duration::from_millis(500),
        Command::new("git")
            .args(["branch", "-a", "--format", "%(refname:short)"])
            .current_dir(repo_path)
            .output(),
    )
    .await;

    match branch_output {
        Ok(Ok(output)) if output.status.success() => {
            let branches: Vec<CompletionItem> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .filter(|line| partial.is_empty() || line.contains(partial))
                .map(|branch| CompletionItem {
                    label: branch.to_string(),
                    kind: Some(CompletionItemKind::REFERENCE),
                    detail: Some("branch".to_string()),
                    ..Default::default()
                })
                .collect();
            log::debug!("git branch returned {} branches", branches.len());
            items.extend(branches);
        }
        Ok(Ok(output)) => {
            log::debug!("git branch failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(Err(e)) => {
            log::debug!("git branch error: {}", e);
        }
        Err(_) => {
            log::debug!("git branch timed out");
        }
    }

    // Get tags
    log::debug!("running git tag in {}", repo_path.display());
    let tag_output = timeout(
        Duration::from_millis(500),
        Command::new("git")
            .args(["tag", "-l"])
            .current_dir(repo_path)
            .output(),
    )
    .await;

    match tag_output {
        Ok(Ok(output)) if output.status.success() => {
            let tags: Vec<CompletionItem> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .filter(|line| partial.is_empty() || line.contains(partial))
                .map(|tag| CompletionItem {
                    label: tag.to_string(),
                    kind: Some(CompletionItemKind::CONSTANT),
                    detail: Some("tag".to_string()),
                    ..Default::default()
                })
                .collect();
            log::debug!("git tag returned {} tags", tags.len());
            items.extend(tags);
        }
        Ok(Ok(output)) => {
            log::debug!("git tag failed: {}", String::from_utf8_lossy(&output.stderr));
        }
        Ok(Err(e)) => {
            log::debug!("git tag error: {}", e);
        }
        Err(_) => {
            log::debug!("git tag timed out");
        }
    }

    items
}

/// Complete repo names from a list of known names.
pub fn complete_repo_names(names: &[String], partial: &str) -> Vec<CompletionItem> {
    log::debug!("completing repo names: {} names, partial='{}'", names.len(), partial);
    names
        .iter()
        .filter(|name| partial.is_empty() || name.contains(partial))
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::MODULE),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_repo_names_filtering() {
        let names = vec![
            "repo-alpha".to_string(),
            "repo-beta".to_string(),
            "other-repo".to_string(),
        ];

        let results = complete_repo_names(&names, "alpha");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].label, "repo-alpha");

        let results = complete_repo_names(&names, "repo-");
        assert_eq!(results.len(), 2);

        let results = complete_repo_names(&names, "");
        assert_eq!(results.len(), 3);

        let results = complete_repo_names(&names, "xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_complete_repo_names_kind() {
        let names = vec!["test-repo".to_string()];
        let results = complete_repo_names(&names, "");
        assert_eq!(results[0].kind, Some(CompletionItemKind::MODULE));
    }

    #[tokio::test]
    async fn test_complete_files_requires_two_chars() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("test.txt"), "content").unwrap();

        let results = complete_files(temp.path(), "t").await;
        assert!(results.is_empty());

        let results = complete_files(temp.path(), "te").await;
        assert!(!results.is_empty());
    }
}
