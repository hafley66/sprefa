//! Completion providers that use external tools (ripgrep, git, find).

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

use tower_lsp::lsp_types::{Range, TextEdit, CompletionTextEdit};

/// Max items to show per folder before showing "+N more" option (folders only)
const MAX_PER_FOLDER: usize = 3;
/// Max items to show per folder for files (no drill-down, just show more)
const MAX_FILES_PER_FOLDER: usize = 10;

/// Group files by parent folder and limit items per folder.
/// Returns (folder_path, items_in_folder, total_in_folder)
fn group_by_folder(files: Vec<String>) -> Vec<(String, Vec<String>, usize)> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    
    for file in files {
        let folder = file.rfind('/')
            .map(|i| file[..i].to_string())
            .unwrap_or_default();
        groups.entry(folder).or_default().push(file);
    }
    
    // Sort folders (root first, then alphabetically)
    let mut folders: Vec<String> = groups.keys().cloned().collect();
    folders.sort_by(|a, b| {
        if a.is_empty() { return std::cmp::Ordering::Less; }
        if b.is_empty() { return std::cmp::Ordering::Greater; }
        a.cmp(b)
    });
    
    folders.into_iter()
        .map(|folder| {
            let items = groups.get(&folder).cloned().unwrap_or_default();
            let total = items.len();
            (folder, items, total)
        })
        .collect()
}

/// Complete file paths using ripgrep, with find as fallback.
pub async fn complete_files(root: &Path, partial: &str, replace_range: Range) -> Vec<CompletionItem> {
    // Normalize partial: treat "./" as empty (list all files)
    let partial = partial.strip_prefix("./").unwrap_or(partial);
    
    // Empty partial = list all files (no filtering)
    // Short partials (< 2 chars) without glob chars = wait for more input
    if partial.len() > 0 && partial.len() < 2 && !partial.contains('*') && !partial.contains('?') {
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

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|s| s.to_string())
        .collect();
    
    let grouped = group_by_folder(files);
    let mut items = Vec::new();
    
    for (folder, files_in_folder, total) in grouped {
        let limit = MAX_FILES_PER_FOLDER.min(total);
        
        // Add file items (up to limit per folder)
        for matched_path in files_in_folder.iter().take(limit) {
            let breadcrumb = matched_path.rfind('/')
                .map(|i| matched_path[..i].to_string())
                .unwrap_or_default();
            
            let (label, new_text, filter_text) = if is_glob && !glob_prefix.is_empty() {
                let filename = matched_path.rfind('/')
                    .map(|i| &matched_path[i+1..])
                    .unwrap_or(matched_path);
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
                    matched_path.to_string(),
                    matched_path.to_string(),
                    matched_path.to_string()
                )
            };
            
            items.push(CompletionItem {
                label,
                kind: Some(CompletionItemKind::FILE),
                detail: Some(if breadcrumb.is_empty() { "file".to_string() } else { breadcrumb }),
                filter_text: Some(filter_text),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text,
                })),
                ..Default::default()
            });
        }
        
        // Show "+N more" hint as a disabled item (informational only, no insertion)
        if total > MAX_FILES_PER_FOLDER {
            let remaining = total - MAX_FILES_PER_FOLDER;
            let folder_name = if folder.is_empty() { "root" } else { folder.split('/').last().unwrap_or(&folder) };
            
            items.push(CompletionItem {
                label: format!("+{} more files in {}/", remaining, folder_name),
                kind: Some(CompletionItemKind::FILE),
                detail: Some(format!("{} total files, type more specific pattern", total)),
                filter_text: Some("zzz".to_string()), // Sort to end
                text_edit: None, // No insertion
                insert_text: None,
                ..Default::default()
            });
        }
    }
    
    // Overall limit to prevent overwhelming the UI
    items.truncate(100);
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

/// Filter directories to only show top-level ones (depth <= 2 from root)
/// and limit total results.
fn filter_top_level_dirs(dirs: Vec<String>) -> Vec<String> {
    // Only show directories that are:
    // 1. Top-level (no slash): "src/", "crates/"
    // 2. One level deep (one slash): "crates/cache/", ".git/objects/"
    // Skip deeply nested ones like ".git/objects/61/"
    dirs.into_iter()
        .filter(|d| {
            let clean = d.trim_end_matches('/');
            clean.matches('/').count() <= 1
        })
        .collect()
}

/// Complete directory paths using find.
/// 
/// The `replace_range` is used to set text_edit on each item so the partial text
/// is properly replaced instead of appended.
pub async fn complete_dirs(root: &Path, partial: &str, replace_range: Range) -> Vec<CompletionItem> {
    // Normalize partial: treat "./" as empty (list all dirs)
    let partial = partial.strip_prefix("./").unwrap_or(partial);
    
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
        .filter(|line| !line.contains("/.git/") && !line.starts_with(".git/") && *line != ".git")
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
                        if !clean.is_empty() && clean != "." && clean != parent
                            && !clean.contains("/.git/") && !clean.starts_with(".git/") && clean != ".git" {
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

    // Filter to top-level dirs only (avoid overwhelming nested .git/ structure)
    let filtered_dirs = filter_top_level_dirs(filtered_dirs);
    
    // Build completion items
    let mut items = Vec::new();
    let total = filtered_dirs.len();
    let show_more = total > MAX_PER_FOLDER;
    let limit = MAX_PER_FOLDER.min(total);
    
    for dir_path in filtered_dirs.iter().take(limit) {
        let clean_path = dir_path.trim_end_matches('/');
        let basename = clean_path.rsplit_once('/').map(|(_, n)| n).unwrap_or(clean_path);
        
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
        
        items.push(CompletionItem {
            label: format!("{}/", basename),
            kind: Some(CompletionItemKind::FOLDER),
            detail: Some(if breadcrumb.is_empty() { "folder".to_string() } else { breadcrumb }),
            filter_text,
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                range: replace_range,
                new_text: dir_path.clone(),
            })),
            ..Default::default()
        });
    }
    
    // Add "+N more" hint if there are more folders
    if show_more {
        let remaining = total - MAX_PER_FOLDER;
        items.push(CompletionItem {
            label: format!("+{} more folders", remaining),
            kind: Some(CompletionItemKind::FOLDER),
            detail: Some("type to filter or navigate deeper".to_string()),
            filter_text: Some("zzz".to_string()), // Sort to end
            text_edit: None,
            ..Default::default()
        });
    }

    log::debug!("folder completion returned {} items (from {} total)", items.len(), total);
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

// Group files by folder for find fallback
fn group_files_by_folder_find(files: Vec<String>) -> Vec<(String, Vec<String>, usize)> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    
    for file in files {
        let folder = file.rfind('/')
            .map(|i| file[..i].to_string())
            .unwrap_or_default();
        groups.entry(folder).or_default().push(file);
    }
    
    let mut folders: Vec<String> = groups.keys().cloned().collect();
    folders.sort_by(|a, b| {
        if a.is_empty() { return std::cmp::Ordering::Less; }
        if b.is_empty() { return std::cmp::Ordering::Greater; }
        a.cmp(b)
    });
    
    folders.into_iter()
        .map(|folder| {
            let items = groups.get(&folder).cloned().unwrap_or_default();
            let total = items.len();
            (folder, items, total)
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

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().strip_prefix("./").unwrap_or(line.trim()))
        .filter(|line| !line.is_empty())
        .filter(|line| !line.contains("/.git/") && !line.starts_with(".git/"))
        .map(|s| s.to_string())
        .collect();
    
    let grouped = group_files_by_folder_find(files);
    let mut items = Vec::new();
    
    for (folder, files_in_folder, total) in grouped {
        let show_more = total > MAX_PER_FOLDER;
        let limit = if show_more { MAX_PER_FOLDER } else { total };
        
        for path in files_in_folder.iter().take(limit) {
            let filter_text = if is_glob {
                Some(format!("{} {}", partial, path))
            } else {
                Some(path.to_string())
            };
            
            items.push(CompletionItem {
                label: path.to_string(),
                kind: Some(CompletionItemKind::FILE),
                filter_text,
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: path.to_string(),
                })),
                ..Default::default()
            });
        }
        
        if show_more {
            let remaining = total - MAX_PER_FOLDER;
            let drill_path = if folder.is_empty() { "/".to_string() } else { format!("{}/", folder) };
            
            items.push(CompletionItem {
                label: format!("+{} more in {}/", remaining, folder.split('/').last().unwrap_or(&folder)),
                kind: Some(CompletionItemKind::FOLDER),
                detail: Some(format!("{} total files", total)),
                filter_text: Some(drill_path.clone()),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: replace_range,
                    new_text: drill_path,
                })),
                ..Default::default()
            });
        }
    }
    
    items
}
