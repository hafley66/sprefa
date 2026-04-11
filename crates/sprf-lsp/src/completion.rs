//! Completion providers that use external tools (ripgrep, git, find).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use tower_lsp::lsp_types::{Range, TextEdit, CompletionTextEdit};

/// Complete file paths using ripgrep, with find as fallback.
pub async fn complete_files(root: &Path, partial: &str, replace_range: Range) -> Vec<CompletionItem> {
    // Require 2+ chars before searching (user preference)
    if partial.len() < 2 {
        return vec![];
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

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .take(100)
        .map(|matched_path| {
            // Build breadcrumb (parent directory)
            let breadcrumb = matched_path.rfind('/')
                .map(|i| matched_path[..i].to_string())
                .unwrap_or_default();
            
            let (label, new_text, filter_text) = if is_glob {
                // Preserve star: show "*/Cargo.toml" with breadcrumb showing full path
                let filename = matched_path.rfind('/')
                    .map(|i| &matched_path[i+1..])
                    .unwrap_or(matched_path);
                let pattern_label = format!("{}/{}", glob_prefix.trim_end_matches('/'), filename);
                (
                    pattern_label.clone(),
                    pattern_label,
                    format!("{} {}", partial, matched_path)
                )
            } else {
                (
                    matched_path.to_string(),
                    matched_path.to_string(),
                    matched_path.to_string()
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
/// 
/// The `replace_range` is used to set text_edit on each item so the partial text
/// is properly replaced instead of appended.
pub async fn complete_dirs(root: &Path, partial: &str, replace_range: Range) -> Vec<CompletionItem> {
    log::debug!("completing dirs in {} with partial='{}'", root.display(), partial);

    // Check if this is a glob pattern (contains * or ?)
    let is_glob = partial.contains('*') || partial.contains('?');
    let partial_ends_with_slash = partial.ends_with('/');

    // For glob patterns with trailing slash (e.g., "*/" or "src/*/"), 
    // we want to find directories that match the parent pattern and show their children
    let (find_start_dir, glob_regex, list_children) = if is_glob {
        if partial_ends_with_slash {
            // Pattern like "*/" or "src/*/" - find dirs matching the pattern, then list their children
            let pattern = partial.trim_end_matches('/');
            let regex = match glob_to_regex(pattern) {
                Ok(r) => r,
                Err(e) => {
                    log::debug!("invalid glob pattern: {}", e);
                    return vec![];
                }
            };
            (".", Some(regex), true)
        } else {
            // Pattern like "*crates" - match directory paths
            let regex = match glob_to_regex(partial) {
                Ok(r) => r,
                Err(e) => {
                    log::debug!("invalid glob pattern: {}", e);
                    return vec![];
                }
            };
            (".", Some(regex), false)
        }
    } else if partial_ends_with_slash {
        // Specific directory - list its contents
        let dir = partial.trim_end_matches('/');
        if dir.is_empty() || dir == "." {
            (".", None, true)
        } else {
            (dir, None, true)
        }
    } else if partial.contains('/') {
        // Path prefix - use it as starting point
        let parent = partial.rfind('/').map(|i| &partial[..i]).unwrap_or(".");
        (parent, None, false)
    } else {
        // Just a basename - search from root
        (".", None, false)
    };

    // Build find command
    let mut cmd = Command::new("find");
    cmd.current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if list_children && !is_glob {
        // List contents of specific directory
        cmd.args([find_start_dir, "-maxdepth", "1", "-type", "d"]);
    } else {
        // Find all directories (for glob matching)
        cmd.args([find_start_dir, "-type", "d"]);
    }

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

    let dirs: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| *line != ".")
        .map(|path| path.strip_prefix("./").unwrap_or(path).to_string())
        .collect();

    // Apply glob filtering if needed
    let filtered_dirs: Vec<String> = if let Some(ref regex) = glob_regex {
        if list_children {
            // For patterns like "*/", find directories matching the pattern,
            // then get their immediate children
            let matching_dirs: Vec<String> = dirs.iter()
                .filter(|d| regex.is_match(d))
                .cloned()
                .collect();
            
            // Get children of matching directories
            let mut children = Vec::new();
            for parent in matching_dirs {
                let child_cmd = Command::new("find")
                    .arg(&parent)
                    .args(["-maxdepth", "1", "-type", "d"])
                    .current_dir(root)
                    .output();
                
                if let Ok(Ok(output)) = timeout(Duration::from_millis(200), child_cmd).await {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        let clean = line.trim().strip_prefix("./").unwrap_or(line.trim());
                        if !clean.is_empty() && clean != "." && clean != parent {
                            children.push(format!("{}/", clean));
                        }
                    }
                }
            }
            children
        } else {
            // Just filter directories by glob pattern
            dirs.into_iter()
                .filter(|d| regex.is_match(d))
                .map(|d| format!("{}/", d))
                .collect()
        }
    } else if partial.contains('/') && !partial_ends_with_slash && !is_glob {
        // Path prefix filtering
        let prefix = partial;
        dirs.into_iter()
            .filter(|d| d.starts_with(prefix))
            .map(|d| format!("{}/", d))
            .collect()
    } else if !partial.is_empty() && !partial_ends_with_slash && !is_glob {
        // Basename prefix filtering
        dirs.into_iter()
            .filter(|d| d.starts_with(partial))
            .map(|d| format!("{}/", d))
            .collect()
    } else {
        dirs.into_iter()
            .map(|d| format!("{}/", d))
            .collect()
    };

    // Build completion items with nesting support
    let mut items = Vec::new();
    let include_children = is_glob && partial_ends_with_slash;
    
    for dir_path in filtered_dirs.into_iter().take(50) {
        let clean_path = dir_path.trim_end_matches('/');
        let basename = clean_path.rsplit_once('/').map(|(_, n)| n).unwrap_or(clean_path);
        
        // Get children
        let children = if clean_path.len() > 50 {
            Vec::new()
        } else {
            let child_cmd = Command::new("find")
                .arg(clean_path)
                .args(["-maxdepth", "1", "-type", "d"])
                .current_dir(root)
                .output();
            
            match timeout(Duration::from_millis(100), child_cmd).await {
                Ok(Ok(output)) if output.status.success() => {
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(|l| l.trim().strip_prefix("./").unwrap_or(l.trim()).to_string())
                        .filter(|l| !l.is_empty() && l != "." && l != clean_path)
                        .take(10)
                        .collect()
                }
                _ => Vec::new()
            }
        };

        // Parent item with breadcrumb detail
        let breadcrumb = if clean_path.contains('/') {
            clean_path.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default()
        } else {
            String::new()
        };
        
        let filter_text = if is_glob {
            Some(format!("{} {}", partial, dir_path))
        } else {
            Some(dir_path.clone())
        };

        // Parent folder (with / commit character to expand)
        items.push(CompletionItem {
            label: format!("{}/", basename),
            kind: Some(CompletionItemKind::FOLDER),
            detail: Some(if breadcrumb.is_empty() { "folder".to_string() } else { breadcrumb }),
            filter_text: filter_text.clone(),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace_range,
                new_text: dir_path.clone(),
            })),
            ..Default::default()
        });

        // Indented children for glob patterns
        if include_children {
            for child in &children {
                let child_basename = child.rsplit_once('/').map(|(_, n)| n).unwrap_or(child);
                items.push(CompletionItem {
                    label: format!("  ↳ {}/", child_basename),
                    kind: Some(CompletionItemKind::FOLDER),
                    detail: Some(format!("in {}", basename)),
                    filter_text: Some(format!("{} {}", partial, child)),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: replace_range,
                        new_text: format!("{}/", child),
                    })),
                    ..Default::default()
                });
            }
        }
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

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().strip_prefix("./").unwrap_or(line.trim()))
        .filter(|line| !line.is_empty())
        .take(100)
        .map(|path| {
            let filter_text = if is_glob {
                Some(format!("{} {}", partial, path))
            } else {
                Some(path.to_string())
            };
            
            CompletionItem {
                label: path.to_string(),
                kind: Some(CompletionItemKind::FILE),
                filter_text,
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: path.to_string(),
                })),
                ..Default::default()
            }
        })
        .collect()
}
