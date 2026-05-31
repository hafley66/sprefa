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
            bail!("overlapping edits in {} near byte {}..{}", e.path, e.lo, e.hi);
        }
        let (lo, hi) = (e.lo as usize, e.hi as usize);
        if hi > out.len() || !out.is_char_boundary(lo) || !out.is_char_boundary(hi) {
            bail!("edit span {}..{} out of bounds / not a char boundary in {}", lo, hi, e.path);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(lo: u32, hi: u32, old: &str, new: &str) -> Edit {
        Edit { path: "src/a.rs".into(), lo, hi, old_text: old.into(), new_text: new.into() }
    }

    #[test]
    fn splices_multiple_spans_desc_by_lo() {
        // `use crate::utils::Foo; use crate::utils::Bar;` — two independent rewrites.
        let content = "use crate::utils::Foo;\nuse crate::utils::Bar;\n";
        let foo_lo = content.find("crate::utils::Foo").unwrap() as u32;
        let bar_lo = content.find("crate::utils::Bar").unwrap() as u32;
        let edits = vec![
            ed(foo_lo, foo_lo + 17, "crate::utils::Foo", "crate::helpers::utils::Foo"),
            ed(bar_lo, bar_lo + 17, "crate::utils::Bar", "crate::helpers::utils::Bar"),
        ];
        let out = splice_file(content, &edits).unwrap();
        assert_eq!(out, "use crate::helpers::utils::Foo;\nuse crate::helpers::utils::Bar;\n");
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
    fn groups_by_file() {
        let edits = vec![
            Edit { path: "b.rs".into(), lo: 0, hi: 1, old_text: "x".into(), new_text: "y".into() },
            Edit { path: "a.rs".into(), lo: 0, hi: 1, old_text: "x".into(), new_text: "y".into() },
            Edit { path: "a.rs".into(), lo: 2, hi: 3, old_text: "x".into(), new_text: "y".into() },
        ];
        let g = group_by_file(edits);
        assert_eq!(g.len(), 2);
        assert_eq!(g["a.rs"].len(), 2);
        assert_eq!(g["b.rs"].len(), 1);
    }
}
