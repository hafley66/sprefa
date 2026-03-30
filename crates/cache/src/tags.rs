use anyhow::Result;
use sqlx::SqlitePool;

/// Upsert git tags for a repo. Resolves repo_id from name, then delegates
/// to the schema layer for bulk insert.
pub async fn flush_git_tags(
    db: &SqlitePool,
    repo_name: &str,
    tags: &[(String, Option<String>, bool)],
) -> Result<usize> {
    if tags.is_empty() {
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

    sprefa_schema::upsert_git_tags(db, repo_id, tags).await
}
