use std::collections::HashSet;

use sqlx::SqlitePool;

/// Fetch all already-scanned (repo_name, rev) pairs in one query.
pub async fn scanned_revs(pool: &SqlitePool) -> anyhow::Result<HashSet<(String, String)>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT r.name, rv.rev
        FROM repo_revs rv
        JOIN repos r ON rv.repo_id = r.id
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}
