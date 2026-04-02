use std::collections::HashSet;

use sqlx::SqlitePool;

/// A discovered (repo, rev) target from match_labels annotations.
#[derive(Debug, Clone)]
pub struct DiscoveryTarget {
    pub repo_name: String,
    pub rev: String,
    pub source_repo: String,
    pub source_file: Option<String>,
    pub source_kind: Option<String>,
}

/// Entry for batch-inserting discovery log rows.
pub struct DiscoveryLogEntry<'a> {
    pub iteration: i32,
    pub target: &'a DiscoveryTarget,
    pub status: &'a str,
    pub files_scanned: Option<usize>,
    pub refs_inserted: Option<usize>,
}

/// Query match_labels for (repo, rev) pairs linked by IS_REPO/IS_REV annotations.
///
/// Finds refs with scan=rev, then joins to a sibling ref in the same file
/// with scan=repo. Siblings are matched by file_id -- both the repo and rev
/// captures come from the same source file (e.g. values.yaml).
pub async fn discover_scan_targets(pool: &SqlitePool) -> anyhow::Result<Vec<DiscoveryTarget>> {
    let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>)>(
        r#"
        SELECT
            repo_s.value AS repo_name,
            rev_s.value AS rev_value,
            rp.name AS source_repo,
            f.path AS source_file,
            rev_m.kind AS source_kind
        FROM match_labels rev_ml
        JOIN matches rev_m ON rev_ml.match_id = rev_m.id
            AND rev_ml.key = 'scan' AND rev_ml.value = 'rev'
        JOIN refs rev_r ON rev_m.ref_id = rev_r.id
        JOIN strings rev_s ON rev_r.string_id = rev_s.id
        JOIN files f ON rev_r.file_id = f.id
        JOIN repos rp ON f.repo_id = rp.id
        -- find sibling ref in same file with scan=repo
        JOIN refs repo_r ON rev_r.file_id = repo_r.file_id
        JOIN matches repo_m ON repo_m.ref_id = repo_r.id
        JOIN match_labels repo_ml ON repo_m.id = repo_ml.match_id
            AND repo_ml.key = 'scan' AND repo_ml.value = 'repo'
        JOIN strings repo_s ON repo_r.string_id = repo_s.id
        GROUP BY repo_name, rev_value
        "#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(repo_name, rev, source_repo, source_file, source_kind)| DiscoveryTarget {
            repo_name,
            rev,
            source_repo,
            source_file,
            source_kind,
        })
        .collect())
}

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

/// Batch-insert discovery log entries.
pub async fn log_discovery_batch(
    pool: &SqlitePool,
    entries: &[DiscoveryLogEntry<'_>],
) -> anyhow::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    // Build a single INSERT with multiple VALUE tuples.
    let mut sql = String::from(
        "INSERT INTO discovery_log (iteration, source_repo, source_file, source_kind, \
         target_repo, target_rev, status, files_scanned, refs_inserted) VALUES ",
    );
    let mut first = true;
    for _ in entries {
        if !first {
            sql.push(',');
        }
        sql.push_str("(?,?,?,?,?,?,?,?,?)");
        first = false;
    }

    let mut q = sqlx::query(&sql);
    for e in entries {
        q = q
            .bind(e.iteration)
            .bind(&e.target.source_repo)
            .bind(&e.target.source_file)
            .bind(&e.target.source_kind)
            .bind(&e.target.repo_name)
            .bind(&e.target.rev)
            .bind(e.status)
            .bind(e.files_scanned.map(|n| n as i64))
            .bind(e.refs_inserted.map(|n| n as i64));
    }
    q.execute(pool).await?;
    Ok(())
}
