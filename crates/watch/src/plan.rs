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
#[tracing::instrument(skip(pool, changes, rewriters), fields(change_count = changes.len()))]
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

/// A file moved. Find all refs targeting it and rewrite them.
///
/// For JS/TS: finds ImportPath refs with target_file_id pointing at the moved file.
/// For Rust: finds RsUse refs whose module path prefix matches the old module path.
async fn plan_file_move(
    pool: &SqlitePool,
    file_id: i64,
    old_path: &str,
    new_path: &str,
    rewriters: &[Box<dyn PathRewriter>],
    edits: &mut Vec<Edit>,
) -> anyhow::Result<()> {
    // JS/TS: ImportPath refs linked by target_file_id.
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

    // Rust: RsUse refs matching the old module path.
    // Fetches all RsUse refs in the repo, resolves super::/self:: to absolute
    // form, then filters to those referencing the moved module.
    let old_mod = crate::rs_path::file_to_mod_path(old_path);
    let new_mod = crate::rs_path::file_to_mod_path(new_path);
    if let (Some(old_mod), Some(_new_mod)) = (old_mod, new_mod) {
        let all_rs = queries::all_rs_uses_in_repo(pool, file_id).await?;
        let rs_affected = crate::rs_path::filter_rs_uses_by_prefix(&all_rs, &old_mod);
        let rs_rewriter = crate::rs_path::RsPathRewriter;
        for aref in rs_affected {
            let source_abs = aref.source_abs_path();
            if let Some(new_import) = rs_rewriter.rewrite_import(
                &source_abs, old_path, new_path, &aref.value,
            ) {
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

/// A declaration was renamed. Find all refs that import the old name
/// from the file where it was renamed, and rewrite them.
///
/// Handles both JS/TS (ExportName -> ImportName refs) and Rust
/// (RsDeclare -> RsUse refs with matching module path suffix).
///
/// For JS/TS, follows re-export chains transitively: if barrel.ts
/// re-exports Foo from utils.ts (without aliasing), and Foo is renamed
/// in utils.ts, consumers of barrel.ts also get rewritten.
async fn plan_decl_rename(
    pool: &SqlitePool,
    file_id: i64,
    kind: RefKind,
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<Edit>,
) -> anyhow::Result<()> {
    let source_file = queries::file_abs_path(pool, file_id)
        .await?
        .unwrap_or_else(|| format!("file_id={}", file_id));

    match kind {
        RefKind::ExportName => {
            // JS/TS: find ImportName refs that import old_name from the target file,
            // then follow re-export chains transitively.
            rename_through_reexports(pool, file_id, old_name, new_name, &source_file, edits).await?;
        }
        RefKind::RsDeclare => {
            // Rust: find RsUse refs like `crate::mod_path::OldName` (or
            // `super::OldName`, `self::OldName`) and rewrite to NewName.
            // Resolves all prefix styles to absolute form before matching.
            let mod_path = crate::rs_path::file_to_mod_path(&source_file);
            if let Some(mod_path) = mod_path {
                let all_rs = queries::all_rs_uses_in_repo(pool, file_id).await?;
                let affected = crate::rs_path::filter_rs_uses_by_target(&all_rs, &mod_path, old_name);
                for aref in affected {
                    // The span covers the entire use path (e.g. `crate::utils::Foo`).
                    // Replace the last segment only: rebuild with new name.
                    let new_use_value = if let Some(prefix_end) = aref.value.rfind("::") {
                        format!("{}::{}", &aref.value[..prefix_end], new_name)
                    } else {
                        new_name.to_string()
                    };
                    edits.push(Edit {
                        file_path: aref.source_abs_path(),
                        span_start: aref.span_start,
                        span_end: aref.span_end,
                        new_value: new_use_value,
                        reason: EditReason::DeclRename {
                            old_name: old_name.to_string(),
                            new_name: new_name.to_string(),
                            source_file: source_file.clone(),
                        },
                    });
                }
            }
        }
        RefKind::ImportName => {
            // User renamed an import binding. Propagate upstream to the declaring
            // file, then back down to all other consumers in the chain.
            //
            // Example: consumer.ts renames `import { Foo }` to `import { Bar }`.
            // Find that Foo is exported from barrel.ts, which re-exports from utils.ts.
            // Walk up to utils.ts (the root), rename ExportName there, then propagate
            // downstream to barrel.ts and all other consumers. Skip consumer.ts
            // (already edited by user).
            let target = queries::upstream_export_file(pool, file_id, old_name).await?;
            if let Some(target_id) = target {
                let root_id = find_chain_root(pool, target_id, old_name).await?;

                // Rename ExportName in the root declaring file
                let export_refs = queries::export_ref_in_file(pool, root_id, old_name).await?;
                for aref in &export_refs {
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

                // Propagate downstream from root, skipping the originating file
                let mut visited = std::collections::HashSet::new();
                rename_chain_step(
                    pool, root_id, old_name, new_name,
                    &source_file, edits, &mut visited, Some(&source_file),
                ).await?;
            }
        }
        _ => {
            // Other kinds (RsMod, etc.) not handled yet.
        }
    }

    Ok(())
}

/// Walk upstream through re-export chains to find the root declaring file.
///
/// Starting from `file_id`, check if this file also imports `name` from an
/// upstream file. If so, follow the chain. The root is the file that exports
/// `name` without importing it from somewhere else.
async fn find_chain_root(
    pool: &SqlitePool,
    file_id: i64,
    name: &str,
) -> anyhow::Result<i64> {
    let mut current = file_id;
    let mut visited = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            break; // cycle detected
        }
        match queries::upstream_export_file(pool, current, name).await? {
            Some(upstream_id) => current = upstream_id,
            None => break, // current is the root
        }
    }
    Ok(current)
}

/// Follow re-export chains transitively for a renamed name.
///
/// Given that `old_name` was renamed to `new_name` in `source_file_id`:
/// 1. Find all files that import `old_name` from `source_file_id` (direct consumers)
/// 2. Find relay files that re-export `old_name` from `source_file_id` without aliasing
/// 3. For each relay, rename its ImportName ref and recurse (its consumers also need updates)
///
/// Cycle detection via `visited` set prevents infinite loops in pathological re-export cycles.
/// `skip_abs_path`: if set, refs in files matching this absolute path are not edited
/// (used to skip the file the user already changed when propagating upstream).
async fn rename_through_reexports(
    pool: &SqlitePool,
    source_file_id: i64,
    old_name: &str,
    new_name: &str,
    original_source_file: &str,
    edits: &mut Vec<Edit>,
) -> anyhow::Result<()> {
    let mut visited = std::collections::HashSet::new();
    rename_chain_step(pool, source_file_id, old_name, new_name, original_source_file, edits, &mut visited, None).await
}

async fn rename_chain_step(
    pool: &SqlitePool,
    source_file_id: i64,
    old_name: &str,
    new_name: &str,
    original_source_file: &str,
    edits: &mut Vec<Edit>,
    visited: &mut std::collections::HashSet<i64>,
    skip_abs_path: Option<&str>,
) -> anyhow::Result<()> {
    if !visited.insert(source_file_id) {
        return Ok(());
    }

    // Direct consumers: files that `import { old_name } from <source_file>`
    let affected = queries::import_names_from_file(pool, source_file_id, old_name).await?;
    for aref in &affected {
        let abs = aref.source_abs_path();
        if skip_abs_path.is_some_and(|s| s == abs) {
            continue;
        }
        edits.push(Edit {
            file_path: abs,
            span_start: aref.span_start,
            span_end: aref.span_end,
            new_value: new_name.to_string(),
            reason: EditReason::DeclRename {
                old_name: old_name.to_string(),
                new_name: new_name.to_string(),
                source_file: original_source_file.to_string(),
            },
        });
    }

    // Relay files: re-export old_name from source_file without aliasing.
    // These are barrel files whose consumers also need updating.
    let relays = queries::reexport_relay_file_ids(pool, source_file_id, old_name).await?;
    for relay_file_id in relays {
        // Recurse: consumers of the relay file also import old_name transitively.
        Box::pin(rename_chain_step(
            pool, relay_file_id, old_name, new_name,
            original_source_file, edits, visited, skip_abs_path,
        )).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_schema::{init_db, RefKind};
    use sqlx::SqlitePool;

    async fn make_db() -> SqlitePool {
        init_db(":memory:").await.unwrap()
    }

    async fn seed_repo(db: &SqlitePool, root: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO repos (name, root_path) VALUES ('test', ?) RETURNING id",
        )
        .bind(root)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn seed_file(db: &SqlitePool, repo_id: i64, path: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "INSERT INTO files (repo_id, path, content_hash) VALUES (?, ?, 'h') RETURNING id",
        )
        .bind(repo_id)
        .bind(path)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn seed_string(db: &SqlitePool, value: &str) -> i64 {
        sqlx::query("INSERT OR IGNORE INTO strings (value, norm) VALUES (?, ?)")
            .bind(value)
            .bind(value)
            .execute(db)
            .await
            .unwrap();
        sqlx::query_scalar::<_, i64>("SELECT id FROM strings WHERE value = ?")
            .bind(value)
            .fetch_one(db)
            .await
            .unwrap()
    }

    /// Auto-incrementing span counter to ensure unique (file_id, string_id, span_start).
    static SPAN_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

    async fn seed_ref(
        db: &SqlitePool,
        file_id: i64,
        value: &str,
        kind: RefKind,
        target_file_id: Option<i64>,
    ) {
        let string_id = seed_string(db, value).await;
        let span = SPAN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        sqlx::query(
            "INSERT INTO refs (string_id, file_id, span_start, span_end, is_path, ref_kind, target_file_id)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(string_id)
        .bind(file_id)
        .bind(span)
        .bind(span)
        .bind(kind == RefKind::ImportPath)
        .bind(kind.as_u8() as i64)
        .bind(target_file_id)
        .execute(db)
        .await
        .unwrap();
    }

    // ── re-export chain tests ───────────────────────────────────────────

    /// utils.ts exports Foo
    /// barrel.ts: export { Foo } from './utils'  (re-export, no alias)
    /// consumer.ts: import { Foo } from './barrel'
    ///
    /// Rename Foo -> Bar in utils.ts should propagate to consumer.ts
    #[tokio::test]
    async fn rename_propagates_through_barrel() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let barrel_id = seed_file(&db, repo_id, "src/barrel.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;

        // utils.ts: ExportName "Foo"
        seed_ref(&db, utils_id, "Foo", RefKind::ExportName, None).await;

        // barrel.ts: ImportPath -> utils, ImportName "Foo", ExportName "Foo"
        seed_ref(&db, barrel_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", RefKind::ImportName, None).await;
        seed_ref(&db, barrel_id, "Foo", RefKind::ExportName, None).await;

        // consumer.ts: ImportPath -> barrel, ImportName "Foo"
        seed_ref(&db, consumer_id, "./barrel", RefKind::ImportPath, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "Foo", RefKind::ImportName, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: RefKind::ExportName,
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();

        // Should have edits in both barrel.ts and consumer.ts
        let edited_files: Vec<&str> = edits.iter()
            .map(|e| e.file_path.as_str())
            .collect();
        assert!(edited_files.contains(&"/repo/src/barrel.ts"), "barrel.ts should be edited");
        assert!(edited_files.contains(&"/repo/src/consumer.ts"), "consumer.ts should be edited");
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().all(|e| e.new_value == "Bar"));
    }

    /// utils.ts exports Foo
    /// barrel.ts: export { Foo as PublicFoo } from './utils'  (aliased)
    /// consumer.ts: import { PublicFoo } from './barrel'
    ///
    /// Rename Foo -> Bar in utils.ts should hit barrel's ImportName
    /// but NOT propagate to consumer (consumer imports PublicFoo, not Foo)
    #[tokio::test]
    async fn aliased_reexport_stops_propagation() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let barrel_id = seed_file(&db, repo_id, "src/barrel.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;

        // utils.ts: ExportName "Foo"
        seed_ref(&db, utils_id, "Foo", RefKind::ExportName, None).await;

        // barrel.ts: ImportPath -> utils, ImportName "Foo", ExportName "PublicFoo"
        seed_ref(&db, barrel_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", RefKind::ImportName, None).await;
        seed_ref(&db, barrel_id, "PublicFoo", RefKind::ExportName, None).await;

        // consumer.ts: ImportPath -> barrel, ImportName "PublicFoo"
        seed_ref(&db, consumer_id, "./barrel", RefKind::ImportPath, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "PublicFoo", RefKind::ImportName, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: RefKind::ExportName,
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();

        // Only barrel.ts ImportName "Foo" should be edited.
        // Consumer imports "PublicFoo" which is unaffected.
        let edited_files: Vec<&str> = edits.iter()
            .map(|e| e.file_path.as_str())
            .collect();
        assert!(edited_files.contains(&"/repo/src/barrel.ts"));
        assert!(!edited_files.contains(&"/repo/src/consumer.ts"),
            "consumer should not be edited (imports aliased name)");
        assert_eq!(edits.len(), 1);
    }

    /// Two-hop chain: utils -> barrel1 -> barrel2 -> consumer
    #[tokio::test]
    async fn two_hop_reexport_chain() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let barrel1_id = seed_file(&db, repo_id, "src/barrel1.ts").await;
        let barrel2_id = seed_file(&db, repo_id, "src/barrel2.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;

        // utils: ExportName "Foo"
        seed_ref(&db, utils_id, "Foo", RefKind::ExportName, None).await;

        // barrel1: re-exports Foo from utils
        seed_ref(&db, barrel1_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, barrel1_id, "Foo", RefKind::ImportName, None).await;
        seed_ref(&db, barrel1_id, "Foo", RefKind::ExportName, None).await;

        // barrel2: re-exports Foo from barrel1
        seed_ref(&db, barrel2_id, "./barrel1", RefKind::ImportPath, Some(barrel1_id)).await;
        seed_ref(&db, barrel2_id, "Foo", RefKind::ImportName, None).await;
        seed_ref(&db, barrel2_id, "Foo", RefKind::ExportName, None).await;

        // consumer: imports Foo from barrel2
        seed_ref(&db, consumer_id, "./barrel2", RefKind::ImportPath, Some(barrel2_id)).await;
        seed_ref(&db, consumer_id, "Foo", RefKind::ImportName, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: RefKind::ExportName,
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();

        // All three downstream files should be edited
        let edited_files: std::collections::HashSet<&str> = edits.iter()
            .map(|e| e.file_path.as_str())
            .collect();
        assert!(edited_files.contains("/repo/src/barrel1.ts"));
        assert!(edited_files.contains("/repo/src/barrel2.ts"));
        assert!(edited_files.contains("/repo/src/consumer.ts"));
        assert_eq!(edits.len(), 3);
    }

    /// Direct import without re-export chain still works
    #[tokio::test]
    async fn direct_import_rename_no_chain() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;

        seed_ref(&db, utils_id, "Foo", RefKind::ExportName, None).await;
        seed_ref(&db, consumer_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, consumer_id, "Foo", RefKind::ImportName, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: RefKind::ExportName,
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].file_path, "/repo/src/consumer.ts");
        assert_eq!(edits[0].new_value, "Bar");
    }

    // ── upstream (ImportName) rename tests ───────────────────────────────

    /// User renames import in consumer.ts. Should propagate up to utils.ts
    /// ExportName and sideways to other_consumer.ts ImportName.
    #[tokio::test]
    async fn import_rename_propagates_upstream_and_sideways() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;
        let other_id = seed_file(&db, repo_id, "src/other.ts").await;

        // utils.ts: ExportName "Foo"
        seed_ref(&db, utils_id, "Foo", RefKind::ExportName, None).await;

        // consumer.ts: imports Foo from utils
        seed_ref(&db, consumer_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, consumer_id, "Foo", RefKind::ImportName, None).await;

        // other.ts: also imports Foo from utils
        seed_ref(&db, other_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, other_id, "Foo", RefKind::ImportName, None).await;

        // User renamed ImportName in consumer.ts: Foo -> Bar
        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: consumer_id,
            kind: RefKind::ImportName,
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();

        let edited_files: std::collections::HashSet<&str> = edits.iter()
            .map(|e| e.file_path.as_str())
            .collect();

        // ExportName in utils.ts should be renamed
        assert!(edited_files.contains("/repo/src/utils.ts"),
            "utils.ts ExportName should be edited");
        // other.ts ImportName should be renamed
        assert!(edited_files.contains("/repo/src/other.ts"),
            "other.ts ImportName should be edited");
        // consumer.ts should NOT be edited (user already changed it)
        assert!(!edited_files.contains("/repo/src/consumer.ts"),
            "consumer.ts should be skipped (user already edited)");

        assert!(edits.iter().all(|e| e.new_value == "Bar"));
        assert_eq!(edits.len(), 2);
    }

    /// Upstream through barrel: consumer -> barrel -> utils
    /// User renames in consumer, should propagate all the way to utils ExportName
    #[tokio::test]
    async fn import_rename_walks_up_through_barrel() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let barrel_id = seed_file(&db, repo_id, "src/barrel.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;

        // utils.ts: ExportName "Foo"
        seed_ref(&db, utils_id, "Foo", RefKind::ExportName, None).await;

        // barrel.ts: re-exports Foo from utils
        seed_ref(&db, barrel_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", RefKind::ImportName, None).await;
        seed_ref(&db, barrel_id, "Foo", RefKind::ExportName, None).await;

        // consumer.ts: imports Foo from barrel
        seed_ref(&db, consumer_id, "./barrel", RefKind::ImportPath, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "Foo", RefKind::ImportName, None).await;

        // User renamed ImportName in consumer.ts: Foo -> Bar
        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: consumer_id,
            kind: RefKind::ImportName,
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();

        let edited_files: std::collections::HashSet<&str> = edits.iter()
            .map(|e| e.file_path.as_str())
            .collect();

        // Root: utils.ts ExportName renamed
        assert!(edited_files.contains("/repo/src/utils.ts"));
        // Intermediate: barrel.ts ImportName renamed (downstream from root)
        assert!(edited_files.contains("/repo/src/barrel.ts"));
        // Consumer skipped (user already edited)
        assert!(!edited_files.contains("/repo/src/consumer.ts"));

        assert!(edits.iter().all(|e| e.new_value == "Bar"));
        assert_eq!(edits.len(), 2);
    }

    /// Import rename with no resolvable target (bare specifier) does nothing
    #[tokio::test]
    async fn import_rename_bare_specifier_no_propagation() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let consumer_id = seed_file(&db, repo_id, "src/app.ts").await;

        // Import from 'react' -- no target_file_id
        seed_ref(&db, consumer_id, "react", RefKind::ImportPath, None).await;
        seed_ref(&db, consumer_id, "useState", RefKind::ImportName, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: consumer_id,
            kind: RefKind::ImportName,
            old_name: "useState".to_string(),
            new_name: "useMyState".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();
        // No target to propagate to
        assert!(edits.is_empty());
    }

    /// Rename in a barrel file's ExportName propagates both upstream and downstream
    #[tokio::test]
    async fn barrel_export_rename_propagates_to_consumers() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let barrel_id = seed_file(&db, repo_id, "src/barrel.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;

        seed_ref(&db, utils_id, "Foo", RefKind::ExportName, None).await;

        seed_ref(&db, barrel_id, "./utils", RefKind::ImportPath, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", RefKind::ImportName, None).await;
        seed_ref(&db, barrel_id, "Foo", RefKind::ExportName, None).await;

        seed_ref(&db, consumer_id, "./barrel", RefKind::ImportPath, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "Foo", RefKind::ImportName, None).await;

        // ExportName rename in barrel (middle of chain)
        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: barrel_id,
            kind: RefKind::ExportName,
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let edits = plan_rewrites(&db, &changes, &[]).await.unwrap();

        let edited_files: std::collections::HashSet<&str> = edits.iter()
            .map(|e| e.file_path.as_str())
            .collect();

        // Downstream: consumer.ts ImportName should be renamed
        assert!(edited_files.contains("/repo/src/consumer.ts"));
        assert!(edits.iter().all(|e| e.new_value == "Bar"));
    }
}
