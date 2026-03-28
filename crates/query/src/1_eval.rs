use sqlx::SqlitePool;

use crate::{Atom, Expr, Filter, Hit, HitSet, SetOp};

/// Evaluate an expression against the index and return matching hits.
pub fn eval<'a>(pool: &'a SqlitePool, expr: &'a Expr) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<HitSet>> + Send + 'a>> {
    Box::pin(async move {
        match expr {
            Expr::Atom(atom) => eval_atom(pool, atom).await,
            Expr::Filter(inner, filter) => {
                let mut hits = eval(pool, inner).await?;
                apply_filter(&mut hits, filter);
                Ok(hits)
            }
            Expr::SetOp(left, op, right) => {
                let l = eval(pool, left).await?;
                let r = eval(pool, right).await?;
                Ok(set_op(&l, &r, *op))
            }
            Expr::Cascade(left, right) => {
                let l = eval(pool, left).await?;
                eval_cascade(pool, &l, right).await
            }
        }
    })
}

// (string_id, value, kind, rule_name, file_id, span_start, file_path, repo_name, span_end, branch)
type AtomRow = (i64, String, String, String, i64, i64, String, String, i64, String);

async fn eval_atom(pool: &SqlitePool, atom: &Atom) -> anyhow::Result<HitSet> {
    let (sql, binds) = atom_to_sql(atom);
    let mut query = sqlx::query_as::<_, AtomRow>(&sql);
    for b in &binds {
        query = query.bind(b.as_str());
    }
    let rows = query.fetch_all(pool).await?;

    let conf = atom_confidence(atom);
    let hits = rows.into_iter().map(|(string_id, value, kind, rule_name, file_id, span_start, file_path, repo_name, span_end, branch)| {
        Hit {
            string_id,
            value,
            confidence: conf,
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

fn atom_confidence(atom: &Atom) -> f64 {
    match atom {
        Atom::Exact(_) => 1.0,
        Atom::Substring(_) => 0.8,
        Atom::Path(_) => 0.9,
        Atom::Mod(_) => 0.9,
        Atom::Stem(_) => 0.85,
        Atom::Seg(_) => 0.7,
        Atom::Fuzzy(_, threshold) => *threshold,
    }
}

/// SELECT returning (string_id, value, kind, rule_name, file_id, span_start, file_path, repo_name, span_end, branch).
fn atom_to_sql(atom: &Atom) -> (String, Vec<String>) {
    let base_cols = "s.id, s.value, COALESCE(m.kind, ''), COALESCE(m.rule_name, ''), \
                     r.file_id, r.span_start, f.path, repos.name, r.span_end, bf.branch";
    let base_joins = "JOIN refs r ON r.string_id = s.id \
                      LEFT JOIN matches m ON m.ref_id = r.id \
                      JOIN files f ON r.file_id = f.id \
                      JOIN repos ON f.repo_id = repos.id \
                      JOIN branch_files bf ON bf.file_id = f.id AND bf.repo_id = f.repo_id \
                      JOIN repo_branches rb ON rb.repo_id = bf.repo_id AND rb.branch = bf.branch";
    let committed = "rb.is_working_tree = 0";

    match atom {
        Atom::Substring(s) => {
            let sql = format!(
                "SELECT {base_cols} \
                 FROM strings_fts fts \
                 JOIN strings s ON s.id = fts.rowid \
                 {base_joins} \
                 WHERE fts.norm MATCH ? AND {committed} \
                 LIMIT 500"
            );
            (sql, vec![format!("\"{}\"", s)])
        }
        Atom::Exact(s) => {
            let sql = format!(
                "SELECT {base_cols} \
                 FROM strings s \
                 {base_joins} \
                 WHERE s.value = ? AND {committed} \
                 LIMIT 500"
            );
            (sql, vec![s.clone()])
        }
        Atom::Path(s) => {
            let pattern = format!("%{}%", s.replace('/', "%/%"));
            let sql = format!(
                "SELECT {base_cols} \
                 FROM strings s \
                 {base_joins} \
                 WHERE s.norm LIKE ? AND {committed} \
                 LIMIT 500"
            );
            (sql, vec![pattern])
        }
        Atom::Mod(s) => {
            let pattern = format!("%{}%", s.replace("::", "%::%"));
            let sql = format!(
                "SELECT {base_cols} \
                 FROM strings s \
                 {base_joins} \
                 WHERE s.norm LIKE ? AND {committed} \
                 LIMIT 500"
            );
            (sql, vec![pattern])
        }
        Atom::Stem(s) => {
            let sql = format!(
                "SELECT {base_cols} \
                 FROM strings s \
                 {base_joins} \
                 WHERE f.stem = ? AND {committed} \
                 LIMIT 500"
            );
            (sql, vec![s.clone()])
        }
        Atom::Seg(segments) => {
            let pattern = format!("%{}%", segments.join("%"));
            let sql = format!(
                "SELECT {base_cols} \
                 FROM strings s \
                 {base_joins} \
                 WHERE s.norm LIKE ? AND {committed} \
                 LIMIT 500"
            );
            (sql, vec![pattern])
        }
        Atom::Fuzzy(s, _) => {
            // Trigram FTS as candidate filter; edit distance post-filter is TODO.
            let sql = format!(
                "SELECT {base_cols} \
                 FROM strings_fts fts \
                 JOIN strings s ON s.id = fts.rowid \
                 {base_joins} \
                 WHERE fts.norm MATCH ? AND {committed} \
                 LIMIT 500"
            );
            (sql, vec![format!("\"{}\"", s)])
        }
    }
}

fn apply_filter(hits: &mut HitSet, filter: &Filter) {
    match filter {
        Filter::InRepo(repo) => {
            hits.hits.retain(|h| h.repo_name == *repo);
        }
        Filter::InFile(pattern) => {
            let pat = glob::Pattern::new(pattern)
                .unwrap_or_else(|_| glob::Pattern::new("*").unwrap());
            hits.hits.retain(|h| pat.matches(&h.file_path));
        }
        Filter::OfKind(kinds) => {
            if !kinds.is_empty() {
                hits.hits.retain(|h| kinds.iter().any(|k| k == &h.kind));
            }
        }
        Filter::NotKind(kinds) => {
            hits.hits.retain(|h| !kinds.iter().any(|k| k == &h.kind));
        }
        Filter::OnBranch(glob_pattern) => {
            let pat = glob::Pattern::new(glob_pattern)
                .unwrap_or_else(|_| glob::Pattern::new("*").unwrap());
            hits.hits.retain(|h| pat.matches(&h.branch));
        }
        Filter::Resolved => {
            // TODO: needs target_file_id in Hit
        }
        Filter::Unresolved => {
            // TODO: needs target_file_id in Hit
        }
        Filter::DepthMin(n) => {
            let n = *n as usize;
            hits.hits.retain(|h| h.file_path.matches('/').count() >= n);
        }
        Filter::DepthMax(n) => {
            let n = *n as usize;
            hits.hits.retain(|h| h.file_path.matches('/').count() <= n);
        }
    }
}

fn set_op(left: &HitSet, right: &HitSet, op: SetOp) -> HitSet {
    use std::collections::{HashMap, HashSet};

    match op {
        SetOp::Intersect => {
            let right_keys: HashMap<(i64, i64, i64), f64> = right.hits.iter()
                .map(|h| (h.ref_key(), h.confidence))
                .collect();
            let hits = left.hits.iter()
                .filter(|h| right_keys.contains_key(&h.ref_key()))
                .map(|h| {
                    let rc = right_keys.get(&h.ref_key()).copied().unwrap_or(0.0);
                    Hit { confidence: h.confidence.min(rc), ..h.clone() }
                })
                .collect();
            HitSet { hits }
        }
        SetOp::Union => {
            let mut merged: HashMap<(i64, i64, i64), Hit> = HashMap::new();
            for h in &left.hits {
                merged.insert(h.ref_key(), h.clone());
            }
            for h in &right.hits {
                merged.entry(h.ref_key())
                    .and_modify(|existing| {
                        existing.confidence = existing.confidence.max(h.confidence);
                    })
                    .or_insert_with(|| h.clone());
            }
            let mut hits: Vec<Hit> = merged.into_values().collect();
            hits.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
            HitSet { hits }
        }
        SetOp::Diff => {
            let right_keys: HashSet<(i64, i64, i64)> = right.hits.iter()
                .map(|h| h.ref_key())
                .collect();
            let hits = left.hits.iter()
                .filter(|h| !right_keys.contains(&h.ref_key()))
                .cloned()
                .collect();
            HitSet { hits }
        }
    }
}

/// Cascade: evaluate RHS, keep only hits whose *string_id* also appeared in LHS.
/// This is the cross-repo join: "backend exports X" >> "frontend imports X" works
/// because both sides match the same string value, not the same file.
async fn eval_cascade(pool: &SqlitePool, left: &HitSet, right: &Expr) -> anyhow::Result<HitSet> {
    let left_string_ids: std::collections::HashSet<i64> = left.hits.iter()
        .map(|h| h.string_id)
        .collect();

    let mut rhs = eval(pool, right).await?;
    rhs.hits.retain(|h| left_string_ids.contains(&h.string_id));

    // Boost confidence for co-occurrence.
    for h in &mut rhs.hits {
        h.confidence = (h.confidence * 1.2).min(1.0);
    }

    Ok(rhs)
}
