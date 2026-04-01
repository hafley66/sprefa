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

/// Check whether a (repo, rev) has already been scanned.
pub async fn is_rev_scanned(pool: &SqlitePool, repo_name: &str, rev: &str) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM repo_revs rv
        JOIN repos r ON rv.repo_id = r.id
        WHERE r.name = ? AND rv.rev = ?
        "#,
    )
    .bind(repo_name)
    .bind(rev)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Log a discovery event.
pub async fn log_discovery(
    pool: &SqlitePool,
    iteration: i32,
    target: &DiscoveryTarget,
    status: &str,
    files_scanned: Option<usize>,
    refs_inserted: Option<usize>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO discovery_log (iteration, source_repo, source_file, source_kind,
                                   target_repo, target_rev, status, files_scanned, refs_inserted)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(iteration)
    .bind(&target.source_repo)
    .bind(&target.source_file)
    .bind(&target.source_kind)
    .bind(&target.repo_name)
    .bind(&target.rev)
    .bind(status)
    .bind(files_scanned.map(|n| n as i64))
    .bind(refs_inserted.map(|n| n as i64))
    .execute(pool)
    .await?;
    Ok(())
}
