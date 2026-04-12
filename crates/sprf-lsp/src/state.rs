//! LSP State: Cached analysis and document management.
//!
//! This module provides the central state management for the LSP server,
//! including:
//! - Analysis cache (uri -> PartialAnalysis)
//! - Document store (unsaved content)
//! - Workspace root tracking

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use sprefa_sprf::analyze::{
    analyze_partial, find_symbol_at_position, get_definition_location,
    PartialAnalysis, AnalyzeOptions, SourceRange, SymbolAtCursor, SymbolLocation,
};

/// LSP server state with analysis cache.
pub struct LspState {
    /// Analysis cache: uri -> cached analysis
    pub cache: Mutex<AnalysisCache>,
    /// Document store: uri -> unsaved content
    pub documents: Mutex<HashMap<String, String>>,
    /// Workspace root for file operations
    pub workspace_root: Mutex<Option<PathBuf>>,
}

/// Cached analysis result with versioning.
pub struct CachedAnalysis {
    /// The analysis result
    pub analysis: PartialAnalysis,
    /// Document version at time of analysis
    pub version: i32,
    /// Timestamp for cache expiration
    pub timestamp: Instant,
}

/// Analysis cache with LRU-style eviction.
pub struct AnalysisCache {
    /// Cached analyses
    analyses: HashMap<String, CachedAnalysis>,
    /// Maximum cache entries
    max_entries: usize,
}

impl LspState {
    /// Create a new LSP state.
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(AnalysisCache::new(100)),
            documents: Mutex::new(HashMap::new()),
            workspace_root: Mutex::new(None),
        }
    }

    /// Get analysis for a URI, computing if necessary.
    pub fn get_analysis(&self, uri: &str) -> Option<PartialAnalysis> {
        let cache = self.cache.lock().unwrap();
        cache.analyses.get(uri).map(|c| c.analysis.clone())
    }

    /// Get or compute analysis for a URI.
    pub fn get_or_analyze(
        &self,
        uri: &str,
        content: &str,
        version: i32,
    ) -> PartialAnalysis {
        // Check cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.analyses.get(uri) {
                if cached.version == version {
                    log::debug!("Using cached analysis for {} (version {})", uri, version);
                    return cached.analysis.clone();
                }
            }
        }

        // Compute new analysis
        log::debug!("Computing analysis for {} (version {})", uri, version);
        let workspace_root = self.workspace_root.lock().unwrap().clone();
        let documents = self.documents.lock().unwrap().clone();

        let options = AnalyzeOptions {
            run_extraction: true,
            workspace_root: workspace_root.unwrap_or_else(|| PathBuf::from(".")),
            documents,
        };

        let analysis = analyze_partial(content, options);

        // Cache the result
        self.cache_analysis(uri, analysis.clone(), version);

        analysis
    }

    /// Cache an analysis result.
    pub fn cache_analysis(&self, uri: &str, analysis: PartialAnalysis, version: i32) {
        let mut cache = self.cache.lock().unwrap();
        cache.insert(
            uri.to_string(),
            CachedAnalysis {
                analysis,
                version,
                timestamp: Instant::now(),
            },
        );
    }

    /// Store document content.
    pub fn store_document(&self, uri: &str, content: String) {
        let mut docs = self.documents.lock().unwrap();
        docs.insert(uri.to_string(), content);
    }

    /// Remove document from store.
    pub fn remove_document(&self, uri: &str) {
        let mut docs = self.documents.lock().unwrap();
        docs.remove(uri);
    }

    /// Get document content if stored.
    pub fn get_document(&self, uri: &str) -> Option<String> {
        let docs = self.documents.lock().unwrap();
        docs.get(uri).cloned()
    }

    /// Set workspace root.
    pub fn set_workspace_root(&self, root: PathBuf) {
        let mut ws = self.workspace_root.lock().unwrap();
        *ws = Some(root);
    }

    /// Get workspace root.
    pub fn get_workspace_root(&self) -> Option<PathBuf> {
        self.workspace_root.lock().unwrap().clone()
    }

    /// Find symbol at position in a document.
    pub fn find_symbol(
        &self,
        uri: &str,
        offset: usize,
    ) -> Option<SymbolAtCursor> {
        let analysis = self.get_analysis(uri)?;
        find_symbol_at_position(&analysis, offset)
    }

    /// Get definition location for a symbol.
    pub fn get_definition(
        &self,
        uri: &str,
        symbol: &SymbolAtCursor,
    ) -> Option<SymbolLocation> {
        let analysis = self.get_analysis(uri)?;
        get_definition_location(&analysis, symbol)
    }

    /// Clear the analysis cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.analyses.clear();
    }

    /// Invalidate cache entry for a URI.
    pub fn invalidate(&self, uri: &str) {
        let mut cache = self.cache.lock().unwrap();
        cache.analyses.remove(uri);
    }
}

impl Default for LspState {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisCache {
    /// Create a new cache with max entries.
    fn new(max_entries: usize) -> Self {
        Self {
            analyses: HashMap::new(),
            max_entries,
        }
    }

    /// Insert a cached analysis, evicting oldest if at capacity.
    fn insert(&mut self, uri: String, analysis: CachedAnalysis) {
        if self.analyses.len() >= self.max_entries && !self.analyses.contains_key(&uri) {
            // Evict oldest entry
            if let Some(oldest) = self
                .analyses
                .iter()
                .min_by_key(|(_, v)| v.timestamp)
                .map(|(k, _)| k.clone())
            {
                self.analyses.remove(&oldest);
            }
        }
        self.analyses.insert(uri, analysis);
    }

    /// Get cached analysis.
    fn get(&self, uri: &str) -> Option<&CachedAnalysis> {
        self.analyses.get(uri)
    }

    /// Clear all entries.
    fn clear(&mut self) {
        self.analyses.clear();
    }

    /// Remove a specific entry.
    fn remove(&mut self, uri: &str) -> Option<CachedAnalysis> {
        self.analyses.remove(uri)
    }
}

/// Convert byte offset to LSP Position.
pub fn offset_to_position(text: &str, offset: usize) -> tower_lsp::lsp_types::Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i == offset {
            return tower_lsp::lsp_types::Position::new(line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    tower_lsp::lsp_types::Position::new(line, col)
}

/// Convert LSP Position to byte offset.
pub fn position_to_offset(text: &str, pos: tower_lsp::lsp_types::Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if line == pos.line && col == pos.character {
            return i;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    text.len()
}

/// Convert SourceRange to LSP Range.
pub fn source_range_to_lsp(
    source: &str,
    range: &SourceRange,
) -> tower_lsp::lsp_types::Range {
    tower_lsp::lsp_types::Range {
        start: offset_to_position(source, range.start),
        end: offset_to_position(source, range.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic() {
        let state = LspState::new();
        let analysis = analyze_partial("rule(test) {}", AnalyzeOptions::default());
        
        state.cache_analysis("file://test.sprf", analysis.clone(), 1);
        
        let cached = state.get_analysis("file://test.sprf");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().stmts.len(), analysis.stmts.len());
    }

    #[test]
    fn test_cache_version_invalidation() {
        let state = LspState::new();
        
        let analysis1 = analyze_partial("rule(test1) {}", AnalyzeOptions::default());
        state.cache_analysis("file://test.sprf", analysis1, 1);
        
        // Same version should use cache
        let cached = state.get_or_analyze("file://test.sprf", "rule(test2) {}", 1);
        // Should return cached version (test1)
        assert!(cached.source.contains("test1"));
        
        // Different version should recompute
        let cached = state.get_or_analyze("file://test.sprf", "rule(test2) {}", 2);
        // Should return new version (test2)
        assert!(cached.source.contains("test2"));
    }

    #[test]
    fn test_document_store() {
        let state = LspState::new();
        
        state.store_document("file://test.txt", "content".to_string());
        assert_eq!(state.get_document("file://test.txt"), Some("content".to_string()));
        
        state.remove_document("file://test.txt");
        assert_eq!(state.get_document("file://test.txt"), None);
    }

    #[test]
    fn test_position_offset_roundtrip() {
        let text = "line 1\nline 2\nline 3";
        
        // Test start of file
        let pos = offset_to_position(text, 0);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
        assert_eq!(position_to_offset(text, pos), 0);
        
        // Test middle of file
        let pos = offset_to_position(text, 10);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 3);
        assert_eq!(position_to_offset(text, pos), 10);
        
        // Test end of file
        let end = text.len();
        let pos = offset_to_position(text, end);
        assert_eq!(position_to_offset(text, pos), end);
    }
}
