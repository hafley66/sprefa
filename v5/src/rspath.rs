//! Rust `use`-path rewrite arithmetic for the auto-refactor sink (Route A).
//! Pure string/path math, no engine or DB access: given a file move
//! `old_target -> new_target`, rewrite a `use` specifier so it still points at
//! the moved module, preserving the `crate::`/`self::`/`super::` prefix style
//! the original used. Ported from the OG coordinate model
//! (`sprefa-archive-20260428/crates/watch/src/rs_path.rs`). The engine feeds
//! these the leaf text located in `_where_bytes` (see `ref`); the new text is
//! interned and spliced back at the same byte span.

use std::path::Path;

/// Convert a file path to its Rust module path.
///
///   src/lib.rs         -> "crate"
///   src/main.rs        -> "crate"
///   src/foo.rs         -> "crate::foo"
///   src/foo/mod.rs     -> "crate::foo"
///   src/foo/bar.rs     -> "crate::foo::bar"
///   src/foo/bar/mod.rs -> "crate::foo::bar"
///
/// Keys off the last `src/` component, so workspace members
/// (`crates/foo/src/bar.rs`) resolve within their own crate (`crate::bar`).
/// Returns `None` for a path with no `src/` or nothing after it.
pub fn file_to_mod_path(file_path: &str) -> Option<String> {
    let components: Vec<&str> = Path::new(file_path)
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect();
    let src_idx = components.iter().rposition(|c| *c == "src")?;
    let after_src: Vec<&str> = components[src_idx + 1..].to_vec();
    if after_src.is_empty() {
        return None;
    }
    let last = *after_src.last().unwrap();
    let stem = Path::new(last).file_stem().and_then(|s| s.to_str()).unwrap_or(last);

    // lib.rs / main.rs at the crate root is the crate itself.
    if after_src.len() == 1 && (stem == "lib" || stem == "main") {
        return Some("crate".to_string());
    }
    let mut segments = Vec::with_capacity(after_src.len() + 1);
    segments.push("crate");
    if stem == "mod" {
        // src/foo/bar/mod.rs -> crate::foo::bar (directories only).
        for dir in &after_src[..after_src.len() - 1] { segments.push(dir); }
    } else {
        // src/foo/bar.rs -> crate::foo::bar (directories + file stem).
        for dir in &after_src[..after_src.len() - 1] { segments.push(dir); }
        segments.push(stem);
    }
    Some(segments.join("::"))
}

/// Resolve a `use` path to absolute (`crate::...`) form for matching.
/// `crate::` stays; `self::` resolves relative to `from_mod`; `super::` pops one
/// segment of `from_mod` per `super::`. External crate paths (`std::`, `serde::`)
/// are not rewritable by a file move and return `None`.
pub fn resolve_to_absolute(use_path: &str, from_mod: &str) -> Option<String> {
    if use_path == "crate" || use_path.starts_with("crate::") {
        return Some(use_path.to_string());
    }
    if let Some(rest) = use_path.strip_prefix("self::") {
        return Some(format!("{from_mod}::{rest}"));
    }
    if use_path.starts_with("super::") {
        let mut current = from_mod.to_string();
        let mut path = use_path;
        while let Some(rest) = path.strip_prefix("super::") {
            path = rest;
            match current.rfind("::") {
                Some(pos) => current = current[..pos].to_string(),
                None => return None, // super:: beyond crate root
            }
        }
        return Some(format!("{current}::{path}"));
    }
    None
}

/// After rewriting the absolute path, re-express it in the prefix style the
/// original used (`super::`/`self::`/`crate::`), falling back to `crate::` when
/// the moved target no longer sits under the original relative anchor.
fn reconvert_prefix(new_abs: &str, original: &str, from_mod: &str) -> String {
    if original == "crate" || original.starts_with("crate::") {
        return new_abs.to_string();
    }
    if original.starts_with("self::") {
        let prefix = format!("{from_mod}::");
        if let Some(rest) = new_abs.strip_prefix(&prefix) {
            return format!("self::{rest}");
        }
        return new_abs.to_string();
    }
    if original.starts_with("super::") {
        let super_count = original.matches("super::").count();
        let mut parent = from_mod.to_string();
        for _ in 0..super_count {
            match parent.rfind("::") {
                Some(pos) => parent = parent[..pos].to_string(),
                None => return new_abs.to_string(),
            }
        }
        let prefix = format!("{parent}::");
        if let Some(rest) = new_abs.strip_prefix(&prefix) {
            return format!("{}{}", "super::".repeat(super_count), rest);
        }
        return new_abs.to_string();
    }
    new_abs.to_string()
}

/// Rewrite a `use` path after a module move from `old_mod` to `new_mod`, as seen
/// from the file whose module path is `from_mod`. Returns `None` when the path
/// does not reference anything under `old_mod` (or is an external crate path).
///
/// `use crate::old::path::Item` with old=`crate::old::path`, new=`crate::new::path`
/// becomes `crate::new::path::Item`. `super::`/`self::` are resolved to absolute,
/// substituted, then re-expressed in the original prefix style.
pub fn rewrite_use_path(
    use_path: &str,
    old_mod: &str,
    new_mod: &str,
    from_mod: &str,
) -> Option<String> {
    let abs_path = resolve_to_absolute(use_path, from_mod)?;
    if abs_path == old_mod {
        // Importing the module itself (e.g. `use crate::utils` and utils.rs moved).
        return Some(reconvert_prefix(new_mod, use_path, from_mod));
    }
    let old_prefix = format!("{old_mod}::");
    if let Some(suffix) = abs_path.strip_prefix(&old_prefix) {
        let new_abs = format!("{new_mod}::{suffix}");
        return Some(reconvert_prefix(&new_abs, use_path, from_mod));
    }
    None
}

/// Rewrite a `use` specifier in `from_file` for a file move `old_target ->
/// new_target` (all file paths). Composes `file_to_mod_path` on the three files
/// with `rewrite_use_path`. Returns `None` if any file is not a `src/` Rust file
/// or the specifier does not reference the moved module.
pub fn rewrite_import(
    from_file: &str,
    old_target: &str,
    new_target: &str,
    use_path: &str,
) -> Option<String> {
    let old_mod = file_to_mod_path(old_target)?;
    let new_mod = file_to_mod_path(new_target)?;
    let from_mod = file_to_mod_path(from_file)?;
    rewrite_use_path(use_path, &old_mod, &new_mod, &from_mod)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(from: &str, old: &str, new: &str, import: &str) -> Option<String> {
        rewrite_import(from, old, new, import)
    }

    #[test]
    fn file_to_mod_path_conventions() {
        assert_eq!(file_to_mod_path("/repo/src/lib.rs").as_deref(), Some("crate"));
        assert_eq!(file_to_mod_path("/repo/src/main.rs").as_deref(), Some("crate"));
        assert_eq!(file_to_mod_path("/repo/src/utils.rs").as_deref(), Some("crate::utils"));
        assert_eq!(file_to_mod_path("/repo/src/foo/bar.rs").as_deref(), Some("crate::foo::bar"));
        assert_eq!(file_to_mod_path("/repo/src/foo/mod.rs").as_deref(), Some("crate::foo"));
        assert_eq!(file_to_mod_path("/repo/src/a/b/mod.rs").as_deref(), Some("crate::a::b"));
        // Workspace member resolves within its own crate.
        assert_eq!(file_to_mod_path("/repo/crates/foo/src/bar.rs").as_deref(), Some("crate::bar"));
        assert_eq!(file_to_mod_path("/repo/lib/utils.rs"), None);
    }

    #[test]
    fn rewrite_crate_prefixed_use() {
        assert_eq!(
            rewrite("/repo/src/app.rs", "/repo/src/utils.rs", "/repo/src/helpers/utils.rs",
                "crate::utils::Foo").as_deref(),
            Some("crate::helpers::utils::Foo"));
    }

    #[test]
    fn rewrite_module_itself() {
        assert_eq!(
            rewrite("/repo/src/app.rs", "/repo/src/utils.rs", "/repo/src/helpers/utils.rs",
                "crate::utils").as_deref(),
            Some("crate::helpers::utils"));
    }

    #[test]
    fn rewrite_super_falls_back_to_crate_when_unexpressible() {
        // from crate::foo::consumer; old crate::foo::bar -> new crate::baz::bar.
        // super::bar::Thing can't stay super:: across parents -> crate::baz::bar::Thing.
        assert_eq!(
            rewrite("/repo/src/foo/consumer.rs", "/repo/src/foo/bar.rs", "/repo/src/baz/bar.rs",
                "super::bar::Thing").as_deref(),
            Some("crate::baz::bar::Thing"));
    }

    #[test]
    fn rewrite_super_stays_super_when_possible() {
        // Move stays under the same parent (crate::foo) -> super:: is preserved.
        assert_eq!(
            rewrite("/repo/src/foo/consumer.rs", "/repo/src/foo/bar.rs", "/repo/src/foo/qux.rs",
                "super::bar::Thing").as_deref(),
            Some("super::qux::Thing"));
    }

    #[test]
    fn rewrite_self_prefixed_use() {
        assert_eq!(
            rewrite("/repo/src/foo/mod.rs", "/repo/src/foo/bar.rs", "/repo/src/foo/baz.rs",
                "self::bar::X").as_deref(),
            Some("self::baz::X"));
    }

    #[test]
    fn external_and_unrelated_return_none() {
        assert_eq!(
            rewrite("/repo/src/app.rs", "/repo/src/utils.rs", "/repo/src/helpers/utils.rs",
                "std::collections::HashMap"),
            None);
        assert_eq!(
            rewrite("/repo/src/app.rs", "/repo/src/utils.rs", "/repo/src/helpers/utils.rs",
                "crate::config::Settings"),
            None);
    }

    #[test]
    fn glob_import_rewrite() {
        assert_eq!(
            rewrite("/repo/src/app.rs", "/repo/src/utils.rs", "/repo/src/helpers/utils.rs",
                "crate::utils::*").as_deref(),
            Some("crate::helpers::utils::*"));
    }
}
