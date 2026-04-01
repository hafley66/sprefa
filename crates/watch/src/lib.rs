pub mod change;
pub mod diff;
pub mod js_path;
pub mod plan;
pub mod queries;
pub mod rewrite;
pub mod rs_path;
pub mod watcher;
pub mod workspace;

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
