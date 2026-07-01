use anyhow::{bail, Result};
use std::collections::HashMap;

use crate::ast::*;

pub fn tbl(name: &str) -> String { format!("rel_{name}") }

/// Pass-through string builtins: (dsl name, sql UDF, arity). All text->text,
/// registered in db.rs::register_string_fns. Shared with typecheck (the known-fn
/// whitelist) so a new entry lights up lowering AND type checking at once.
pub const STR_FNS: &[(&str, &str, usize)] = &[
    ("lower", "sprf_lower", 1),
    ("upper", "sprf_upper", 1),
    ("lcfirst", "sprf_lcfirst", 1),
    ("ucfirst", "sprf_ucfirst", 1),
    ("trim", "sprf_trim", 1),
    ("strip_prefix", "sprf_strip_prefix", 2),
    ("strip_suffix", "sprf_strip_suffix", 2),
    ("replace_re", "sprf_replace_re", 3),
];

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
        Term::Call { name, args } => {
            let arg_sqls: Vec<String> = args.iter()
                .map(|a| term_sql(a, canon)).collect::<Result<_>>()?;
            match name.as_str() {
                // SQLite native: replace(X, Y, Z) replaces all Y in X with Z.
                "replace" if args.len() == 3 =>
                    Ok(format!("replace({})", arg_sqls.join(", "))),
                // Registered UDF (db.rs): sprf_split(text, sep, idx).
                "split" if args.len() == 3 =>
                    Ok(format!("sprf_split({})", arg_sqls.join(", "))),
                // SQLite native: text->int coercion (leading-int prefix, else 0),
                // so a numeric shell/json string can fill an int column or be
                // compared numerically instead of as text against "0".
                "int" if args.len() == 1 =>
                    Ok(format!("CAST({} AS INTEGER)", arg_sqls[0])),
                // Registered string UDFs (db.rs::register_string_fns), all
                // text->text. STR_FNS = (dsl name, sql fn, arity).
                _ if STR_FNS.iter().any(|(n, _, k)| *n == name && *k == args.len()) => {
                    let (_, sql, _) = STR_FNS.iter().find(|(n, _, _)| *n == name).unwrap();
                    Ok(format!("{sql}({})", arg_sqls.join(", ")))
                }
                other => bail!("unknown or mis-arity function `{other}` (known: split/3, replace/3, int/1, {})",
                    STR_FNS.iter().map(|(n, _, k)| format!("{n}/{k}")).collect::<Vec<_>>().join(", ")),
            }
        }
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
                    Term::Call { .. } => bail!("function call only allowed in a rule head or comparison, not a body atom"),
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
                    Term::Call { .. } => bail!("function call only allowed in a rule head or comparison, not a body atom"),
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
            // Computed binding: `var = expr` where the var is unbound and the
            // other side is a Call/Arith (a value-producing expression, not a
            // literal/var). Bind the var to the expr's SQL so later body items
            // and the head see the computed value. Same canon slot a Pos atom
            // fills; the expr is inlined at each use (SQLite re-evals, cheap
            // for split/replace). Literal `x = "foo"` stays a WHERE filter.
            if c.op == CmpOp::Eq {
                let bound = match (&c.lhs, &c.rhs) {
                    (Term::Var(v), rhs) if v != "_" && !canon.contains_key(v) && has_computation(rhs) => {
                        let e = term_sql(rhs, &canon)?;
                        canon.insert(v.clone(), e);
                        true
                    }
                    (lhs, Term::Var(v)) if v != "_" && !canon.contains_key(v) && has_computation(lhs) => {
                        let e = term_sql(lhs, &canon)?;
                        canon.insert(v.clone(), e);
                        true
                    }
                    _ => false,
                };
                if bound { continue; }
            }
            let l = term_sql(&c.lhs, &canon)?;
            let r = term_sql(&c.rhs, &canon)?;
            wheres.push(format!("{l} {} {r}", c.op.sql()));
        }
    }

    Ok((canon, froms, wheres))
}

/// Lower a derived rule (Pos/Neg/Cmp only) to one `INSERT OR IGNORE ... SELECT`.
/// Lower an `@async` rule body to a `SELECT DISTINCT` of the named bound vars
/// (in the given order), each aliased to its var name. No head rel: the columns
/// are the request-arg variables themselves, projected out of the converged
/// relations. The engine reads each row into a JSON arg object. See engine
/// `rebuild_async` and docs §8.
pub fn lower_body_projection(body: &[BodyItem], rels: &Rels, vars: &[String]) -> Result<String> {
    let (canon, froms, wheres) = body_sql(body, rels)?;
    if froms.is_empty() {
        bail!("@async rule body has no positive atom to bind request args");
    }
    let mut exprs = Vec::new();
    for v in vars {
        let e = canon.get(v).cloned()
            .ok_or_else(|| anyhow::anyhow!("@async request var {v} is unbound in the body"))?;
        exprs.push(format!("{e} AS \"{v}\""));
    }
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    Ok(format!("SELECT DISTINCT {} FROM {}{}", exprs.join(", "), froms.join(", "), where_sql))
}

pub fn lower_rule(rule: &Rule, rels: &Rels) -> Result<String> {
    lower_rule_to(rule, rels, &tbl(&rule.head.rel), &[])
}

/// Lower a rule into an arbitrary `target` table, appending `extra` constant
/// columns `(col_name, value_sql)` to the SELECT. The plain rule path is
/// `lower_rule_to(rule, rels, &tbl(head), &[])`. The `@next` carry path passes
/// `target = carry_<rel>` and `extra = [("tx", "<next_tx>")]` so the body's rows
/// land in the carry buffer stamped with the next generation instead of in the
/// head relation this tick. Existing callers go through `lower_rule` unchanged.
pub fn lower_rule_to(rule: &Rule, rels: &Rels, target: &str, extra: &[(String, String)]) -> Result<String> {
    let (canon, froms, mut wheres) = body_sql(&rule.body, rels)?;

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
    let mut cols: Vec<String> = head_meta.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
    for (n, _) in extra { cols.push(format!("\"{n}\"")); }
    let extra_vals: Vec<String> = extra.iter().map(|(_, v)| v.clone()).collect();
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };

    // Lattice merge path: a `key(...) merge(MaxBy(col))` relation lowers to an
    // UPSERT keyed on the declared FD, replacing the stored row's non-key columns
    // with the incoming row's only when the incoming `col` is strictly greater.
    // The whole winning row stays intact (row-selection, not per-column Galois
    // mixing), which is exactly what MCP dispatch needs: one response per
    // request id, the highest-priority matching rule. NOT `INSERT OR IGNORE`
    // (that would first-wins, ignoring a higher-priority later rule). The
    // fixpoint loop converges: once the max-prio row is in, any re-insert of a
    // lower-or-equal row fails the WHERE and affects 0 rows (delta 0). Agg +
    // merge is rejected (an aggregating head has no single row to rank).
    if let Some(crate::ast::MergeFn::MaxBy(mc)) = &head_meta.merge {
        if rule.has_agg() {
            bail!("rel {} has both an aggregate head and merge(...); pick one", rule.head.rel);
        }
        let key = head_meta.key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("rel {} has merge(...) without key(...)", rule.head.rel))?;
        let key_cols: Vec<String> = key.iter().map(|c| format!("\"{c}\"")).collect();
        // Non-key columns to copy on a winning conflict: every user column not in
        // the key, in declaration order, plus any extra columns (e.g. carry tx).
        let mut non_key: Vec<String> = Vec::new();
        for c in &head_meta.cols {
            if !key.contains(&c.name) { non_key.push(c.name.clone()); }
        }
        for (n, _) in extra { non_key.push(n.clone()); }
        let mut exprs = Vec::new();
        for term in &rule.head.terms {
            let e = term_sql(term, &canon)?;
            if has_call(term) { wheres.push(format!("{e} IS NOT NULL")); }
            exprs.push(e);
        }
        exprs.extend(extra_vals.iter().cloned());
        let from_sql = if froms.is_empty() { String::new() } else { format!(" FROM {}", froms.join(", ")) };
        // SQLite parses `FROM t ON CONFLICT` as a join (ON = join condition),
        // so the UPSERT clause must be preceded by a terminated SELECT. A bare
        // `FROM t` with no WHERE leaves `ON CONFLICT` ambiguous and fails with
        // "near DO: syntax error". Always emit a WHERE — `WHERE 1` when the
        // body has no constraints — so the SELECT body closes before ON CONFLICT.
        let where_sql = if wheres.is_empty() { " WHERE 1".to_string() } else { format!(" WHERE {}", wheres.join(" AND ")) };
        let set_clause: Vec<String> = non_key.iter()
            .map(|c| format!("\"{c}\" = excluded.\"{c}\"")).collect();
        return Ok(format!(
            "INSERT INTO {} ({}) SELECT {}{}{} ON CONFLICT({}) DO UPDATE SET {} WHERE excluded.\"{}\" > \"{}\"",
            target,
            cols.join(", "),
            exprs.join(", "),
            from_sql,
            where_sql,
            key_cols.join(", "),
            set_clause.join(", "),
            mc, mc,
        ));
    }

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
                    // SUM over zero rows is SQL NULL; INSERT OR IGNORE never
                    // dedups NULLs (NULL != NULL), so the fixpoint loop would
                    // diverge re-inserting the same NULL each iteration. Pin
                    // empty-sum to 0. COUNT/MIN/MAX don't need it (COUNT is
                    // never NULL; MIN/MAX of nothing being NULL is the
                    // intended "no value" semantics).
                    if matches!(f, crate::ast::AggFn::Sum) {
                        exprs.push(format!("COALESCE({}({arg}), 0)", f.sql()));
                    } else {
                        exprs.push(format!("{}({arg})", f.sql()));
                    }
                }
                None => {
                    let g = term_sql(term, &canon)?;
                    exprs.push(g.clone());
                    group.push(g);
                }
            }
        }
        let group_sql = if group.is_empty() { String::new() } else { format!(" GROUP BY {}", group.join(", ")) };
        exprs.extend(extra_vals.iter().cloned());
        return Ok(format!(
            "INSERT OR IGNORE INTO {} ({}) SELECT {} FROM {}{}{}",
            target,
            cols.join(", "),
            exprs.join(", "),
            froms.join(", "),
            where_sql,
            group_sql,
        ));
    }

    let mut exprs = Vec::new();
    for term in &rule.head.terms {
        let e = term_sql(term, &canon)?;
        // A head term containing a Call (split/replace) may evaluate to NULL
        // when the function misses (split out-of-range). A NULL row inserted
        // into the derived table never dedups in the fixpoint delta (NULL !=
        // NULL), so convergence breaks. Guard: filter NULL-producing head
        // expressions out of the SELECT entirely. The row drops, which is the
        // intended "no match" semantics.
        if has_call(term) { wheres.push(format!("{e} IS NOT NULL")); }
        exprs.push(e);
    }

    exprs.extend(extra_vals.iter().cloned());
    let from_sql = if froms.is_empty() { String::new() } else { format!(" FROM {}", froms.join(", ")) };
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    Ok(format!(
        "INSERT OR IGNORE INTO {} ({}) SELECT {}{}{}",
        target,
        cols.join(", "),
        exprs.join(", "),
        from_sql,
        where_sql
    ))
}

/// Does a term contain a `Term::Call` anywhere? Used to decide whether the
/// lowered SQL expression may return NULL (split out-of-range), requiring an
/// `IS NOT NULL` guard on the SELECT.
fn has_call(t: &Term) -> bool {
    match t {
        Term::Call { args, .. } => true || args.iter().any(has_call),
        Term::Arith { lhs, rhs, .. } => has_call(lhs) || has_call(rhs),
        _ => false,
    }
}

/// Is this term a value-producing computation (Call or Arith)? Used by
/// `body_sql`'s computed-binding path to distinguish `x = split(...)` (bind x
/// to the computed expr) from `x = "literal"` (filter WHERE).
fn has_computation(t: &Term) -> bool {
    matches!(t, Term::Call { .. } | Term::Arith { .. })
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
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    if sel.is_empty() {
        // A constant gen row (no template holes). Legitimate for a fully-
        // generated file: a title / header / separator line has no variables,
        // and there's no static file to put it in. DISTINCT collapses the body's
        // rows to exactly one, so the line is emitted once.
        return Ok(format!("SELECT DISTINCT 1 FROM {}{}", froms.join(", "), where_sql));
    }
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
            Term::Call { .. } => bail!("function call not supported in a query head (derive a relation with the computed column and query that)"),
            Term::Wild => {}
        }
    }
    if sel.is_empty() { sel.push("*".into()); }
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    let sql = format!("SELECT DISTINCT {} FROM {}{} ORDER BY 1", sel.join(", "), tbl(&q.head.rel), where_sql);
    Ok((sql, headers))
}
