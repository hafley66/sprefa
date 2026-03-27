pub mod change;
pub mod diff;
pub mod js_path;
pub mod plan;
pub mod queries;
pub mod rewrite;
pub mod rs_path;
pub mod watcher;
pub mod workspace;

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
