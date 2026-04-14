//! Store - free reads for operators.

use std::collections::HashMap;
use std::path::PathBuf;

pub trait Store: Send + Sync {
    fn list_files(&self, pattern: &str) -> Vec<PathBuf>;
    fn read_file(&self, path: &PathBuf) -> Option<String>;
    fn query_refs(&self, rule: &str, var: &str, repo: &str, rev: &str) -> Vec<String>;
}

/// In-memory store for testing.
pub struct MemStore {
    files: HashMap<PathBuf, String>,
    refs: HashMap<(String, String, String, String), Vec<String>>,
}

impl MemStore {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            refs: HashMap::new(),
        }
    }
    
    pub fn add_file(&mut self, path: &str, content: &str) {
        self.files.insert(PathBuf::from(path), content.to_string());
    }
    
    pub fn add_refs(&mut self, rule: &str, var: &str, repo: &str, rev: &str, values: Vec<String>) {
        self.refs.insert(
            (rule.to_string(), var.to_string(), repo.to_string(), rev.to_string()),
            values
        );
    }
}

impl Store for MemStore {
    fn list_files(&self, pattern: &str) -> Vec<PathBuf> {
        self.files.keys()
            .filter(|p| {
                let s = p.to_string_lossy();
                // Very simple glob matching
                if pattern.starts_with("**/") {
                    s.ends_with(&pattern[3..])
                } else if pattern.contains('*') {
                    let parts: Vec<&str> = pattern.split('*').collect();
                    s.starts_with(parts[0]) && s.ends_with(parts[parts.len()-1])
                } else {
                    s.contains(pattern)
                }
            })
            .cloned()
            .collect()
    }
    
    fn read_file(&self, path: &PathBuf) -> Option<String> {
        self.files.get(path).cloned()
    }
    
    fn query_refs(&self, rule: &str, var: &str, repo: &str, rev: &str) -> Vec<String> {
        self.refs.get(&(rule.to_string(), var.to_string(), repo.to_string(), rev.to_string()))
            .cloned()
            .unwrap_or_default()
    }
}
