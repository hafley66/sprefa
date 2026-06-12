use anyhow::{bail, Result};
use std::collections::HashMap;

use crate::ast::*;

pub fn tbl(name: &str) -> String { format!("rel_{name}") }

fn esc(s: &str) -> String { s.replace('\'', "''") }

fn lit_sql(t: &Term) -> Option<String> {
    match t {
        Term::Str(s) => Some(format!("'{}'", esc(s))),
        Term::Int(n) => Some(n.to_string()),
        _ => None,
    }
}

/// SQL string concatenation for an interpolated term: literals quoted, vars are
/// the canonical column reference. `"${ty}::${name}"` -> `ty_col || '::' || name_col`.
fn interp_sql(parts: &[InterpPart], canon: &HashMap<String, String>) -> Result<String> {
    let mut pieces = Vec::new();
    for p in parts {
        pieces.push(match p {
            InterpPart::Lit(s) => format!("'{}'", esc(s)),
            InterpPart::Var(v) => canon.get(v).cloned()
                .ok_or_else(|| anyhow::anyhow!("unbound variable {v} in interpolation"))?,
        });
    }
    Ok(if pieces.is_empty() { "''".into() } else { pieces.join(" || ") })
}

fn term_sql(t: &Term, canon: &HashMap<String, String>) -> Result<String> {
    match t {
        Term::Var(v) => canon.get(v).cloned()
            .ok_or_else(|| anyhow::anyhow!("unbound variable {v}")),
        Term::Str(_) | Term::Int(_) => Ok(lit_sql(t).unwrap()),
        Term::Interp(parts) => interp_sql(parts, canon),
        Term::Wild => bail!("'_' not allowed here"),
        Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
        Term::Arith { op, lhs, rhs } => Ok(format!(
            "({} {} {})", term_sql(lhs, canon)?, op.sql(), term_sql(rhs, canon)?)),
    }
}

/// Walk a body's Pos/Neg/Cmp items into (canon var->cell, FROM aliases, WHERE
/// conds). Shared by `lower_rule` and `lower_gen`; other body items are the
/// caller's concern (a derived rule never carries them, gen rejects them).
fn body_sql(body: &[BodyItem], rels: &Rels)
    -> Result<(HashMap<String, String>, Vec<String>, Vec<String>)>
{
    let mut canon: HashMap<String, String> = HashMap::new();
    let mut wheres: Vec<String> = Vec::new();
    let mut froms: Vec<String> = Vec::new();
    let mut k = 0usize;

    for item in body {
        if let BodyItem::Pos(a) = item {
            let meta = rels.get(&a.rel).ok_or_else(|| anyhow::anyhow!("unknown relation {}", a.rel))?;
            if a.terms.len() != meta.cols.len() {
                bail!("relation {} expects {} cols, got {}", a.rel, meta.cols.len(), a.terms.len());
            }
            let alias = format!("r{k}");
            froms.push(format!("{} {alias}", tbl(&a.rel)));
            for (pos, term) in a.terms.iter().enumerate() {
                let cell = format!("{alias}.\"{}\"", meta.col_name(pos));
                match term {
                    Term::Var(v) => match canon.get(v) {
                        Some(prev) => wheres.push(format!("{cell} = {prev}")),
                        None => { canon.insert(v.clone(), cell); }
                    },
                    Term::Str(_) | Term::Int(_) => wheres.push(format!("{cell} = {}", lit_sql(term).unwrap())),
                    Term::Interp(_) => bail!("interpolated string only allowed in a rule head, not a body atom"),
                    Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                    Term::Arith { .. } => bail!("arithmetic only allowed in a rule head or comparison, not a body atom"),
                    Term::Wild => {}
                }
            }
            k += 1;
        }
    }

    let mut neg_m = 0usize;
    for item in body {
        if let BodyItem::Neg(a) = item {
            let meta = rels.get(&a.rel).ok_or_else(|| anyhow::anyhow!("unknown relation {}", a.rel))?;
            if a.terms.len() != meta.cols.len() {
                bail!("relation {} expects {} cols, got {}", a.rel, meta.cols.len(), a.terms.len());
            }
            let alias = format!("ax{neg_m}");
            let mut local: HashMap<String, String> = HashMap::new();
            let mut sub: Vec<String> = Vec::new();
            for (pos, term) in a.terms.iter().enumerate() {
                let cell = format!("{alias}.\"{}\"", meta.col_name(pos));
                match term {
                    Term::Var(v) => {
                        if let Some(outer) = canon.get(v) {
                            sub.push(format!("{cell} = {outer}"));
                        } else if let Some(prev) = local.get(v) {
                            sub.push(format!("{cell} = {prev}"));
                        } else {
                            local.insert(v.clone(), cell);
                        }
                    }
                    Term::Str(_) | Term::Int(_) => sub.push(format!("{cell} = {}", lit_sql(term).unwrap())),
                    Term::Interp(_) => bail!("interpolated string only allowed in a rule head, not a body atom"),
                    Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                    Term::Arith { .. } => bail!("arithmetic only allowed in a rule head or comparison, not a body atom"),
                    Term::Wild => {}
                }
            }
            let cond = if sub.is_empty() { "1=1".to_string() } else { sub.join(" AND ") };
            wheres.push(format!("NOT EXISTS (SELECT 1 FROM {} {alias} WHERE {cond})", tbl(&a.rel)));
            neg_m += 1;
        }
    }

    for item in body {
        if let BodyItem::Cmp(c) = item {
            let l = term_sql(&c.lhs, &canon)?;
            let r = term_sql(&c.rhs, &canon)?;
            wheres.push(format!("{l} {} {r}", c.op.sql()));
        }
    }

    Ok((canon, froms, wheres))
}

/// Lower a derived rule (Pos/Neg/Cmp only) to one `INSERT OR IGNORE ... SELECT`.
pub fn lower_rule(rule: &Rule, rels: &Rels) -> Result<String> {
    let (canon, froms, wheres) = body_sql(&rule.body, rels)?;

    // a ground fact (empty body) lowers to a FROM-less SELECT of literals;
    // a non-empty body still needs a positive atom to range over
    if froms.is_empty() && !rule.body.is_empty() {
        bail!("rule {} has no positive body atom (unsafe)", rule.head.rel);
    }

    let head_meta = rels.get(&rule.head.rel)
        .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rule.head.rel))?;
    if rule.head.terms.len() != head_meta.cols.len() {
        bail!("head {} expects {} cols, got {}", rule.head.rel, head_meta.cols.len(), rule.head.terms.len());
    }
    let cols: Vec<String> = head_meta.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };

    // An aggregating head selects `AGG(arg)` for each aggregate term and the plain
    // head terms as the GROUP BY list. `count(_)` aggregates the whole group, so a
    // wildcard arg lowers to `COUNT(*)`.
    if rule.has_agg() {
        let mut exprs = Vec::new();
        let mut group: Vec<String> = Vec::new();
        for (i, term) in rule.head.terms.iter().enumerate() {
            match rule.aggs.get(i).copied().flatten() {
                Some(f) => {
                    let arg = match term {
                        Term::Wild => "*".to_string(),
                        _ => term_sql(term, &canon)?,
                    };
                    exprs.push(format!("{}({arg})", f.sql()));
                }
                None => {
                    let g = term_sql(term, &canon)?;
                    exprs.push(g.clone());
                    group.push(g);
                }
            }
        }
        let group_sql = if group.is_empty() { String::new() } else { format!(" GROUP BY {}", group.join(", ")) };
        return Ok(format!(
            "INSERT OR IGNORE INTO {} ({}) SELECT {} FROM {}{}{}",
            tbl(&rule.head.rel),
            cols.join(", "),
            exprs.join(", "),
            froms.join(", "),
            where_sql,
            group_sql,
        ));
    }

    let mut exprs = Vec::new();
    for term in &rule.head.terms { exprs.push(term_sql(term, &canon)?); }

    let from_sql = if froms.is_empty() { String::new() } else { format!(" FROM {}", froms.join(", ")) };
    Ok(format!(
        "INSERT OR IGNORE INTO {} ({}) SELECT {}{}{}",
        tbl(&rule.head.rel),
        cols.join(", "),
        exprs.join(", "),
        from_sql,
        where_sql
    ))
}

/// Lower a gen body to a deterministic row source:
/// `SELECT DISTINCT v1, v2, ... FROM ... WHERE ... ORDER BY <all>`.
/// `vars` are the variables the gen templates reference, target vars first.
pub fn lower_gen(vars: &[String], body: &[BodyItem], rels: &Rels) -> Result<String> {
    if let Some(b) = body.iter().find(|b|
        !matches!(b, BodyItem::Pos(_) | BodyItem::Neg(_) | BodyItem::Cmp(_)))
    {
        bail!("gen body must be derived-style (relation atoms and comparisons only), got {b:?}");
    }
    let (canon, froms, wheres) = body_sql(body, rels)?;
    if froms.is_empty() { bail!("gen body has no positive atom"); }
    let mut sel = Vec::new();
    for v in vars {
        let cell = canon.get(v)
            .ok_or_else(|| anyhow::anyhow!("unbound variable {v} in gen template"))?;
        sel.push(format!("{cell} AS \"{v}\""));
    }
    if sel.is_empty() { bail!("gen templates bind no variables (static content needs no gen)"); }
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    let order: Vec<String> = (1..=vars.len()).map(|i| i.to_string()).collect();
    Ok(format!("SELECT DISTINCT {} FROM {}{} ORDER BY {}",
        sel.join(", "), froms.join(", "), where_sql, order.join(", ")))
}

/// Lower a query to (sql, headers).
pub fn lower_query(q: &Query, rels: &Rels) -> Result<(String, Vec<String>)> {
    let meta = rels.get(&q.head.rel)
        .ok_or_else(|| anyhow::anyhow!("unknown relation {}", q.head.rel))?;
    if q.head.terms.len() != meta.cols.len() {
        bail!("query {} expects {} cols, got {}", q.head.rel, meta.cols.len(), q.head.terms.len());
    }
    let mut canon: HashMap<String, String> = HashMap::new();
    let mut wheres: Vec<String> = Vec::new();
    let mut sel: Vec<String> = Vec::new();
    let mut headers: Vec<String> = Vec::new();

    for (pos, term) in q.head.terms.iter().enumerate() {
        let cell = format!("\"{}\"", meta.col_name(pos));
        match term {
            Term::Var(v) => match canon.get(v) {
                Some(prev) => wheres.push(format!("{cell} = {prev}")),
                None => {
                    canon.insert(v.clone(), cell.clone());
                    sel.push(format!("{cell} AS \"{v}\""));
                    headers.push(v.clone());
                }
            },
            Term::Str(_) | Term::Int(_) => wheres.push(format!("{cell} = {}", lit_sql(term).unwrap())),
            Term::Interp(_) => bail!("interpolated string not supported in a query head"),
            Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
            Term::Arith { .. } => bail!("arithmetic not supported in a query head (derive a relation with the computed column and query that)"),
            Term::Wild => {}
        }
    }
    if sel.is_empty() { sel.push("*".into()); }
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    let sql = format!("SELECT DISTINCT {} FROM {}{} ORDER BY 1", sel.join(", "), tbl(&q.head.rel), where_sql);
    Ok((sql, headers))
}
