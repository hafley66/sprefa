use sprefa_extract::{kind, ExtractContext, Extractor, RawRef};

use crate::change::DeclChange;

/// Kinds that represent declarations or references worth tracking for rename.
///
/// Declarations (export_name, rs_declare, rs_mod) propagate downstream to consumers.
/// References (import_name) propagate upstream to the declaring file, then back
/// down to all other consumers. This enables "rename at any point in the chain"
/// behavior.
const DECL_KINDS: &[&str] = &[
    kind::EXPORT_NAME,
    kind::RS_DECLARE,
    kind::RS_MOD,
    kind::IMPORT_NAME,
];

/// Maximum byte distance between old and new spans to consider them
/// the "same" declaration (for rename detection).
const SPAN_PROXIMITY_THRESHOLD: u32 = 64;

/// Diff old refs (from the index) against new refs (from re-extraction)
/// for a single file. Returns declaration-level changes.
///
/// The algorithm:
/// 1. Filter both sets to DECL_KINDS only.
/// 2. For each old decl, find a new decl with same kind + nearby span.
///    - If found and value differs → Rename
///    - If found and value matches → no change (skip)
///    - If not found → Removed
/// 3. Unmatched new decls → Added
pub fn diff_refs(
    file_id: i64,
    old_refs: &[RawRef],
    new_refs: &[RawRef],
) -> Vec<DeclChange> {
    let old_decls: Vec<&RawRef> = old_refs
        .iter()
        .filter(|r| DECL_KINDS.contains(&r.kind.as_str()))
        .collect();

    let new_decls: Vec<&RawRef> = new_refs
        .iter()
        .filter(|r| DECL_KINDS.contains(&r.kind.as_str()))
        .collect();

    let mut matched_new: Vec<bool> = vec![false; new_decls.len()];
    let mut changes = Vec::new();

    for old in &old_decls {
        let best = new_decls
            .iter()
            .enumerate()
            .filter(|(i, _)| !matched_new[*i])
            .filter(|(_, n)| n.kind == old.kind)
            .filter(|(_, n)| span_distance(old.span_start, n.span_start) <= SPAN_PROXIMITY_THRESHOLD)
            .min_by_key(|(_, n)| span_distance(old.span_start, n.span_start));

        match best {
            Some((idx, new)) if new.value != old.value => {
                matched_new[idx] = true;
                changes.push(DeclChange::Rename {
                    file_id,
                    kind: old.kind.clone(),
                    old_name: old.value.clone(),
                    new_name: new.value.clone(),
                    new_span_start: new.span_start,
                    new_span_end: new.span_end,
                });
            }
            Some((idx, _)) => {
                // Same value, no change.
                matched_new[idx] = true;
            }
            None => {
                changes.push(DeclChange::Removed {
                    file_id,
                    kind: old.kind.clone(),
                    name: old.value.clone(),
                });
            }
        }
    }

    for (i, new) in new_decls.iter().enumerate() {
        if !matched_new[i] {
            changes.push(DeclChange::Added {
                file_id,
                kind: new.kind.clone(),
                name: new.value.clone(),
            });
        }
    }

    changes
}

fn span_distance(a: u32, b: u32) -> u32 {
    if a > b { a - b } else { b - a }
}

/// Re-extract a file and diff against old refs from the index.
/// Returns the list of declaration-level changes.
///
/// `old_refs` should be the refs for this file as they exist in the DB.
/// `source` is the new file content from disk.
/// `path` is the relative path (for extractor dispatch).
pub fn detect_decl_changes(
    file_id: i64,
    old_refs: &[RawRef],
    source: &[u8],
    path: &str,
    extractors: &[Box<dyn Extractor>],
    ctx: &ExtractContext,
) -> Vec<DeclChange> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let extractor = extractors
        .iter()
        .find(|ex| ex.extensions().contains(&ext));

    let new_refs = match extractor {
        Some(ex) => ex.extract(source, path, ctx),
        None => return vec![],
    };

    diff_refs(file_id, old_refs, &new_refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ref(value: &str, kind: &str, span_start: u32) -> RawRef {
        RawRef {
            value: value.to_string(),
            span_start,
            span_end: span_start + value.len() as u32,
            kind: kind.to_string(),
            rule_name: "test".to_string(),
            is_path: false,
            parent_key: None,
            node_path: None,
            scan: None,
            group: None,
        }
    }

    #[test]
    fn detects_rename() {
        let old = vec![make_ref("Foo", kind::EXPORT_NAME, 10)];
        let new = vec![make_ref("Bar", kind::EXPORT_NAME, 10)];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            DeclChange::Rename { old_name, new_name, .. } => {
                assert_eq!(old_name, "Foo");
                assert_eq!(new_name, "Bar");
            }
            _ => panic!("expected Rename"),
        }
    }

    #[test]
    fn detects_added() {
        let old = vec![];
        let new = vec![make_ref("Baz", kind::EXPORT_NAME, 20)];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            DeclChange::Added { name, .. } => assert_eq!(name, "Baz"),
            _ => panic!("expected Added"),
        }
    }

    #[test]
    fn detects_removed() {
        let old = vec![make_ref("Gone", kind::RS_DECLARE, 5)];
        let new = vec![];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 1);
        match &changes[0] {
            DeclChange::Removed { name, .. } => assert_eq!(name, "Gone"),
            _ => panic!("expected Removed"),
        }
    }

    #[test]
    fn unchanged_produces_no_changes() {
        let old = vec![make_ref("Same", kind::EXPORT_NAME, 10)];
        let new = vec![make_ref("Same", kind::EXPORT_NAME, 10)];
        let changes = diff_refs(1, &old, &new);
        assert!(changes.is_empty());
    }

    #[test]
    fn ignores_non_decl_kinds() {
        let old = vec![make_ref("./utils", kind::IMPORT_PATH, 20)];
        let new = vec![make_ref("./lib/utils", kind::IMPORT_PATH, 20)];
        let changes = diff_refs(1, &old, &new);
        // ImportPath is not a DECL_KIND, so no changes detected
        assert!(changes.is_empty());
    }

    #[test]
    fn span_too_far_is_remove_plus_add() {
        let old = vec![make_ref("Foo", kind::EXPORT_NAME, 10)];
        let new = vec![make_ref("Bar", kind::EXPORT_NAME, 500)];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|c| matches!(c, DeclChange::Removed { .. })));
        assert!(changes.iter().any(|c| matches!(c, DeclChange::Added { .. })));
    }

    // ── edge cases ────────────────────────────────────────────────────────

    #[test]
    fn span_at_exact_threshold_matches() {
        // Exactly SPAN_PROXIMITY_THRESHOLD apart should still match
        let old = vec![make_ref("Foo", kind::EXPORT_NAME, 10)];
        let new = vec![make_ref("Bar", kind::EXPORT_NAME, 10 + SPAN_PROXIMITY_THRESHOLD)];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], DeclChange::Rename { old_name, new_name, .. }
            if old_name == "Foo" && new_name == "Bar"));
    }

    #[test]
    fn span_one_past_threshold_splits() {
        let old = vec![make_ref("Foo", kind::EXPORT_NAME, 10)];
        let new = vec![make_ref("Bar", kind::EXPORT_NAME, 11 + SPAN_PROXIMITY_THRESHOLD)];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 2);
    }

    #[test]
    fn same_name_different_kind_no_match() {
        // ExportName "Foo" and RsDeclare "Foo" should not match each other
        let old = vec![make_ref("Foo", kind::EXPORT_NAME, 10)];
        let new = vec![make_ref("Foo", kind::RS_DECLARE, 10)];
        let changes = diff_refs(1, &old, &new);
        // Old ExportName removed, new RsDeclare added
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().any(|c| matches!(c, DeclChange::Removed { kind, .. } if *kind == kind::EXPORT_NAME)));
        assert!(changes.iter().any(|c| matches!(c, DeclChange::Added { kind, .. } if *kind == kind::RS_DECLARE)));
    }

    #[test]
    fn multiple_decls_same_kind_matched_by_proximity() {
        // Two exports at different positions, both renamed
        let old = vec![
            make_ref("Alpha", kind::EXPORT_NAME, 10),
            make_ref("Beta", kind::EXPORT_NAME, 100),
        ];
        let new = vec![
            make_ref("AlphaV2", kind::EXPORT_NAME, 12),
            make_ref("BetaV2", kind::EXPORT_NAME, 102),
        ];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 2);
        let renames: Vec<_> = changes.iter().filter_map(|c| match c {
            DeclChange::Rename { old_name, new_name, .. } => Some((old_name.as_str(), new_name.as_str())),
            _ => None,
        }).collect();
        assert!(renames.contains(&("Alpha", "AlphaV2")));
        assert!(renames.contains(&("Beta", "BetaV2")));
    }

    #[test]
    fn swap_rename_detects_closest_match() {
        // A and B swap positions -- proximity matching picks closest
        let old = vec![
            make_ref("A", kind::EXPORT_NAME, 10),
            make_ref("B", kind::EXPORT_NAME, 50),
        ];
        let new = vec![
            make_ref("B", kind::EXPORT_NAME, 12),
            make_ref("A", kind::EXPORT_NAME, 52),
        ];
        let changes = diff_refs(1, &old, &new);
        // Old "A" at 10 matches new "B" at 12 (distance 2), old "B" at 50 matches new "A" at 52 (distance 2)
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| matches!(c, DeclChange::Rename { .. })));
    }

    #[test]
    fn rs_mod_is_tracked_as_decl() {
        // RsMod is in DECL_KINDS, so mod renames should be detected
        let old = vec![make_ref("old_mod", kind::RS_MOD, 4)];
        let new = vec![make_ref("new_mod", kind::RS_MOD, 4)];
        let changes = diff_refs(1, &old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], DeclChange::Rename { old_name, new_name, .. }
            if old_name == "old_mod" && new_name == "new_mod"));
    }

    #[test]
    fn empty_both_sides() {
        let changes = diff_refs(1, &[], &[]);
        assert!(changes.is_empty());
    }

    #[test]
    fn many_additions_no_old() {
        let new: Vec<_> = (0..10)
            .map(|i| make_ref(&format!("Item{}", i), kind::EXPORT_NAME, i * 20))
            .collect();
        let changes = diff_refs(1, &[], &new);
        assert_eq!(changes.len(), 10);
        assert!(changes.iter().all(|c| matches!(c, DeclChange::Added { .. })));
    }

    #[test]
    fn many_removals_no_new() {
        let old: Vec<_> = (0..10)
            .map(|i| make_ref(&format!("Item{}", i), kind::RS_DECLARE, i * 20))
            .collect();
        let changes = diff_refs(1, &old, &[]);
        assert_eq!(changes.len(), 10);
        assert!(changes.iter().all(|c| matches!(c, DeclChange::Removed { .. })));
    }

    #[test]
    fn mixed_ref_kinds_filtered_correctly() {
        // ImportPath refs are ignored; ImportName, ExportName, RsDeclare are tracked
        let old = vec![
            make_ref("./utils", kind::IMPORT_PATH, 5),
            make_ref("foo", kind::IMPORT_NAME, 15),
            make_ref("MyExport", kind::EXPORT_NAME, 30),
            make_ref("my_fn", kind::RS_DECLARE, 50),
        ];
        let new = vec![
            make_ref("./lib/utils", kind::IMPORT_PATH, 5),
            make_ref("bar", kind::IMPORT_NAME, 15),
            make_ref("MyExportV2", kind::EXPORT_NAME, 30),
            make_ref("my_fn_v2", kind::RS_DECLARE, 50),
        ];
        let changes = diff_refs(1, &old, &new);
        // ImportName, ExportName, and RsDeclare changes detected (ImportPath ignored)
        assert_eq!(changes.len(), 3);
        let renames: Vec<_> = changes.iter().filter_map(|c| match c {
            DeclChange::Rename { old_name, new_name, .. } => Some((old_name.as_str(), new_name.as_str())),
            _ => None,
        }).collect();
        assert!(renames.contains(&("foo", "bar")));
        assert!(renames.contains(&("MyExport", "MyExportV2")));
        assert!(renames.contains(&("my_fn", "my_fn_v2")));
    }
}
