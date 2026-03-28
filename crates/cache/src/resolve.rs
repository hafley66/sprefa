use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use oxc_resolver::{ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences};
use sqlx::SqlitePool;


/// Resolve `refs.target_file_id` for all `ImportPath` refs in a repo that currently
/// have no target set.
///
/// Uses `oxc_resolver` for Node-compatible resolution:
///   - relative specifiers with extension probing and index files
///   - bare specifiers via node_modules
///   - tsconfig.json paths/baseUrl (auto-discovered)
///
/// Falls back to the `repo_packages` table for bare specifiers that
/// `oxc_resolver` can't find (e.g. workspace packages not in node_modules).
///
/// Updates `refs.target_file_id` in place. Idempotent: only touches NULL rows.
/// Returns the number of refs resolved.
#[tracing::instrument(skip(db), fields(repo = %repo_name))]
pub async fn resolve_import_targets(db: &SqlitePool, repo_name: &str) -> Result<usize> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, root_path FROM repos WHERE name = ?",
    )
    .bind(repo_name)
    .fetch_optional(db)
    .await?;

    let Some((repo_id, root_path_raw)) = row else {
        return Ok(0);
    };

    // Canonicalize root_path so it matches oxc_resolver's canonicalized output.
    // (e.g. on macOS, /var -> /private/var)
    let root_path = std::fs::canonicalize(&root_path_raw)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or(root_path_raw);

    // file path (relative) -> file_id
    let file_rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, path FROM files WHERE repo_id = ?")
            .bind(repo_id)
            .fetch_all(db)
            .await?;
    let file_map: HashMap<String, i64> = file_rows.into_iter().map(|(id, p)| (p, id)).collect();

    // Reverse map: absolute path -> file_id (for oxc_resolver results)
    let abs_map: HashMap<String, i64> = file_map
        .iter()
        .map(|(rel, &id)| (format!("{}/{}", root_path, rel), id))
        .collect();

    // Bare specifier fallback: package_name -> manifest file_id
    let pkg_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT p.package_name, f.id
         FROM repo_packages p
         JOIN files f ON f.repo_id = p.repo_id AND f.path = p.manifest_path
         WHERE p.repo_id = ?",
    )
    .bind(repo_id)
    .fetch_all(db)
    .await?;
    let pkg_map: HashMap<String, i64> = pkg_rows.into_iter().collect();

    // All unresolved ImportPath refs: (ref_id, specifier, importing_file_relative_path)
    let unresolved: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT r.id, s.value, f.path
         FROM refs r
         JOIN files f ON r.file_id = f.id
         JOIN strings s ON r.string_id = s.id
         JOIN matches_v2 m ON m.ref_id = r.id
         WHERE f.repo_id = ? AND m.kind = 'import_path' AND r.target_file_id IS NULL",
    )
    .bind(repo_id)
    .fetch_all(db)
    .await?;

    // Build resolver with tsconfig support.
    // This is a synchronous, CPU-bound operation so it's fine on the async thread
    // (oxc_resolver does filesystem reads internally but they're fast stat calls).
    let resolver = build_resolver(&root_path);

    let mut updates: Vec<(i64, i64)> = Vec::new();
    for (ref_id, specifier, importing_rel_path) in &unresolved {
        let importing_abs = format!("{}/{}", root_path, importing_rel_path);
        let importing_dir = Path::new(&importing_abs)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| root_path.clone());

        // Try oxc_resolver first (handles relative, bare, tsconfig paths)
        if let Some(target_id) = resolve_via_oxc(&resolver, &importing_dir, specifier, &abs_map) {
            updates.push((target_id, *ref_id));
            continue;
        }

        // Fallback: bare specifier -> repo_packages table
        if !specifier.starts_with('.') {
            if let Some(target_id) = resolve_bare_fallback(specifier, &pkg_map) {
                updates.push((target_id, *ref_id));
            }
        }
    }

    let resolved = updates.len();

    let mut tx = db.begin().await?;
    for chunk in updates.chunks(500) {
        // Batch UPDATE via CASE/WHEN: one statement per chunk instead of per row.
        let whens = chunk.iter().map(|_| "WHEN id = ? THEN ?").collect::<Vec<_>>().join(" ");
        let ids = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE refs SET target_file_id = CASE {} END WHERE id IN ({})",
            whens, ids
        );
        let mut q = sqlx::query(&sql);
        for (target_id, ref_id) in chunk {
            q = q.bind(ref_id).bind(target_id);
        }
        for (_target_id, ref_id) in chunk {
            q = q.bind(ref_id);
        }
        q.execute(&mut *tx).await?;
    }
    tx.commit().await?;

    tracing::debug!("{}: resolved {}/{} import targets", repo_name, resolved, unresolved.len());
    Ok(resolved)
}

fn build_resolver(root_path: &str) -> Resolver {
    let options = ResolveOptions {
        extensions: vec![
            ".ts".into(),
            ".tsx".into(),
            ".js".into(),
            ".jsx".into(),
            ".mjs".into(),
            ".cjs".into(),
            ".mts".into(),
            ".cts".into(),
            ".json".into(),
        ],
        main_fields: vec!["module".into(), "main".into()],
        condition_names: vec!["import".into(), "require".into(), "default".into()],
        tsconfig: {
            let tsconfig_path = Path::new(root_path).join("tsconfig.json");
            if tsconfig_path.exists() {
                Some(TsconfigDiscovery::Manual(TsconfigOptions {
                    config_file: tsconfig_path.into(),
                    references: TsconfigReferences::Auto,
                }))
            } else {
                None
            }
        },
        ..ResolveOptions::default()
    };
    Resolver::new(options)
}

/// Resolve a specifier using oxc_resolver, then map the result to a file_id.
fn resolve_via_oxc(
    resolver: &Resolver,
    importing_dir: &str,
    specifier: &str,
    abs_map: &HashMap<String, i64>,
) -> Option<i64> {
    let result = resolver.resolve(importing_dir, specifier).ok()?;
    let resolved_path = result.full_path().to_string_lossy().to_string();

    // Direct lookup
    if let Some(&id) = abs_map.get(&resolved_path) {
        return Some(id);
    }

    // oxc_resolver may return a non-canonical path. Try canonicalizing.
    if let Ok(canonical) = std::fs::canonicalize(&resolved_path) {
        let canonical_str = canonical.to_string_lossy().to_string();
        if let Some(&id) = abs_map.get(&canonical_str) {
            return Some(id);
        }
    }

    None
}

/// Fallback for bare specifiers not resolvable by oxc_resolver
/// (e.g. workspace packages registered in repo_packages but not in node_modules).
fn resolve_bare_fallback(specifier: &str, pkg_map: &HashMap<String, i64>) -> Option<i64> {
    let pkg_name = if specifier.starts_with('@') {
        let mut parts = specifier.splitn(3, '/');
        let scope = parts.next()?;
        let name = parts.next()?;
        format!("{scope}/{name}")
    } else {
        specifier.split('/').next()?.to_string()
    };

    pkg_map.get(&pkg_name).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_schema::init_db;

    async fn make_db() -> SqlitePool {
        init_db(":memory:").await.unwrap()
    }

    async fn seed_repo(db: &SqlitePool, name: &str, path: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO repos (name, root_path) VALUES (?, ?) RETURNING id",
        )
        .bind(name)
        .bind(path)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn seed_file(db: &SqlitePool, repo_id: i64, path: &str) -> i64 {
        sqlx::query_scalar(
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
        sqlx::query_scalar("SELECT id FROM strings WHERE value = ?")
            .bind(value)
            .fetch_one(db)
            .await
            .unwrap()
    }

    async fn seed_import_ref(db: &SqlitePool, file_id: i64, specifier: &str) -> i64 {
        let string_id = seed_string(db, specifier).await;
        let ref_id: i64 = sqlx::query_scalar(
            "INSERT INTO refs (string_id, file_id, span_start, span_end, is_path)
             VALUES (?, ?, 0, 0, 1) RETURNING id",
        )
        .bind(string_id)
        .bind(file_id)
        .fetch_one(db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO matches_v2 (ref_id, rule_name, kind) VALUES (?, 'test', 'import_path')",
        )
        .bind(ref_id)
        .execute(db)
        .await
        .unwrap();
        ref_id
    }

    async fn target_file_id(db: &SqlitePool, ref_id: i64) -> Option<i64> {
        sqlx::query_scalar("SELECT target_file_id FROM refs WHERE id = ?")
            .bind(ref_id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    // -- oxc_resolver resolves against real filesystem, so the tests that used
    // -- a pure HashMap-based resolver (DB paths only, no files on disk) no longer
    // -- work without real files. Keep the bare-specifier fallback test and the
    // -- idempotency test. For relative resolution, use tempdir with real files.

    #[tokio::test]
    async fn resolves_relative_with_real_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // Create src/app.ts and src/utils.ts
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/app.ts"), "import './utils';\n").unwrap();
        std::fs::write(root.join("src/utils.ts"), "export const x = 1;\n").unwrap();

        let root_str = root.to_string_lossy().to_string();

        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", &root_str).await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "./utils").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(utils_id));
    }

    #[tokio::test]
    async fn resolves_directory_index_with_real_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("src/utils")).unwrap();
        std::fs::write(root.join("src/app.ts"), "import './utils';\n").unwrap();
        std::fs::write(root.join("src/utils/index.ts"), "export const x = 1;\n").unwrap();

        let root_str = root.to_string_lossy().to_string();

        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", &root_str).await;
        let index_id = seed_file(&db, repo_id, "src/utils/index.ts").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "./utils").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(index_id));
    }

    #[tokio::test]
    async fn resolves_tsconfig_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        // tsconfig with paths alias
        std::fs::write(
            root.join("tsconfig.json"),
            r#"{
                "compilerOptions": {
                    "baseUrl": ".",
                    "paths": {
                        "@lib/*": ["src/lib/*"]
                    }
                }
            }"#,
        )
        .unwrap();

        std::fs::create_dir_all(root.join("src/lib")).unwrap();
        std::fs::create_dir_all(root.join("src/app")).unwrap();
        std::fs::write(root.join("src/lib/utils.ts"), "export const x = 1;\n").unwrap();
        std::fs::write(root.join("src/app/main.ts"), "import '@lib/utils';\n").unwrap();

        let root_str = root.to_string_lossy().to_string();

        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", &root_str).await;
        let utils_id = seed_file(&db, repo_id, "src/lib/utils.ts").await;
        let app_id = seed_file(&db, repo_id, "src/app/main.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "@lib/utils").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(utils_id));
    }

    #[tokio::test]
    async fn resolves_parent_dir_with_real_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("lib")).unwrap();
        std::fs::create_dir_all(root.join("src/components")).unwrap();
        std::fs::write(root.join("lib/index.ts"), "export const x = 1;\n").unwrap();
        std::fs::write(root.join("src/components/Button.ts"), "import '../../lib';\n").unwrap();

        let root_str = root.to_string_lossy().to_string();

        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", &root_str).await;
        let lib_id = seed_file(&db, repo_id, "lib/index.ts").await;
        let comp_id = seed_file(&db, repo_id, "src/components/Button.ts").await;
        let ref_id = seed_import_ref(&db, comp_id, "../../lib").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(lib_id));
    }

    #[tokio::test]
    async fn unresolved_stays_null() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/app.ts"), "import './nonexistent';\n").unwrap();

        let root_str = root.to_string_lossy().to_string();

        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", &root_str).await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "./nonexistent").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, None);
    }

    #[tokio::test]
    async fn resolves_bare_specifier_via_repo_packages() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/app.ts"), "import 'express';\n").unwrap();

        let root_str = root.to_string_lossy().to_string();

        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", &root_str).await;
        let pkg_file_id = seed_file(
            &db,
            repo_id,
            "node_modules/express/package.json",
        )
        .await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "express").await;

        sqlx::query(
            "INSERT INTO repo_packages (repo_id, package_name, ecosystem, manifest_path)
             VALUES (?, 'express', 'npm', 'node_modules/express/package.json')",
        )
        .bind(repo_id)
        .execute(&db)
        .await
        .unwrap();

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(pkg_file_id));
    }

    #[tokio::test]
    async fn idempotent_on_already_resolved() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/app.ts"), "import './utils';\n").unwrap();
        std::fs::write(root.join("src/utils.ts"), "export const x = 1;\n").unwrap();

        let root_str = root.to_string_lossy().to_string();

        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", &root_str).await;
        let _utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let _ref_id = seed_import_ref(&db, app_id, "./utils").await;

        let first = resolve_import_targets(&db, "app").await.unwrap();
        let second = resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 0);
    }
}
