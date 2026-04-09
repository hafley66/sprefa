use std::collections::HashMap;
use std::path::Path;

use crate::plan::{Edit, RustRewrite};

/// Result of applying a set of edits.
#[derive(Debug, Default)]
pub struct ApplyResult {
    /// Files that were successfully rewritten.
    pub rewritten: Vec<String>,
    /// Edits that failed (file not found, IO error, etc).
    pub failed: Vec<(Edit, String)>,
    /// Rust files rewritten via syn.
    pub rust_rewritten: Vec<String>,
    /// Rust rewrites that failed.
    pub rust_failed: Vec<(RustRewrite, String)>,
}

/// Apply span-based edits and syn-based Rust rewrites to the filesystem.
#[tracing::instrument(skip(edits, rust_rewrites), fields(edit_count = edits.len(), rust_count = rust_rewrites.len()))]
pub fn apply(edits: &[Edit], rust_rewrites: &[RustRewrite]) -> ApplyResult {
    let mut result = ApplyResult::default();

    // Span-based edits (JS/TS).
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

    // Syn-based Rust rewrites. Multiple rewrites targeting the same file
    // are applied sequentially (each parses the result of the previous).
    let mut rust_by_file: HashMap<&str, Vec<&RustRewrite>> = HashMap::new();
    for rw in rust_rewrites {
        rust_by_file.entry(&rw.file_path).or_default().push(rw);
    }
    for (file_path, rewrites) in &rust_by_file {
        match apply_rust_rewrites(file_path, rewrites) {
            Ok(true) => result.rust_rewritten.push(file_path.to_string()),
            Ok(false) => {} // no changes needed
            Err(e) => {
                for rw in rewrites {
                    result.rust_failed.push(((*rw).clone(), e.to_string()));
                }
            }
        }
    }

    result
}

fn apply_rust_rewrites(file_path: &str, rewrites: &[&RustRewrite]) -> anyhow::Result<bool> {
    let path = Path::new(file_path);
    let mut content = std::fs::read_to_string(path)?;
    let mut total_edits = 0;

    for rw in rewrites {
        let prefixes: Vec<&str> = rw.use_prefixes.iter().map(|s| s.as_str()).collect();
        let (new_content, n) = sprefa_rs::rewrite_module_refs(
            &content, &rw.old_stem, &rw.new_stem, &prefixes, rw.rewrite_mod_decl,
        );
        content = new_content;
        total_edits += n;
    }

    if total_edits > 0 {
        std::fs::write(path, &content)?;
        Ok(true)
    } else {
        Ok(false)
    }
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

        let result = apply(&edits, &[]);
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

        let result = apply(&edits, &[]);
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

        let result = apply(&edits, &[]);
        assert!(result.rewritten.is_empty());
        assert_eq!(result.failed.len(), 1);
    }

    // ── edge cases ────────────────────────────────────────────────────────

    fn make_edit(path: &str, start: u32, end: u32, new_val: &str) -> Edit {
        Edit {
            file_path: path.to_string(),
            span_start: start,
            span_end: end,
            new_value: new_val.to_string(),
            reason: EditReason::FileMove {
                old_target: "old".to_string(),
                new_target: "new".to_string(),
            },
        }
    }

    #[test]
    fn edit_at_start_of_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "use old::path;").unwrap();

        let edits = vec![make_edit(
            &file.to_string_lossy(),
            0, 3,  // "use"
            "pub use",
        )];
        let result = apply(&edits, &[]);
        assert_eq!(result.rewritten.len(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "pub use old::path;");
    }

    #[test]
    fn edit_at_end_of_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        //                          0123456789...
        let content = "use crate::foo";
        std::fs::write(&file, content).unwrap();

        let edits = vec![make_edit(
            &file.to_string_lossy(),
            4, 14,  // "crate::foo" starts at byte 4
            "crate::bar",
        )];
        let result = apply(&edits, &[]);
        assert_eq!(result.rewritten.len(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "use crate::bar");
    }

    #[test]
    fn edit_that_grows_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        //                          0         1         2
        //                          0123456789012345678901234
        let content = "import { x } from './a';";
        std::fs::write(&file, content).unwrap();

        // './a' occupies bytes 19..22 (inside the quotes)
        let edits = vec![make_edit(
            &file.to_string_lossy(),
            19, 22,
            "./very/long/path",
        )];
        let result = apply(&edits, &[]);
        assert_eq!(result.rewritten.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "import { x } from './very/long/path';"
        );
    }

    #[test]
    fn edit_that_shrinks_content() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        //                          0         1         2         3         4
        //                          01234567890123456789012345678901234567890123
        let content = "import { x } from '../../long/path/utils';";
        std::fs::write(&file, content).unwrap();

        // '../../long/path/utils' occupies bytes 19..40 (inside the quotes)
        let edits = vec![make_edit(
            &file.to_string_lossy(),
            19, 40,
            "./u",
        )];
        let result = apply(&edits, &[]);
        assert_eq!(result.rewritten.len(), 1);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "import { x } from './u';"
        );
    }

    #[test]
    fn multiple_files_in_batch() {
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.ts");
        let file_b = dir.path().join("b.ts");
        std::fs::write(&file_a, "import './old';").unwrap();
        std::fs::write(&file_b, "import './old';").unwrap();

        let edits = vec![
            make_edit(&file_a.to_string_lossy(), 8, 13, "./new"),
            make_edit(&file_b.to_string_lossy(), 8, 13, "./new"),
        ];
        let result = apply(&edits, &[]);
        assert_eq!(result.rewritten.len(), 2);
        assert!(result.failed.is_empty());
        assert_eq!(std::fs::read_to_string(&file_a).unwrap(), "import './new';");
        assert_eq!(std::fs::read_to_string(&file_b).unwrap(), "import './new';");
    }

    #[test]
    fn missing_file_fails_gracefully() {
        let edits = vec![make_edit(
            "/nonexistent/path/file.ts",
            0, 5,
            "hello",
        )];
        let result = apply(&edits, &[]);
        assert!(result.rewritten.is_empty());
        assert_eq!(result.failed.len(), 1);
    }

    #[test]
    fn empty_file_with_zero_span() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("empty.ts");
        std::fs::write(&file, "").unwrap();

        let edits = vec![make_edit(
            &file.to_string_lossy(),
            0, 0,
            "// inserted",
        )];
        let result = apply(&edits, &[]);
        assert_eq!(result.rewritten.len(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "// inserted");
    }

    #[test]
    fn start_equals_end_inserts_without_deleting() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "use crate::foo;").unwrap();

        // Insert at position 14 (before the semicolon) without removing anything
        let edits = vec![make_edit(
            &file.to_string_lossy(),
            14, 14,
            "::bar",
        )];
        let result = apply(&edits, &[]);
        assert_eq!(result.rewritten.len(), 1);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "use crate::foo::bar;");
    }
}
