//! Completion providers that use external tools (ripgrep, git, find).

use ignore::gitignore::GitignoreBuilder;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use tower_lsp::lsp_types::{Range, TextEdit, CompletionTextEdit};

/// Max total completion items to return
const MAX_ITEMS: usize = 50;

/// Check if a path should be ignored based on .gitignore files.
fn is_ignored(path: &Path, root: &Path) -> bool {
    // Normalize path: strip "./" prefix if present
    let path_str = path.to_str().unwrap_or("");
    let normalized = path_str.strip_prefix("./").unwrap_or(path_str);
    let normalized_path = Path::new(normalized);
    
    // Quick check for .git directories (always ignore)
    if normalized.starts_with(".git") || normalized.contains("/.git/") {
        return true;
    }
    
    // Try to load .gitignore from root
    let gitignore_path = root.join(".gitignore");
    if !gitignore_path.exists() {
        return false;
    }
    
    let mut builder = GitignoreBuilder::new(root);
    if builder.add(gitignore_path).is_some() {
        return false;
    }
    let gitignore = match builder.build() {
        Ok(g) => g,
        Err(_) => return false,
    };
    
    // Check if this path or any parent directory is ignored
    let mut current = Some(normalized_path);
    while let Some(p) = current {
        if gitignore.matched(p, true).is_ignore() {
            return true;
        }
        current = p.parent();
    }
    
    false
}

/// Complete file paths with depth limit (1 = top-level only, 0 = unlimited/recursive).
/// Uses ripgrep for recursive search and find for depth-limited search.
pub async fn complete_files(root: &Path, partial: &str, replace_range: Range, max_depth: usize) -> Vec<CompletionItem> {
    // Normalize partial: treat "./" as empty (list all files)
    let partial = partial.strip_prefix("./").unwrap_or(partial);
    
    // Empty partial = list all files (no filtering)
    // Short partials (< 2 chars) without glob chars = wait for more input
    if partial.len() > 0 && partial.len() < 2 && !partial.contains('*') && !partial.contains('?') {
        return vec![];
    }

    // For depth 1, use find which supports -maxdepth natively
    if max_depth == 1 {
        return complete_files_depth1(root, partial, replace_range).await;
    }

    // Convert sprf-style glob to regex-ish pattern for ripgrep
    let glob_pattern = format!("{}*", partial);

    // Try ripgrep first (user's preferred tool)
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
        Ok(Ok(output)) if output.status.success() => output,
        _ => {
            // Fallback to find
            log::debug!("ripgrep failed, falling back to find");
            return complete_files_find(root, partial, replace_range).await;
        }
    };

    let is_glob = partial.contains('*') || partial.contains('?');
    
    // Build glob prefix for star preservation
    let glob_prefix = if is_glob {
        partial.rfind('/').map(|i| &partial[..i+1]).unwrap_or("")
    } else {
        ""
    };

    // Collect and sort paths
    let mut paths: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .filter(|line| !is_ignored(Path::new(line), root))
        .take(MAX_ITEMS)
        .collect();
    
    // Sort alphabetically by path for consistent ordering
    paths.sort();

    paths.into_iter()
        .map(|matched_path| {
            let breadcrumb = matched_path.rfind('/')
                .map(|i| matched_path[..i].to_string())
                .unwrap_or_default();
            
            let (label, new_text, filter_text) = if is_glob && !glob_prefix.is_empty() {
                let filename = matched_path.rfind('/')
                    .map(|i| &matched_path[i+1..])
                    .unwrap_or(&matched_path);
                let prefix = glob_prefix.trim_end_matches('/');
                let pattern_label = if prefix.is_empty() {
                    filename.to_string()
                } else {
                    format!("{}/{}", prefix, filename)
                };
                (
                    pattern_label.clone(),
                    pattern_label,
                    format!("{} {}", partial, matched_path)
                )
            } else {
                (
                    matched_path.clone(),
                    matched_path.clone(),
                    matched_path
                )
            };
            
            CompletionItem {
                label,
                kind: Some(CompletionItemKind::FILE),
                detail: Some(if breadcrumb.is_empty() { "file".to_string() } else { breadcrumb }),
                filter_text: Some(filter_text),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text,
                })),
                ..Default::default()
            }
        })
        .collect()
}

/// Complete top-level files only (depth 1) using find.
async fn complete_files_depth1(root: &Path, partial: &str, replace_range: Range) -> Vec<CompletionItem> {
    log::debug!("completing files depth 1 in {} with partial='{}'", root.display(), partial);
    
    let partial_ends_with_slash = partial.ends_with('/');
    
    // Determine start dir based on partial
    let (start_dir, prefix_filter) = if partial.is_empty() {
        (".", "")
    } else if partial_ends_with_slash {
        // List files in this dir
        let dir = partial.trim_end_matches('/');
        if dir.is_empty() || dir == "." {
            (".", "")
        } else {
            (dir, "")
        }
    } else if partial.contains('/') {
        // Has path separator - filter within that dir
        let parent = partial.rfind('/').map(|i| &partial[..i]).unwrap_or(".");
        let name = &partial[partial.rfind('/').map(|i| i + 1).unwrap_or(0)..];
        if parent.is_empty() || parent == "." {
            (".", name)
        } else {
            (parent, name)
        }
    } else {
        // Simple name - filter top-level files
        (".", partial)
    };
    
    // Build find command for files at depth 1
    let mut cmd = Command::new("find");
    cmd.current_dir(root)
        .args([start_dir, "-maxdepth", "1", "-type", "f"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    
    log::debug!("find files command: {:?}", cmd);
    
    let output = match timeout(Duration::from_millis(500), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
        Ok(Ok(output)) => {
            log::debug!("find files failed: {}", String::from_utf8_lossy(&output.stderr));
            return vec![];
        }
        Ok(Err(e)) => {
            log::debug!("find files error: {}", e);
            return vec![];
        }
        Err(_) => {
            log::debug!("find files timed out");
            return vec![];
        }
    };
    
    let mut files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| *line != start_dir)
        .filter(|line| !is_ignored(Path::new(line), root))
        .map(|path| path.strip_prefix("./").unwrap_or(path).to_string())
        .filter(|path| {
            // Apply prefix filter if any
            if prefix_filter.is_empty() {
                true
            } else {
                path.rfind('/')
                    .map(|i| path[i+1..].starts_with(prefix_filter))
                    .unwrap_or(path.starts_with(prefix_filter))
            }
        })
        .take(MAX_ITEMS)
        .collect();
    
    // Sort alphabetically for consistent ordering
    files.sort();
    
    // Build completion items
    let mut items = Vec::new();
    for file_path in files {
        let breadcrumb = file_path.rfind('/')
            .map(|i| file_path[..i].to_string())
            .unwrap_or_default();
        
        items.push(CompletionItem {
            label: file_path.clone(),
            kind: Some(CompletionItemKind::FILE),
            detail: Some(if breadcrumb.is_empty() { "file".to_string() } else { breadcrumb }),
            filter_text: Some(file_path.clone()),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace_range,
                new_text: file_path,
            })),
            ..Default::default()
        });
    }
    
    log::debug!("depth1 file completion returned {} items", items.len());
    items
}

/// Convert a simple glob pattern to regex.
/// Handles: * (any chars), ? (single char), ** (recursive - treated as *)
fn glob_to_regex(pattern: &str) -> Result<regex::Regex, regex::Error> {
    let mut regex_str = String::new();
    regex_str.push('^');
    
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    // ** - recursive glob, treat as .*
                    chars.next(); // consume second *
                    regex_str.push_str(".*");
                } else {
                    regex_str.push_str("[^/]*");
                }
            }
            '?' => regex_str.push('.'),
            '.' => regex_str.push_str("\\."),
            '+' => regex_str.push_str("\\+"),
            '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' => {
                regex_str.push('\\');
                regex_str.push(c);
            }
            '/' => regex_str.push('/'),
            c => regex_str.push(c),
        }
    }
    
    regex_str.push('$');
    regex::Regex::new(&regex_str)
}

/// Complete directory paths using find.
/// Shows only one level at a time - no child expansion.
pub async fn complete_dirs(root: &Path, partial: &str, replace_range: Range) -> Vec<CompletionItem> {
    // Normalize partial: treat "./" as empty
    let partial = partial.strip_prefix("./").unwrap_or(partial);
    
    log::debug!("completing dirs in {} with partial='{}'", root.display(), partial);

    let partial_ends_with_slash = partial.ends_with('/');

    // Determine what to list:
    // - Empty partial: list top-level dirs only
    // - Ends with /: list that dir's children
    // - Has / but no trailing: filter dirs at that level
    // - Simple name: filter top-level dirs
    let (start_dir, max_depth) = if partial.is_empty() {
        (".", 1) // Top-level only
    } else if partial_ends_with_slash {
        // List children of this dir
        let dir = partial.trim_end_matches('/');
        if dir.is_empty() || dir == "." {
            (".", 1)
        } else {
            (dir, 1)
        }
    } else if partial.contains('/') {
        // Has path separator - we're completing within that hierarchy
        let parent = partial.rfind('/').map(|i| &partial[..i]).unwrap_or(".");
        if parent.is_empty() || parent == "." {
            (".", 1)
        } else {
            (parent, 1)
        }
    } else {
        // Simple name - search top-level
        (".", 1)
    };

    // Build find command with maxdepth
    let mut cmd = Command::new("find");
    cmd.current_dir(root)
        .args([start_dir, "-maxdepth", &max_depth.to_string(), "-type", "d"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    log::debug!("find command: {:?}", cmd);

    let output = match timeout(Duration::from_millis(500), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => output,
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

    let mut dirs: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| *line != ".")
        .filter(|line| *line != start_dir) // Don't include the start dir itself
        .filter(|line| !is_ignored(Path::new(line), root))
        .map(|path| path.strip_prefix("./").unwrap_or(path).to_string())
        .collect();
    
    // Sort alphabetically for consistent ordering
    dirs.sort();

    // Build completion items from collected dirs
    let mut items = Vec::new();
    
    for dir_path in dirs.into_iter().take(MAX_ITEMS) {
        let clean_path = dir_path.trim_end_matches('/');
        let basename = clean_path.rsplit_once('/').map(|(_, n)| n).unwrap_or(clean_path);
        
        let breadcrumb = if clean_path.contains('/') {
            clean_path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default()
        } else {
            String::new()
        };
        
        let filter_text = Some(dir_path.clone());
        
        items.push(CompletionItem {
            label: format!("{}/", basename),
            kind: Some(CompletionItemKind::FOLDER),
            detail: Some(if breadcrumb.is_empty() { "folder".to_string() } else { breadcrumb }),
            filter_text: Some(format!("{}/", dir_path)),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace_range,
                new_text: format!("{}/", dir_path),
            })),
            ..Default::default()
        });
    }

    log::debug!("folder completion returned {} items", items.len());
    items
}

/// Complete git refs (branches and tags) for a repository.
pub async fn complete_git_refs(repo_path: &Path, partial: &str) -> Vec<CompletionItem> {
    let mut items = vec![];

    // Get branches
    let branch_output = match timeout(
        Duration::from_millis(500),
        Command::new("git")
            .args(["branch", "-a", "--format", "%(refname:short)"])
            .current_dir(repo_path)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return vec![],
    };

    let branches = String::from_utf8_lossy(&branch_output.stdout);
    for branch in branches.lines() {
        let clean = branch.trim().strip_prefix("origin/").unwrap_or(branch.trim());
        if clean.contains(partial) && !clean.is_empty() {
            items.push(CompletionItem {
                label: clean.to_string(),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some("branch".to_string()),
                ..Default::default()
            });
        }
    }

    // Get tags
    let tag_pattern = format!("{}*", partial);
    let tag_output = match timeout(
        Duration::from_millis(500),
        Command::new("git")
            .args(["tag", "-l", &tag_pattern])
            .current_dir(repo_path)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return items,
    };

    let tags = String::from_utf8_lossy(&tag_output.stdout);
    for tag in tags.lines() {
        let clean = tag.trim();
        if !clean.is_empty() {
            items.push(CompletionItem {
                label: clean.to_string(),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some("tag".to_string()),
                ..Default::default()
            });
        }
    }

    items
}

/// Complete repo names.
pub fn complete_repo_names(names: &[String], partial: &str) -> Vec<CompletionItem> {
    names
        .iter()
        .filter(|name| name.contains(partial) || partial.is_empty())
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some("repo".to_string()),
            ..Default::default()
        })
        .collect()
}

// Fallback file completion using find
async fn complete_files_find(root: &Path, partial: &str, replace_range: Range) -> Vec<CompletionItem> {
    let pattern = format!("{}*", partial);
    let output = match timeout(
        Duration::from_millis(500),
        Command::new("find")
            .args([".", "-type", "f", "-name", &pattern])
            .current_dir(root)
            .output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => output,
        _ => return vec![],
    };

    let is_glob = partial.contains('*') || partial.contains('?');

    let mut paths: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().strip_prefix("./").unwrap_or(line.trim()).to_string())
        .filter(|line| !line.is_empty())
        .filter(|line| !is_ignored(Path::new(line), root))
        .take(MAX_ITEMS)
        .collect();
    
    // Sort alphabetically for consistent ordering
    paths.sort();

    paths.into_iter()
        .map(|path| {
            let filter_text = if is_glob {
                Some(format!("{} {}", partial, path))
            } else {
                Some(path.clone())
            };
            
            CompletionItem {
                label: path.clone(),
                kind: Some(CompletionItemKind::FILE),
                filter_text,
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: path,
                })),
                ..Default::default()
            }
        })
        .collect()
}
