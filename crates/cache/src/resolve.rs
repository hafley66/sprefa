use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use anyhow::Result;
use sqlx::SqlitePool;

use sprefa_schema::RefKind;

/// Resolve `refs.target_file_id` for all `ImportPath` refs in a repo that currently
/// have no target set.
///
/// Three-tier resolution:
///   1. Relative specifiers (starts with `.`) -- path join + extension probing
///   2. Bare specifiers -- lookup in `repo_packages` table
///
/// Updates `refs.target_file_id` in place. Idempotent: only touches NULL rows.
/// Returns the number of refs resolved.
pub async fn resolve_import_targets(db: &SqlitePool, repo_name: &str) -> Result<usize> {
    let repo_id: Option<i64> = sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
        .bind(repo_name)
        .fetch_optional(db)
        .await?;

    let Some(repo_id) = repo_id else {
        return Ok(0);
    };

    // file path -> file_id for this repo
    let file_rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, path FROM files WHERE repo_id = ?")
            .bind(repo_id)
            .fetch_all(db)
            .await?;
    let file_map: HashMap<String, i64> = file_rows.into_iter().map(|(id, p)| (p, id)).collect();

    // package_name -> manifest file_id for this repo (tier 2)
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

    // All unresolved ImportPath refs for this repo: (ref_id, specifier, importing_file_path)
    let import_path_kind = RefKind::ImportPath.as_u8() as i64;
    let unresolved: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT r.id, s.value, f.path
         FROM refs r
         JOIN files f ON r.file_id = f.id
         JOIN strings s ON r.string_id = s.id
         WHERE f.repo_id = ? AND r.ref_kind = ? AND r.target_file_id IS NULL",
    )
    .bind(repo_id)
    .bind(import_path_kind)
    .fetch_all(db)
    .await?;

    let mut updates: Vec<(i64, i64)> = Vec::new(); // (target_file_id, ref_id)
    for (ref_id, specifier, importing_path) in &unresolved {
        if let Some(target_id) =
            resolve_specifier(specifier, importing_path, &file_map, &pkg_map)
        {
            updates.push((target_id, *ref_id));
        }
    }

    let resolved = updates.len();

    // Bulk update in chunks. Each UPDATE is cheap (PK lookup).
    let mut tx = db.begin().await?;
    for chunk in updates.chunks(500) {
        for (target_id, ref_id) in chunk {
            sqlx::query("UPDATE refs SET target_file_id = ? WHERE id = ?")
                .bind(target_id)
                .bind(ref_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;

    tracing::debug!("{}: resolved {}/{} import targets", repo_name, resolved, unresolved.len());
    Ok(resolved)
}

fn resolve_specifier(
    specifier: &str,
    importing_path: &str,
    file_map: &HashMap<String, i64>,
    pkg_map: &HashMap<String, i64>,
) -> Option<i64> {
    if specifier.starts_with('.') {
        resolve_relative(specifier, importing_path, file_map)
    } else {
        resolve_bare(specifier, pkg_map)
    }
}

fn resolve_relative(
    specifier: &str,
    importing_path: &str,
    file_map: &HashMap<String, i64>,
) -> Option<i64> {
    let dir = Path::new(importing_path).parent()?;
    let joined = dir.join(specifier);

    // Normalize: collapse . and .. without hitting the filesystem
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::ParentDir => { normalized.pop(); }
            Component::CurDir => {}
            c => normalized.push(c),
        }
    }

    let base = normalized.to_string_lossy().replace('\\', "/");

    // Exact match (specifier already has extension)
    if let Some(&id) = file_map.get(base.as_str()) {
        return Some(id);
    }

    // Extension probing
    for ext in ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"] {
        let candidate = format!("{base}.{ext}");
        if let Some(&id) = file_map.get(candidate.as_str()) {
            return Some(id);
        }
    }

    // Directory index
    for ext in ["ts", "tsx", "js", "jsx"] {
        let candidate = format!("{base}/index.{ext}");
        if let Some(&id) = file_map.get(candidate.as_str()) {
            return Some(id);
        }
    }

    None
}

fn resolve_bare(specifier: &str, pkg_map: &HashMap<String, i64>) -> Option<i64> {
    // Extract package name, stripping subpaths.
    // @scope/pkg/sub -> @scope/pkg
    // pkg/sub -> pkg
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
    use sprefa_schema::{init_db, RefKind};

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
        sqlx::query_scalar(
            "INSERT INTO refs (string_id, file_id, span_start, span_end, is_path, ref_kind)
             VALUES (?, ?, 0, 0, 1, ?) RETURNING id",
        )
        .bind(string_id)
        .bind(file_id)
        .bind(RefKind::ImportPath.as_u8() as i64)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn target_file_id(db: &SqlitePool, ref_id: i64) -> Option<i64> {
        sqlx::query_scalar("SELECT target_file_id FROM refs WHERE id = ?")
            .bind(ref_id)
            .fetch_one(db)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn resolves_relative_with_extension_probe() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", "/app").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "./utils").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(utils_id));
    }

    #[tokio::test]
    async fn resolves_relative_with_parent_dir() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", "/app").await;
        let lib_id = seed_file(&db, repo_id, "lib/index.ts").await;
        let comp_id = seed_file(&db, repo_id, "src/components/Button.ts").await;
        let ref_id = seed_import_ref(&db, comp_id, "../../lib").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(lib_id));
    }

    #[tokio::test]
    async fn resolves_directory_index() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", "/app").await;
        let index_id = seed_file(&db, repo_id, "src/utils/index.ts").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "./utils").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, Some(index_id));
    }

    #[tokio::test]
    async fn unresolved_stays_null() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", "/app").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "./nonexistent").await;

        resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(target_file_id(&db, ref_id).await, None);
    }

    #[tokio::test]
    async fn resolves_bare_specifier_via_repo_packages() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", "/app").await;
        let pkg_file_id = seed_file(&db, repo_id, "node_modules/express/package.json").await;
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
    async fn resolves_scoped_package() {
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", "/app").await;
        let pkg_file_id =
            seed_file(&db, repo_id, "node_modules/@scope/ui/package.json").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        // subpath import -- should still resolve to the package manifest
        let ref_id = seed_import_ref(&db, app_id, "@scope/ui/Button").await;

        sqlx::query(
            "INSERT INTO repo_packages (repo_id, package_name, ecosystem, manifest_path)
             VALUES (?, '@scope/ui', 'npm', 'node_modules/@scope/ui/package.json')",
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
        let db = make_db().await;
        let repo_id = seed_repo(&db, "app", "/app").await;
        let utils_id = seed_file(&db, repo_id, "src/utils.ts").await;
        let app_id = seed_file(&db, repo_id, "src/app.ts").await;
        let ref_id = seed_import_ref(&db, app_id, "./utils").await;

        let first = resolve_import_targets(&db, "app").await.unwrap();
        let second = resolve_import_targets(&db, "app").await.unwrap();

        assert_eq!(first, 1);
        assert_eq!(second, 0); // already resolved, no NULL rows left
        assert_eq!(target_file_id(&db, ref_id).await, Some(utils_id));
    }
}
