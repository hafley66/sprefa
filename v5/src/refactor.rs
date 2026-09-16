//! The auto-refactor edit sink (Route A). Turns located `use`-path spans into
//! byte-span edits and applies them: group by file, splice DESC by `lo` so
//! earlier offsets stay valid as later ones are replaced, and reject overlapping
//! spans. The rewrite text comes from `rspath::rewrite_import`; the coordinate
//! `(path, lo, hi)` comes from the ref-spine (`Engine::located_spans`).

use std::collections::BTreeMap;

use anyhow::{bail, Result};

/// One byte-span replacement in a file. `lo..hi` are byte offsets into the
/// file's WORK content; `new_text` replaces that range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub path: String,
    pub lo: u32,
    pub hi: u32,
    pub old_text: String,
    pub new_text: String,
}

/// Apply all edits targeting one file's `content`, returning the rewritten text.
/// Splices DESC by `lo` (higher offsets first, so lower ones stay valid) and
/// errors on any overlap. Spans are assumed to be char boundaries (use paths are
/// ASCII identifiers in source).
pub fn splice_file(content: &str, edits: &[Edit]) -> Result<String> {
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by(|a, b| b.lo.cmp(&a.lo));
    let mut out = content.to_string();
    let mut prev_lo = u32::MAX; // lo of the last (higher-offset) edit applied
    for e in ordered {
        if e.hi > prev_lo {
            bail!(
                "overlapping edits in {} near byte {}..{}",
                e.path,
                e.lo,
                e.hi
            );
        }
        let (lo, hi) = (e.lo as usize, e.hi as usize);
        if hi > out.len() || !out.is_char_boundary(lo) || !out.is_char_boundary(hi) {
            bail!(
                "edit span {}..{} out of bounds / not a char boundary in {}",
                lo,
                hi,
                e.path
            );
        }
        out.replace_range(lo..hi, &e.new_text);
        prev_lo = e.lo;
    }
    Ok(out)
}

/// Group edits by their file path (sorted), preserving per-file order.
pub fn group_by_file(edits: Vec<Edit>) -> BTreeMap<String, Vec<Edit>> {
    let mut by_file: BTreeMap<String, Vec<Edit>> = BTreeMap::new();
    for e in edits {
        by_file.entry(e.path.clone()).or_default().push(e);
    }
    by_file
}

fn mod_decl_re(name: &str) -> regex::Regex {
    regex::Regex::new(&format!(
        r"(?m)^[ \t]*((?:pub(?:\([^)]*\))?[ \t]+)?)mod[ \t]+{}[ \t]*;[ \t]*\r?\n?",
        regex::escape(name)
    ))
    .unwrap()
}

/// Remove `mod <name>;` from a parent module file's content. Returns the
/// content without the decl line plus the decl's visibility modifier (`""`,
/// `"pub "`, `"pub(crate) "`, …) so the new parent re-declares it verbatim.
/// `None` when the file declares no such mod.
pub fn remove_mod_decl(content: &str, name: &str) -> Option<(String, String)> {
    let re = mod_decl_re(name);
    let c = re.captures(content)?;
    let vis = c[1].to_string();
    let m = c.get(0).unwrap();
    let mut out = String::with_capacity(content.len());
    out.push_str(&content[..m.start()]);
    out.push_str(&content[m.end()..]);
    Some((out, vis))
}

/// Append `mod <name>;` to a parent module file's content (no-op when already
/// declared). End-of-file placement is always syntactically valid.
pub fn add_mod_decl(content: &str, name: &str, vis: &str) -> String {
    if mod_decl_re(name).is_match(content) {
        return content.to_string();
    }
    let mut out = content.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!("{vis}mod {name};\n"));
    out
}

/// Names of the child modules a file declares (`mod x;`). A moved file's
/// children resolve relative to the file's location and do NOT follow a
/// rename — the caller warns loudly.
pub fn child_mod_decls(content: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+(\w+)[ \t]*;").unwrap()
    });
    re.captures_iter(content)
        .map(|c| c[1].to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(lo: u32, hi: u32, old: &str, new: &str) -> Edit {
        Edit {
            path: "src/a.rs".into(),
            lo,
            hi,
            old_text: old.into(),
            new_text: new.into(),
        }
    }

    #[test]
    fn splices_multiple_spans_desc_by_lo() {
        // `use crate::utils::Foo; use crate::utils::Bar;` — two independent rewrites.
        let content = "use crate::utils::Foo;\nuse crate::utils::Bar;\n";
        let foo_lo = content.find("crate::utils::Foo").unwrap() as u32;
        let bar_lo = content.find("crate::utils::Bar").unwrap() as u32;
        let edits = vec![
            ed(
                foo_lo,
                foo_lo + 17,
                "crate::utils::Foo",
                "crate::helpers::utils::Foo",
            ),
            ed(
                bar_lo,
                bar_lo + 17,
                "crate::utils::Bar",
                "crate::helpers::utils::Bar",
            ),
        ];
        let out = splice_file(content, &edits).unwrap();
        assert_eq!(
            out,
            "use crate::helpers::utils::Foo;\nuse crate::helpers::utils::Bar;\n"
        );
    }

    #[test]
    fn order_of_input_edits_does_not_matter() {
        let content = "aXbYc";
        // replace X (1..2) -> "11" and Y (3..4) -> "22"; pass in ascending order.
        let edits = vec![ed(1, 2, "X", "11"), ed(3, 4, "Y", "22")];
        assert_eq!(splice_file(content, &edits).unwrap(), "a11b22c");
    }

    #[test]
    fn rejects_overlapping_spans() {
        let content = "abcdef";
        let edits = vec![ed(1, 4, "bcd", "X"), ed(2, 5, "cde", "Y")];
        assert!(splice_file(content, &edits).is_err());
    }

    #[test]
    fn mod_decl_surgery() {
        let lib = "mod utils;\npub(crate) mod config;\nfn main() {}\n";
        let (without, vis) = remove_mod_decl(lib, "utils").unwrap();
        assert_eq!(without, "pub(crate) mod config;\nfn main() {}\n");
        assert_eq!(vis, "");
        let (without2, vis2) = remove_mod_decl(lib, "config").unwrap();
        assert_eq!(without2, "mod utils;\nfn main() {}\n");
        assert_eq!(vis2, "pub(crate) ");
        assert!(remove_mod_decl(lib, "nope").is_none());
        // `mod utils_extra;` must not match `utils`
        assert!(remove_mod_decl("mod utils_extra;\n", "utils").is_none());

        assert_eq!(
            add_mod_decl("fn x() {}", "utils", "pub "),
            "fn x() {}\npub mod utils;\n"
        );
        // already declared -> no-op
        assert_eq!(add_mod_decl(lib, "utils", ""), lib);
    }

    #[test]
    fn child_mods_listed() {
        let src = "pub mod a;\nmod b;\nfn f() { /* mod c; */ }\nmod d { }\n";
        assert_eq!(child_mod_decls(src), vec!["a", "b"]);
    }

    #[test]
    fn groups_by_file() {
        let edits = vec![
            Edit {
                path: "b.rs".into(),
                lo: 0,
                hi: 1,
                old_text: "x".into(),
                new_text: "y".into(),
            },
            Edit {
                path: "a.rs".into(),
                lo: 0,
                hi: 1,
                old_text: "x".into(),
                new_text: "y".into(),
            },
            Edit {
                path: "a.rs".into(),
                lo: 2,
                hi: 3,
                old_text: "x".into(),
                new_text: "y".into(),
            },
        ];
        let g = group_by_file(edits);
        assert_eq!(g.len(), 2);
        assert_eq!(g["a.rs"].len(), 2);
        assert_eq!(g["b.rs"].len(), 1);
    }
}
