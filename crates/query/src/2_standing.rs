use sqlx::SqlitePool;

use crate::{Expr, HitSet};

/// A standing rule: a named URTSL expression that re-evaluates on index changes.
#[derive(Debug, Clone)]
pub struct StandingRule {
    pub name: String,
    pub expr: Expr,
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
    // Check if rule exists with same hash (no-op).
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
        // Hash changed: delete old matches, update rule.
        sqlx::query("DELETE FROM matches WHERE rule_id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        sqlx::query(
            "UPDATE rules SET selector = ?, rule_hash = ? WHERE id = ?"
        )
        .bind(&rule.name) // selector stores the name for now; will store serialized expr
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
    let rule_id = upsert_rule(pool, rule).await?;
    let hits = crate::eval(pool, &rule.expr).await?;

    // Current matches in DB.
    let existing: Vec<i64> = sqlx::query_scalar(
        "SELECT ref_id FROM matches WHERE rule_id = ?"
    )
    .bind(rule_id)
    .fetch_all(pool)
    .await?;

    let existing_set: std::collections::HashSet<i64> = existing.into_iter().collect();

    // Build set of ref_ids from hits. We need to look up ref_id from
    // (file_id, string_id, span_start).
    let mut new_ref_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for hit in &hits.hits {
        let ref_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM refs WHERE file_id = ? AND string_id = ? AND span_start = ?"
        )
        .bind(hit.file_id)
        .bind(hit.string_id)
        .bind(hit.span_start)
        .fetch_optional(pool)
        .await?;
        if let Some(id) = ref_id {
            new_ref_ids.insert(id);
        }
    }

    // Insert new matches.
    let to_insert: Vec<i64> = new_ref_ids.difference(&existing_set).copied().collect();
    for ref_id in &to_insert {
        sqlx::query("INSERT OR IGNORE INTO matches (rule_id, ref_id) VALUES (?, ?)")
            .bind(rule_id)
            .bind(ref_id)
            .execute(pool)
            .await?;
    }

    // Remove stale matches.
    let to_remove: Vec<i64> = existing_set.difference(&new_ref_ids).copied().collect();
    for ref_id in &to_remove {
        sqlx::query("DELETE FROM matches WHERE rule_id = ? AND ref_id = ?")
            .bind(rule_id)
            .bind(ref_id)
            .execute(pool)
            .await?;
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
    let rows: Vec<(i64, String, i64, i64, i64, String, String, i64, String)> = sqlx::query_as(
        "SELECT s.id, s.value, r.ref_kind, r.file_id, r.span_start, f.path, repos.name, r.span_end, \
         COALESCE(bf.branch, '') \
         FROM matches m \
         JOIN rules rl ON rl.id = m.rule_id \
         JOIN refs r ON r.id = m.ref_id \
         JOIN strings s ON s.id = r.string_id \
         JOIN files f ON r.file_id = f.id \
         JOIN repos ON f.repo_id = repos.id \
         LEFT JOIN branch_files bf ON bf.file_id = f.id AND bf.repo_id = f.repo_id \
         WHERE rl.name = ? \
         ORDER BY repos.name, f.path"
    )
    .bind(rule_name)
    .fetch_all(pool)
    .await?;

    let hits = rows.into_iter().map(|(string_id, value, ref_kind, file_id, span_start, file_path, repo_name, span_end, branch)| {
        crate::Hit {
            string_id,
            value,
            confidence: 1.0,
            file_id,
            file_path,
            repo_name,
            branch,
            ref_kind: sprefa_schema::RefKind::from_u8(ref_kind as u8)
                .unwrap_or(sprefa_schema::RefKind::StringLiteral),
            span_start,
            span_end,
        }
    }).collect();

    Ok(HitSet { hits })
}
