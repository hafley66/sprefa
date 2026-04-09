use anyhow::Result;
use sqlx::SqlitePool;

const FILE_CHUNK: usize = 2000;

/// Remove files that were deleted in a git diff. Cascades through
/// rev_files, per-rule _data tables, refs, and files.
///
/// Uses a temp table to resolve file IDs once, then four non-looping
/// DELETEs that join against it.
pub async fn delete_rev_files_by_paths(
    db: &SqlitePool,
    repo_name: &str,
    branch: &str,
    deleted_paths: &[String],
) -> Result<usize> {
    if deleted_paths.is_empty() {
        return Ok(0);
    }

    let repo_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
            .bind(repo_name)
            .fetch_optional(db)
            .await?;

    let Some(repo_id) = repo_id else {
        return Ok(0);
    };

    let mut tx = db.begin().await?;

    sqlx::query("CREATE TEMP TABLE _dead_files (id INTEGER PRIMARY KEY)")
        .execute(&mut *tx).await?;

    for chunk in deleted_paths.chunks(FILE_CHUNK) {
        let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT INTO _dead_files (id) SELECT id FROM files WHERE repo_id = ? AND path IN ({ph})"
        );
        let mut q = sqlx::query(&sql);
        q = q.bind(repo_id);
        for path in chunk { q = q.bind(path.as_str()); }
        q.execute(&mut *tx).await?;
    }

    // Delete per-rule table rows for dead files.
    for k in sprefa_schema::rule_tables::BUILTIN_KINDS {
        let sql = format!(
            "DELETE FROM \"{k}_data\" WHERE file_id IN (SELECT id FROM _dead_files)"
        );
        let _ = sqlx::query(&sql).execute(&mut *tx).await;
    }

    sqlx::query(
        "DELETE FROM refs WHERE file_id IN (SELECT id FROM _dead_files)"
    ).execute(&mut *tx).await?;

    sqlx::query(
        "DELETE FROM rev_files WHERE repo_id = ? AND rev = ? AND file_id IN (SELECT id FROM _dead_files)"
    ).bind(repo_id).bind(branch).execute(&mut *tx).await?;
    let deleted: i64 = sqlx::query_scalar("SELECT changes()")
        .fetch_one(&mut *tx).await?;

    sqlx::query(
        "DELETE FROM files WHERE id IN (SELECT id FROM _dead_files)"
    ).execute(&mut *tx).await?;

    sqlx::query("DROP TABLE _dead_files").execute(&mut *tx).await?;

    tx.commit().await?;
    Ok(deleted as usize)
}

/// Update file paths for pure renames (same content, different path).
/// Preserves file_id and refs -- only the path column changes.
/// Returns the number of files updated.
///
/// Uses a temp table to batch all renames, then a single UPDATE ... FROM join.
pub async fn rename_file_paths(
    db: &SqlitePool,
    repo_name: &str,
    renames: &[(String, String)],
) -> Result<usize> {
    if renames.is_empty() {
        return Ok(0);
    }

    let repo_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
            .bind(repo_name)
            .fetch_optional(db)
            .await?;

    let Some(repo_id) = repo_id else {
        return Ok(0);
    };

    struct Rename {
        old_path: String,
        new_path: String,
        new_stem: Option<String>,
        new_ext: Option<String>,
    }
    let rows: Vec<Rename> = renames.iter().map(|(old, new)| {
        let new_stem = new.rsplit('/').next()
            .and_then(|n| n.split('.').next())
            .map(String::from);
        let new_ext = new.rsplit('.').next()
            .filter(|_| new.contains('.'))
            .map(String::from);
        Rename { old_path: old.clone(), new_path: new.clone(), new_stem, new_ext }
    }).collect();

    let mut tx = db.begin().await?;

    sqlx::query(
        "CREATE TEMP TABLE _renames (old_path TEXT, new_path TEXT, new_stem TEXT, new_ext TEXT)"
    ).execute(&mut *tx).await?;

    for chunk in rows.chunks(FILE_CHUNK) {
        let ph = chunk.iter().map(|_| "(?,?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!("INSERT INTO _renames VALUES {ph}");
        let mut q = sqlx::query(&sql);
        for r in chunk {
            q = q.bind(&r.old_path).bind(&r.new_path).bind(r.new_stem.as_deref()).bind(r.new_ext.as_deref());
        }
        q.execute(&mut *tx).await?;
    }

    sqlx::query(
        "UPDATE files SET
            path = _renames.new_path,
            stem = _renames.new_stem,
            ext  = _renames.new_ext
         FROM _renames
         WHERE files.repo_id = ? AND files.path = _renames.old_path"
    ).bind(repo_id).execute(&mut *tx).await?;
    let updated: i64 = sqlx::query_scalar("SELECT changes()")
        .fetch_one(&mut *tx).await?;

    sqlx::query("DROP TABLE _renames").execute(&mut *tx).await?;
    tx.commit().await?;

    Ok(updated as usize)
}
