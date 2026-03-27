use sqlx::SqlitePool;

use std::collections::HashMap;

use crate::{BranchScope, QueryHit, RefLocation, Repo, StringRow};

// -- repos --

pub async fn upsert_repo(pool: &SqlitePool, name: &str, root_path: &str) -> anyhow::Result<i64> {
    let result = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO repos (name, root_path)
        VALUES (?, ?)
        ON CONFLICT(name) DO UPDATE SET root_path = excluded.root_path
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(root_path)
    .fetch_one(pool)
    .await?;
    Ok(result)
}

pub async fn get_repo_by_name(pool: &SqlitePool, name: &str) -> anyhow::Result<Option<Repo>> {
    let row = sqlx::query_as::<_, (i64, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT id, name, root_path, org, git_hash, last_fetched_at, last_synced_at, last_remote_commit_at, scanned_at FROM repos WHERE name = ?"
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| Repo {
        id: r.0,
        name: r.1,
        root_path: r.2,
        org: r.3,
        git_hash: r.4,
        last_fetched_at: r.5,
        last_synced_at: r.6,
        last_remote_commit_at: r.7,
        scanned_at: r.8,
    }))
}

pub async fn list_repos(pool: &SqlitePool) -> anyhow::Result<Vec<Repo>> {
    let rows = sqlx::query_as::<_, (i64, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>)>(
        "SELECT id, name, root_path, org, git_hash, last_fetched_at, last_synced_at, last_remote_commit_at, scanned_at FROM repos ORDER BY name"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| Repo {
        id: r.0,
        name: r.1,
        root_path: r.2,
        org: r.3,
        git_hash: r.4,
        last_fetched_at: r.5,
        last_synced_at: r.6,
        last_remote_commit_at: r.7,
        scanned_at: r.8,
    }).collect())
}

// -- files --

pub async fn upsert_file(
    pool: &SqlitePool,
    repo_id: i64,
    path: &str,
    content_hash: &str,
    stem: Option<&str>,
    ext: Option<&str>,
) -> anyhow::Result<i64> {
    let result = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO files (repo_id, path, content_hash, stem, ext)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(repo_id, path, content_hash) DO UPDATE SET scanned_at = NULL
        RETURNING id
        "#,
    )
    .bind(repo_id)
    .bind(path)
    .bind(content_hash)
    .bind(stem)
    .bind(ext)
    .fetch_one(pool)
    .await?;
    Ok(result)
}

pub async fn count_files_for_repo(pool: &SqlitePool, repo_id: i64) -> anyhow::Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM files WHERE repo_id = ?"
    )
    .bind(repo_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

// -- strings --

pub async fn upsert_string(
    pool: &SqlitePool,
    value: &str,
    norm: &str,
    norm2: Option<&str>,
) -> anyhow::Result<i64> {
    let result = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO strings (value, norm, norm2)
        VALUES (?, ?, ?)
        ON CONFLICT(value) DO UPDATE SET norm = excluded.norm, norm2 = excluded.norm2
        RETURNING id
        "#,
    )
    .bind(value)
    .bind(norm)
    .bind(norm2)
    .fetch_one(pool)
    .await?;
    Ok(result)
}

/// FTS5 trigram substring search on normalized strings.
pub async fn search_strings(pool: &SqlitePool, query: &str) -> anyhow::Result<Vec<StringRow>> {
    let rows = sqlx::query_as::<_, (i64, String, String, Option<String>)>(
        r#"
        SELECT s.id, s.value, s.norm, s.norm2
        FROM strings_fts fts
        JOIN strings s ON s.id = fts.rowid
        WHERE fts.norm MATCH ?
        ORDER BY rank
        LIMIT 100
        "#,
    )
    .bind(format!("\"{}\"", query))  // trigram requires quoted phrase
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| StringRow {
        id: r.0,
        value: r.1,
        norm: r.2,
        norm2: r.3,
    }).collect())
}

/// FTS5 trigram search returning matched strings grouped with all their ref locations.
///
/// When `scope` is `None` or `Some(All)`, returns all refs (no branch filtering).
/// When `Committed`, only refs whose file belongs to a non-wt branch.
/// When `Local`, only refs whose file belongs to a +wt branch.
pub async fn search_refs(
    pool: &SqlitePool,
    query: &str,
    scope: Option<BranchScope>,
) -> anyhow::Result<Vec<QueryHit>> {
    let scope = scope.unwrap_or(BranchScope::Committed);

    let (branch_join, branch_where) = match scope {
        BranchScope::All => ("", ""),
        BranchScope::Committed => (
            "JOIN branch_files bf ON bf.file_id = f.id AND bf.repo_id = f.repo_id \
             JOIN repo_branches rb ON rb.repo_id = bf.repo_id AND rb.branch = bf.branch",
            "AND rb.is_working_tree = 0",
        ),
        BranchScope::Local => (
            "JOIN branch_files bf ON bf.file_id = f.id AND bf.repo_id = f.repo_id \
             JOIN repo_branches rb ON rb.repo_id = bf.repo_id AND rb.branch = bf.branch",
            "AND rb.is_working_tree = 1",
        ),
    };

    let sql = format!(
        r#"
        SELECT DISTINCT s.id, s.value, s.norm,
               r.span_start, r.span_end, r.ref_kind,
               f.path AS file_path,
               repos.name AS repo_name
        FROM strings_fts fts
        JOIN strings s ON s.id = fts.rowid
        JOIN refs r ON r.string_id = s.id
        JOIN files f ON r.file_id = f.id
        JOIN repos ON f.repo_id = repos.id
        {branch_join}
        WHERE fts.norm MATCH ?
        {branch_where}
        ORDER BY rank, repos.name, f.path
        LIMIT 500
        "#,
    );

    let rows: Vec<(i64, String, String, i64, i64, i64, String, String)> = sqlx::query_as(&sql)
        .bind(format!("\"{}\"", query))
        .fetch_all(pool)
        .await?;

    // Group rows by string_id, preserving rank order of first occurrence.
    let mut hits: Vec<QueryHit> = Vec::new();
    let mut id_to_idx: HashMap<i64, usize> = HashMap::new();

    for (string_id, value, norm, span_start, span_end, ref_kind, file_path, repo_name) in rows {
        let idx = *id_to_idx.entry(string_id).or_insert_with(|| {
            let idx = hits.len();
            hits.push(QueryHit { string_id, value, norm, refs: Vec::new() });
            idx
        });
        hits[idx].refs.push(RefLocation {
            repo: repo_name,
            file_path,
            ref_kind: ref_kind as u8,
            span_start,
            span_end,
        });
    }

    Ok(hits)
}

// -- refs --

pub async fn insert_ref(
    pool: &SqlitePool,
    string_id: i64,
    file_id: i64,
    span_start: i64,
    span_end: i64,
    is_path: bool,
    ref_kind: u8,
    parent_key_string_id: Option<i64>,
    node_path: Option<&str>,
) -> anyhow::Result<i64> {
    let result = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO refs (string_id, file_id, span_start, span_end, is_path, ref_kind, parent_key_string_id, node_path)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(file_id, string_id, span_start) DO NOTHING
        RETURNING id
        "#,
    )
    .bind(string_id)
    .bind(file_id)
    .bind(span_start)
    .bind(span_end)
    .bind(is_path)
    .bind(ref_kind)
    .bind(parent_key_string_id)
    .bind(node_path)
    .fetch_optional(pool)
    .await?;
    Ok(result.unwrap_or(0))
}

pub async fn count_refs_for_repo(pool: &SqlitePool, repo_id: i64) -> anyhow::Result<i64> {
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM refs r JOIN files f ON r.file_id = f.id WHERE f.repo_id = ?"
    )
    .bind(repo_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

// -- branch_files --

pub async fn upsert_branch_file(
    pool: &SqlitePool,
    repo_id: i64,
    branch: &str,
    file_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO branch_files (repo_id, branch, file_id)
        VALUES (?, ?, ?)
        ON CONFLICT(repo_id, branch, file_id) DO NOTHING
        "#,
    )
    .bind(repo_id)
    .bind(branch)
    .bind(file_id)
    .execute(pool)
    .await?;
    Ok(())
}

// -- repo_branches --

pub async fn upsert_repo_branch(
    pool: &SqlitePool,
    repo_id: i64,
    branch: &str,
    git_hash: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO repo_branches (repo_id, branch, git_hash)
        VALUES (?, ?, ?)
        ON CONFLICT(repo_id, branch) DO UPDATE SET git_hash = excluded.git_hash
        "#,
    )
    .bind(repo_id)
    .bind(branch)
    .bind(git_hash)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_repo_branch_hash(
    pool: &SqlitePool,
    repo_id: i64,
    branch: &str,
) -> anyhow::Result<Option<String>> {
    let hash = sqlx::query_scalar::<_, String>(
        "SELECT git_hash FROM repo_branches WHERE repo_id = ? AND branch = ?"
    )
    .bind(repo_id)
    .bind(branch)
    .fetch_optional(pool)
    .await?;
    Ok(hash)
}
