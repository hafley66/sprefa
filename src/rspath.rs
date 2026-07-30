//! Rust `use`-path rewrite arithmetic for the auto-refactor sink (Route A).
//! Pure string/path math, no engine or DB access: given a file move
//! `old_target -> new_target`, rewrite a `use` specifier so it still points at
//! the moved module, preserving the `crate::`/`self::`/`super::` prefix style
//! the original used. Ported from the OG coordinate model
//! (`sprefa-archive-20260428/crates/watch/src/rs_path.rs`). The engine feeds
//! these the leaf text located in `_where_bytes` (see `ref`); the new text is
//! interned and spliced back at the same byte span.

use std::path::Path;

/// Module path from the path components AFTER a crate source root.
///
///   []  / ["lib.rs"] / ["main.rs"] -> "crate"
///   ["foo.rs"]                      -> "crate::foo"
///   ["foo", "mod.rs"]               -> "crate::foo"
///   ["foo", "bar.rs"]               -> "crate::foo::bar"
///   ["foo", "bar", "mod.rs"]        -> "crate::foo::bar"
///
/// Shared by the `src/`-keyed `file_to_mod_path` and the root-anchored
/// `file_to_mod_path_rooted`. Returns `None` for an empty tail.
fn mod_path_from_tail(after_root: &[&str]) -> Option<String> {
    let last = *after_root.last()?;
    let stem = Path::new(last)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(last);
    // lib.rs / main.rs at the crate root is the crate itself.
    if after_root.len() == 1 && (stem == "lib" || stem == "main") {
        return Some("crate".to_string());
    }
    let mut segments = Vec::with_capacity(after_root.len() + 1);
    segments.push("crate");
    // directories always contribute; the file stem does too unless it is `mod`.
    for dir in &after_root[..after_root.len() - 1] {
        segments.push(dir);
    }
    if stem != "mod" {
        segments.push(stem);
    }
    Some(segments.join("::"))
}

fn path_components(p: &str) -> Vec<&str> {
    Path::new(p)
        .components()
        .map(|c| c.as_os_str().to_str().unwrap_or(""))
        .collect()
}

/// Convert a file path to its Rust module path, keying off the last `src/`
/// component, so workspace members (`crates/foo/src/bar.rs`) resolve within
/// their own crate (`crate::bar`).
///
///   src/lib.rs -> "crate"; src/foo.rs -> "crate::foo"; src/foo/mod.rs -> "crate::foo".
///
/// Returns `None` for a path with no `src/` or nothing after it. Prefer
/// `file_to_mod_path_rooted` when crate roots are known (handles non-`src/`
/// layouts like the kernel's `rust/kernel/lib.rs`); this is the fallback.
pub fn file_to_mod_path(file_path: &str) -> Option<String> {
    let components = path_components(file_path);
    let src_idx = components.iter().rposition(|c| *c == "src")?;
    mod_path_from_tail(&components[src_idx + 1..])
}

/// Crate source roots discovered from a file set: every directory that directly
/// holds a `lib.rs` or `main.rs`. Longest path first so a nested crate root wins
/// over an enclosing one. These anchor `file_to_mod_path_rooted`, generalizing
/// the bare `src/` convention to any crate layout (`rust/kernel/lib.rs` -> root
/// `rust/kernel`).
pub fn crate_roots(files: &[String]) -> Vec<String> {
    let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
    for f in files {
        let p = Path::new(f);
        let stem = p.file_stem().and_then(|s| s.to_str());
        if matches!(stem, Some("lib") | Some("main")) {
            let dir = p.parent().and_then(|d| d.to_str()).unwrap_or("");
            set.insert(dir.to_string());
        }
    }
    let mut roots: Vec<String> = set.into_iter().collect();
    roots.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    roots
}

/// Like `file_to_mod_path` but anchored on discovered crate `roots` (longest
/// first). The module path is computed from the components after the longest
/// root that contains the file; falls back to the `src/` heuristic when no root
/// matches (so `--move` still works on a checkout with no discovered root).
pub fn file_to_mod_path_rooted(file_path: &str, roots: &[String]) -> Option<String> {
    let components = path_components(file_path);
    for root in roots {
        let rc = path_components(root);
        if components.len() > rc.len() && components[..rc.len()] == rc[..] {
            return mod_path_from_tail(&components[rc.len()..]);
        }
    }
    file_to_mod_path(file_path)
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

/// Rewrite the leaf text of a brace import — `use <prefix>::{ <leaf>, .. }` —
/// when the moved-module boundary sits INSIDE the leaf, where the shared head
/// span can't reach (`use crate::{old::A, b}`). Returns the new leaf text;
/// `None` when the leaf is unaffected, or when the rewritten path no longer
/// sits under the unchanged brace head (the move sink then regenerates the
/// whole statement body via `regroup_use_items`). An empty `prefix` (bare use)
/// returns the whole rewritten path.
pub fn rewrite_brace_leaf(
    full: &str,
    prefix: &str,
    old_mod: &str,
    new_mod: &str,
    from_mod: &str,
) -> Option<String> {
    let new_full = rewrite_use_path(full, old_mod, new_mod, from_mod)?;
    if prefix.is_empty() {
        return Some(new_full);
    }
    new_full
        .strip_prefix(&format!("{prefix}::"))
        .map(str::to_string)
}

/// Regenerate a whole `use` statement body after a move forces at least one
/// brace leaf OUT of its shared head (`use crate::a::{b::X, c}` with `a::b`
/// moved to `z::b` — the new path `crate::z::b::X` no longer sits under
/// `crate::a`). `items` are the statement's leaves in original order as
/// `(full_path, alias)`, each path already rewritten. Factors the longest
/// common `::`-segment prefix back out; a single leaf collapses to a bare
/// path; a leaf equal to the common prefix renders as `self`.
pub fn regroup_use_items(items: &[(String, Option<String>)]) -> String {
    let with_alias = |path: &str, alias: &Option<String>| match alias {
        Some(alias_name) => format!("{path} as {alias_name}"),
        None => path.to_string(),
    };
    let [(only_path, only_alias)] = items else {
        let segmented: Vec<Vec<&str>> = items
            .iter()
            .map(|(path, _)| path.split("::").collect())
            .collect();
        let mut common = 0usize;
        while let Some(first_seg) = segmented[0].get(common) {
            if !segmented
                .iter()
                .all(|segs| segs.get(common) == Some(first_seg))
            {
                break;
            }
            common += 1;
        }
        let grouped: Vec<String> = segmented
            .iter()
            .zip(items)
            .map(|(segs, (_, alias))| {
                let rest = segs[common..].join("::");
                with_alias(if rest.is_empty() { "self" } else { &rest }, alias)
            })
            .collect();
        let group = grouped.join(", ");
        return if common == 0 {
            format!("{{{group}}}")
        } else {
            format!("{}::{{{group}}}", segmented[0][..common].join("::"))
        };
    };
    with_alias(only_path, only_alias)
}

/// Re-anchor a `use` path written inside the MOVED file itself: the file's own
/// module path changes `old_mod -> new_mod`, so `super::` anchors shift and
/// self-references to the old module follow the move. `self::` paths track the
/// module wherever it goes (no edit). Returns `None` for external paths or
/// when nothing changes.
pub fn rewrite_moved_file_use(use_path: &str, old_mod: &str, new_mod: &str) -> Option<String> {
    let abs = resolve_to_absolute(use_path, old_mod)?;
    let abs2 = if abs == old_mod {
        new_mod.to_string()
    } else if let Some(rest) = abs.strip_prefix(&format!("{old_mod}::")) {
        format!("{new_mod}::{rest}")
    } else {
        abs
    };
    let out = reconvert_prefix(&abs2, use_path, new_mod);
    (out != use_path).then_some(out)
}

/// Module files that could declare `mod <stem>;` for a file at `path`, in
/// preference order: the directory's sibling file (`a/b.rs` for `a/b/c.rs`),
/// its `mod.rs`, and — when the directory is a crate root — `lib.rs`/`main.rs`.
pub fn mod_parent_candidates(path: &str) -> Vec<String> {
    let p = Path::new(path);
    let Some(dir) = p.parent().and_then(|d| d.to_str()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if !dir.is_empty() {
        out.push(format!("{dir}.rs"));
        out.push(format!("{dir}/mod.rs"));
        out.push(format!("{dir}/lib.rs"));
        out.push(format!("{dir}/main.rs"));
    } else {
        out.push("mod.rs".to_string());
        out.push("lib.rs".to_string());
        out.push("main.rs".to_string());
    }
    out
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
    rewrite_import_rooted(from_file, old_target, new_target, use_path, &[])
}

/// `rewrite_import` with explicit crate `roots`, so non-`src/` layouts (the
/// kernel's `rust/kernel/*.rs`) derive a module path and rewrite. Each of the
/// three files resolves via `file_to_mod_path_rooted`, which falls back to the
/// `src/` heuristic for files outside every discovered root.
pub fn rewrite_import_rooted(
    from_file: &str,
    old_target: &str,
    new_target: &str,
    use_path: &str,
    roots: &[String],
) -> Option<String> {
    let old_mod = file_to_mod_path_rooted(old_target, roots)?;
    let new_mod = file_to_mod_path_rooted(new_target, roots)?;
    let from_mod = file_to_mod_path_rooted(from_file, roots)?;
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
        assert_eq!(
            file_to_mod_path("/repo/src/lib.rs").as_deref(),
            Some("crate")
        );
        assert_eq!(
            file_to_mod_path("/repo/src/main.rs").as_deref(),
            Some("crate")
        );
        assert_eq!(
            file_to_mod_path("/repo/src/utils.rs").as_deref(),
            Some("crate::utils")
        );
        assert_eq!(
            file_to_mod_path("/repo/src/foo/bar.rs").as_deref(),
            Some("crate::foo::bar")
        );
        assert_eq!(
            file_to_mod_path("/repo/src/foo/mod.rs").as_deref(),
            Some("crate::foo")
        );
        assert_eq!(
            file_to_mod_path("/repo/src/a/b/mod.rs").as_deref(),
            Some("crate::a::b")
        );
        // Workspace member resolves within its own crate.
        assert_eq!(
            file_to_mod_path("/repo/crates/foo/src/bar.rs").as_deref(),
            Some("crate::bar")
        );
        assert_eq!(file_to_mod_path("/repo/lib/utils.rs"), None);
    }

    #[test]
    fn rewrite_crate_prefixed_use() {
        assert_eq!(
            rewrite(
                "/repo/src/app.rs",
                "/repo/src/utils.rs",
                "/repo/src/helpers/utils.rs",
                "crate::utils::Foo"
            )
            .as_deref(),
            Some("crate::helpers::utils::Foo")
        );
    }

    #[test]
    fn rewrite_module_itself() {
        assert_eq!(
            rewrite(
                "/repo/src/app.rs",
                "/repo/src/utils.rs",
                "/repo/src/helpers/utils.rs",
                "crate::utils"
            )
            .as_deref(),
            Some("crate::helpers::utils")
        );
    }

    #[test]
    fn rewrite_super_falls_back_to_crate_when_unexpressible() {
        // from crate::foo::consumer; old crate::foo::bar -> new crate::baz::bar.
        // super::bar::Thing can't stay super:: across parents -> crate::baz::bar::Thing.
        assert_eq!(
            rewrite(
                "/repo/src/foo/consumer.rs",
                "/repo/src/foo/bar.rs",
                "/repo/src/baz/bar.rs",
                "super::bar::Thing"
            )
            .as_deref(),
            Some("crate::baz::bar::Thing")
        );
    }

    #[test]
    fn rewrite_super_stays_super_when_possible() {
        // Move stays under the same parent (crate::foo) -> super:: is preserved.
        assert_eq!(
            rewrite(
                "/repo/src/foo/consumer.rs",
                "/repo/src/foo/bar.rs",
                "/repo/src/foo/qux.rs",
                "super::bar::Thing"
            )
            .as_deref(),
            Some("super::qux::Thing")
        );
    }

    #[test]
    fn rewrite_self_prefixed_use() {
        assert_eq!(
            rewrite(
                "/repo/src/foo/mod.rs",
                "/repo/src/foo/bar.rs",
                "/repo/src/foo/baz.rs",
                "self::bar::X"
            )
            .as_deref(),
            Some("self::baz::X")
        );
    }

    #[test]
    fn external_and_unrelated_return_none() {
        assert_eq!(
            rewrite(
                "/repo/src/app.rs",
                "/repo/src/utils.rs",
                "/repo/src/helpers/utils.rs",
                "std::collections::HashMap"
            ),
            None
        );
        assert_eq!(
            rewrite(
                "/repo/src/app.rs",
                "/repo/src/utils.rs",
                "/repo/src/helpers/utils.rs",
                "crate::config::Settings"
            ),
            None
        );
    }

    #[test]
    fn crate_roots_discovers_non_src_layouts() {
        let files = vec![
            "rust/kernel/lib.rs".to_string(),
            "rust/kernel/clk.rs".to_string(),
            "rust/macros/lib.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let roots = crate_roots(&files);
        // longest first; dirs holding lib.rs/main.rs.
        assert_eq!(roots, vec!["rust/kernel", "rust/macros", "src"]);
    }

    #[test]
    fn rooted_mod_path_for_non_src_crate() {
        let roots = vec!["rust/kernel".to_string()];
        assert_eq!(
            file_to_mod_path_rooted("rust/kernel/lib.rs", &roots).as_deref(),
            Some("crate")
        );
        assert_eq!(
            file_to_mod_path_rooted("rust/kernel/clk.rs", &roots).as_deref(),
            Some("crate::clk")
        );
        assert_eq!(
            file_to_mod_path_rooted("rust/kernel/net/phy.rs", &roots).as_deref(),
            Some("crate::net::phy")
        );
        assert_eq!(
            file_to_mod_path_rooted("rust/kernel/net.rs", &roots).as_deref(),
            Some("crate::net")
        );
    }

    #[test]
    fn rooted_falls_back_to_src_heuristic_off_root() {
        let roots = vec!["rust/kernel".to_string()];
        // a file under no discovered root still resolves via the `src/` rule.
        assert_eq!(
            file_to_mod_path_rooted("other/src/foo.rs", &roots).as_deref(),
            Some("crate::foo")
        );
    }

    #[test]
    fn rewrite_non_src_kernel_move() {
        // move rust/kernel/clk.rs -> rust/kernel/hw/clk.rs; a sibling's
        // `use crate::clk::Hertz` must follow to `crate::hw::clk::Hertz`.
        let roots = vec!["rust/kernel".to_string()];
        assert_eq!(
            rewrite_import_rooted(
                "rust/kernel/device.rs",
                "rust/kernel/clk.rs",
                "rust/kernel/hw/clk.rs",
                "crate::clk::Hertz",
                &roots
            )
            .as_deref(),
            Some("crate::hw::clk::Hertz")
        );
        // without roots, the same move can't derive a module path -> no rewrite.
        assert_eq!(
            rewrite_import(
                "rust/kernel/device.rs",
                "rust/kernel/clk.rs",
                "rust/kernel/hw/clk.rs",
                "crate::clk::Hertz"
            ),
            None
        );
    }

    #[test]
    fn brace_leaf_rewrites_inside_head() {
        // use crate::{utils::Foo, b}: leaf "utils::Foo" under head "crate"
        assert_eq!(
            rewrite_brace_leaf(
                "crate::utils::Foo",
                "crate",
                "crate::utils",
                "crate::helpers::utils",
                "crate::app"
            )
            .as_deref(),
            Some("helpers::utils::Foo")
        );
        // the leaf IS the moved module
        assert_eq!(
            rewrite_brace_leaf(
                "crate::utils",
                "crate",
                "crate::utils",
                "crate::helpers::utils",
                "crate::app"
            )
            .as_deref(),
            Some("helpers::utils")
        );
        // unaffected leaf
        assert_eq!(
            rewrite_brace_leaf(
                "crate::misc",
                "crate",
                "crate::utils",
                "crate::helpers::utils",
                "crate::app"
            ),
            None
        );
        // rewrite leaves the brace head -> unexpressible in place
        assert_eq!(
            rewrite_brace_leaf(
                "crate::a::utils::Foo",
                "crate::a",
                "crate::a::utils",
                "crate::b::utils",
                "crate::app"
            ),
            None
        );
    }

    #[test]
    fn regroup_factors_common_prefix() {
        let path = |text: &str| (text.to_string(), None);
        // one leaf collapses to a bare path
        assert_eq!(
            regroup_use_items(&[path("crate::z::b::X")]),
            "crate::z::b::X"
        );
        // an exiting leaf plus a staying sibling refactor onto the shared `crate`
        assert_eq!(
            regroup_use_items(&[path("crate::z::b::X"), path("crate::a::c")]),
            "crate::{z::b::X, a::c}"
        );
        // deeper common prefix stays factored
        assert_eq!(
            regroup_use_items(&[path("crate::a::b::X"), path("crate::a::b::Y")]),
            "crate::a::b::{X, Y}"
        );
        // no common prefix -> top-level brace group
        assert_eq!(
            regroup_use_items(&[path("std::io::Read"), path("crate::a::c")]),
            "{std::io::Read, crate::a::c}"
        );
        // a leaf equal to the common prefix renders as `self`
        assert_eq!(
            regroup_use_items(&[path("crate::a"), path("crate::a::c")]),
            "crate::a::{self, c}"
        );
        // aliases ride along
        assert_eq!(
            regroup_use_items(&[
                ("crate::z::b::X".to_string(), Some("Ex".to_string())),
                path("crate::a::c"),
            ]),
            "crate::{z::b::X as Ex, a::c}"
        );
    }

    #[test]
    fn moved_file_use_reanchors() {
        // src/utils.rs -> src/helpers/utils.rs; its `use super::config::C`
        // anchored at the old parent must fall back to crate::
        assert_eq!(
            rewrite_moved_file_use("super::config::C", "crate::utils", "crate::helpers::utils")
                .as_deref(),
            Some("crate::config::C")
        );
        // self:: tracks the module wherever it goes — no edit
        assert_eq!(
            rewrite_moved_file_use("self::sub::X", "crate::utils", "crate::helpers::utils"),
            None
        );
        // a self-reference through crate:: follows the move
        assert_eq!(
            rewrite_moved_file_use(
                "crate::utils::helper",
                "crate::utils",
                "crate::helpers::utils"
            )
            .as_deref(),
            Some("crate::helpers::utils::helper")
        );
        // external paths are untouched
        assert_eq!(
            rewrite_moved_file_use("std::io::Read", "crate::utils", "crate::helpers::utils"),
            None
        );
    }

    #[test]
    fn mod_parent_candidate_order() {
        assert_eq!(
            mod_parent_candidates("src/foo/bar.rs"),
            vec![
                "src/foo.rs",
                "src/foo/mod.rs",
                "src/foo/lib.rs",
                "src/foo/main.rs"
            ]
        );
        assert_eq!(
            mod_parent_candidates("bar.rs"),
            vec!["mod.rs", "lib.rs", "main.rs"]
        );
    }

    #[test]
    fn glob_import_rewrite() {
        assert_eq!(
            rewrite(
                "/repo/src/app.rs",
                "/repo/src/utils.rs",
                "/repo/src/helpers/utils.rs",
                "crate::utils::*"
            )
            .as_deref(),
            Some("crate::helpers::utils::*")
        );
    }
}
