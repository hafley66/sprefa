use std::collections::HashSet;

use anyhow::Result;
use sqlx::SqlitePool;

pub struct ScanContext {
    /// (rel_path, content_hash) pairs already in the DB that were scanned with
    /// the current binary hash.  Files in this set can skip extraction.
    pub skip_set: HashSet<(String, String)>,
}

/// Load the set of files for `repo_name` that were last scanned with
/// `scanner_hash`.  Returns an empty set if the repo does not exist yet.
pub async fn load_scan_context(
    db: &SqlitePool,
    repo_name: &str,
    scanner_hash: &str,
) -> Result<ScanContext> {
    let repo_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
            .bind(repo_name)
            .fetch_optional(db)
            .await?;

    let Some(repo_id) = repo_id else {
        return Ok(ScanContext { skip_set: HashSet::new() });
    };

    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT path, content_hash FROM files WHERE repo_id = ? AND scanner_hash = ?",
    )
    .bind(repo_id)
    .bind(scanner_hash)
    .fetch_all(db)
    .await?;

    Ok(ScanContext {
        skip_set: rows.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sprefa_schema::init_db;

    async fn make_db() -> SqlitePool {
        init_db(":memory:").await.unwrap()
    }

    async fn insert_repo(db: &SqlitePool, name: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO repos (name, root_path) VALUES (?, ?) RETURNING id",
        )
        .bind(name)
        .bind("/tmp/test")
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn insert_file(
        db: &SqlitePool,
        repo_id: i64,
        path: &str,
        content_hash: &str,
        scanner_hash: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO files (repo_id, path, content_hash, scanner_hash) VALUES (?, ?, ?, ?)",
        )
        .bind(repo_id)
        .bind(path)
        .bind(content_hash)
        .bind(scanner_hash)
        .execute(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn empty_when_repo_missing() {
        let db = make_db().await;
        let ctx = load_scan_context(&db, "nonexistent", "abc").await.unwrap();
        assert!(ctx.skip_set.is_empty());
    }

    #[tokio::test]
    async fn returns_files_with_matching_scanner_hash() {
        let db = make_db().await;
        let repo_id = insert_repo(&db, "myrepo").await;
        insert_file(&db, repo_id, "src/a.ts", "hash1", Some("binary-v1")).await;
        insert_file(&db, repo_id, "src/b.ts", "hash2", Some("binary-v1")).await;

        let ctx = load_scan_context(&db, "myrepo", "binary-v1").await.unwrap();
        assert_eq!(ctx.skip_set.len(), 2);
        assert!(ctx.skip_set.contains(&("src/a.ts".to_string(), "hash1".to_string())));
        assert!(ctx.skip_set.contains(&("src/b.ts".to_string(), "hash2".to_string())));
    }

    #[tokio::test]
    async fn excludes_different_scanner_hash() {
        let db = make_db().await;
        let repo_id = insert_repo(&db, "myrepo").await;
        insert_file(&db, repo_id, "src/a.ts", "hash1", Some("binary-v1")).await;
        insert_file(&db, repo_id, "src/b.ts", "hash2", Some("binary-v2")).await;
        insert_file(&db, repo_id, "src/c.ts", "hash3", None).await;

        let ctx = load_scan_context(&db, "myrepo", "binary-v1").await.unwrap();
        assert_eq!(ctx.skip_set.len(), 1);
        assert!(ctx.skip_set.contains(&("src/a.ts".to_string(), "hash1".to_string())));
    }

    #[tokio::test]
    async fn excludes_stale_content_hash() {
        // Same path, different content hash -- must NOT skip (file changed).
        let db = make_db().await;
        let repo_id = insert_repo(&db, "myrepo").await;
        // File was scanned at hash1; on disk it's now hash2.
        insert_file(&db, repo_id, "src/a.ts", "hash1", Some("binary-v1")).await;

        let ctx = load_scan_context(&db, "myrepo", "binary-v1").await.unwrap();
        // (path, hash2) is NOT in the skip set -- different content_hash.
        assert!(!ctx.skip_set.contains(&("src/a.ts".to_string(), "hash2".to_string())));
    }
}
