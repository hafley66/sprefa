pub mod flush;
pub mod match_links;
pub mod meta;
pub mod resolve;
pub mod scan_context;

pub mod tags;

pub use flush::{flush, delete_branch_files_by_paths, rename_file_paths};
pub use meta::flush_repo_meta;
pub use tags::flush_git_tags;
pub use match_links::resolve_match_links;
pub use sprefa_rules::LinkRule;
pub use resolve::resolve_import_targets;
pub use scan_context::{has_stale_scanner_hash, load_scan_context, ScanContext};

/// Return the working-tree branch name for a given base branch.
/// e.g. `"main"` -> `"main+wt"`
pub fn wt_branch(branch: &str) -> String {
    format!("{branch}+wt")
}

/// Check whether a branch name represents a working-tree branch.
pub fn is_wt_branch(branch: &str) -> bool {
    branch.ends_with("+wt")
}

/// Strip the `+wt` suffix to recover the base branch name.
/// Returns the input unchanged if there is no suffix.
pub fn base_branch(branch: &str) -> &str {
    branch.strip_suffix("+wt").unwrap_or(branch)
}
