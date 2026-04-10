/// Storage trait: the boundary between extraction logic and persistence.
///
/// All data storage flows through this trait. SqliteStore writes to per-rule
/// SQLite tables + a string index. MemoryStore holds HashMaps. A JsonStore
/// could write NDJSON. The extraction pipeline and discovery loop never
/// touch storage directly.
use anyhow::Result;
pub use sprefa_sprf::hash::RuleHashes;
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
    /// - Writing `scanner_hash` so the skip set works on subsequent scans
    ///
    /// Returns total row count across all rules.
    fn flush_batch(
        &self,
        repo: &str,
        rev: &str,
        files: &[FileResult],
        scanner_hash: &str,
    ) -> impl std::future::Future<Output = Result<usize>> + Send;

    /// Create per-rule tables/structures from rule definitions.
    /// Called at startup after parsing .sprf files.
    ///
    /// If `hashes` is provided, checks sprf_meta for changes:
    /// - SchemaChanged: DROP table + CREATE
    /// - ExtractChanged: DELETE all rows (keep table)
    /// - Unchanged: skip (table + data preserved)
    fn create_rule_tables(
        &self,
        tables: &[RuleTableSpec],
        hashes: Option<&std::collections::HashMap<String, RuleHashes>>,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// Find repo names in the given rule table + column that haven't been scanned.
    ///
    /// When `norm` is true, the captured value is compared via its normalized
    /// form (`strings.norm`) against `sprf_norm(repos.name)`, letting e.g.
    /// `Auth-Service` satisfy an already-scanned `auth_service` repo.
    fn unscanned_repos(
        &self,
        table: &str,
        column: &str,
        norm: bool,
    ) -> impl std::future::Future<Output = Result<Vec<String>>> + Send;

    /// Find (repo, rev) pairs in the given rule table + column that haven't been scanned.
    fn unscanned_revs(
        &self,
        table: &str,
        column: &str,
        norm: bool,
    ) -> impl std::future::Future<Output = Result<Vec<(String, String)>>> + Send;

    /// Find (repo_name, rev) pairs from paired columns that haven't been scanned.
    /// Joins repo_column and rev_column from the same row in the per-rule data table.
    ///
    /// `repo_norm` / `rev_norm` independently select normalized comparison
    /// for each side (the `.norm` scan annotation variants).
    fn unscanned_rev_pairs(
        &self,
        table: &str,
        repo_column: &str,
        rev_column: &str,
        repo_norm: bool,
        rev_norm: bool,
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

    /// Return relative paths of files scanned with a different binary hash.
    /// These need re-extraction to pick up extraction logic changes.
    /// Empty if no stale files exist.
    fn stale_file_paths(
        &self,
        repo: &str,
        current_hash: &str,
    ) -> impl std::future::Future<Output = Result<Vec<String>>> + Send;

    /// Load the set of files for `repo_name` that were last scanned with
    /// `scanner_hash`. Returns empty set if the repo does not exist yet.
    fn load_scan_context(
        &self,
        repo: &str,
        scanner_hash: &str,
    ) -> impl std::future::Future<Output = Result<ScanContext>> + Send;

    /// Check rule hashes for change detection.
    ///
    /// Returns `None` if rule not in sprf_meta (new rule) or table doesn't exist.
    /// Returns `Some` with comparison result for existing rules.
    fn check_rule_hashes(
        &self,
        rule_name: &str,
        schema_hash: &str,
        extract_hash: &str,
    ) -> impl std::future::Future<Output = Result<Option<RuleChangeKind>>> + Send;

    /// Update rule hashes after successful extraction.
    fn update_rule_hashes(
        &self,
        rule_name: &str,
        source_file: &str,
        schema_hash: &str,
        extract_hash: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;

    /// TEMPORARY: Access the underlying SQLite pool for legacy modules
    /// that haven't been migrated to the Store trait yet.
    /// This will be removed once all cache modules are behind the Store boundary.
    fn sqlite_pool(&self) -> Option<&sqlx::SqlitePool>;

    /// Run a check block SQL query and store violations.
    /// Returns the number of violations found.
    fn run_check(
        &self,
        check_name: &str,
        sql: &str,
    ) -> impl std::future::Future<Output = Result<usize>> + Send;

    /// Query all stored violations, optionally filtered by check name.
    fn list_violations(
        &self,
        check_name: Option<&str>,
    ) -> impl std::future::Future<Output = Result<Vec<ViolationEntry>>> + Send;
}

/// A single violation entry from the invariant_violations table.
#[derive(Debug, Clone)]
pub struct ViolationEntry {
    pub id: i64,
    pub check_name: String,
    pub violation_data: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

/// Result of rule hash comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleChangeKind {
    /// No changes detected - skip extraction.
    Unchanged,
    /// Schema changed (columns) - DROP + CREATE table + full extract.
    SchemaChanged,
    /// Pattern changed (but not columns) - DELETE rows + re-extract.
    ExtractChanged,
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
    /// None = default schema (builtins). Some(stem) = namespaced from .sprf filename.
    pub namespace: Option<String>,
    /// (column_name_lowercase, scan_annotation)
    pub columns: Vec<(String, Option<String>)>,
}

/// Convert extraction output (Vec<ExtractedFile>) into Store-compatible FileResults.
///
/// Groups RawRefs by (rule_name, group) to reconstruct extraction tuples.
/// Non-grouped refs are excluded from per-rule table insertion.
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
                let is_builtin = refs.len() == 1 && refs[0].kind.eq_ignore_ascii_case(rule_name);
                let row = ExtractionRow {
                    captures: refs
                        .iter()
                        .map(|r| CaptureEntry {
                            column: if is_builtin { "value".to_string() } else { r.kind.to_lowercase() },
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
