use std::collections::HashMap;
use std::path::Path;

/// Maps Rust crate names (underscored) to their `src/` directory absolute paths.
///
/// Used for resolving cross-crate workspace imports like `use other_crate::foo::Bar`
/// where `other_crate` is a workspace member.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceMap {
    /// crate_name (underscored) -> absolute path to the crate's src/ directory
    crates: HashMap<String, String>,
}

impl WorkspaceMap {
    pub fn is_empty(&self) -> bool {
        self.crates.is_empty()
    }

    /// Determine which workspace crate a file belongs to, by matching against
    /// known src roots. Returns the crate name (underscored).
    pub fn crate_for_file(&self, file_abs_path: &str) -> Option<&str> {
        for (name, src_root) in &self.crates {
            if file_abs_path.starts_with(src_root) {
                return Some(name);
            }
        }
        None
    }

    /// Check if a use path's first segment is a known workspace crate.
    /// Returns the crate name if found.
    pub fn is_workspace_crate(&self, first_segment: &str) -> bool {
        self.crates.contains_key(first_segment)
    }
}

/// Build a workspace map by parsing Cargo.toml files starting from `repo_root`.
///
/// Looks for a workspace-level Cargo.toml, enumerates its members,
/// and maps each member's package name to its src/ directory.
///
/// Returns an empty map if no workspace is found or if parsing fails.
pub fn build_workspace_map(repo_root: &str) -> WorkspaceMap {
    let root_toml = Path::new(repo_root).join("Cargo.toml");
    let content = match std::fs::read_to_string(&root_toml) {
        Ok(c) => c,
        Err(_) => return WorkspaceMap::default(),
    };

    let doc: toml::Value = match content.parse() {
        Ok(d) => d,
        Err(_) => return WorkspaceMap::default(),
    };

    // Get workspace.members array
    let members = match doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        Some(m) => m,
        None => return WorkspaceMap::default(),
    };

    let mut map = HashMap::new();

    for member_val in members {
        let pattern = match member_val.as_str() {
            Some(s) => s,
            None => continue,
        };

        // Resolve glob patterns (e.g., "crates/*")
        let glob_pattern = format!("{}/{}/Cargo.toml", repo_root, pattern);
        let paths = match glob::glob(&glob_pattern) {
            Ok(p) => p,
            Err(_) => continue,
        };

        for entry in paths.flatten() {
            let member_toml = match std::fs::read_to_string(&entry) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let member_doc: toml::Value = match member_toml.parse() {
                Ok(d) => d,
                Err(_) => continue,
            };

            let pkg_name = match member_doc
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
            {
                Some(n) => n,
                None => continue,
            };

            // Cargo normalizes hyphens to underscores in use statements
            let crate_name = pkg_name.replace('-', "_");

            // src/ directory for this crate
            let member_dir = entry.parent().unwrap_or(Path::new(""));
            let src_dir = member_dir.join("src");
            if src_dir.is_dir() {
                map.insert(crate_name, src_dir.to_string_lossy().to_string());
            }
        }
    }

    WorkspaceMap { crates: map }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_for_file_matches() {
        let mut crates = HashMap::new();
        crates.insert("lib_a".to_string(), "/repo/crates/lib_a/src".to_string());
        crates.insert("lib_b".to_string(), "/repo/crates/lib_b/src".to_string());
        let ws = WorkspaceMap { crates };

        assert_eq!(ws.crate_for_file("/repo/crates/lib_a/src/utils.rs"), Some("lib_a"));
        assert_eq!(ws.crate_for_file("/repo/crates/lib_b/src/foo/bar.rs"), Some("lib_b"));
        assert_eq!(ws.crate_for_file("/repo/other/file.rs"), None);
    }

    #[test]
    fn is_workspace_crate_checks() {
        let mut crates = HashMap::new();
        crates.insert("lib_a".to_string(), "/repo/crates/lib_a/src".to_string());
        let ws = WorkspaceMap { crates };

        assert!(ws.is_workspace_crate("lib_a"));
        assert!(!ws.is_workspace_crate("std"));
        assert!(!ws.is_workspace_crate("serde"));
    }

    #[test]
    fn hyphen_normalization() {
        // Build with a temp dir that has a workspace Cargo.toml
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Write workspace Cargo.toml
        std::fs::write(
            root.join("Cargo.toml"),
            r#"
[workspace]
members = ["crates/*"]
"#,
        ).unwrap();

        // Create a member crate with hyphenated name
        let member_dir = root.join("crates/my-lib");
        std::fs::create_dir_all(member_dir.join("src")).unwrap();
        std::fs::write(
            member_dir.join("Cargo.toml"),
            r#"
[package]
name = "my-lib"
version = "0.1.0"
edition = "2021"
"#,
        ).unwrap();

        let ws = build_workspace_map(root.to_str().unwrap());
        assert!(ws.is_workspace_crate("my_lib"));
        assert!(!ws.is_workspace_crate("my-lib"));
    }

    #[test]
    fn nonexistent_repo_returns_empty() {
        let ws = build_workspace_map("/nonexistent/path");
        assert!(ws.is_empty());
    }
}
