use sqlx::SqlitePool;
use sprefa_extract::kind;

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

/// A syn-based Rust file rewrite. The rewriter reads the file, parses with syn,
/// and replaces module idents directly in the AST.
#[derive(Debug, Clone)]
pub struct RustRewrite {
    pub file_path: String,
    pub old_stem: String,
    pub new_stem: String,
    pub use_prefixes: Vec<String>,
    pub rewrite_mod_decl: bool,
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
) -> anyhow::Result<(Vec<Edit>, Vec<RustRewrite>)> {
    let mut edits: Vec<Edit> = Vec::new();
    let mut rust_rewrites: Vec<RustRewrite> = Vec::new();

    // Build #[path] attribute override map once for the whole batch.
    // Uses the first file_id from any change to identify the repo.
    let (mod_overrides, workspace, repo_root) = if let Some(fid) = first_file_id(changes) {
        let rows = queries::path_attr_overrides(pool, fid).await?;
        let overrides = crate::rs_path::build_mod_overrides(&rows);
        // Build workspace map from repo root for cross-crate resolution.
        let root = queries::repo_root_for_file(pool, fid).await?.unwrap_or_default();
        let ws = if root.is_empty() {
            crate::workspace::WorkspaceMap::default()
        } else {
            crate::workspace::build_workspace_map(&root)
        };
        (overrides, ws, root)
    } else {
        (crate::rs_path::ModOverrides::new(), crate::workspace::WorkspaceMap::default(), String::new())
    };

    for change in changes {
        match change {
            Change::Fs(FsChange::Move { file_id, old_path, new_path }) => {
                plan_file_move(pool, *file_id, old_path, new_path, rewriters, &mut edits, &mut rust_rewrites, &mod_overrides, &workspace, &repo_root).await?;
            }
            Change::Decl(DeclChange::Rename { file_id, kind, old_name, new_name, .. }) => {
                plan_decl_rename(pool, *file_id, kind, old_name, new_name, &mut edits, &mod_overrides, &workspace).await?;
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

    Ok((edits, rust_rewrites))
}

fn first_file_id(changes: &[Change]) -> Option<i64> {
    changes.iter().find_map(|c| match c {
        Change::Fs(FsChange::Move { file_id, .. })
        | Change::Fs(FsChange::Delete { file_id, .. })
        | Change::Fs(FsChange::ContentChange { file_id, .. })
        | Change::Decl(DeclChange::Rename { file_id, .. })
        | Change::Decl(DeclChange::Added { file_id, .. })
        | Change::Decl(DeclChange::Removed { file_id, .. }) => Some(*file_id),
        Change::Fs(FsChange::Create { .. }) => None,
    })
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
    rust_rewrites: &mut Vec<RustRewrite>,
    mod_overrides: &crate::rs_path::ModOverrides,
    workspace: &crate::workspace::WorkspaceMap,
    repo_root: &str,
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

    // Rust: syn-based rewriting. DB narrows which files to parse, syn does the actual edits.
    let old_mod = crate::rs_path::file_to_mod_path_checked(old_path, mod_overrides);
    let new_mod = crate::rs_path::file_to_mod_path_checked(new_path, mod_overrides);
    if let (Some(old_mod), Some(new_mod)) = (old_mod, new_mod) {
        let old_stem = old_mod.rsplit("::").next().unwrap_or(&old_mod).to_string();
        let new_stem = new_mod.rsplit("::").next().unwrap_or(&new_mod).to_string();
        if old_stem == new_stem {
            return Ok(());
        }

        // Collect unique affected file paths from DB (scoping which files to parse).
        let all_rs = queries::all_rs_uses_in_repo(pool, file_id).await?;
        let intra_affected = crate::rs_path::filter_rs_uses_by_prefix(&all_rs, &old_mod, mod_overrides);
        let mut affected_files: std::collections::HashSet<String> = std::collections::HashSet::new();
        for aref in &intra_affected {
            affected_files.insert(aref.source_abs_path());
        }

        // Intra-crate: use paths with `crate::old_stem`
        for file_path in &affected_files {
            rust_rewrites.push(RustRewrite {
                file_path: file_path.clone(),
                old_stem: old_stem.clone(),
                new_stem: new_stem.clone(),
                use_prefixes: vec!["crate".to_string()],
                rewrite_mod_decl: false,
            });
        }

        // Parent module: mod declaration + relative uses
        if let Some((_, candidates)) = crate::rs_path::mod_parent_candidates(old_path, repo_root) {
            for candidate in &candidates {
                if let Ok(Some(_)) = queries::file_id_by_path(pool, file_id, candidate).await {
                    let parent_abs = format!("{}/{}", repo_root, candidate);
                    rust_rewrites.push(RustRewrite {
                        file_path: parent_abs,
                        old_stem: old_stem.clone(),
                        new_stem: new_stem.clone(),
                        use_prefixes: vec!["crate".to_string()],
                        rewrite_mod_decl: true,
                    });
                }
            }
        }

        // Cross-crate: any file that references the moved crate at all.
        // Scoping by exact `crate_name::old_stem` misses inline paths like
        // `sprefa_rules::types::Foo` in match arms with no `use` import.
        // Run the syn visitor on every file that has any ref to the crate.
        if let Some(crate_name) = workspace.crate_for_file(old_path) {
            let crate_prefix = format!("{}::", crate_name);
            for aref in &all_rs {
                if aref.value.starts_with(&crate_prefix) {
                    let path = aref.source_abs_path();
                    if !affected_files.contains(&path) {
                        affected_files.insert(path.clone());
                        rust_rewrites.push(RustRewrite {
                            file_path: path,
                            old_stem: old_stem.clone(),
                            new_stem: new_stem.clone(),
                            use_prefixes: vec![crate_name.to_string()],
                            rewrite_mod_decl: false,
                        });
                    }
                }
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
    kind: &str,
    old_name: &str,
    new_name: &str,
    edits: &mut Vec<Edit>,
    mod_overrides: &crate::rs_path::ModOverrides,
    workspace: &crate::workspace::WorkspaceMap,
) -> anyhow::Result<()> {
    let source_file = queries::file_abs_path(pool, file_id)
        .await?
        .unwrap_or_else(|| format!("file_id={}", file_id));

    match kind {
        kind::EXPORT_NAME => {
            // JS/TS: find ImportName refs that import old_name from the target file,
            // then follow re-export chains transitively.
            rename_through_reexports(pool, file_id, old_name, new_name, &source_file, edits).await?;
        }
        kind::RS_DECLARE => {
            // Rust: find RsUse refs like `crate::mod_path::OldName` (or
            // `super::OldName`, `self::OldName`) and rewrite to NewName.
            // Resolves all prefix styles to absolute form before matching.
            let mod_path = crate::rs_path::file_to_mod_path_checked(&source_file, mod_overrides);
            if let Some(mod_path) = mod_path {
                let all_rs = queries::all_rs_uses_in_repo(pool, file_id).await?;
                let affected = crate::rs_path::filter_rs_uses_by_target(&all_rs, &mod_path, old_name, mod_overrides);
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

                // Glob expansion: find `use crate::mod_path::*` and expand to
                // explicit imports with the rename applied.
                let glob_uses = crate::rs_path::filter_rs_glob_uses(&all_rs, &mod_path, mod_overrides);
                if !glob_uses.is_empty() {
                    let mut decl_names = queries::declarations_in_file(pool, file_id).await?;
                    // Apply the rename to the declaration list
                    for name in &mut decl_names {
                        if name == old_name {
                            *name = new_name.to_string();
                        }
                    }
                    decl_names.sort();
                    for aref in glob_uses {
                        // Replace `crate::utils::*` with `crate::utils::{Bar, Foo, ...}`
                        let prefix = &aref.value[..aref.value.len() - 1]; // strip trailing *
                        let expanded = format!("{}{{{}}}", prefix, decl_names.join(", "));
                        edits.push(Edit {
                            file_path: aref.source_abs_path(),
                            span_start: aref.span_start,
                            span_end: aref.span_end,
                            new_value: expanded,
                            reason: EditReason::DeclRename {
                                old_name: old_name.to_string(),
                                new_name: new_name.to_string(),
                                source_file: source_file.clone(),
                            },
                        });
                    }
                }

                // Cross-crate: find `use crate_name::mod_path::OldName` in other crates
                if let Some(crate_name) = workspace.crate_for_file(&source_file) {
                    let cross_target = format!("{}::{}::{}", crate_name, mod_path, old_name)
                        .replacen(&format!("{}::crate::", crate_name), &format!("{}::", crate_name), 1);
                    for aref in &all_rs {
                        if aref.value == cross_target {
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
            }
        }
        kind::IMPORT_NAME => {
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
                let root_id = queries::find_chain_root(pool, target_id, old_name).await?;

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
                rename_through_reexports_skip(
                    pool, root_id, old_name, new_name,
                    &source_file, edits, Some(&source_file),
                ).await?;
            }
        }
        _ => {
            // Other kinds (RsMod, etc.) not handled yet.
        }
    }

    Ok(())
}


/// Follow re-export chains transitively for a renamed name using iterative BFS.
///
/// Given that `old_name` was renamed to `new_name` in `source_file_id`:
/// 1. Find all files that import `old_name` from `source_file_id` (direct consumers)
/// 2. Find relay files that re-export `old_name` from `source_file_id` without aliasing
/// 3. Enqueue each relay for the same treatment (its consumers also need updates)
///
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
    rename_through_reexports_skip(pool, source_file_id, old_name, new_name, original_source_file, edits, None).await
}

async fn rename_through_reexports_skip(
    pool: &SqlitePool,
    source_file_id: i64,
    old_name: &str,
    new_name: &str,
    original_source_file: &str,
    edits: &mut Vec<Edit>,
    skip_abs_path: Option<&str>,
) -> anyhow::Result<()> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = std::collections::VecDeque::from([source_file_id]);

    while let Some(fid) = queue.pop_front() {
        if !visited.insert(fid) {
            continue;
        }

        // Direct consumers: files that `import { old_name } from <fid>`
        let affected = queries::import_names_from_file(pool, fid, old_name).await?;
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

        // Relay files: re-export old_name without aliasing. Enqueue for BFS.
        let relays = queries::reexport_relay_file_ids(pool, fid, old_name).await?;
        queue.extend(relays);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_schema::init_db;
    use sqlx::SqlitePool;

    async fn make_db() -> SqlitePool {
        let pool = init_db(":memory:").await.unwrap();
        // Create builtin per-rule tables so test seed_ref can insert into them.
        for def in sprefa_schema::rule_tables::builtin_rule_table_defs() {
            sqlx::query(&def.create_table_sql()).execute(&pool).await.unwrap();
            sqlx::query(&def.create_view_sql()).execute(&pool).await.unwrap();
            sqlx::query(&def.create_refs_view_sql()).execute(&pool).await.unwrap();
        }
        pool
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
        kind: &str,
        target_file_id: Option<i64>,
    ) {
        let string_id = seed_string(db, value).await;
        let span = SPAN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let ref_id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO refs (string_id, file_id, span_start, span_end, is_path, target_file_id)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(string_id)
        .bind(file_id)
        .bind(span)
        .bind(span)
        .bind(kind == kind::IMPORT_PATH)
        .bind(target_file_id)
        .fetch_one(db)
        .await
        .unwrap();
        // Insert into per-rule _data table.
        let table = format!("{kind}_data");
        let sql = format!(
            "INSERT OR IGNORE INTO \"{table}\" (value_ref, value_str, repo_id, file_id, rev) \
             VALUES (?, ?, (SELECT repo_id FROM files WHERE id = ?), ?, '')"
        );
        sqlx::query(&sql)
            .bind(ref_id)
            .bind(string_id)
            .bind(file_id)
            .bind(file_id)
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
        seed_ref(&db, utils_id, "Foo", kind::EXPORT_NAME, None).await;

        // barrel.ts: ImportPath -> utils, ImportName "Foo", ExportName "Foo"
        seed_ref(&db, barrel_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", kind::IMPORT_NAME, None).await;
        seed_ref(&db, barrel_id, "Foo", kind::EXPORT_NAME, None).await;

        // consumer.ts: ImportPath -> barrel, ImportName "Foo"
        seed_ref(&db, consumer_id, "./barrel", kind::IMPORT_PATH, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "Foo", kind::IMPORT_NAME, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::EXPORT_NAME.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();

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
        seed_ref(&db, utils_id, "Foo", kind::EXPORT_NAME, None).await;

        // barrel.ts: ImportPath -> utils, ImportName "Foo", ExportName "PublicFoo"
        seed_ref(&db, barrel_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", kind::IMPORT_NAME, None).await;
        seed_ref(&db, barrel_id, "PublicFoo", kind::EXPORT_NAME, None).await;

        // consumer.ts: ImportPath -> barrel, ImportName "PublicFoo"
        seed_ref(&db, consumer_id, "./barrel", kind::IMPORT_PATH, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "PublicFoo", kind::IMPORT_NAME, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::EXPORT_NAME.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();

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
        seed_ref(&db, utils_id, "Foo", kind::EXPORT_NAME, None).await;

        // barrel1: re-exports Foo from utils
        seed_ref(&db, barrel1_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, barrel1_id, "Foo", kind::IMPORT_NAME, None).await;
        seed_ref(&db, barrel1_id, "Foo", kind::EXPORT_NAME, None).await;

        // barrel2: re-exports Foo from barrel1
        seed_ref(&db, barrel2_id, "./barrel1", kind::IMPORT_PATH, Some(barrel1_id)).await;
        seed_ref(&db, barrel2_id, "Foo", kind::IMPORT_NAME, None).await;
        seed_ref(&db, barrel2_id, "Foo", kind::EXPORT_NAME, None).await;

        // consumer: imports Foo from barrel2
        seed_ref(&db, consumer_id, "./barrel2", kind::IMPORT_PATH, Some(barrel2_id)).await;
        seed_ref(&db, consumer_id, "Foo", kind::IMPORT_NAME, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::EXPORT_NAME.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();

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

        seed_ref(&db, utils_id, "Foo", kind::EXPORT_NAME, None).await;
        seed_ref(&db, consumer_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, consumer_id, "Foo", kind::IMPORT_NAME, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::EXPORT_NAME.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();
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
        seed_ref(&db, utils_id, "Foo", kind::EXPORT_NAME, None).await;

        // consumer.ts: imports Foo from utils
        seed_ref(&db, consumer_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, consumer_id, "Foo", kind::IMPORT_NAME, None).await;

        // other.ts: also imports Foo from utils
        seed_ref(&db, other_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, other_id, "Foo", kind::IMPORT_NAME, None).await;

        // User renamed ImportName in consumer.ts: Foo -> Bar
        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: consumer_id,
            kind: kind::IMPORT_NAME.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();

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
        seed_ref(&db, utils_id, "Foo", kind::EXPORT_NAME, None).await;

        // barrel.ts: re-exports Foo from utils
        seed_ref(&db, barrel_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", kind::IMPORT_NAME, None).await;
        seed_ref(&db, barrel_id, "Foo", kind::EXPORT_NAME, None).await;

        // consumer.ts: imports Foo from barrel
        seed_ref(&db, consumer_id, "./barrel", kind::IMPORT_PATH, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "Foo", kind::IMPORT_NAME, None).await;

        // User renamed ImportName in consumer.ts: Foo -> Bar
        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: consumer_id,
            kind: kind::IMPORT_NAME.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();

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
        seed_ref(&db, consumer_id, "react", kind::IMPORT_PATH, None).await;
        seed_ref(&db, consumer_id, "useState", kind::IMPORT_NAME, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: consumer_id,
            kind: kind::IMPORT_NAME.to_string(),
            old_name: "useState".to_string(),
            new_name: "useMyState".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();
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

        seed_ref(&db, utils_id, "Foo", kind::EXPORT_NAME, None).await;

        seed_ref(&db, barrel_id, "./utils", kind::IMPORT_PATH, Some(utils_id)).await;
        seed_ref(&db, barrel_id, "Foo", kind::IMPORT_NAME, None).await;
        seed_ref(&db, barrel_id, "Foo", kind::EXPORT_NAME, None).await;

        seed_ref(&db, consumer_id, "./barrel", kind::IMPORT_PATH, Some(barrel_id)).await;
        seed_ref(&db, consumer_id, "Foo", kind::IMPORT_NAME, None).await;

        // ExportName rename in barrel (middle of chain)
        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: barrel_id,
            kind: kind::EXPORT_NAME.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();

        let edited_files: std::collections::HashSet<&str> = edits.iter()
            .map(|e| e.file_path.as_str())
            .collect();

        // Downstream: consumer.ts ImportName should be renamed
        assert!(edited_files.contains("/repo/src/consumer.ts"));
        assert!(edits.iter().all(|e| e.new_value == "Bar"));
    }

    // ── Rust glob import expansion tests ──────────────────────────────

    /// utils.rs has `pub fn Foo` and `pub fn Other`.
    /// consumer.rs has `use crate::utils::*`.
    /// Renaming Foo -> Bar in utils.rs should expand the glob to
    /// `use crate::utils::{Bar, Other}`.
    #[tokio::test]
    async fn glob_import_expanded_on_rename() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.rs").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.rs").await;

        // utils.rs: declares Foo and Other
        seed_ref(&db, utils_id, "Foo", kind::RS_DECLARE, None).await;
        seed_ref(&db, utils_id, "Other", kind::RS_DECLARE, None).await;

        // consumer.rs: use crate::utils::*
        seed_ref(&db, consumer_id, "crate::utils::*", kind::RS_USE, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::RS_DECLARE.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].file_path, "/repo/src/consumer.rs");
        // Glob expanded with rename applied, sorted
        assert_eq!(edits[0].new_value, "crate::utils::{Bar, Other}");
    }

    /// No glob uses from the renamed module produces no glob edits
    #[tokio::test]
    async fn no_glob_no_expansion() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.rs").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.rs").await;

        seed_ref(&db, utils_id, "Foo", kind::RS_DECLARE, None).await;
        // Explicit import, not glob
        seed_ref(&db, consumer_id, "crate::utils::Foo", kind::RS_USE, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::RS_DECLARE.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();
        assert_eq!(edits.len(), 1);
        // Should be a direct rename, not glob expansion
        assert_eq!(edits[0].new_value, "crate::utils::Bar");
    }

    /// Glob from an unrelated module should not be expanded
    #[tokio::test]
    async fn glob_from_different_module_ignored() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.rs").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.rs").await;

        seed_ref(&db, utils_id, "Foo", kind::RS_DECLARE, None).await;
        // Glob from a different module
        seed_ref(&db, consumer_id, "crate::other::*", kind::RS_USE, None).await;

        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::RS_DECLARE.to_string(),
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();
        assert!(edits.is_empty());
    }

    // ── integration: extractor → DB → plan ──────────────────────────────

    /// Full pipeline: run the JS extractor on real source code, insert refs
    /// into the DB through the same INSERT OR IGNORE path the scanner uses,
    /// then verify plan_rewrites produces the expected edits.
    ///
    /// This catches UNIQUE constraint collisions that silently drop refs
    /// (the ExportName + ImportName at same span issue on re-exports).
    #[tokio::test]
    async fn integration_extractor_to_plan_reexport_rename() {
        use sprefa_extract::{ExtractContext, Extractor};
        let js_ext = sprefa_js::JsExtractor;
        let ctx = ExtractContext::default();

        let utils_src = b"export function computeScore(): number { return 42; }\nexport function formatOutput(val: number): string { return `${val}`; }";

        let barrel_src = b"export { computeScore, formatOutput } from './utils';";

        let consumer_src = b"import { computeScore, formatOutput } from './barrel';\nconsole.log(formatOutput(computeScore()));";

        // Extract refs from actual source code
        let utils_refs = js_ext.extract(utils_src, "utils.ts", &ctx);
        let barrel_refs = js_ext.extract(barrel_src, "barrel.ts", &ctx);
        let consumer_refs = js_ext.extract(consumer_src, "consumer.ts", &ctx);

        // Barrel must produce both ExportName and ImportName for re-exported names
        let barrel_export_names: Vec<_> = barrel_refs.iter()
            .filter(|r| r.kind == kind::EXPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        let barrel_import_names: Vec<_> = barrel_refs.iter()
            .filter(|r| r.kind == kind::IMPORT_NAME)
            .map(|r| r.value.as_str())
            .collect();
        assert!(barrel_export_names.contains(&"computeScore"), "barrel missing ExportName computeScore");
        assert!(barrel_import_names.contains(&"computeScore"), "barrel missing ImportName computeScore");

        // Set up DB and insert refs via the same INSERT OR IGNORE used by scanner
        let db = make_db().await;
        let repo_id = seed_repo(&db, "/repo").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let barrel_id = seed_file(&db, repo_id, "src/barrel.ts").await;
        let consumer_id = seed_file(&db, repo_id, "src/consumer.ts").await;

        // Insert all refs using the real spans from extraction, through INSERT OR IGNORE.
        // This is the path that was dropping ImportName refs before the UNIQUE fix.
        for (file_id, refs) in [
            (utils_id, &utils_refs),
            (barrel_id, &barrel_refs),
            (consumer_id, &consumer_refs),
        ] {
            for r in refs.iter() {
                let string_id = seed_string(&db, &r.value).await;
                let parent_key_sid = match &r.parent_key {
                    Some(pk) => Some(seed_string(&db, pk).await),
                    None => None,
                };
                // Use INSERT OR IGNORE -- same as scanner flush and watcher replace_file_refs
                let ref_id = sqlx::query_scalar::<_, i64>(
                    "INSERT INTO refs (string_id, file_id, span_start, span_end, is_path, parent_key_string_id, node_path)
                     VALUES (?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(file_id, string_id, span_start) DO UPDATE SET span_end = excluded.span_end
                     RETURNING id",
                )
                .bind(string_id)
                .bind(file_id)
                .bind(r.span_start as i64)
                .bind(r.span_end as i64)
                .bind(r.is_path)
                .bind(parent_key_sid)
                .bind(r.node_path.as_deref())
                .fetch_one(&db)
                .await
                .unwrap();
                // Insert into per-rule _data table.
                let table = format!("{}_data", r.kind);
                let sql = format!(
                    "INSERT OR IGNORE INTO \"{table}\" (value_ref, value_str, repo_id, file_id, rev) \
                     VALUES (?, ?, (SELECT repo_id FROM files WHERE id = ?), ?, '')"
                );
                sqlx::query(&sql)
                    .bind(ref_id)
                    .bind(string_id)
                    .bind(file_id)
                    .bind(file_id)
                    .execute(&db)
                    .await
                    .unwrap();
            }
        }

        // Wire up ImportPath target_file_id links (barrel->utils, consumer->barrel)
        sqlx::query(
            "UPDATE refs SET target_file_id = ? WHERE file_id = ? AND id IN (SELECT r.id FROM refs r JOIN import_path_data d ON d.value_ref = r.id WHERE r.file_id = ? AND r.string_id IN (SELECT id FROM strings WHERE value = './utils'))"
        ).bind(utils_id).bind(barrel_id).bind(barrel_id).execute(&db).await.unwrap();
        sqlx::query(
            "UPDATE refs SET target_file_id = ? WHERE file_id = ? AND id IN (SELECT r.id FROM refs r JOIN import_path_data d ON d.value_ref = r.id WHERE r.file_id = ? AND r.string_id IN (SELECT id FROM strings WHERE value = './barrel'))"
        ).bind(barrel_id).bind(consumer_id).bind(consumer_id).execute(&db).await.unwrap();

        // Verify ImportName refs survived insertion
        let barrel_import_count: i64 = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM refs r JOIN import_name_data d ON d.value_ref = r.id WHERE r.file_id = ?"
        ).bind(barrel_id).fetch_one(&db).await.unwrap();
        assert!(barrel_import_count >= 2,
            "barrel.ts should have ImportName refs after INSERT OR IGNORE (got {barrel_import_count})");

        // Simulate rename: computeScore → calculateScore in utils.ts
        let changes = vec![Change::Decl(DeclChange::Rename {
            file_id: utils_id,
            kind: kind::EXPORT_NAME.to_string(),
            old_name: "computeScore".to_string(),
            new_name: "calculateScore".to_string(),
            new_span_start: 0,
            new_span_end: 0,
        })];

        let (edits, _rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();

        let edited_files: Vec<&str> = edits.iter().map(|e| e.file_path.as_str()).collect();
        assert!(edited_files.contains(&"/repo/src/barrel.ts"),
            "barrel.ts should be edited (re-export ImportName)");
        assert!(edited_files.contains(&"/repo/src/consumer.ts"),
            "consumer.ts should be edited (direct ImportName)");
        assert!(edits.iter().all(|e| e.new_value == "calculateScore"),
            "all edits should rename to calculateScore");
    }

    /// Kitchen sink: every form of Rust module reference across a workspace.
    ///
    /// - mod declaration in parent (lib.rs)
    /// - pub use re-export (relative, lib.rs)
    /// - simple use (ast.rs)
    /// - grouped import with multiple items (extractor.rs)
    /// - inline qualified path in fn signature (extractor.rs)
    /// - cross-crate use from another crate (consumer.rs)
    /// - cross-crate inline path in matches! macro (consumer.rs)
    /// - use inside function body (consumer.rs)
    #[tokio::test]
    async fn rust_workspace_file_move_kitchen_sink() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().to_string_lossy().to_string();

        // Create directory structure first
        let rules_dir = dir.path().join("crates/rules/src");
        let cli_dir = dir.path().join("crates/cli/src");
        std::fs::create_dir_all(&rules_dir).unwrap();
        std::fs::create_dir_all(&cli_dir).unwrap();

        // Workspace Cargo.toml so build_workspace_map can resolve crate names
        std::fs::write(dir.path().join("Cargo.toml"), "\
[workspace]\nmembers = [\"crates/rules\", \"crates/cli\"]\nresolver = \"2\"\n").unwrap();
        std::fs::write(dir.path().join("crates/rules/Cargo.toml"), "\
[package]\nname = \"sprefa_rules\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();
        std::fs::write(dir.path().join("crates/cli/Cargo.toml"), "\
[package]\nname = \"sprefa\"\nversion = \"0.1.0\"\nedition = \"2021\"\n").unwrap();

        let lib_src = "\
pub mod ast;
pub mod emit;
pub mod types;
pub mod walk;

pub use types::*;
";
        let ast_src = "\
use crate::types::AstSelector;
use crate::walk::MatchResult;
";
        let ext_src = "\
use crate::{
    ast, emit,
    types::{AstSelector, LineMatcher, MatchDef, RuleSet, SelectStep},
    walk,
};

fn compile_rule(r: &crate::types::Rule) -> anyhow::Result<()> {
    todo!()
}
";
        std::fs::write(rules_dir.join("lib.rs"), lib_src).unwrap();
        std::fs::write(rules_dir.join("types.rs"), "pub struct AstSelector;\npub struct Rule;").unwrap();
        std::fs::write(rules_dir.join("ast.rs"), ast_src).unwrap();
        std::fs::write(rules_dir.join("extractor.rs"), ext_src).unwrap();

        // Crate B: crates/cli/src/ -- cross-crate consumer
        let consumer_src = "\
use sprefa_rules::types::RuleSet;

fn check(s: &sprefa_rules::types::SelectStep) -> bool {
    matches!(s, sprefa_rules::types::SelectStep::File { .. })
}

fn inner() {
    use sprefa_rules::types::Rule;
    let _ = Rule;
}
";
        std::fs::write(cli_dir.join("main.rs"), consumer_src).unwrap();

        // Seed DB
        let db = make_db().await;
        let repo_id = seed_repo(&db, &repo_root).await;
        let lib_id = seed_file(&db, repo_id, "crates/rules/src/lib.rs").await;
        let types_id = seed_file(&db, repo_id, "crates/rules/src/types.rs").await;
        let ast_id = seed_file(&db, repo_id, "crates/rules/src/ast.rs").await;
        let ext_id = seed_file(&db, repo_id, "crates/rules/src/extractor.rs").await;
        let cli_id = seed_file(&db, repo_id, "crates/cli/src/main.rs").await;

        // lib.rs refs
        seed_ref(&db, lib_id, "types", kind::RS_MOD, None).await;
        seed_ref(&db, lib_id, "crate::types", kind::RS_USE, None).await;
        // ast.rs refs
        seed_ref(&db, ast_id, "crate::types::AstSelector", kind::RS_USE, None).await;
        // extractor.rs refs (grouped import -- DB stores full paths)
        seed_ref(&db, ext_id, "crate::types::AstSelector", kind::RS_USE, None).await;
        seed_ref(&db, ext_id, "crate::types::LineMatcher", kind::RS_USE, None).await;
        seed_ref(&db, ext_id, "crate::types::MatchDef", kind::RS_USE, None).await;
        seed_ref(&db, ext_id, "crate::types::RuleSet", kind::RS_USE, None).await;
        seed_ref(&db, ext_id, "crate::types::SelectStep", kind::RS_USE, None).await;
        // consumer refs (cross-crate)
        seed_ref(&db, cli_id, "sprefa_rules::types::RuleSet", kind::RS_USE, None).await;

        // Simulate: mv types.rs → _0_types.rs
        let old_path = format!("{}/crates/rules/src/types.rs", repo_root);
        let new_path = format!("{}/crates/rules/src/_0_types.rs", repo_root);
        let changes = vec![Change::Fs(FsChange::Move {
            file_id: types_id,
            old_path,
            new_path,
        })];

        let (edits, rust_rewrites) = plan_rewrites(&db, &changes, &[]).await.unwrap();
        assert!(edits.is_empty(), "Rust moves produce RustRewrites, not span Edits");
        assert!(!rust_rewrites.is_empty(), "should have Rust rewrites");

        let result = crate::rewrite::apply(&edits, &rust_rewrites);
        assert!(result.rust_failed.is_empty(), "no failures: {:?}", result.rust_failed);

        // ── lib.rs: mod decl + relative re-export ──
        let lib = std::fs::read_to_string(rules_dir.join("lib.rs")).unwrap();
        assert!(lib.contains("pub mod _0_types;"), "mod decl\n{lib}");
        assert!(lib.contains("pub use _0_types::*;"), "re-export\n{lib}");
        assert!(!lib.contains("pub mod types;"), "old mod gone\n{lib}");

        // ── ast.rs: simple use ──
        let ast = std::fs::read_to_string(rules_dir.join("ast.rs")).unwrap();
        assert!(ast.contains("crate::_0_types::AstSelector"), "simple use\n{ast}");

        // ── extractor.rs: grouped import + inline path in fn sig ──
        let ext = std::fs::read_to_string(rules_dir.join("extractor.rs")).unwrap();
        assert!(ext.contains("_0_types::{AstSelector"), "grouped import\n{ext}");
        assert!(ext.contains("crate::_0_types::Rule"), "inline fn sig path\n{ext}");
        assert!(ext.contains("ast, emit,"), "other group items untouched\n{ext}");

        // ── consumer (cross-crate): use, inline path, matches! macro, fn-body use ──
        let con = std::fs::read_to_string(cli_dir.join("main.rs")).unwrap();
        assert!(con.contains("use sprefa_rules::_0_types::RuleSet;"), "cross-crate use\n{con}");
        assert!(con.contains("sprefa_rules::_0_types::SelectStep) -> bool"), "cross-crate inline\n{con}");
        assert!(con.contains("sprefa_rules::_0_types::SelectStep::File"), "matches! macro\n{con}");
        assert!(con.contains("use sprefa_rules::_0_types::Rule;"), "fn-body use\n{con}");
        assert!(!con.contains("sprefa_rules::types::"), "all old paths gone\n{con}");
    }
}
