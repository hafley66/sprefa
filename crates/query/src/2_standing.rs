use sqlx::SqlitePool;

use crate::{Expr, Hit, HitSet};

/// A standing rule: a named URTSL expression that re-evaluates on index changes.
#[derive(Debug, Clone)]
pub struct StandingRule {
    pub name: String,
    pub expr: Expr,
    /// The kind string to write into matches for hits produced by this rule.
    /// e.g. "standing", "stale_ref", or whatever semantic label fits.
    pub kind: String,
    /// Hash of the serialized expression. Used to detect when a rule changes
    /// and its matches need to be invalidated.
    pub rule_hash: String,
}

/// Result of evaluating a standing rule: which refs matched.
#[derive(Debug)]
pub struct RuleEvalResult {
    pub rule_name: String,
    pub new_matches: usize,
    pub stale_removed: usize,
}

/// Store a standing rule in the DB. Overwrites if the name already exists
/// and the rule_hash differs (invalidates old matches).
pub async fn upsert_rule(
    pool: &SqlitePool,
    rule: &StandingRule,
) -> anyhow::Result<i64> {
    let existing: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, rule_hash FROM rules WHERE name = ?"
    )
    .bind(&rule.name)
    .fetch_optional(pool)
    .await?;

    if let Some((id, hash)) = existing {
        if hash == rule.rule_hash {
            return Ok(id);
        }
        // Hash changed: delete old matches by rule_name, update rule row.
        sqlx::query("DELETE FROM matches WHERE rule_name = ?")
            .bind(&rule.name)
            .execute(pool)
            .await?;
        sqlx::query(
            "UPDATE rules SET selector = ?, rule_hash = ? WHERE id = ?"
        )
        .bind(&rule.name)
        .bind(&rule.rule_hash)
        .bind(id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    // New rule.
    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO rules (name, selector, ref_kind, rule_hash) \
         VALUES (?, ?, 'urtsl', ?) RETURNING id"
    )
    .bind(&rule.name)
    .bind(&rule.name)
    .bind(&rule.rule_hash)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

/// Evaluate a standing rule and update the matches table.
/// Returns how many new matches were inserted and stale matches removed.
pub async fn evaluate_rule(
    pool: &SqlitePool,
    rule: &StandingRule,
) -> anyhow::Result<RuleEvalResult> {
    upsert_rule(pool, rule).await?;
    let hits = crate::eval(pool, &rule.expr).await?;

    // Current matches in DB for this rule_name.
    let existing: Vec<i64> = sqlx::query_scalar(
        "SELECT ref_id FROM matches WHERE rule_name = ?"
    )
    .bind(&rule.name)
    .fetch_all(pool)
    .await?;
    let existing_set: std::collections::HashSet<i64> = existing.into_iter().collect();

    // Batch resolve ref_ids from hits via OR-chained conditions.
    let mut new_ref_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for chunk in hits.hits.chunks(100) {
        let conditions: Vec<String> = chunk.iter()
            .map(|_| "(r.file_id = ? AND r.string_id = ? AND r.span_start = ?)".to_string())
            .collect();
        let sql = format!(
            "SELECT r.id FROM refs r WHERE {}",
            conditions.join(" OR ")
        );
        let mut query = sqlx::query_scalar::<_, i64>(&sql);
        for hit in chunk {
            query = query.bind(hit.file_id).bind(hit.string_id).bind(hit.span_start);
        }
        let ids = query.fetch_all(pool).await?;
        new_ref_ids.extend(ids);
    }

    // Insert new matches.
    let to_insert: Vec<i64> = new_ref_ids.difference(&existing_set).copied().collect();
    for chunk in to_insert.chunks(100) {
        let placeholders: Vec<String> = chunk.iter()
            .map(|_| "(?, ?, ?)".to_string())
            .collect();
        let sql = format!(
            "INSERT OR IGNORE INTO matches (ref_id, rule_name, kind) VALUES {}",
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql);
        for ref_id in chunk {
            query = query.bind(ref_id).bind(&rule.name).bind(&rule.kind);
        }
        query.execute(pool).await?;
    }

    // Remove stale matches.
    let to_remove: Vec<i64> = existing_set.difference(&new_ref_ids).copied().collect();
    for chunk in to_remove.chunks(100) {
        let placeholders: Vec<String> = chunk.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM matches WHERE rule_name = ? AND ref_id IN ({})",
            placeholders.join(", ")
        );
        let mut query = sqlx::query(&sql);
        query = query.bind(&rule.name);
        for ref_id in chunk {
            query = query.bind(ref_id);
        }
        query.execute(pool).await?;
    }

    Ok(RuleEvalResult {
        rule_name: rule.name.clone(),
        new_matches: to_insert.len(),
        stale_removed: to_remove.len(),
    })
}

/// Get all matches for a standing rule by name.
pub async fn get_matches(
    pool: &SqlitePool,
    rule_name: &str,
) -> anyhow::Result<HitSet> {
    let rows: Vec<(i64, String, String, String, i64, i64, String, String, i64, String)> = sqlx::query_as(
        "SELECT s.id, s.value, COALESCE(m.kind, ''), m.rule_name, \
         r.file_id, r.span_start, f.path, repos.name, r.span_end, \
         COALESCE(bf.branch, '') \
         FROM matches m \
         JOIN refs r ON r.id = m.ref_id \
         JOIN strings s ON s.id = r.string_id \
         JOIN files f ON r.file_id = f.id \
         JOIN repos ON f.repo_id = repos.id \
         LEFT JOIN branch_files bf ON bf.file_id = f.id AND bf.repo_id = f.repo_id \
         WHERE m.rule_name = ? \
         ORDER BY repos.name, f.path"
    )
    .bind(rule_name)
    .fetch_all(pool)
    .await?;

    let hits = rows.into_iter().map(|(string_id, value, kind, rule_name, file_id, span_start, file_path, repo_name, span_end, branch)| {
        Hit {
            string_id,
            value,
            confidence: 1.0,
            file_id,
            file_path,
            repo_name,
            branch,
            kind,
            rule_name,
            span_start,
            span_end,
        }
    }).collect();

    Ok(HitSet { hits })
}
