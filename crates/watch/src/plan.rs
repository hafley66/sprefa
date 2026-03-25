use sprefa_schema::RefKind;
use sqlx::SqlitePool;

use crate::change::{Change, DeclChange, FsChange};
use crate::queries;

// ── per-language path rewriter trait ─────────────────────────────────────────

/// Computes a new import string after a target file has moved.
///
/// Each language implements this differently:
/// - JS/TS: relative path math, respecting extension probing and index files
/// - Rust: module path recomputation based on mod tree structure (not yet built)
pub trait PathRewriter: Send + Sync {
    /// File extensions this rewriter handles (same contract as Extractor).
    fn extensions(&self) -> &[&str];

    /// Given that a file moved, compute the new import string.
    ///
    /// Returns `None` if this rewriter can't handle the import
    /// (e.g. bare specifier like `react` that doesn't resolve to a file path).
    fn rewrite_import(
        &self,
        from_file: &str,
        old_target: &str,
        new_target: &str,
        old_import_str: &str,
    ) -> Option<String>;
}

// ── rewrite plan ─────────────────────────────────────────────────────────────

/// A single source edit to apply.
#[derive(Debug, Clone)]
pub struct Edit {
    pub file_path: String,
    pub span_start: u32,
    pub span_end: u32,
    pub new_value: String,
    pub reason: EditReason,
}

#[derive(Debug, Clone)]
pub enum EditReason {
    FileMove {
        old_target: String,
        new_target: String,
    },
    DeclRename {
        old_name: String,
        new_name: String,
        source_file: String,
    },
}

/// Build a rewrite plan from a set of classified changes.
///
/// Queries the index to find all refs affected by each change,
/// then computes the new value for each affected ref.
///
/// Returns edits sorted by (file_path asc, span_start desc)
/// so applying them in order doesn't invalidate subsequent offsets.
pub async fn plan_rewrites(
    pool: &SqlitePool,
    changes: &[Change],
    rewriters: &[Box<dyn PathRewriter>],
) -> anyhow::Result<Vec<Edit>> {
    let mut edits: Vec<Edit> = Vec::new();

    for change in changes {
        match change {
            Change::Fs(FsChange::Move { file_id, old_path, new_path }) => {
                plan_file_move(pool, *file_id, old_path, new_path, rewriters, &mut edits).await?;
            }
            Change::Decl(DeclChange::Rename { file_id, kind, old_name, new_name, .. }) => {
                plan_decl_rename(pool, *file_id, *kind, old_name, new_name, &mut edits).await?;
            }
            Change::Fs(FsChange::Delete { file_id, path }) => {
                let count = queries::import_paths_targeting(pool, *file_id).await?.len();
                if count > 0 {
                    tracing::warn!(
                        "{} deleted: {} import paths now point at a missing file",
                        path, count,
                    );
                }
            }
            Change::Fs(FsChange::Create { .. })
            | Change::Fs(FsChange::ContentChange { .. })
            | Change::Decl(DeclChange::Added { .. })
            | Change::Decl(DeclChange::Removed { .. }) => {
                // Create: handled by re-indexing (not an edit).
                // ContentChange: upstream converts to DeclChanges.
                // Added: no existing refs to rewrite.
                // Removed: flagged as broken refs (like Delete).
            }
        }
    }

    edits.sort_by(|a, b| {
        a.file_path.cmp(&b.file_path).then(b.span_start.cmp(&a.span_start))
    });

    Ok(edits)
}

/// A file moved. Find all ImportPath refs targeting it and rewrite them.
async fn plan_file_move(
    pool: &SqlitePool,
    file_id: i64,
    old_path: &str,
    new_path: &str,
    rewriters: &[Box<dyn PathRewriter>],
    edits: &mut Vec<Edit>,
) -> anyhow::Result<()> {
    let affected = queries::import_paths_targeting(pool, file_id).await?;

    for aref in affected {
        let source_abs = aref.source_abs_path();
        let ext = std::path::Path::new(&source_abs)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let rewriter = rewriters.iter().find(|r| r.extensions().contains(&ext));
        if let Some(rw) = rewriter {
            if let Some(new_import) = rw.rewrite_import(&source_abs, old_path, new_path, &aref.value) {
                edits.push(Edit {
                    file_path: source_abs,
                    span_start: aref.span_start,
                    span_end: aref.span_end,
                    new_value: new_import,
                    reason: EditReason::FileMove {
                        old_target: old_path.to_string(),
                        new_target: new_path.to_string(),
                    },
                });
            }
        }
    }

    Ok(())
}

/// A declaration was renamed. Find all ImportName refs that import the old name
/// from the file where it was renamed, and rewrite them.
///
/// Currently handles JS/TS only. Rust use-paths store full module paths
/// (e.g. `crate::foo::Bar`) so a leaf rename requires path-aware rewriting
/// and a Rust module resolver -- not yet built.
async fn plan_decl_rename(
    pool: &SqlitePool,
    file_id: i64,
    kind: RefKind,
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<Edit>,
) -> anyhow::Result<()> {
    // Only JS ExportName renames are handled for now.
    // Rust RsDeclare would need RsUse suffix matching + module resolver.
    if kind != RefKind::ExportName {
        return Ok(());
    }

    let source_file = queries::file_abs_path(pool, file_id)
        .await?
        .unwrap_or_else(|| format!("file_id={}", file_id));

    let affected = queries::import_names_from_file(pool, file_id, old_name).await?;

    for aref in affected {
        edits.push(Edit {
            file_path: aref.source_abs_path(),
            span_start: aref.span_start,
            span_end: aref.span_end,
            new_value: new_name.to_string(),
            reason: EditReason::DeclRename {
                old_name: old_name.to_string(),
                new_name: new_name.to_string(),
                source_file: source_file.clone(),
            },
        });
    }

    Ok(())
}
