//! Kotlin move-refactor path math. A Kotlin import is `package.Decl` and the
//! `package` declaration is truth (the directory is advisory), so a file move
//! only changes imports when the new directory implies a new package under the
//! SAME source root the old file sat in. The plan derives that root from the
//! old path + declared package; a layout that disagrees with the package is a
//! loud skip, never a guessed rewrite.

use anyhow::{bail, Result};

use crate::modgraph::{kotlin_decl_re, kotlin_package_re, strip_kotlin};

/// One Kotlin file move, resolved to the import-rewrite it implies.
#[derive(Debug)]
pub struct KotlinMove {
    pub old_pkg: String,
    pub new_pkg: String,
    /// Column-0 decl names the moved file exports — the only names whose
    /// imports rewrite.
    pub decls: Vec<String>,
}

/// Derive the rewrite plan for moving `old_path` (with `content`) to
/// `new_path`. Errors name exactly why no rewrite is possible.
pub fn plan_move(old_path: &str, new_path: &str, content: &str) -> Result<KotlinMove> {
    let clean = strip_kotlin(content);
    let Some(old_pkg) = package_of(&clean) else {
        bail!("{old_path} has no package declaration; default-package decls are not importable");
    };
    let Some(root) = source_root(old_path, &old_pkg) else {
        bail!("{old_path}'s directory does not match its package {old_pkg} \
            (the declaration is truth) — cannot infer the new package from {new_path}");
    };
    let Some(new_pkg) = package_for(new_path, &root) else {
        bail!("{new_path} is outside the source root {root:?} of {old_path}");
    };
    let decls: Vec<String> = kotlin_decl_re().captures_iter(&clean)
        .map(|c| c[1].trim_matches('`').to_string())
        .collect();
    if decls.is_empty() {
        bail!("{old_path} has no column-0 declarations; nothing imports it by name");
    }
    Ok(KotlinMove { old_pkg, new_pkg, decls })
}

/// The declared package of (already-stripped) .kt content.
fn package_of(clean: &str) -> Option<String> {
    kotlin_package_re().captures(clean).map(|c| c[1].replace('`', ""))
}

/// The source root `R` such that `dir(path) == R/pkg-as-dirs` ("" = repo
/// root). None when the directory layout disagrees with the package.
fn source_root(path: &str, pkg: &str) -> Option<String> {
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    if pkg.is_empty() { return Some(dir.to_string()); }
    let suffix = pkg.replace('.', "/");
    if dir == suffix { return Some(String::new()); }
    dir.strip_suffix(&format!("/{suffix}")).map(str::to_string)
}

/// The package `path` answers to under source root `root`.
fn package_for(path: &str, root: &str) -> Option<String> {
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let rel = if root.is_empty() {
        dir
    } else if dir == root {
        ""
    } else {
        dir.strip_prefix(&format!("{root}/"))?
    };
    Some(rel.replace('/', "."))
}

impl KotlinMove {
    /// Rewrite one import specifier, or None when this move does not touch it.
    /// Matches `old_pkg.Name` / `old_pkg.Name.Nested` where `Name` is a decl
    /// of the moved file; `old_pkg.*` wildcards are NOT rewritten (the package
    /// may keep other files) — the caller counts those loudly.
    pub fn rewrite_import(&self, spec: &str) -> Option<String> {
        let rest = spec.strip_prefix(&self.old_pkg)?.strip_prefix('.')?;
        let name = rest.split('.').next().unwrap_or(rest);
        if !self.decls.iter().any(|d| d == name) { return None; }
        Some(format!("{}.{rest}", self.new_pkg))
    }

    /// The wildcard specifier of the old package, for skip accounting.
    pub fn old_wildcard(&self) -> String { format!("{}.*", self.old_pkg) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_package_math() {
        assert_eq!(source_root("lib/com/lib/Util.kt", "com.lib").as_deref(), Some("lib"));
        assert_eq!(source_root("com/lib/Util.kt", "com.lib").as_deref(), Some(""));
        assert_eq!(source_root("weird/spot/Util.kt", "com.lib"), None);
        assert_eq!(package_for("lib/com/core/Util.kt", "lib").as_deref(), Some("com.core"));
        assert_eq!(package_for("lib/Util.kt", "lib").as_deref(), Some(""));
        assert_eq!(package_for("other/com/core/Util.kt", "lib"), None);
    }

    #[test]
    fn plan_rewrites_decl_imports_only() {
        let content = "package com.lib\n\nclass Util\nfun helper() = 1\n";
        let mv = plan_move("lib/com/lib/Util.kt", "lib/com/core/Util.kt", content).unwrap();
        assert_eq!(mv.old_pkg, "com.lib");
        assert_eq!(mv.new_pkg, "com.core");
        assert_eq!(mv.rewrite_import("com.lib.Util").as_deref(), Some("com.core.Util"));
        assert_eq!(mv.rewrite_import("com.lib.Util.Inner").as_deref(), Some("com.core.Util.Inner"));
        assert_eq!(mv.rewrite_import("com.lib.helper").as_deref(), Some("com.core.helper"));
        // another file's decl in the same package, a wildcard, a foreign package
        assert_eq!(mv.rewrite_import("com.lib.Other"), None);
        assert_eq!(mv.rewrite_import("com.lib.*"), None);
        assert_eq!(mv.rewrite_import("com.libx.Util"), None);
    }

    #[test]
    fn plan_is_loud_when_layout_disagrees_with_package() {
        let content = "package com.lib\nclass Util\n";
        let err = plan_move("weird/spot/Util.kt", "lib/com/core/Util.kt", content).unwrap_err();
        assert!(err.to_string().contains("does not match its package"), "{err}");
    }
}
