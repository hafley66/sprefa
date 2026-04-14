//! Store trait - FREE READS for operators.
//!
//! DESIGN: Store is the boundary. Operators never touch fs/db directly.
//! All reads are free (sync). Writes go through Effect system (async, gated).

use std::path::PathBuf;
// Cursor not used directly here

/// Store provides free reads for operators.
/// 
/// NOTE: This is v1 Store trait simplified. 
/// v1 had async, we're sync for v2 prototype.
pub trait Store: Send + Sync {
    /// List files matching glob pattern.
    fn list_files(&self, base: &PathBuf, pattern: &str) -> Vec<PathBuf>;
    
    /// Read file content.
    fn read_file(&self, path: &PathBuf) -> Option<String>;
    
    /// Query refs from upstream rule (for cross-refs).
    /// ${rule.$VAR} resolves through this.
    fn query_refs(&self, rule: &str, var: &str, repo: &str, rev: &str) -> Vec<String>;
    
    /// Discovery: find unscanned repos from scan-annotated captures.
    fn unscanned_repos(&self, table: &str, column: &str, norm: bool) -> Vec<String>;
    
    /// Discovery: find unscanned (repo, rev) pairs.
    fn unscanned_revs(&self, table: &str, column: &str, norm: bool) -> Vec<(String, String)>;
}

/// Simple in-memory store for testing.
#[derive(Debug, Default)]
pub struct MemStore {
    files: std::collections::HashMap<PathBuf, String>,
    refs: std::collections::HashMap<(String, String, String, String), Vec<String>>,
}

impl MemStore {
    pub fn add_file(&mut self, path: PathBuf, content: String) {
        self.files.insert(path, content);
    }
    
    pub fn add_refs(&mut self, rule: &str, var: &str, repo: &str, rev: &str, values: Vec<String>) {
        self.refs.insert((rule.to_string(), var.to_string(), repo.to_string(), rev.to_string()), values);
    }
}

impl Store for MemStore {
    fn list_files(&self, _base: &PathBuf, pattern: &str) -> Vec<PathBuf> {
        // Simple glob matching for prototype
        self.files.keys()
            .filter(|p| p.to_string_lossy().contains(&pattern.replace("**/", "").replace('*', "")))
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
    
    fn unscanned_repos(&self, _table: &str, _column: &str, _norm: bool) -> Vec<String> {
        vec![]
    }
    
    fn unscanned_revs(&self, _table: &str, _column: &str, _norm: bool) -> Vec<(String, String)> {
        vec![]
    }
}
