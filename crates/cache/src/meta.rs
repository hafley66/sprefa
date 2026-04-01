use anyhow::Result;
use sqlx::SqlitePool;

use sprefa_index::{GitRev, normalize};

const STR_CHUNK: usize = 2000;
const META_CHUNK: usize = 2000;

/// Intern repo-level metadata strings and create repo_refs + matches.
///
/// Inserts `repo_name`, `repo_org`, and each git rev as
/// repo-anchored refs that participate in the match_links system.
pub async fn flush_repo_meta(
    db: &SqlitePool,
    repo_name: &str,
    org: Option<&str>,
    revs: &[GitRev],
) -> Result<usize> {
    let repo_id: Option<i64> =
        sqlx::query_scalar("SELECT id FROM repos WHERE name = ?")
            .bind(repo_name)
            .fetch_optional(db)
            .await?;

    let Some(repo_id) = repo_id else {
        return Ok(0);
    };

    // Build entity list: (value, kind)
    let mut entities: Vec<(&str, &str)> = Vec::new();
    entities.push((repo_name, "repo_name"));
    if let Some(org) = org {
        entities.push((org, "repo_org"));
    }
    for rev in revs {
        let kind = if rev.is_tag { "git_tag" } else { "branch_name" };
        entities.push((&rev.name, kind));
    }

    if entities.is_empty() {
        return Ok(0);
    }

    let mut tx = db.begin().await?;

    // Intern strings.
    let unique_values: Vec<&str> = {
        let mut seen = std::collections::HashSet::new();
        entities.iter().filter_map(|(v, _)| {
            if seen.insert(*v) { Some(*v) } else { None }
        }).collect()
    };

    for chunk in unique_values.chunks(STR_CHUNK) {
        let ph = chunk.iter().map(|_| "(?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!("INSERT OR IGNORE INTO strings (value, norm, norm2) VALUES {ph}");
        let mut q = sqlx::query(&sql);
        for v in chunk {
            q = q.bind(*v).bind(normalize(v)).bind(None::<&str>);
        }
        q.execute(&mut *tx).await?;
    }

    // Read back string IDs.
    let mut string_id_map = std::collections::HashMap::new();
    for chunk in unique_values.chunks(STR_CHUNK) {
        let ph = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("SELECT id, value FROM strings WHERE value IN ({ph})");
        let mut q = sqlx::query_as::<_, (i64, String)>(&sql);
        for v in chunk { q = q.bind(*v); }
        for (id, value) in q.fetch_all(&mut *tx).await? {
            string_id_map.insert(value, id);
        }
    }

    // Insert repo_refs.
    for chunk in entities.chunks(META_CHUNK) {
        let ph = chunk.iter().map(|_| "(?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT OR IGNORE INTO repo_refs (string_id, repo_id, kind) VALUES {ph}"
        );
        let mut q = sqlx::query(&sql);
        for (value, kind) in chunk {
            let string_id = string_id_map.get(*value).copied().unwrap_or(0);
            q = q.bind(string_id).bind(repo_id).bind(*kind);
        }
        q.execute(&mut *tx).await?;
    }

    // Read back repo_ref IDs.
    let repo_ref_rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT rr.id, rr.kind FROM repo_refs rr WHERE rr.repo_id = ?",
    )
    .bind(repo_id)
    .fetch_all(&mut *tx)
    .await?;

    // Insert matches for each repo_ref.
    let mut total = 0usize;
    for chunk in repo_ref_rows.chunks(META_CHUNK) {
        let ph = chunk.iter().map(|_| "(NULL,?,?,?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT OR IGNORE INTO matches (ref_id, repo_ref_id, rule_name, kind) VALUES {ph}"
        );
        let mut q = sqlx::query(&sql);
        for (rr_id, kind) in chunk {
            q = q.bind(rr_id).bind("__meta__").bind(kind.as_str());
        }
        let r = q.execute(&mut *tx).await?;
        total += r.rows_affected() as usize;
    }

    tx.commit().await?;

    if total > 0 {
        tracing::debug!("{}: {} repo meta matches created", repo_name, total);
    }
    Ok(total)
}
