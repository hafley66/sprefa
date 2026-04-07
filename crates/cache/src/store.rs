/// Storage trait: the boundary between extraction logic and persistence.
///
/// All data storage flows through this trait. SqliteStore writes to per-rule
/// SQLite tables + a string index. MemoryStore holds HashMaps. A JsonStore
/// could write NDJSON. The extraction pipeline and discovery loop never
/// touch storage directly.
use anyhow::Result;
pub use sqlx;

/// One captured value from an extraction event.
#[derive(Debug, Clone)]
pub struct CaptureEntry {
    /// Lowercase variable name (e.g. "svc", "repo", "tag").
    pub column: String,
    /// The captured string value.
    pub value: String,
    pub span_start: u32,
    pub span_end: u32,
    /// Structural path through the parsed tree (e.g. "services/web/image").
    pub node_path: Option<String>,
    /// Whether this value is a file path (for import resolution).
    pub is_path: bool,
    /// Parent key in the source structure (for contextual grouping).
    pub parent_key: Option<String>,
    /// "repo" or "rev" if this capture drives demand scanning.
    pub scan: Option<String>,
}

/// One extraction event: all captures from one rule applied to one site.
/// Becomes one row in a per-rule table.
#[derive(Debug, Clone)]
pub struct ExtractionRow {
    pub captures: Vec<CaptureEntry>,
}

/// All extraction results for one file, grouped by rule.
#[derive(Debug, Clone)]
pub struct FileResult {
    pub rel_path: String,
    pub content_hash: String,
    pub stem: Option<String>,
    pub ext: Option<String>,
    /// rule_name -> rows extracted by that rule from this file.
    pub rule_rows: Vec<(String, Vec<ExtractionRow>)>,
}

pub trait Store: Send + Sync {
    /// Register a repo, return its ID.
    fn ensure_repo(
        &self,
        name: &str,
        root_path: &str,
    ) -> impl std::future::Future<Output = Result<i64>> + Send;

    /// Register a rev for a repo.
    fn ensure_rev(
        &self,
        repo: &str,
        rev: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Flush extraction results for a batch of files.
    ///
    /// Each file's results are grouped by rule. The store handles:
    /// - String interning (if applicable)
    /// - Ref/span storage (if applicable)
    /// - Per-rule table row insertion
    /// - File registration and rev_files junction
    ///
    /// Returns total row count across all rules.
    fn flush_batch(
        &self,
        repo: &str,
        rev: &str,
        files: &[FileResult],
    ) -> impl std::future::Future<Output = Result<usize>> + Send;

    /// Create per-rule tables/structures from rule definitions.
    /// Called at startup after parsing .sprf files.
    fn create_rule_tables(
        &self,
        tables: &[RuleTableSpec],
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Find repo names in the given rule table + column that haven't been scanned.
    fn unscanned_repos(
        &self,
        table: &str,
        column: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>>> + Send;

    /// Find (repo, rev) pairs in the given rule table + column that haven't been scanned.
    fn unscanned_revs(
        &self,
        table: &str,
        column: &str,
    ) -> impl std::future::Future<Output = Result<Vec<(String, String)>>> + Send;

    /// Find (repo_name, rev) pairs from paired columns that haven't been scanned.
    /// Joins repo_column and rev_column from the same row in the per-rule data table.
    fn unscanned_rev_pairs(
        &self,
        table: &str,
        repo_column: &str,
        rev_column: &str,
    ) -> impl std::future::Future<Output = Result<Vec<(String, String)>>> + Send;

    /// Remove files and their associated extraction data.
    fn delete_files(
        &self,
        repo: &str,
        rev: &str,
        paths: &[String],
    ) -> impl std::future::Future<Output = Result<usize>> + Send;

    /// Update file paths (pure renames, preserving extraction data).
    fn rename_files(
        &self,
        repo: &str,
        renames: &[(String, String)],
    ) -> impl std::future::Future<Output = Result<usize>> + Send;

    /// Check whether any file in the repo was scanned with a different binary hash.
    /// Returns true if at least one file has `scanner_hash != current_hash`,
    /// meaning a full re-scan is needed to pick up extraction logic changes.
    fn has_stale_scanner_hash(
        &self,
        repo: &str,
        current_hash: &str,
    ) -> impl std::future::Future<Output = Result<bool>> + Send;

    /// Load the set of files for `repo_name` that were last scanned with
    /// `scanner_hash`. Returns empty set if the repo does not exist yet.
    fn load_scan_context(
        &self,
        repo: &str,
        scanner_hash: &str,
    ) -> impl std::future::Future<Output = Result<ScanContext>> + Send;

    /// TEMPORARY: Access the underlying SQLite pool for legacy modules
    /// that haven't been migrated to the Store trait yet.
    /// This will be removed once all cache modules are behind the Store boundary.
    fn sqlite_pool(&self) -> Option<&sqlx::SqlitePool>;
}

/// Context for incremental scanning: which files can be skipped.
#[derive(Debug, Clone)]
pub struct ScanContext {
    /// (rel_path, content_hash) pairs already in the DB that were scanned with
    /// the current binary hash. Files in this set can skip extraction.
    pub skip_set: std::collections::HashSet<(String, String)>,
}

/// Spec for creating a per-rule table. Passed to Store::create_rule_tables().
#[derive(Debug, Clone)]
pub struct RuleTableSpec {
    pub rule_name: String,
    /// (column_name_lowercase, scan_annotation)
    pub columns: Vec<(String, Option<String>)>,
}

/// Convert extraction output (Vec<ExtractedFile>) into Store-compatible FileResults.
///
/// Groups RawRefs by (rule_name, group) to reconstruct extraction tuples.
/// Non-grouped refs (from built-in extractors) are excluded -- they go through
/// the legacy flush path.
pub fn to_file_results(files: &[sprefa_index::ExtractedFile]) -> Vec<FileResult> {
    files
        .iter()
        .map(|file| {
            // Group refs by (rule_name, group) -> Vec<RawRef>
            let mut groups: std::collections::HashMap<(&str, u32), Vec<&sprefa_extract::RawRef>> =
                std::collections::HashMap::new();
            for r in &file.refs {
                if let Some(g) = r.group {
                    groups.entry((r.rule_name.as_str(), g)).or_default().push(r);
                }
            }

            // Collect by rule_name.
            let mut rule_rows: std::collections::HashMap<&str, Vec<ExtractionRow>> =
                std::collections::HashMap::new();
            for ((rule_name, _), refs) in &groups {
                let row = ExtractionRow {
                    captures: refs
                        .iter()
                        .map(|r| CaptureEntry {
                            column: r.kind.to_lowercase(),
                            value: r.value.clone(),
                            span_start: r.span_start,
                            span_end: r.span_end,
                            node_path: r.node_path.clone(),
                            is_path: r.is_path,
                            parent_key: r.parent_key.clone(),
                            scan: r.scan.clone(),
                        })
                        .collect(),
                };
                rule_rows.entry(rule_name).or_default().push(row);
            }

            FileResult {
                rel_path: file.rel_path.clone(),
                content_hash: file.content_hash.clone(),
                stem: file.stem.clone(),
                ext: file.ext.clone(),
                rule_rows: rule_rows
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            }
        })
        .filter(|fr| !fr.rule_rows.is_empty())
        .collect()
}
