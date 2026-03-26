use std::path::Path;

use crate::plan::PathRewriter;

const RS_EXTENSIONS: &[&str] = &["rs"];

pub struct RsPathRewriter;

impl PathRewriter for RsPathRewriter {
    fn extensions(&self) -> &[&str] {
        RS_EXTENSIONS
    }

    fn rewrite_import(
        &self,
        from_file: &str,
        old_target: &str,
        new_target: &str,
        old_import_str: &str,
    ) -> Option<String> {
        // RsUse refs store full paths like `crate::utils::Foo` or `super::bar::Baz`.
        // When a file moves, its module path changes.
        // We need to replace the old module path prefix with the new one inside the use path.

        let old_mod = file_to_mod_path(old_target)?;
        let new_mod = file_to_mod_path(new_target)?;

        // Determine which prefix style the use statement uses (crate::, self::, super::)
        // and what the old module path looks like from the importing file's perspective.
        let from_mod = file_to_mod_path(from_file)?;

        // Try to match and rewrite the use path.
        rewrite_use_path(old_import_str, &old_mod, &new_mod, &from_mod)
    }
}

/// Convert a file path to a Rust module path.
///
/// Examples:
///   src/lib.rs       -> "crate"
///   src/main.rs      -> "crate"
///   src/foo.rs       -> "crate::foo"
///   src/foo/mod.rs   -> "crate::foo"
///   src/foo/bar.rs   -> "crate::foo::bar"
///   src/foo/bar/mod.rs -> "crate::foo::bar"
///
/// Returns None if the path doesn't look like a Rust source file under src/.
pub fn file_to_mod_path(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);

    // Find the "src" directory in the path to determine the crate root.
    let components: Vec<&str> = path
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();

    let src_idx = components.iter().rposition(|c| *c == "src")?;

    // Everything after "src/" forms the module path.
    let after_src: Vec<&str> = components[src_idx + 1..].to_vec();

    if after_src.is_empty() {
        return None;
    }

    let last = *after_src.last().unwrap();
    let stem = Path::new(last)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(last);

    // lib.rs or main.rs at the crate root = "crate"
    if after_src.len() == 1 && (stem == "lib" || stem == "main") {
        return Some("crate".to_string());
    }

    // mod.rs = the directory is the module
    let mut segments = Vec::with_capacity(after_src.len());
    segments.push("crate");

    if stem == "mod" {
        // src/foo/bar/mod.rs -> crate::foo::bar (directories only, skip mod.rs)
        for dir in &after_src[..after_src.len() - 1] {
            segments.push(dir);
        }
    } else {
        // src/foo/bar.rs -> crate::foo::bar (directories + file stem)
        for dir in &after_src[..after_src.len() - 1] {
            segments.push(dir);
        }
        segments.push(stem);
    }

    Some(segments.join("::"))
}

/// Rewrite a use path after a module move.
///
/// Given `use crate::old::path::Item` and old_mod=`crate::old::path`, new_mod=`crate::new::path`,
/// produces `crate::new::path::Item`.
///
/// Also handles super:: and self:: prefixes by resolving them to absolute paths first,
/// performing the substitution, then converting back to the appropriate prefix style.
fn rewrite_use_path(
    use_path: &str,
    old_mod: &str,
    new_mod: &str,
    from_mod: &str,
) -> Option<String> {
    // Resolve the use path to an absolute (crate::...) form for matching.
    let abs_path = resolve_to_absolute(use_path, from_mod)?;

    // Check if this use path references something under the old module path.
    if abs_path == old_mod {
        // Importing the module itself (e.g., `use crate::utils` and utils.rs moved).
        return Some(reconvert_prefix(new_mod, use_path, from_mod));
    }

    let old_prefix = format!("{}::", old_mod);
    if abs_path.starts_with(&old_prefix) {
        let suffix = &abs_path[old_prefix.len()..];
        let new_abs = format!("{}::{}", new_mod, suffix);
        return Some(reconvert_prefix(&new_abs, use_path, from_mod));
    }

    None
}

/// Resolve a use path to absolute form.
/// - `crate::foo::Bar` stays as is
/// - `self::bar::Baz` resolves relative to from_mod
/// - `super::baz::Qux` resolves by popping one segment from from_mod
fn resolve_to_absolute(use_path: &str, from_mod: &str) -> Option<String> {
    if use_path.starts_with("crate::") || use_path == "crate" {
        return Some(use_path.to_string());
    }

    if use_path.starts_with("self::") {
        let rest = &use_path[6..]; // skip "self::"
        return Some(format!("{}::{}", from_mod, rest));
    }

    if use_path.starts_with("super::") {
        let mut current = from_mod.to_string();
        let mut path = use_path;

        while path.starts_with("super::") {
            path = &path[7..]; // skip "super::"
            // Pop last segment from current module.
            if let Some(pos) = current.rfind("::") {
                current = current[..pos].to_string();
            } else {
                return None; // super:: beyond crate root
            }
        }

        return Some(format!("{}::{}", current, path));
    }

    // External crate paths (std::, serde::, etc.) -- not rewritable by file moves.
    None
}

/// After rewriting the absolute path, convert back to the prefix style the original used.
/// If the original used `super::`, try to express the result as super:: relative to from_mod.
/// If the original used `self::`, try to express as self::.
/// Otherwise, use crate::.
fn reconvert_prefix(new_abs: &str, original: &str, from_mod: &str) -> String {
    if original.starts_with("crate::") || original == "crate" {
        return new_abs.to_string();
    }

    if original.starts_with("self::") {
        let prefix = format!("{}::", from_mod);
        if new_abs.starts_with(&prefix) {
            return format!("self::{}", &new_abs[prefix.len()..]);
        }
        // Can't express as self:: anymore, fall back to crate::
        return new_abs.to_string();
    }

    if original.starts_with("super::") {
        // Count how many super:: the original had.
        let super_count = original.matches("super::").count();
        let mut parent = from_mod.to_string();
        for _ in 0..super_count {
            if let Some(pos) = parent.rfind("::") {
                parent = parent[..pos].to_string();
            } else {
                return new_abs.to_string();
            }
        }
        let prefix = format!("{}::", parent);
        if new_abs.starts_with(&prefix) {
            let super_chain = "super::".repeat(super_count);
            return format!("{}{}", super_chain, &new_abs[prefix.len()..]);
        }
        // Can't express as super:: anymore, fall back to crate::
        return new_abs.to_string();
    }

    new_abs.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── file_to_mod_path ────────────────────────────────────────────────────

    #[test]
    fn lib_rs_is_crate() {
        assert_eq!(file_to_mod_path("/repo/src/lib.rs").as_deref(), Some("crate"));
    }

    #[test]
    fn main_rs_is_crate() {
        assert_eq!(file_to_mod_path("/repo/src/main.rs").as_deref(), Some("crate"));
    }

    #[test]
    fn simple_module() {
        assert_eq!(
            file_to_mod_path("/repo/src/utils.rs").as_deref(),
            Some("crate::utils")
        );
    }

    #[test]
    fn nested_module() {
        assert_eq!(
            file_to_mod_path("/repo/src/foo/bar.rs").as_deref(),
            Some("crate::foo::bar")
        );
    }

    #[test]
    fn mod_rs() {
        assert_eq!(
            file_to_mod_path("/repo/src/foo/mod.rs").as_deref(),
            Some("crate::foo")
        );
    }

    #[test]
    fn deeply_nested_mod_rs() {
        assert_eq!(
            file_to_mod_path("/repo/src/a/b/mod.rs").as_deref(),
            Some("crate::a::b")
        );
    }

    // ── rewrite_import (PathRewriter trait) ─────────────────────────────────

    fn rewrite(from: &str, old: &str, new: &str, import: &str) -> Option<String> {
        RsPathRewriter.rewrite_import(from, old, new, import)
    }

    #[test]
    fn rewrite_crate_prefixed_use() {
        // src/utils.rs moves to src/lib/utils.rs
        // use crate::utils::Foo -> use crate::lib::utils::Foo
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/src/utils.rs",
            "/repo/src/helpers/utils.rs",
            "crate::utils::Foo",
        );
        assert_eq!(result.as_deref(), Some("crate::helpers::utils::Foo"));
    }

    #[test]
    fn rewrite_module_itself() {
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/src/utils.rs",
            "/repo/src/helpers/utils.rs",
            "crate::utils",
        );
        assert_eq!(result.as_deref(), Some("crate::helpers::utils"));
    }

    #[test]
    fn rewrite_super_prefixed_use() {
        // from: src/foo/consumer.rs (crate::foo::consumer)
        // old:  src/foo/bar.rs     (crate::foo::bar)
        // new:  src/baz/bar.rs     (crate::baz::bar)
        // use super::bar::Thing -> can't express as super:: anymore, falls back to crate::
        let result = rewrite(
            "/repo/src/foo/consumer.rs",
            "/repo/src/foo/bar.rs",
            "/repo/src/baz/bar.rs",
            "super::bar::Thing",
        );
        assert_eq!(result.as_deref(), Some("crate::baz::bar::Thing"));
    }

    #[test]
    fn rewrite_super_stays_super_when_possible() {
        // from: src/foo/consumer.rs (crate::foo::consumer)
        // old:  src/foo/bar.rs      (crate::foo::bar -> crate::foo::bar)
        // new:  src/foo/qux.rs      (crate::foo::qux)
        // super::bar::Thing -> super::qux::Thing (still under same parent)
        // Wait -- from_mod is crate::foo::consumer, super:: resolves to crate::foo
        // old_mod is crate::foo::bar, new_mod is crate::foo::qux
        // abs = crate::foo::bar::Thing, prefix match, new_abs = crate::foo::qux::Thing
        // reconvert: parent after 1 super is crate::foo, prefix is crate::foo::
        // new_abs starts with crate::foo:: -> super::qux::Thing
        let result = rewrite(
            "/repo/src/foo/consumer.rs",
            "/repo/src/foo/bar.rs",
            "/repo/src/foo/qux.rs",
            "super::bar::Thing",
        );
        assert_eq!(result.as_deref(), Some("super::qux::Thing"));
    }

    #[test]
    fn rewrite_self_prefixed_use() {
        // from: src/foo/mod.rs (crate::foo)
        // old:  src/foo/bar.rs (crate::foo::bar)
        // new:  src/foo/baz.rs (crate::foo::baz)
        // self::bar::X -> self::baz::X
        let result = rewrite(
            "/repo/src/foo/mod.rs",
            "/repo/src/foo/bar.rs",
            "/repo/src/foo/baz.rs",
            "self::bar::X",
        );
        assert_eq!(result.as_deref(), Some("self::baz::X"));
    }

    #[test]
    fn external_crate_returns_none() {
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/src/utils.rs",
            "/repo/src/helpers/utils.rs",
            "std::collections::HashMap",
        );
        assert_eq!(result, None);
    }

    #[test]
    fn unrelated_module_returns_none() {
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/src/utils.rs",
            "/repo/src/helpers/utils.rs",
            "crate::config::Settings",
        );
        assert_eq!(result, None);
    }

    #[test]
    fn glob_import_rewrite() {
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/src/utils.rs",
            "/repo/src/helpers/utils.rs",
            "crate::utils::*",
        );
        assert_eq!(result.as_deref(), Some("crate::helpers::utils::*"));
    }

    #[test]
    fn workspace_crate_path() {
        // Workspace member: crates/foo/src/bar.rs
        assert_eq!(
            file_to_mod_path("/repo/crates/foo/src/bar.rs").as_deref(),
            Some("crate::bar")
        );
    }

    // ── file_to_mod_path edge cases ───────────────────────────────────────

    #[test]
    fn no_src_dir_returns_none() {
        assert_eq!(file_to_mod_path("/repo/lib/utils.rs"), None);
    }

    #[test]
    fn file_directly_in_src_no_extension() {
        // Pathological: no .rs extension. file_stem returns the whole filename.
        assert_eq!(
            file_to_mod_path("/repo/src/Makefile").as_deref(),
            Some("crate::Makefile")
        );
    }

    #[test]
    fn multiple_src_dirs_uses_last() {
        // rposition picks the last "src" occurrence
        assert_eq!(
            file_to_mod_path("/repo/src/generated/src/models.rs").as_deref(),
            Some("crate::models")
        );
    }

    #[test]
    fn src_alone_returns_none() {
        // Just "src" with nothing after it
        assert_eq!(file_to_mod_path("/repo/src/"), None);
    }

    #[test]
    fn deeply_nested_file() {
        assert_eq!(
            file_to_mod_path("/repo/src/a/b/c/d/e.rs").as_deref(),
            Some("crate::a::b::c::d::e")
        );
    }

    // ── rewrite edge cases ────────────────────────────────────────────────

    #[test]
    fn double_super() {
        // from: src/a/b/consumer.rs (crate::a::b::consumer)
        // old:  src/a/target.rs     (crate::a::target)
        // new:  src/c/target.rs     (crate::c::target)
        // super::super::target::X resolves to crate::a::target::X (pop b, pop a... wait)
        // Actually from_mod = crate::a::b::consumer
        // super:: pops to crate::a::b, super:: again pops to crate::a
        // So super::super::target::X = crate::a::target::X
        // After move: crate::c::target::X
        // Can we express as super::super::? parent after 2 supers = crate::a
        // crate::c::target::X doesn't start with crate::a:: -> fallback to crate::
        let result = rewrite(
            "/repo/src/a/b/consumer.rs",
            "/repo/src/a/target.rs",
            "/repo/src/c/target.rs",
            "super::super::target::X",
        );
        assert_eq!(result.as_deref(), Some("crate::c::target::X"));
    }

    #[test]
    fn double_super_stays_when_possible() {
        // from: src/a/b/consumer.rs (crate::a::b::consumer)
        // old:  src/a/old.rs        (crate::a::old)
        // new:  src/a/new.rs        (crate::a::new)
        // super::super::old::X -> super::super::new::X (same grandparent)
        let result = rewrite(
            "/repo/src/a/b/consumer.rs",
            "/repo/src/a/old.rs",
            "/repo/src/a/new.rs",
            "super::super::old::X",
        );
        assert_eq!(result.as_deref(), Some("super::super::new::X"));
    }

    #[test]
    fn self_fallback_to_crate_when_moved_out() {
        // from: src/foo/mod.rs (crate::foo)
        // old:  src/foo/bar.rs (crate::foo::bar)
        // new:  src/other/bar.rs (crate::other::bar)
        // self::bar::X -> crate::other::bar::X (can't be self:: anymore)
        let result = rewrite(
            "/repo/src/foo/mod.rs",
            "/repo/src/foo/bar.rs",
            "/repo/src/other/bar.rs",
            "self::bar::X",
        );
        assert_eq!(result.as_deref(), Some("crate::other::bar::X"));
    }

    #[test]
    fn super_beyond_crate_root_returns_none() {
        // from: src/foo.rs (crate::foo) -- one level deep
        // super:: resolves to crate, then super:: again tries to pop past crate -> None
        let result = rewrite(
            "/repo/src/foo.rs",
            "/repo/src/bar.rs",
            "/repo/src/baz.rs",
            "super::super::bar::X",
        );
        assert_eq!(result, None);
    }

    #[test]
    fn mod_rs_file_moves() {
        // Moving a mod.rs to a different directory
        // from: src/app.rs (crate::app)
        // old:  src/utils/mod.rs (crate::utils)
        // new:  src/helpers/mod.rs (crate::helpers)
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/src/utils/mod.rs",
            "/repo/src/helpers/mod.rs",
            "crate::utils::Foo",
        );
        assert_eq!(result.as_deref(), Some("crate::helpers::Foo"));
    }

    #[test]
    fn mod_rs_module_itself() {
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/src/utils/mod.rs",
            "/repo/src/helpers/mod.rs",
            "crate::utils",
        );
        assert_eq!(result.as_deref(), Some("crate::helpers"));
    }

    #[test]
    fn glob_with_super() {
        // super::old::* where old moves but stays as sibling
        let result = rewrite(
            "/repo/src/foo/consumer.rs",
            "/repo/src/foo/old.rs",
            "/repo/src/foo/fresh.rs",
            "super::old::*",
        );
        assert_eq!(result.as_deref(), Some("super::fresh::*"));
    }

    #[test]
    fn from_file_has_no_src_returns_none() {
        // from_file doesn't have src/ -> file_to_mod_path returns None -> overall None
        let result = rewrite(
            "/repo/lib/consumer.rs",
            "/repo/src/utils.rs",
            "/repo/src/helpers/utils.rs",
            "crate::utils::Foo",
        );
        assert_eq!(result, None);
    }

    #[test]
    fn target_has_no_src_returns_none() {
        let result = rewrite(
            "/repo/src/app.rs",
            "/repo/lib/utils.rs",
            "/repo/src/utils.rs",
            "crate::utils::Foo",
        );
        assert_eq!(result, None);
    }
}
