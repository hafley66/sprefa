use std::collections::HashMap;
use std::path::Path;

use crate::plan::Edit;

/// Result of applying a set of edits.
#[derive(Debug, Default)]
pub struct ApplyResult {
    /// Files that were successfully rewritten.
    pub rewritten: Vec<String>,
    /// Edits that failed (file not found, IO error, etc).
    pub failed: Vec<(Edit, String)>,
}

/// Apply a sorted edit plan to the filesystem.
///
/// Edits MUST be sorted by (file_path asc, span_start desc) so that
/// splicing earlier spans doesn't invalidate later ones within the same file.
///
/// This is a destructive operation -- it modifies source files on disk.
/// The caller is responsible for confirming with the user if needed.
#[tracing::instrument(skip(edits), fields(edit_count = edits.len()))]
pub fn apply(edits: &[Edit]) -> ApplyResult {
    let mut result = ApplyResult::default();

    // Group edits by file to minimize reads/writes.
    let mut by_file: HashMap<&str, Vec<&Edit>> = HashMap::new();
    for edit in edits {
        by_file.entry(&edit.file_path).or_default().push(edit);
    }

    for (file_path, file_edits) in &by_file {
        match apply_to_file(file_path, file_edits) {
            Ok(()) => result.rewritten.push(file_path.to_string()),
            Err(e) => {
                for edit in file_edits {
                    result.failed.push(((*edit).clone(), e.to_string()));
                }
            }
        }
    }

    result
}

/// Apply all edits for a single file.
/// Edits are assumed to be in descending span_start order.
fn apply_to_file(file_path: &str, edits: &[&Edit]) -> anyhow::Result<()> {
    let path = Path::new(file_path);
    let mut content = std::fs::read_to_string(path)?;

    for edit in edits {
        let start = edit.span_start as usize;
        let end = edit.span_end as usize;

        if start > content.len() || end > content.len() || start > end {
            anyhow::bail!(
                "span {}..{} out of bounds for file {} (len={})",
                start, end, file_path, content.len()
            );
        }

        content.replace_range(start..end, &edit.new_value);
    }

    std::fs::write(path, &content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Edit, EditReason};
    use std::io::Write;

    #[test]
    fn apply_single_edit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            f.write_all(b"import { Foo } from './old';").unwrap();
        }

        let edits = vec![Edit {
            file_path: file.to_string_lossy().to_string(),
            span_start: 21,
            span_end: 26,
            new_value: "./new".to_string(),
            reason: EditReason::FileMove {
                old_target: "src/old.ts".to_string(),
                new_target: "src/new.ts".to_string(),
            },
        }];

        let result = apply(&edits);
        assert_eq!(result.rewritten.len(), 1);
        assert!(result.failed.is_empty());

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "import { Foo } from './new';");
    }

    #[test]
    fn apply_multiple_edits_descending() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            //                    0123456789...
            f.write_all(b"use aaa::Bbb;\nuse ccc::Ddd;").unwrap();
        }

        // Descending order: second edit first.
        let edits = vec![
            Edit {
                file_path: file.to_string_lossy().to_string(),
                span_start: 18,
                span_end: 26,
                new_value: "ccc::Eee".to_string(),
                reason: EditReason::DeclRename {
                    old_name: "ccc::Ddd".to_string(),
                    new_name: "ccc::Eee".to_string(),
                    source_file: "lib.rs".to_string(),
                },
            },
            Edit {
                file_path: file.to_string_lossy().to_string(),
                span_start: 4,
                span_end: 12,
                new_value: "aaa::Xxx".to_string(),
                reason: EditReason::DeclRename {
                    old_name: "aaa::Bbb".to_string(),
                    new_name: "aaa::Xxx".to_string(),
                    source_file: "lib.rs".to_string(),
                },
            },
        ];

        let result = apply(&edits);
        assert_eq!(result.rewritten.len(), 1);

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "use aaa::Xxx;\nuse ccc::Eee;");
    }

    #[test]
    fn span_out_of_bounds_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        std::fs::write(&file, "short").unwrap();

        let edits = vec![Edit {
            file_path: file.to_string_lossy().to_string(),
            span_start: 0,
            span_end: 999,
            new_value: "x".to_string(),
            reason: EditReason::FileMove {
                old_target: "a".to_string(),
                new_target: "b".to_string(),
            },
        }];

        let result = apply(&edits);
        assert!(result.rewritten.is_empty());
        assert_eq!(result.failed.len(), 1);
    }
}
