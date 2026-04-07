pub mod discovery;
pub mod flush;
pub mod meta;
pub mod resolve;
pub mod scan_context;
pub mod store;
pub mod sqlite_store;

pub use flush::{flush, delete_rev_files_by_paths, rename_file_paths};
pub use meta::flush_repo_meta;
pub use resolve::resolve_import_targets;
pub use store::{Store, to_file_results, ScanContext};
pub use sqlite_store::SqliteStore;

/// Return the working-tree rev name for a given base rev.
/// e.g. `"main"` -> `"main+wt"`
pub fn wt_rev(rev: &str) -> String {
    format!("{rev}+wt")
}

/// Check whether a rev name represents a working-tree rev.
pub fn is_wt_rev(rev: &str) -> bool {
    rev.ends_with("+wt")
}

/// Strip the `+wt` suffix to recover the base rev name.
/// Returns the input unchanged if there is no suffix.
pub fn base_rev(rev: &str) -> &str {
    rev.strip_suffix("+wt").unwrap_or(rev)
}
