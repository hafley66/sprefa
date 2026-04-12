/// In-memory extraction for capture values.
///
/// This module provides snappy in-memory extraction without database persistence,
/// suitable for LSP hover and CLI preview operations.
use std::path::{Path, PathBuf};
use anyhow::Result;
use sprefa_rules::types::Rule;
use sprefa_rules::extractor::RuleExtractor;
use sprefa_rules::walk::MatchResult;
use sprefa_extract::ExtractContext;

/// A single extracted value with provenance.
#[derive(Debug, Clone)]
pub struct ExtractedValue {
    /// The captured value (e.g., "sprefa_sprf")
    pub value: String,
    /// Source file path
    pub file_path: String,
    /// Optional line number for hover navigation
    pub line: Option<usize>,
}

/// Extract capture values from files matching a rule.
///
/// This is the "shell for partial extraction" - works without DB,
/// suitable for LSP hover and CLI preview.
///
/// # Limits
/// - Max 50 files per rule
/// - Max 1MB per file
pub fn extract_in_memory(
    rule: &Rule,
    workspace_root: &Path,
    var_name: &str,
) -> Vec<ExtractedValue> {
    let mut results = Vec::new();
    
    // Find files matching the rule's file selector
    let matching_files = find_matching_files(rule, workspace_root);
    
    // Process up to 50 files
    for file_path in matching_files.iter().take(50) {
        match extract_from_file(rule, var_name, file_path) {
            Ok(values) => results.extend(values),
            Err(e) => {
                log::debug!("extraction failed for {}: {}", file_path.display(), e);
            }
        }
    }
    
    results
}

/// Extract capture values from in-memory content (for unsaved files).
///
/// This allows the LSP to show extraction results for files that haven't
/// been saved to disk yet. The file_path is used for pattern matching
/// and reporting, but content is taken from the provided bytes.
pub fn extract_from_content(
    rule: &Rule,
    var_name: &str,
    file_path: &Path,
    content: &[u8],
) -> Vec<ExtractedValue> {
    match extract_from_bytes(rule, var_name, file_path, content) {
        Ok(values) => values,
        Err(e) => {
            log::debug!("extraction failed for {}: {}", file_path.display(), e);
            Vec::new()
        }
    }
}

/// Find files matching a rule's file selector using glob walk.
pub fn find_matching_files(rule: &Rule, workspace_root: &Path) -> Vec<PathBuf> {
    use globset::Glob;
    
    let mut matches = Vec::new();
    
    // Build glob patterns from rule's file selector
    // The rule.file contains the compiled file selector patterns
    let patterns: Vec<String> = rule.select.iter()
        .filter_map(|step| {
            if let sprefa_rules::types::SelectStep::File { pattern, .. } = step {
                Some(pattern.clone())
            } else {
                None
            }
        })
        .collect();
    
    // Also check if there are patterns from the first create_matches slot
    // which is where fs() patterns end up in lowered rules
    
    for pattern in patterns {
        // Use globset for proper glob matching
        let glob_pattern = if pattern.starts_with("**/") {
            pattern.clone()
        } else {
            format!("**/{}", pattern)
        };
        
        match Glob::new(&glob_pattern) {
            Ok(glob) => {
                let matcher = glob.compile_matcher();
                
                // Walk directory looking for matches
                for entry in walkdir::WalkDir::new(workspace_root)
                    .max_depth(10)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    let path = entry.path();
                    if path.is_file() {
                        let relative = path.strip_prefix(workspace_root).unwrap_or(path);
                        if matcher.is_match(relative) || matcher.is_match(path) {
                            matches.push(path.to_path_buf());
                        }
                    }
                }
            }
            Err(e) => {
                log::debug!("Invalid glob pattern '{}': {}", pattern, e);
            }
        }
    }
    
    // Deduplicate and limit
    matches.sort();
    matches.dedup();
    matches.truncate(50);
    matches
}

/// Extract capture values from a single file.
fn extract_from_file(
    rule: &Rule,
    var_name: &str,
    file_path: &Path,
) -> Result<Vec<ExtractedValue>> {
    // Check file size (skip if > 1MB)
    let metadata = std::fs::metadata(file_path)?;
    if metadata.len() > 1_000_000 {
        return Ok(Vec::new());
    }
    
    // Read file contents
    let contents = std::fs::read(file_path)?;
    
    extract_from_bytes(rule, var_name, file_path, &contents)
}

/// Extract capture values from raw bytes.
fn extract_from_bytes(
    rule: &Rule,
    var_name: &str,
    file_path: &Path,
    contents: &[u8],
) -> Result<Vec<ExtractedValue>> {
    // Create a ruleset with just this rule for extraction
    let single_rule_set = sprefa_rules::types::RuleSet {
        schema: None,
        rules: vec![rule.clone()],
    };
    
    // Build extractor and run
    let extractor = RuleExtractor::from_ruleset(&single_rule_set)?;
    let ctx = ExtractContext::default();
    let path_str = file_path.to_string_lossy();
    
    // eval_raw handles all matchers: json, md, line, ast
    let matches = extractor.eval_raw(&contents, &path_str, &ctx);
    
    // Extract the specific variable from results
    let mut values = Vec::new();
    for m in matches {
        if let Some(capture) = m.captures.get(var_name) {
            values.push(ExtractedValue {
                value: capture.text.clone(),
                file_path: path_str.to_string(),
                line: None, // TODO: track line numbers
            });
        }
    }
    
    Ok(values)
}

/// Parse file content based on extension.
fn parse_file_content(path: &Path, contents: &[u8]) -> Option<serde_json::Value> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    match ext.as_str() {
        "json" => serde_json::from_slice(contents).ok(),
        "yaml" | "yml" => {
            serde_yaml::from_slice::<serde_yaml::Value>(contents)
                .ok()
                .and_then(|v| serde_json::to_value(v).ok())
        }
        "toml" => {
            std::str::from_utf8(contents)
                .ok()
                .and_then(|s| toml::from_str::<toml::Value>(s).ok())
                .and_then(|v| serde_json::to_value(v).ok())
        }
        _ => None,
    }
}

/// Simple glob matching.
fn glob_matches(pattern: &str, path: &str) -> bool {
    // Normalize **/ to just look for suffix matching
    let pattern = if pattern.starts_with("**/") {
        &pattern[3..]
    } else {
        pattern
    };
    
    // Very simple glob: * matches any characters
    let pattern_parts: Vec<&str> = pattern.split('*').collect();
    
    if pattern_parts.len() == 1 {
        // No wildcards - exact match (or suffix match if original had **/)
        return path.ends_with(pattern);
    }
    
    let mut path_remainder = path;
    for (i, part) in pattern_parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        
        if i == 0 {
            // First part must match at start (or as suffix if we stripped **/)
            if !path_remainder.starts_with(part) && !path_remainder.ends_with(pattern) {
                return false;
            }
            if path_remainder.starts_with(part) {
                path_remainder = &path_remainder[part.len()..];
            }
        } else if i == pattern_parts.len() - 1 {
            // Last part must match at end
            if !path_remainder.ends_with(part) {
                return false;
            }
        } else {
            // Middle parts must appear somewhere
            if let Some(pos) = path_remainder.find(part) {
                path_remainder = &path_remainder[pos + part.len()..];
            } else {
                return false;
            }
        }
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_matches() {
        assert!(glob_matches("**/Cargo.toml", "/foo/Cargo.toml"));
        assert!(glob_matches("**/Cargo.toml", "Cargo.toml"));
        assert!(!glob_matches("**/Cargo.toml", "/foo/package.json"));
        assert!(glob_matches("*.json", "package.json"));
        assert!(!glob_matches("*.json", "Cargo.toml"));
    }
}
