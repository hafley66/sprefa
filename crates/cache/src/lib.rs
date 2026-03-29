pub mod flush;
pub mod match_links;
pub mod resolve;
pub mod scan_context;

pub use flush::flush;
pub use match_links::resolve_match_links;
pub use sprefa_rules::LinkRule;
pub use resolve::resolve_import_targets;
pub use scan_context::{load_scan_context, ScanContext};

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
