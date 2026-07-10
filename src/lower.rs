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
    ("norm", "sprf_norm", 1),
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

/// Static type of a term at lower time, from the body's var->column-type map.
/// `None` = unknown (an unbound var, a wildcard). Drives the `+` overload:
/// int + int stays SQL addition, text + text lowers to `||`. All non-int base
/// types (path/file/dir/repo/rev) store TEXT, so they concat like text.
fn term_ty(t: &Term, tys: &HashMap<String, Type>) -> Option<Type> {
    match t {
        Term::Var(v) => tys.get(v).copied(),
        Term::Str(_) | Term::Interp(_) => Some(Type::Text),
        Term::Int(_) => Some(Type::Int),
        Term::Call { name, .. } => Some(if name == "int" { Type::Int } else { Type::Text }),
        Term::Arith { op, lhs, rhs } => match op {
            ArithOp::Add => match (term_ty(lhs, tys), term_ty(rhs, tys)) {
                (Some(Type::Int), Some(Type::Int)) => Some(Type::Int),
                (Some(a), Some(b)) if a != Type::Int && b != Type::Int => Some(Type::Text),
                (Some(a), None) | (None, Some(a)) => Some(a),
                _ => None,
            },
            _ => Some(Type::Int),
        },
        Term::Wild | Term::PathLit { .. } => None,
    }
}

fn term_sql(t: &Term, canon: &HashMap<String, String>, tys: &HashMap<String, Type>) -> Result<String> {
    match t {
        Term::Var(v) => canon.get(v).cloned()
            .ok_or_else(|| anyhow::anyhow!("unbound variable {v}")),
        Term::Str(_) | Term::Int(_) => Ok(lit_sql(t).unwrap()),
        Term::Interp(parts) => interp_sql(parts, canon),
        Term::Wild => bail!("'_' not allowed here"),
        Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
        // `+` is overloaded: int + int = SQL addition, text + text = SQLite `||`.
        // Mixed is a typecheck error upstream; the lower-time bail is the
        // backstop for paths typecheck cannot see. Unknown-typed sides keep the
        // legacy numeric `+` (a fully-unknown `+` was numeric before this fork).
        Term::Arith { op: ArithOp::Add, lhs, rhs } => {
            let (lt, rt) = (term_ty(lhs, tys), term_ty(rhs, tys));
            let text = |t: Option<Type>| t.is_some_and(|x| x != Type::Int);
            let sql_op = match (lt, rt) {
                _ if text(lt) && text(rt) => "||",
                (Some(Type::Int), t) | (t, Some(Type::Int)) if text(t) =>
                    bail!("cannot `+` int and text — interpolate (\"${{count}}${{name}}\") or convert with int(..)"),
                _ => "+",
            };
            Ok(format!("({} {sql_op} {})", term_sql(lhs, canon, tys)?, term_sql(rhs, canon, tys)?))
        }
        Term::Arith { op, lhs, rhs } => Ok(format!(
            "({} {} {})", term_sql(lhs, canon, tys)?, op.sql(), term_sql(rhs, canon, tys)?)),
        Term::Call { name, args } => {
            let arg_sqls: Vec<String> = args.iter()
                .map(|a| term_sql(a, canon, tys)).collect::<Result<_>>()?;
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
                // SQLite native JSON constructors (core since 3.38). Variadic:
                // json_object takes (key, value) pairs (even arity >= 2),
                // json_array takes >= 1 element, json(x) validates/minifies.
                "json_object" if args.len() >= 2 && args.len() % 2 == 0 =>
                    Ok(format!("json_object({})", arg_sqls.join(", "))),
                "json_array" if !args.is_empty() =>
                    Ok(format!("json_array({})", arg_sqls.join(", "))),
                "json" if args.len() == 1 =>
                    Ok(format!("json({})", arg_sqls[0])),
                // Registered string UDFs (db.rs::register_string_fns), all
                // text->text. STR_FNS = (dsl name, sql fn, arity).
                _ if STR_FNS.iter().any(|(n, _, k)| *n == name && *k == args.len()) => {
                    let (_, sql, _) = STR_FNS.iter().find(|(n, _, _)| *n == name).unwrap();
                    Ok(format!("{sql}({})", arg_sqls.join(", ")))
                }
                other => bail!("unknown or mis-arity function `{other}` (known: split/3, replace/3, int/1, \
                    json_object/even>=2, json_array/>=1, json/1, {})",
                    STR_FNS.iter().map(|(n, _, k)| format!("{n}/{k}")).collect::<Vec<_>>().join(", ")),
            }
        }
    }
}

/// Walk a body's Pos/Neg/Cmp items into (canon var->cell, var->type, FROM
/// aliases, WHERE conds). Shared by `lower_rule` and `lower_gen`; other body
/// items are the caller's concern (a derived rule never carries them, gen
/// rejects them). Pass order is Pos -> Cmp -> Neg: the Cmp pass may BIND a var
/// to a computed expression (`callee = replace(callee_q, ".", "::")`), and a
/// negation referencing that var must see it in canon — with Neg first the var
/// minted an unconstrained subquery local instead of a join.
fn body_sql(body: &[BodyItem], rels: &Rels)
    -> Result<(HashMap<String, String>, HashMap<String, Type>, Vec<String>, Vec<String>)>
{
    let mut canon: HashMap<String, String> = HashMap::new();
    let mut tys: HashMap<String, Type> = HashMap::new();
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
                        None => {
                            canon.insert(v.clone(), cell);
                            tys.insert(v.clone(), meta.cols[pos].ty);
                        }
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
                        let e = term_sql(rhs, &canon, &tys)?;
                        if let Some(t) = term_ty(rhs, &tys) { tys.insert(v.clone(), t); }
                        canon.insert(v.clone(), e);
                        true
                    }
                    (lhs, Term::Var(v)) if v != "_" && !canon.contains_key(v) && has_computation(lhs) => {
                        let e = term_sql(lhs, &canon, &tys)?;
                        if let Some(t) = term_ty(lhs, &tys) { tys.insert(v.clone(), t); }
                        canon.insert(v.clone(), e);
                        true
                    }
                    _ => false,
                };
                if bound { continue; }
            }
            let l = term_sql(&c.lhs, &canon, &tys)?;
            let r = term_sql(&c.rhs, &canon, &tys)?;
            wheres.push(format!("{l} {} {r}", c.op.sql()));
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

    Ok((canon, tys, froms, wheres))
}

/// Lower a derived rule (Pos/Neg/Cmp only) to one `INSERT OR IGNORE ... SELECT`.
/// Lower an `@async` rule body to a `SELECT DISTINCT` of the named bound vars
/// (in the given order), each aliased to its var name. No head rel: the columns
/// are the request-arg variables themselves, projected out of the converged
/// relations. The engine reads each row into a JSON arg object. See engine
/// `rebuild_async` and docs §8.
pub fn lower_body_projection(body: &[BodyItem], rels: &Rels, vars: &[String]) -> Result<String> {
    let (canon, _tys, froms, wheres) = body_sql(body, rels)?;
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
    let (canon, tys, froms, mut wheres) = body_sql(&rule.body, rels)?;

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
            if matches!(term, Term::Wild) { exprs.push("NULL".into()); continue; }
            let e = term_sql(term, &canon, &tys)?;
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
                    use crate::ast::AggFn;
                    let arg = match term {
                        Term::Wild => "*".to_string(),
                        _ => term_sql(term, &canon, &tys)?,
                    };
                    match f {
                        // SUM over zero rows is SQL NULL; INSERT OR IGNORE never
                        // dedups NULLs (NULL != NULL), so the fixpoint loop would
                        // diverge re-inserting the same NULL each iteration. Pin
                        // empty-sum to 0. COUNT/MIN/MAX don't need it (COUNT is
                        // never NULL; MIN/MAX of nothing being NULL is the
                        // intended "no value" semantics).
                        AggFn::Sum => exprs.push(format!("COALESCE(SUM({arg}), 0)")),
                        // `ORDER BY <arg>` inside the aggregate makes element order
                        // a pure function of the group's rows — otherwise SQLite's
                        // aggregate order is arbitrary and the rel's content digest
                        // would flap every tick, forcing spurious daemon rebuilds.
                        AggFn::JsonGroupArray => {
                            if matches!(term, Term::Wild) {
                                bail!("json_group_array(_) has no value to collect — pass a column, not `_`");
                            }
                            exprs.push(format!("json_group_array({arg} ORDER BY {arg})"));
                        }
                        AggFn::JsonGroupObject => {
                            if matches!(term, Term::Wild) {
                                bail!("json_group_object(_, ..) has no key to build — pass a column, not `_`");
                            }
                            let val = rule.agg_args2.get(i).and_then(|a| a.as_ref())
                                .ok_or_else(|| anyhow::anyhow!(
                                    "json_group_object expects (key, value) — the value arg is missing"))?;
                            let val_sql = term_sql(val, &canon, &tys)?;
                            exprs.push(format!("json_group_object({arg}, {val_sql} ORDER BY {arg})"));
                        }
                        AggFn::Count | AggFn::Min | AggFn::Max =>
                            exprs.push(format!("{}({arg})", f.sql())),
                    }
                }
                None => {
                    let g = term_sql(term, &canon, &tys)?;
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
        // A `Term::Wild` head slot comes from head named-arg padding: a sink
        // rule that names only some columns (`diag(path: p, line: l, msg: m)`)
        // leaves the rest unset. Project SQL NULL so the reader can default it.
        // Sink use only — a NULL never dedups in a fixpoint delta (NULL != NULL),
        // so a Wild in a RECURSIVE head would diverge. Enforced upstream:
        // typecheck's `recursive-null-pad` diag + the `rebuild_derived` bail
        // (both via `Rule::head_null_pads`), so this arm never runs for a
        // recursive component.
        if matches!(term, Term::Wild) { exprs.push("NULL".into()); continue; }
        let e = term_sql(term, &canon, &tys)?;
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
    let (canon, _tys, froms, wheres) = body_sql(body, rels)?;
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
    // A query head carrying one or more aggregate calls switches from
    // SELECT DISTINCT to GROUP BY (the rule-head aggregate shape, positional).
    let has_agg = q.head.terms.iter()
        .any(|t| matches!(t, Term::Call { name, .. } if AggFn::parse(name).is_some()));
    if has_agg {
        return lower_query_agg(q, meta);
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

/// The aggregate arm of `lower_query`. Var terms bind their column positionally,
/// appear in SELECT, and form the GROUP BY set (also the deterministic ORDER BY).
/// Literals stay WHERE filters. Wildcards drop out (collapsed over). A one-arg
/// aggregate at position `i` aggregates column `i`; its arg var is a fresh output
/// label (header), not a bound column. `json_group_object(key, value)` is the only
/// two-arg aggregate: it consumes column `i` (key) and column `i+1` (value), and
/// the term at `i+1` must be the wildcard `_`.
fn lower_query_agg(q: &Query, meta: &RelMeta) -> Result<(String, Vec<String>)> {
    use std::collections::HashSet;
    let terms = &q.head.terms;
    // Group-by var names bound at a plain (non-aggregate) position. An aggregate
    // output label may not collide with one of these, nor with another label.
    let group_vars: HashSet<&str> = terms.iter()
        .filter_map(|t| if let Term::Var(v) = t { Some(v.as_str()) } else { None })
        .collect();
    let mut labels: HashSet<String> = HashSet::new();

    let mut consumed = vec![false; terms.len()];
    let mut canon: HashMap<String, String> = HashMap::new();
    let mut wheres: Vec<String> = Vec::new();
    let mut sel: Vec<String> = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut group_cells: Vec<String> = Vec::new();

    // Register an aggregate output label, rejecting a collision with a bound
    // group var or another aggregate label.
    let take_label = |args: &[Term], labels: &mut HashSet<String>| -> Result<String> {
        let Some(Term::Var(name)) = args.first() else {
            bail!("aggregate argument must be a variable naming the output column, e.g. count(total)");
        };
        if group_vars.contains(name.as_str()) {
            bail!("aggregate output label `{name}` collides with a column var bound elsewhere in the query head; rename it");
        }
        if !labels.insert(name.clone()) {
            bail!("aggregate output label `{name}` is used twice in the query head; rename one");
        }
        Ok(name.clone())
    };

    for i in 0..terms.len() {
        if consumed[i] { continue; }
        let cell = format!("\"{}\"", meta.col_name(i));
        match &terms[i] {
            Term::Wild => {}
            Term::Str(_) | Term::Int(_) => wheres.push(format!("{cell} = {}", lit_sql(&terms[i]).unwrap())),
            Term::Interp(_) => bail!("interpolated string not supported in a query head"),
            Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
            Term::Arith { .. } => bail!("arithmetic not supported in a query head (derive a relation with the computed column and query that)"),
            Term::Var(v) => match canon.get(v) {
                Some(prev) => wheres.push(format!("{cell} = {prev}")),
                None => {
                    canon.insert(v.clone(), cell.clone());
                    sel.push(format!("{cell} AS \"{v}\""));
                    headers.push(v.clone());
                    group_cells.push(cell.clone());
                }
            },
            Term::Call { name, args } => {
                let Some(f) = AggFn::parse(name) else {
                    bail!("function call not supported in a query head (derive a relation with the computed column and query that)");
                };
                if f.is_two_arg() {
                    if args.len() != 2 {
                        bail!("json_group_object(key, value) in a query head takes two argument labels, got {}", args.len());
                    }
                    // The two-arg aggregate consumes the NEXT column as its value,
                    // so the term at i+1 must be `_` (query arity == rel columns).
                    if i + 1 >= terms.len() || !matches!(terms[i + 1], Term::Wild) {
                        bail!("json_group_object(k, v) in a query head consumes two columns: place it at the key column and put _ at the value column (\"? line(order, json_group_object(item, price), _)\")");
                    }
                    consumed[i + 1] = true;
                    let label = take_label(args, &mut labels)?;
                    let val_cell = format!("\"{}\"", meta.col_name(i + 1));
                    sel.push(format!("json_group_object({cell}, {val_cell} ORDER BY {cell}) AS \"{label}\""));
                    headers.push(label);
                } else {
                    if args.len() != 1 {
                        bail!("aggregate `{name}` in a query head takes one argument label, got {}", args.len());
                    }
                    let label = take_label(args, &mut labels)?;
                    let expr = match f {
                        // `ORDER BY <col>` inside the aggregate keeps element order a
                        // pure function of the group's rows, so the JSON text is stable
                        // tick to tick (no spurious daemon rebuild).
                        AggFn::JsonGroupArray => format!("json_group_array({cell} ORDER BY {cell})"),
                        // SUM over zero rows is SQL NULL; pin empty-sum to 0.
                        AggFn::Sum => format!("COALESCE(SUM({cell}), 0)"),
                        AggFn::Count | AggFn::Min | AggFn::Max => format!("{}({cell})", f.sql()),
                        AggFn::JsonGroupObject => unreachable!("handled by the two-arg arm"),
                    };
                    sel.push(format!("{expr} AS \"{label}\""));
                    headers.push(label);
                }
            }
        }
    }

    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    // Zero grouped columns (all wildcards + aggregates) = a whole-rel aggregate:
    // one row, no GROUP BY, no ORDER BY.
    let (group_sql, order_sql) = if group_cells.is_empty() {
        (String::new(), String::new())
    } else {
        (format!(" GROUP BY {}", group_cells.join(", ")),
         format!(" ORDER BY {}", group_cells.join(", ")))
    };
    let sql = format!("SELECT {} FROM {}{}{}{}",
        sel.join(", "), tbl(&q.head.rel), where_sql, group_sql, order_sql);
    Ok((sql, headers))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Item, RelMeta};

    fn rule_and_rels(src: &str) -> (Rule, Rels) {
        let prog = crate::parse::parse(crate::lex::lex(src).unwrap()).unwrap();
        let mut rels = Rels::new();
        let mut rule = None;
        for item in prog.items {
            match item {
                Item::Rel(d) => { rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone(), ..Default::default() }); }
                Item::Rule(r) if !r.body.is_empty() => { rule = Some(r); }
                _ => {}
            }
        }
        (rule.expect("one derived rule"), rels)
    }

    fn query_and_rels(src: &str) -> (Query, Rels) {
        let prog = crate::parse::parse(crate::lex::lex(src).unwrap()).unwrap();
        let mut rels = Rels::new();
        let mut query = None;
        for item in prog.items {
            match item {
                Item::Rel(d) => { rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone(), ..Default::default() }); }
                Item::Query(q) => { query = Some(q); }
                _ => {}
            }
        }
        (query.expect("one query"), rels)
    }

    /// An aggregate query head switches from SELECT DISTINCT to GROUP BY: the
    /// var terms are the group set (and the deterministic ORDER BY), the aggregate
    /// collapses each group. json_group_array orders inside the call.
    #[test]
    fn query_agg_json_group_array_groups_and_orders() {
        let (q, rels) = query_and_rels(concat!(
            "rel member(group_col: text, name: text).\n",
            "? member(group_col, json_group_array(names)).\n"));
        let (sql, headers) = lower_query(&q, &rels).unwrap();
        assert!(sql.contains("json_group_array(\"name\" ORDER BY \"name\") AS \"names\""),
            "array agg orders inside the call, labeled by the arg var: {sql}");
        assert!(sql.contains("GROUP BY \"group_col\""), "group key present: {sql}");
        assert!(sql.contains("ORDER BY \"group_col\""), "deterministic order by group: {sql}");
        assert!(!sql.contains("DISTINCT"), "no DISTINCT under GROUP BY: {sql}");
        assert_eq!(headers, vec!["group_col".to_string(), "names".to_string()]);
    }

    /// count/sum aggregate their positional column; the arg var is the header.
    #[test]
    fn query_agg_count_sum_shape() {
        let (q, rels) = query_and_rels(concat!(
            "rel hit(bucket: text, amount: int).\n",
            "? hit(bucket, count(total)).\n"));
        let (sql, headers) = lower_query(&q, &rels).unwrap();
        assert!(sql.contains("COUNT(\"amount\") AS \"total\""), "count over column i: {sql}");
        assert!(sql.contains("GROUP BY \"bucket\""), "{sql}");
        assert_eq!(headers, vec!["bucket".to_string(), "total".to_string()]);

        let (q, rels) = query_and_rels(concat!(
            "rel hit(bucket: text, amount: int).\n",
            "? hit(bucket, sum(total)).\n"));
        let (sql, _) = lower_query(&q, &rels).unwrap();
        assert!(sql.contains("COALESCE(SUM(\"amount\"), 0) AS \"total\""), "empty-sum pinned to 0: {sql}");
    }

    /// Zero group vars (all wildcards + one aggregate) = a whole-rel aggregate:
    /// one row, no GROUP BY, no ORDER BY.
    #[test]
    fn query_agg_whole_rel_no_group_by() {
        let (q, rels) = query_and_rels(concat!(
            "rel hit(bucket: text, amount: int).\n",
            "? hit(_, count(total)).\n"));
        let (sql, headers) = lower_query(&q, &rels).unwrap();
        assert!(sql.contains("COUNT(\"amount\") AS \"total\""), "{sql}");
        assert!(!sql.contains("GROUP BY"), "no group by for whole-rel aggregate: {sql}");
        assert!(!sql.contains("ORDER BY"), "no order by for one-row aggregate: {sql}");
        assert_eq!(headers, vec!["total".to_string()]);
    }

    /// A literal term stays a WHERE filter alongside the GROUP BY.
    #[test]
    fn query_agg_literal_filter_plus_group() {
        let (q, rels) = query_and_rels(concat!(
            "rel line(kind: text, item: text, price: int).\n",
            "? line(\"food\", item, sum(total)).\n"));
        let (sql, _) = lower_query(&q, &rels).unwrap();
        assert!(sql.contains("WHERE \"kind\" = 'food'"), "literal filters: {sql}");
        assert!(sql.contains("GROUP BY \"item\""), "grouped by the surviving var: {sql}");
    }

    /// json_group_object(key, value) consumes column i (key) and column i+1
    /// (value); the term at i+1 must be `_`. Its header is the key arg label.
    #[test]
    fn query_agg_json_group_object_two_column() {
        let (q, rels) = query_and_rels(concat!(
            "rel line(order_id: text, item: text, price: int).\n",
            "? line(order_id, json_group_object(items, prices), _).\n"));
        let (sql, headers) = lower_query(&q, &rels).unwrap();
        assert!(sql.contains("json_group_object(\"item\", \"price\" ORDER BY \"item\") AS \"items\""),
            "key = col i, value = col i+1, ordered by key: {sql}");
        assert!(sql.contains("GROUP BY \"order_id\""), "{sql}");
        assert_eq!(headers, vec!["order_id".to_string(), "items".to_string()]);
    }

    /// json_group_object without a trailing `_` at the value column is the shaped
    /// two-column error.
    #[test]
    fn query_agg_json_group_object_missing_wildcard_bails() {
        let (q, rels) = query_and_rels(concat!(
            "rel line(order_id: text, item: text).\n",
            "? line(order_id, json_group_object(items, prices)).\n"));
        let e = lower_query(&q, &rels).unwrap_err().to_string();
        assert!(e.contains("consumes two columns") && e.contains("put _ at the value column"),
            "shaped two-column error: {e}");
    }

    /// An aggregate output label colliding with a bound group var is refused.
    #[test]
    fn query_agg_label_collision_bails() {
        let (q, rels) = query_and_rels(concat!(
            "rel hit(bucket: text, amount: int).\n",
            "? hit(bucket, count(bucket)).\n"));
        let e = lower_query(&q, &rels).unwrap_err().to_string();
        assert!(e.contains("collides") && e.contains("rename"), "collision named: {e}");
    }

    /// A non-aggregate function call in a query head keeps the existing bail.
    #[test]
    fn query_non_agg_call_still_bails() {
        let (q, rels) = query_and_rels(concat!(
            "rel raw(name: text).\n",
            "? raw(replace(name, \"a\", \"b\")).\n"));
        let e = lower_query(&q, &rels).unwrap_err().to_string();
        assert!(e.contains("function call not supported in a query head"), "{e}");
    }

    /// A body bind inlines the computed expression into every use site: the
    /// canon slot holds `replace(r0."callee_q", '.', '::')`, so the head SELECT
    /// projects the expression directly — no second evaluator, no extra FROM.
    #[test]
    fn bind_lowers_to_inlined_expr_sql() {
        let (rule, rels) = rule_and_rels(concat!(
            "rel raw_edge(caller: text, callee_q: text).\n",
            "rel resolved(caller: text, callee: text).\n",
            "resolved(caller, callee) <- raw_edge(caller, callee_q), callee = replace(callee_q, \".\", \"::\").\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("replace(r0.\"callee_q\", '.', '::')"), "inlined expr: {sql}");
        assert_eq!(sql.matches("FROM").count(), 1, "single FROM (no subquery): {sql}");
    }

    /// `+` dispatch: text + text lowers to `||`, int + int stays `+`.
    #[test]
    fn plus_dispatches_on_operand_types() {
        let (rule, rels) = rule_and_rels(concat!(
            "rel base_url(host: text).\n",
            "rel endpoint(url: text).\n",
            "endpoint(\"https://\" + host) <- base_url(host).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("'https://' || r0.\"host\""), "text + lowers to ||: {sql}");

        let (rule, rels) = rule_and_rels(concat!(
            "rel hit(line: int).\n",
            "rel next_line(value: int).\n",
            "next_line(line + 1) <- hit(line).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("r0.\"line\" + 1"), "int + stays addition: {sql}");
    }

    /// json aggregates lower to `json_group_array/object(... ORDER BY key)` —
    /// determinism rides the ORDER BY. The object form reads its value arg from
    /// `Rule::agg_args2`.
    #[test]
    fn json_aggs_lower_with_order_by() {
        let (rule, rels) = rule_and_rels(concat!(
            "rel src(g: text, name: text).\n",
            "rel arr(g: text, names: text).\n",
            "arr(g, json_group_array(name)) <- src(g, name).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("json_group_array(r0.\"name\" ORDER BY r0.\"name\")"),
            "array agg orders inside the call: {sql}");
        assert!(sql.contains("GROUP BY r0.\"g\""), "group key present: {sql}");

        let (rule, rels) = rule_and_rels(concat!(
            "rel src(g: text, k: text, v: text).\n",
            "rel obj(g: text, payload: text).\n",
            "obj(g, json_group_object(k, v)) <- src(g, k, v).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("json_group_object(r0.\"k\", r0.\"v\" ORDER BY r0.\"k\")"),
            "object agg reads key + value, orders by key: {sql}");
    }

    /// The json scalar constructors lower to their SQLite natives; odd-arity
    /// json_object is a loud mis-arity bail at lowering.
    #[test]
    fn json_scalar_fns_lower_and_arity() {
        let (rule, rels) = rule_and_rels(concat!(
            "rel src(a: text, b: text).\n",
            "rel out(payload: text).\n",
            "out(json_object(\"k\", a, \"j\", b)) <- src(a, b).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("json_object('k', r0.\"a\", 'j', r0.\"b\")"), "json_object native: {sql}");

        let (rule, rels) = rule_and_rels(concat!(
            "rel src(a: text).\n",
            "rel out(payload: text).\n",
            "out(json_array(a)) <- src(a).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("json_array(r0.\"a\")"), "json_array native: {sql}");

        // Odd-arity json_object: no matching arm -> the mis-arity bail.
        let (rule, rels) = rule_and_rels(concat!(
            "rel src(a: text, b: text, c: text).\n",
            "rel out(payload: text).\n",
            "out(json_object(a, b, c)) <- src(a, b, c).\n"));
        let e = lower_rule(&rule, &rels).unwrap_err().to_string();
        assert!(e.contains("mis-arity") || e.contains("json_object"), "odd json_object bails: {e}");
    }

    /// A bind var referenced inside a NEGATION joins the outer row (Cmp pass
    /// runs before the Neg pass), instead of minting an unconstrained local.
    #[test]
    fn bind_var_joins_negation() {
        let (rule, rels) = rule_and_rels(concat!(
            "rel raw_edge(caller: text, callee_q: text).\n",
            "rel blocked(name: text).\n",
            "rel resolved(caller: text).\n",
            "resolved(caller) <- raw_edge(caller, callee_q), callee = replace(callee_q, \".\", \"::\"), !blocked(callee).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("ax0.\"name\" = replace(r0.\"callee_q\", '.', '::')"),
            "negation must join the bind's expression: {sql}");
    }
}
