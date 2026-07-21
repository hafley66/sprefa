use anyhow::{bail, Result};
use std::collections::{HashMap, HashSet};

use crate::ast::*;

pub fn tbl(name: &str) -> String { format!("rel_{name}") }
pub fn txt_tbl(name: &str) -> String { format!("rel_{name}_txt") }

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

/// Decode a `sym` (interned StringId INTEGER) cell to its text through
/// `_strings`. An indexed point lookup on the INTEGER PRIMARY KEY — used only
/// where a text value is actually consumed (string fns, interpolation,
/// concat, projection into a text column or query output); sym = sym joins
/// and literal filters never decode.
fn sym_decode(e: &str) -> String {
    // A plain interned column decodes through `_strings`, BUT a df-coordinate id
    // carried in an ordinary `text` column (the whole `.dl` ecosystem: flow_edge,
    // string_flow, user reach closures, ...) has no `_strings` row after the
    // coordinate de-intern. Fall back to reconstructing it from `rel_df_node`, so
    // any text column holding a df id still displays its coordinate without the
    // author having to spell the `node` type. COALESCE short-circuits: for a
    // normal interned string (the _strings hit) the df_node fallback never runs.
    format!("COALESCE((SELECT content FROM _strings WHERE id = {e}), {})", coord_reconstruct(e))
}

/// The `rel_df_node` reconstruction subquery for a coordinate id cell (shared by
/// `coord_decode` and `sym_decode`'s fallback).
fn coord_reconstruct(e: &str) -> String {
    // Reconstruct `file:line:col:kind` from `_df_node_dict` — the authority for
    // a coordinate's columns, keyed by the dense surrogate `id` (2026-07-20
    // identity normalization). The dict has a row for EVERY coordinate that ever
    // received a surrogate (incl. a module-level template with no df_node row),
    // so this reconstructs uniformly where the old `rel_df_node` self-lookup
    // could miss. `file`/`kind` are the same interned StringIds, decoded through
    // `_strings`.
    format!(
        "(SELECT (SELECT content FROM _strings WHERE id = dnd.\"file\") || ':' || \
         dnd.\"line\" || ':' || dnd.\"col\" || ':' || \
         (SELECT content FROM _strings WHERE id = dnd.\"kind\") \
         FROM _df_node_dict dnd WHERE dnd.\"id\" = {e} LIMIT 1)"
    )
}

/// Decode a df-coordinate id cell (`Col::coord`, spelled `node`) to its
/// `file:line:col:kind` text by RECONSTRUCTING it from the `rel_df_node`
/// coordinate columns — the coordinate text is no longer interned into
/// `_strings` (it was 91.7% of the dictionary). Every coord id (`df_node.id`,
/// `df_edge.from/to`, `df_arg.call/arg`, `df_lit.id`, `df_param.id`,
/// `nest.call_id`, `template_parts.node`, and any user `node` column) is a
/// df_node id, so one df_node lookup reconstructs all of them; for `df_node.id`
/// itself this is a self-lookup that returns the same row's coordinate. `file`
/// and `kind` stay ordinary interned text (few distinct values), so they decode
/// through `_strings` inside the reconstruction.
fn coord_decode(e: &str) -> String {
    coord_reconstruct(e)
}

/// Decode a cell for a TEXT-consuming position, honoring the column's storage:
/// a df-coordinate id reconstructs (`coord_decode`), a plain interned sym
/// decodes through `_strings` (`sym_decode`), a raw/real column passes through.
fn decode_cell(cell: &str, interned: bool, coord: bool) -> String {
    if coord {
        coord_decode(cell)
    } else if interned {
        sym_decode(cell)
    } else {
        cell.to_string()
    }
}

/// A text literal compared against / inserted into a `sym` column lowers to
/// its content-addressed StringId at compile time — no lookup, no decode.
fn sym_lit(s: &str) -> String {
    crate::spine::StringId::of(s).sqlite().to_string()
}

/// Equality between two cells that may disagree on sym-ness: text = sym stays
/// an int compare; sym vs text hashes the TEXT side (`sprf_sym`) so the sym
/// side never decodes; everything else is plain equality.
#[derive(Clone, Copy, Debug)]
struct VarTy { ty: Type, interned: bool, coord: bool }
type TyEnv = HashMap<String, VarTy>;

fn var_ty(tys: &TyEnv, var: &str) -> Option<Type> { tys.get(var).map(|t| t.ty) }
fn var_interned(tys: &TyEnv, var: &str) -> bool {
    tys.get(var).is_some_and(|t| t.interned)
}
fn var_coord(tys: &TyEnv, var: &str) -> bool {
    tys.get(var).is_some_and(|t| t.coord)
}
fn term_interned(term: &Term, tys: &TyEnv) -> bool {
    match term {
        Term::Var(v) => var_interned(tys, v),
        Term::Call { name, args } if name == "sym" && args.len() == 1 => term_interned(&args[0], tys),
        _ => false,
    }
}

fn eq_cond(a_sql: &str, a_ty: Option<Type>, a_interned: bool,
           b_sql: &str, b_ty: Option<Type>, b_interned: bool) -> String {
    if a_ty.is_some_and(|t| t.textish()) && b_ty.is_some_and(|t| t.textish()) {
        return match (a_interned, b_interned) {
            (true, false) => format!("{a_sql} = sprf_sym({b_sql})"),
            (false, true) => format!("sprf_sym({a_sql}) = {b_sql}"),
            _ => format!("{a_sql} = {b_sql}"),
        };
    }
    format!("{a_sql} = {b_sql}")
}

/// Lower a head term destined for column `col`. Sym columns accept a
/// sym-typed var (int pass-through) or a text literal (compile-time hash) —
/// a computed/text value has no `_strings` row to decode later, so it bails
/// with the fix. Text-ish columns decode sym vars; everything else is plain.
fn head_term_sql(term: &Term, col: &Col, canon: &HashMap<String, String>, tys: &TyEnv) -> Result<String> {
    if col.interned() {
        // A df-coordinate id column stores the hash but never interns the text,
        // so a literal/computed value written into it hashes with `sprf_sym`
        // (pure, no `_strings` queue) rather than `sprf_sym_intern`. An interned
        // source var still passes its int handle through unchanged.
        let hash_fn = if col.coord { "sprf_sym" } else { "sprf_sym_intern" };
        return match term {
            Term::Str(s) => Ok(format!("{hash_fn}('{}')", esc(s))),
            Term::Var(v) if var_interned(tys, v) => term_sql(term, canon, tys),
            Term::Call { name, args } if name == "sym" && args.len() == 1
                && term_interned(&args[0], tys) => term_sql(&args[0], canon, tys),
            _ => Ok(format!("{hash_fn}({})", term_sql_text(term, canon, tys)?)),
        };
    }
    // Non-interned target column: decode any interned source var to its text so
    // a StringId never lands raw in a real-valued column. `term_sql_text` is
    // identical to `term_sql` for non-interned terms, and for an interned source
    // flowing into an `int` column the decoded text ("30") coerces correctly.
    term_sql_text(term, canon, tys)
}

/// `term_sql` for a TEXT-consuming position: a sym-typed var decodes through
/// `_strings`; everything else lowers exactly as `term_sql`.
fn term_sql_text(t: &Term, canon: &HashMap<String, String>, tys: &TyEnv) -> Result<String> {
    if let Term::Call { name, args } = t {
        if name == "sym" && args.len() == 1 {
            return term_sql_text(&args[0], canon, tys);
        }
    }
    if let Term::Var(v) = t {
        if var_coord(tys, v) {
            let cell = canon.get(v)
                .ok_or_else(|| anyhow::anyhow!("unbound variable {v}"))?;
            return Ok(coord_decode(cell));
        }
        if var_interned(tys, v) {
            let cell = canon.get(v)
                .ok_or_else(|| anyhow::anyhow!("unbound variable {v}"))?;
            return Ok(sym_decode(cell));
        }
    }
    term_sql(t, canon, tys)
}

fn lit_sql(t: &Term) -> Option<String> {
    match t {
        Term::Str(s) => Some(format!("'{}'", esc(s))),
        Term::Int(n) => Some(n.to_string()),
        _ => None,
    }
}

/// SQL string concatenation for an interpolated term: literals quoted, vars are
/// the canonical column reference. `"${ty}::${name}"` -> `ty_col || '::' || name_col`.
fn interp_sql(parts: &[InterpPart], canon: &HashMap<String, String>, tys: &TyEnv) -> Result<String> {
    let mut pieces = Vec::new();
    for p in parts {
        pieces.push(match p {
            InterpPart::Lit(s) => format!("'{}'", esc(s)),
            InterpPart::Var(v) => {
                let cell = canon.get(v).cloned()
                    .ok_or_else(|| anyhow::anyhow!("unbound variable {v} in interpolation"))?;
                // Interpolation consumes text: a coord id reconstructs, a sym
                // var decodes here.
                decode_cell(&cell, var_interned(tys, v), var_coord(tys, v))
            }
        });
    }
    Ok(if pieces.is_empty() { "''".into() } else { pieces.join(" || ") })
}

/// Static type of a term at lower time, from the body's var->column-type map.
/// `None` = unknown (an unbound var, a wildcard). Drives the `+` overload:
/// int + int stays SQL addition, text + text lowers to `||`. All non-int base
/// types (path/file/dir/repo/rev) store TEXT, so they concat like text.
fn term_ty(t: &Term, tys: &TyEnv) -> Option<Type> {
    match t {
        Term::Var(v) => var_ty(tys, v),
        Term::Str(_) | Term::Interp(_) => Some(Type::Text),
        Term::Int(_) => Some(Type::Int),
        Term::Call { name, .. } => Some(if matches!(name.as_str(), "int" | "len" | "lines") { Type::Int } else { Type::Text }),
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

fn term_sql(t: &Term, canon: &HashMap<String, String>, tys: &TyEnv) -> Result<String> {
    match t {
        Term::Var(v) => canon.get(v).cloned()
            .ok_or_else(|| anyhow::anyhow!("unbound variable {v}")),
        Term::Str(_) | Term::Int(_) => Ok(lit_sql(t).unwrap()),
        Term::Interp(parts) => interp_sql(parts, canon, tys),
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
            // Text concat consumes text: text operands decode.
            if sql_op == "||" {
                return Ok(format!("({} || {})",
                    term_sql_text(lhs, canon, tys)?, term_sql_text(rhs, canon, tys)?));
            }
            Ok(format!("({} {sql_op} {})", term_sql(lhs, canon, tys)?, term_sql(rhs, canon, tys)?))
        }
        Term::Arith { op, lhs, rhs } => Ok(format!(
            "({} {} {})", term_sql(lhs, canon, tys)?, op.sql(), term_sql(rhs, canon, tys)?)),
        Term::Call { name, args } => {
            // Every function consumes decoded text (string fns; int() decodes
            // then casts) — sym args decode at the argument boundary.
            let arg_sqls: Vec<String> = args.iter()
                .map(|a| term_sql_text(a, canon, tys)).collect::<Result<_>>()?;
            match name.as_str() {
                // SQLite native: replace(X, Y, Z) replaces all Y in X with Z.
                "replace" if args.len() == 3 =>
                    Ok(format!("replace({})", arg_sqls.join(", "))),
                // Registered UDF (db.rs): sprf_split(text, sep, idx).
                "split" if args.len() == 3 =>
                    Ok(format!("sprf_split({})", arg_sqls.join(", "))),
                "sym" if args.len() == 1 => Ok(arg_sqls[0].clone()),
                // SQLite native: text->int coercion (leading-int prefix, else 0),
                // so a numeric shell/json string can fill an int column or be
                // compared numerically instead of as text against "0".
                "int" if args.len() == 1 =>
                    Ok(format!("CAST({} AS INTEGER)", arg_sqls[0])),
                // SQLite native: character length of a text value.
                "len" if args.len() == 1 =>
                    Ok(format!("length({})", arg_sqls[0])),
                // Registered UDF: line count of a text value (file-size rail).
                "lines" if args.len() == 1 =>
                    Ok(format!("sprf_lines({})", arg_sqls[0])),
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
                other => bail!("unknown or mis-arity function `{other}` (known: split/3, replace/3, int/1, len/1, \
                    json_object/even>=2, json_array/>=1, json/1, {})",
                    STR_FNS.iter().map(|(n, _, k)| format!("{n}/{k}")).collect::<Vec<_>>().join(", ")),
            }
        }
    }
}

/// Resolve every literal `WORK_ALIAS` ("WORK") term bound to a `Type::Rev`
/// column — in the rule's body Pos/Neg atoms AND its head — to
/// `resolved_work` (the tick's cached worktree rev text; see
/// `Engine::self_rev_text`). A rev column stores the RESOLVED revision
/// identity, never the alias (revid.rs INV-1): a program that filters or
/// writes the literal "WORK" against a `Type::Rev`-typed position would
/// otherwise silently match nothing (a filter) or store a nonsense value (a
/// write), because storage moved off the alias-as-text convention while the
/// program text still spells the alias. Positions typed anything else —
/// including a plain `Type::Text` column that happens to hold the literal
/// word "WORK" for an unrelated reason (a fact table, a display string) —
/// are left untouched, so this is NOT a blunt substitution of the string
/// "WORK": `.dl/dishonest-flag.dl`'s literal source-text matches over Rust
/// code never pass through this function at all (they scan Rust files, not
/// this program's own rule AST).
///
/// Runs once per rule, immediately before lowering, at every call site that
/// crosses from the AST into a `lower::` SQL-emission entry point. lower.rs
/// itself has no `Engine` access (see `Engine::resolve_rev` /
/// `Engine::self_rev_text`, engine/repo.rs) and must not re-probe git per
/// literal, so the resolution happens here, as a rewrite on an owned clone,
/// using a value the engine already resolved once at the top of this tick.
///
/// GAP, named rather than faked: a `Cmp` comparison (`rev == "WORK"` where
/// `rev` is a bound Var, not a positional atom argument) is not resolved —
/// that needs the var's inferred type, which only exists inside
/// `body_sql_ex`'s own traversal, after this function has already run. No
/// currently known `.dl` program compares WORK this way; a future one would
/// need this function ported into `body_sql_ex`'s `TyEnv`-aware Cmp pass.
pub fn resolve_work_alias(rule: &Rule, rels: &Rels, resolved_work: &str) -> Rule {
    let mut rule = rule.clone();
    resolve_work_alias_body(&mut rule.body, rels, resolved_work);
    if let Some(head_meta) = rels.get(&rule.head.rel) {
        resolve_rev_terms(&mut rule.head.terms, &head_meta.cols, resolved_work);
    }
    rule
}

/// The body-only half of `resolve_work_alias`, for the headless lowering
/// entry points (`lower_gen`, `lower_body_projection`) whose callers hold a
/// `&[BodyItem]` with no enclosing `Rule`/head to resolve.
pub fn resolve_work_alias_body(body: &mut [BodyItem], rels: &Rels, resolved_work: &str) {
    for item in body {
        if let BodyItem::Pos(atom) | BodyItem::Neg(atom) = item {
            if let Some(meta) = rels.get(&atom.rel) {
                resolve_rev_terms(&mut atom.terms, &meta.cols, resolved_work);
            }
        }
    }
}

/// The `?` query half of `resolve_work_alias`: a `Query` is a bare head `Atom`
/// with no body (`lower_query` is its own SQL-emission entry point, entirely
/// separate from `body_sql_ex`/`lower_rule_to_ex`), so it needs its own small
/// wrapper rather than routing through `resolve_work_alias`. Same rule: a
/// literal "WORK" bound to a `Type::Rev`-typed position resolves; everything
/// else is untouched.
pub fn resolve_work_alias_query(q: &Query, rels: &Rels, resolved_work: &str) -> Query {
    let mut q = q.clone();
    if let Some(meta) = rels.get(&q.head.rel) {
        resolve_rev_terms(&mut q.head.terms, &meta.cols, resolved_work);
    }
    q
}

/// Replace `Term::Str(WORK_ALIAS)` with `resolved_work` at every position
/// whose declared column is `Type::Rev`, in step with `cols`. Extra/missing
/// terms (an arity mismatch the caller's own validation will reject) simply
/// stop at the shorter side via `zip`.
fn resolve_rev_terms(terms: &mut [Term], cols: &[Col], resolved_work: &str) {
    for (term, col) in terms.iter_mut().zip(cols.iter()) {
        if col.ty == Type::Rev {
            if let Term::Str(s) = term {
                if s == crate::engine::WORK_ALIAS {
                    *term = Term::Str(resolved_work.to_string());
                }
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
    -> Result<(HashMap<String, String>, TyEnv, Vec<String>, Vec<String>)>
{
    body_sql_ex(body, rels, &HashMap::new())
}

/// Like `body_sql`, but the k-th positive body atom (0-based, counting only
/// `BodyItem::Pos` occurrences in body order — the same `k` that names its
/// SQL alias `r{k}`) reads from `overrides[&k]` instead of its own relation's
/// table when present. This is the semi-naive differentiation seam: a
/// recursive component reruns a rule once per recursive body-atom occurrence,
/// each variant substituting that ONE occurrence's table for a `_delta_<rel>`
/// snapshot (rows new as of the previous iteration) while every other
/// occurrence still reads the full accumulated relation. See engine
/// `rebuild_derived`.
fn body_sql_ex(body: &[BodyItem], rels: &Rels, overrides: &HashMap<usize, String>)
    -> Result<(HashMap<String, String>, TyEnv, Vec<String>, Vec<String>)>
{
    let mut canon: HashMap<String, String> = HashMap::new();
    let mut tys: TyEnv = HashMap::new();
    let mut wheres: Vec<String> = Vec::new();
    let mut froms: Vec<String> = Vec::new();
    let computed_vars: HashSet<String> = body.iter().filter_map(|item| match item {
        BodyItem::Cmp(c) if c.op == CmpOp::Eq => match (&c.lhs, &c.rhs) {
            (Term::Var(v), rhs) if v != "_" && has_computation(rhs) => Some(v.clone()),
            (lhs, Term::Var(v)) if v != "_" && has_computation(lhs) => Some(v.clone()),
            _ => None,
        },
        _ => None,
    }).collect();
    let mut deferred: Vec<(String, String, Type, bool)> = Vec::new();
    let mut k = 0usize;

    for item in body {
        if let BodyItem::Pos(a) = item {
            let meta = rels.get(&a.rel).ok_or_else(|| anyhow::anyhow!("unknown relation {}", a.rel))?;
            if a.terms.len() != meta.cols.len() {
                bail!("relation {} expects {} cols, got {}", a.rel, meta.cols.len(), a.terms.len());
            }
            let alias = format!("r{k}");
            let src = overrides.get(&k).cloned().unwrap_or_else(|| tbl(&a.rel));
            froms.push(format!("{src} {alias}"));
            for (pos, term) in a.terms.iter().enumerate() {
                let cell = format!("{alias}.\"{}\"", meta.col_name(pos));
                match term {
                    Term::Var(v) if computed_vars.contains(v) && !canon.contains_key(v) => {
                        deferred.push((v.clone(), cell, meta.cols[pos].ty, meta.cols[pos].interned()));
                    }
                    Term::Var(v) => match canon.get(v) {
                        Some(prev) => wheres.push(eq_cond(
                            &cell, Some(meta.cols[pos].ty), meta.cols[pos].interned(),
                            prev, var_ty(&tys, v), var_interned(&tys, v))),
                        None => {
                            canon.insert(v.clone(), cell);
                            tys.insert(v.clone(), VarTy {
                                ty: meta.cols[pos].ty,
                                interned: meta.cols[pos].interned(),
                                coord: meta.cols[pos].coord,
                            });
                        }
                    },
                    // A text literal against a sym column filters by its
                    // compile-time StringId — an int compare, no decode.
                    Term::Str(s) if meta.cols[pos].interned() =>
                        wheres.push(format!("{cell} = {}", sym_lit(s))),
                    Term::Str(_) | Term::Int(_) => wheres.push(format!("{cell} = {}", lit_sql(term).unwrap())),
                    Term::Interp(_) => bail!("interpolated string only allowed in a rule head, not a body atom"),
                    Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                    Term::Arith { .. } => wheres.push(format!("{cell} = {}", term_sql(term, &canon, &tys)?)),
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
                        if let Some(t) = term_ty(rhs, &tys) {
                            tys.insert(v.clone(), VarTy { ty: t, interned: false, coord: false });
                        }
                        canon.insert(v.clone(), e);
                        true
                    }
                    (lhs, Term::Var(v)) if v != "_" && !canon.contains_key(v) && has_computation(lhs) => {
                        let e = term_sql(lhs, &canon, &tys)?;
                        if let Some(t) = term_ty(lhs, &tys) {
                            tys.insert(v.clone(), VarTy { ty: t, interned: false, coord: false });
                        }
                        canon.insert(v.clone(), e);
                        true
                    }
                    _ => false,
                };
                if bound { continue; }
            }
            let (lt, rt) = (term_ty(&c.lhs, &tys), term_ty(&c.rhs, &tys));
            let lhs_interned = term_interned(&c.lhs, &tys);
            let rhs_interned = term_interned(&c.rhs, &tys);
            if matches!(c.op, CmpOp::Eq | CmpOp::Ne)
                && (lhs_interned || rhs_interned)
            {
                let l = match &c.lhs {
                    Term::Str(s) if rhs_interned => sym_lit(s),
                    _ => term_sql(&c.lhs, &canon, &tys)?,
                };
                let r = match &c.rhs {
                    Term::Str(s) if lhs_interned => sym_lit(s),
                    _ => term_sql(&c.rhs, &canon, &tys)?,
                };
                let eq = eq_cond(&l, lt, lhs_interned || (matches!(&c.lhs, Term::Str(_)) && rhs_interned),
                    &r, rt, rhs_interned || (matches!(&c.rhs, Term::Str(_)) && lhs_interned));
                wheres.push(if c.op == CmpOp::Eq { eq } else { format!("NOT ({eq})") });
                continue;
            }
            // Ordering (or non-sym compare): text sides decode to text.
            let l = term_sql_text(&c.lhs, &canon, &tys)?;
            let r = term_sql_text(&c.rhs, &canon, &tys)?;
            wheres.push(format!("{l} {} {r}", c.op.sql()));
        }
    }

    for (v, cell, ty, interned) in deferred {
        let prev = canon.get(&v)
            .ok_or_else(|| anyhow::anyhow!("computed variable {v} was not bound"))?;
        wheres.push(eq_cond(&cell, Some(ty), interned, prev,
            var_ty(&tys, &v), var_interned(&tys, &v)));
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
                            sub.push(eq_cond(&cell, Some(meta.cols[pos].ty), meta.cols[pos].interned(),
                                outer, var_ty(&tys, v), var_interned(&tys, v)));
                        } else if let Some(prev) = local.get(v) {
                            sub.push(format!("{cell} = {prev}"));
                        } else {
                            local.insert(v.clone(), cell);
                        }
                    }
                    Term::Str(s) if meta.cols[pos].interned() =>
                        sub.push(format!("{cell} = {}", sym_lit(s))),
                    Term::Str(_) | Term::Int(_) => sub.push(format!("{cell} = {}", lit_sql(term).unwrap())),
                    Term::Interp(_) => bail!("interpolated string only allowed in a rule head, not a body atom"),
                    Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
                    Term::Arith { .. } => sub.push(format!("{cell} = {}", term_sql(term, &canon, &tys)?)),
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
    let (canon, tys, froms, wheres) = body_sql(body, rels)?;
    if froms.is_empty() {
        bail!("@async rule body has no positive atom to bind request args");
    }
    let mut exprs = Vec::new();
    for v in vars {
        let e = term_sql_text(&Term::Var(v.clone()), &canon, &tys)?;
        exprs.push(format!("{e} AS \"{v}\""));
    }
    // A rule that binds no vars (its request identity lives entirely in the
    // digest, e.g. a wildcard-bucket `clock(secs, _)` salt) still needs the body
    // as an emission gate: project a constant so satisfiability yields exactly
    // one row and an empty body result emits nothing.
    if exprs.is_empty() { exprs.push("1 AS \"__gate\"".to_string()); }
    let where_sql = if wheres.is_empty() { String::new() } else { format!(" WHERE {}", wheres.join(" AND ")) };
    Ok(format!("SELECT DISTINCT {} FROM {}{}", exprs.join(", "), froms.join(", "), where_sql))
}

// ARCH {"url":"30-lower","role":"compile"}
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
    lower_rule_to_ex(rule, rels, target, extra, &HashMap::new())
}

/// Every occurrence index (the `k` in body_sql_ex's `overrides`) of a
/// positive body atom whose relation name is in `comp_rels` — the semi-naive
/// engine's recursive-atom occurrences for this rule, in body order.
pub fn recursive_occurrences(rule: &Rule, comp_rels: &std::collections::HashSet<String>) -> Vec<(usize, String)> {
    let mut k = 0usize;
    let mut out = Vec::new();
    for item in &rule.body {
        if let BodyItem::Pos(a) = item {
            if comp_rels.contains(&a.rel) { out.push((k, a.rel.clone())); }
            k += 1;
        }
    }
    out
}

/// `lower_rule_to` with the semi-naive `overrides` seam threaded through the
/// body: `overrides[&k]` replaces the k-th positive body atom's table (see
/// `body_sql_ex`). Empty overrides is byte-identical to `lower_rule_to`.
pub fn lower_rule_to_ex(rule: &Rule, rels: &Rels, target: &str, extra: &[(String, String)], overrides: &HashMap<usize, String>) -> Result<String> {
    let (canon, tys, froms, mut wheres) = body_sql_ex(&rule.body, rels, overrides)?;

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
    if let Some(merge) = &head_meta.merge {
        let (mc, cmp) = merge.col_and_cmp();
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
        for (term, col) in rule.head.terms.iter().zip(head_meta.cols.iter()) {
            if matches!(term, Term::Wild) { exprs.push("NULL".into()); continue; }
            let source = term_sql_text(term, &canon, &tys)?;
            let e = head_term_sql(term, col, &canon, &tys)?;
            if has_call(term) { wheres.push(format!("{source} IS NOT NULL")); }
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
            "INSERT INTO {} ({}) SELECT {}{}{} ON CONFLICT({}) DO UPDATE SET {} WHERE excluded.\"{}\" {} \"{}\"",
            target,
            cols.join(", "),
            exprs.join(", "),
            from_sql,
            where_sql,
            key_cols.join(", "),
            set_clause.join(", "),
            mc, cmp, mc,
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
                        // JSON is a text consumer.  Interned source columns
                        // carry StringIds in the base table, but SQLite's JSON
                        // aggregate must see the decoded value (otherwise it
                        // serializes the integer id, and json() cannot nest it).
                        _ if matches!(f, AggFn::JsonGroupArray | AggFn::JsonGroupObject) =>
                            term_sql_text(term, &canon, &tys)?,
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
                            let json = format!("json_group_array({arg} ORDER BY {arg})");
                            exprs.push(if head_meta.cols[i].interned() {
                                format!("sprf_sym_intern({json})")
                            } else { json });
                        }
                        AggFn::JsonGroupObject => {
                            if matches!(term, Term::Wild) {
                                bail!("json_group_object(_, ..) has no key to build — pass a column, not `_`");
                            }
                            let val = rule.agg_args2.get(i).and_then(|a| a.as_ref())
                                .ok_or_else(|| anyhow::anyhow!(
                                    "json_group_object expects (key, value) — the value arg is missing"))?;
                            let val_sql = term_sql_text(val, &canon, &tys)?;
                            let json = format!("json_group_object({arg}, {val_sql} ORDER BY {arg})");
                            exprs.push(if head_meta.cols[i].interned() {
                                format!("sprf_sym_intern({json})")
                            } else { json });
                        }
                        AggFn::Count | AggFn::Min | AggFn::Max =>
                            exprs.push(format!("{}({arg})", f.sql())),
                    }
                }
                None => {
                    let g = head_term_sql(term, &head_meta.cols[i], &canon, &tys)?;
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
    for (term, col) in rule.head.terms.iter().zip(head_meta.cols.iter()) {
        // A `Term::Wild` head slot comes from head named-arg padding: a sink
        // rule that names only some columns (`diag(path: p, line: l, msg: m)`)
        // leaves the rest unset. Project SQL NULL so the reader can default it.
        // Sink use only — a NULL never dedups in a fixpoint delta (NULL != NULL),
        // so a Wild in a RECURSIVE head would diverge. Enforced upstream:
        // typecheck's `recursive-null-pad` diag + the `rebuild_derived` bail
        // (both via `Rule::head_null_pads`), so this arm never runs for a
        // recursive component.
        if matches!(term, Term::Wild) { exprs.push("NULL".into()); continue; }
        let source = term_sql_text(term, &canon, &tys)?;
        let e = head_term_sql(term, col, &canon, &tys)?;
        // A head term containing a Call (split/replace) may evaluate to NULL
        // when the function misses (split out-of-range). A NULL row inserted
        // into the derived table never dedups in the fixpoint delta (NULL !=
        // NULL), so convergence breaks. Guard: filter NULL-producing head
        // expressions out of the SELECT entirely. The row drops, which is the
        // intended "no match" semantics.
        if has_call(term) { wheres.push(format!("{source} IS NOT NULL")); }
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
    let (canon, tys, froms, wheres) = body_sql(body, rels)?;
    if froms.is_empty() { bail!("gen body has no positive atom"); }
    let mut sel = Vec::new();
    for v in vars {
        let cell = term_sql_text(&Term::Var(v.clone()), &canon, &tys)?;
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
    let mut canon: HashMap<String, (String, bool, Type)> = HashMap::new();
    let mut wheres: Vec<String> = Vec::new();
    let mut sel: Vec<String> = Vec::new();
    let mut headers: Vec<String> = Vec::new();

    let from_tbl = tbl(&q.head.rel);
    for (pos, term) in q.head.terms.iter().enumerate() {
        // Table-qualified: `_strings` also has a column literally named `id`,
        // so a bare `"id"` inside sym_decode's correlated subquery would bind
        // to `_strings.id` (the innermost scope wins), not this outer cell —
        // silently returning ONE arbitrary row's content for every outer row.
        // Qualifying with the source table name disambiguates.
        let cell = format!("{from_tbl}.\"{}\"", meta.col_name(pos));
        let is_sym = meta.cols[pos].interned();
        match term {
            Term::Var(v) => match canon.get(v) {
                // Equality between two head positions binding the same var stays
                // a RAW compare (int=int when both are sym) — only the final
                // user-visible projection decodes.
                Some((prev, prev_interned, prev_ty)) => wheres.push(eq_cond(
                    &cell, Some(meta.cols[pos].ty), is_sym,
                    prev, Some(*prev_ty), *prev_interned)),
                None => {
                    canon.insert(v.clone(), (cell.clone(), is_sym, meta.cols[pos].ty));
                    // A sym column decodes through `_strings` for display so `?`
                    // output stays human text; a df-coordinate id reconstructs
                    // from `rel_df_node` (`coord_decode`). The id itself is an
                    // opaque join key, never a query-visible value.
                    let display = decode_cell(&cell, is_sym, meta.cols[pos].coord);
                    sel.push(format!("{display} AS \"{v}\""));
                    headers.push(v.clone());
                }
            },
            // A text literal against a sym column filters by its compile-time
            // StringId — an int compare, no decode (mirrors the body-atom arm).
            Term::Str(s) if is_sym => wheres.push(format!("{cell} = {}", sym_lit(s))),
            Term::Str(_) | Term::Int(_) => wheres.push(format!("{cell} = {}", lit_sql(term).unwrap())),
            Term::Interp(_) => bail!("interpolated string not supported in a query head"),
            Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
            Term::Arith { .. } => bail!("arithmetic not supported in a query head (derive a relation with the computed column and query that)"),
            Term::Call { .. } => bail!("function call not supported in a query head (derive a relation with the computed column and query that)"),
            Term::Wild => {}
        }
    }
    if sel.is_empty() {
        sel = meta.cols.iter().map(|col| {
            let cell = format!("{from_tbl}.\"{}\"", col.name);
            if col.interned() { sym_decode(&cell) } else { cell }
        }).collect();
    }
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
    let mut canon: HashMap<String, (String, bool, Type)> = HashMap::new();
    let mut wheres: Vec<String> = Vec::new();
    let mut sel: Vec<String> = Vec::new();
    let mut headers: Vec<String> = Vec::new();
    let mut group_cells: Vec<String> = Vec::new();
    let mut order_cells: Vec<String> = Vec::new();

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
        let raw_cell = format!("\"{}\"", meta.col_name(i));
        let cell = format!("{}.\"{}\"", tbl(&q.head.rel), meta.col_name(i));
        let display_cell = decode_cell(&cell, meta.cols[i].interned(), meta.cols[i].coord);
        match &terms[i] {
            Term::Wild => {}
            Term::Str(s) if meta.cols[i].interned() => wheres.push(format!("{raw_cell} = {}", sym_lit(s))),
            Term::Str(_) | Term::Int(_) => wheres.push(format!("{raw_cell} = {}", lit_sql(&terms[i]).unwrap())),
            Term::Interp(_) => bail!("interpolated string not supported in a query head"),
            Term::PathLit { .. } => bail!("path literal not normalized before lowering"),
            Term::Arith { .. } => bail!("arithmetic not supported in a query head (derive a relation with the computed column and query that)"),
            Term::Var(v) => match canon.get(v) {
                Some((prev, prev_interned, prev_ty)) => wheres.push(eq_cond(
                    &cell, Some(meta.cols[i].ty), meta.cols[i].interned(),
                    prev, Some(*prev_ty), *prev_interned)),
                None => {
                    canon.insert(v.clone(), (cell.clone(), meta.cols[i].interned(), meta.cols[i].ty));
                    sel.push(format!("{display_cell} AS \"{v}\""));
                    headers.push(v.clone());
                    group_cells.push(raw_cell.clone());
                    order_cells.push(display_cell.clone());
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
                    let val_raw_cell = format!("\"{}\"", meta.col_name(i + 1));
                    let val_cell = format!("{}.\"{}\"", tbl(&q.head.rel), meta.col_name(i + 1));
                    let key_text = decode_cell(&cell, meta.cols[i].interned(), meta.cols[i].coord);
                    let val_text = decode_cell(&val_cell, meta.cols[i + 1].interned(), meta.cols[i + 1].coord);
                    let key_agg = if meta.cols[i].interned() { key_text } else { raw_cell.clone() };
                    let val_agg = if meta.cols[i + 1].interned() { val_text } else { val_raw_cell };
                    sel.push(format!("json_group_object({key_agg}, {val_agg} ORDER BY {key_agg}) AS \"{label}\""));
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
                        AggFn::JsonGroupArray => format!("json_group_array({display_cell} ORDER BY {display_cell})"),
                        // SUM over zero rows is SQL NULL; pin empty-sum to 0.
                        AggFn::Sum => format!("COALESCE(SUM({cell}), 0)"),
                        AggFn::Count => format!("{}({raw_cell})", f.sql()),
                        AggFn::Min | AggFn::Max => format!("{}({display_cell})", f.sql()),
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
         format!(" ORDER BY {}", order_cells.join(", ")))
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
        assert!(sql.contains("json_group_array(COALESCE((SELECT content FROM _strings")
            && sql.contains("ORDER BY COALESCE((SELECT content FROM _strings")
            && sql.contains("AS \"names\""),
            "array agg decodes and orders inside the call, labeled by the arg var: {sql}");
        assert!(sql.contains("GROUP BY \"group_col\""), "group key present: {sql}");
        assert!(sql.contains("ORDER BY COALESCE((SELECT content FROM _strings"), "deterministic decoded order by group: {sql}");
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
        assert!(sql.contains("COALESCE(SUM(rel_hit.\"amount\"), 0) AS \"total\""), "empty-sum pinned to 0: {sql}");
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
        assert!(sql.contains(&format!("WHERE \"kind\" = {}", sym_lit("food"))), "literal filters: {sql}");
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
        assert!(sql.contains("json_group_object(COALESCE((SELECT content FROM _strings")
            && sql.contains("\"price\" ORDER BY COALESCE((SELECT content FROM _strings")
            && sql.contains("AS \"items\""),
            "key = col i, value = col i+1, ordered by decoded key: {sql}");
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
        assert!(sql.contains("replace(") && sql.contains("callee_q") && sql.contains("sprf_sym_intern"), "inlined expr: {sql}");
        assert_eq!(sql.matches("FROM rel_raw_edge").count(), 1, "single relation FROM: {sql}");
    }

    /// `+` dispatch: text + text lowers to `||`, int + int stays `+`.
    #[test]
    fn plus_dispatches_on_operand_types() {
        let (rule, rels) = rule_and_rels(concat!(
            "rel base_url(host: text).\n",
            "rel endpoint(url: text).\n",
            "endpoint(\"https://\" + host) <- base_url(host).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("'https://' ||") && sql.contains("content FROM _strings"), "text + lowers to ||: {sql}");

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
        // text columns are interned: json (a text consumer) must see the decoded
        // value, and the interned head column re-interns the assembled json.
        // `sym_decode` now COALESCEs the `_strings` lookup with a df_node
        // coordinate fallback; build the expected text from it so the two stay in
        // sync.
        let name_txt = sym_decode("r0.\"name\"");
        assert!(sql.contains(&format!("sprf_sym_intern(json_group_array({name_txt} ORDER BY {name_txt}))")),
            "array agg decodes input, orders inside the call, re-interns: {sql}");
        assert!(sql.contains("GROUP BY r0.\"g\""), "group key present: {sql}");

        let (rule, rels) = rule_and_rels(concat!(
            "rel src(g: text, k: text, v: text).\n",
            "rel obj(g: text, payload: text).\n",
            "obj(g, json_group_object(k, v)) <- src(g, k, v).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        let k_txt = sym_decode("r0.\"k\"");
        let v_txt = sym_decode("r0.\"v\"");
        assert!(sql.contains(&format!("sprf_sym_intern(json_group_object({k_txt}, {v_txt} ORDER BY {k_txt}))")),
            "object agg decodes key + value, orders by key, re-interns: {sql}");
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
        assert!(sql.contains("json_object('k'") && sql.contains("'j'") && sql.contains("sprf_sym_intern"), "json_object native: {sql}");

        let (rule, rels) = rule_and_rels(concat!(
            "rel src(a: text).\n",
            "rel out(payload: text).\n",
            "out(json_array(a)) <- src(a).\n"));
        let sql = lower_rule(&rule, &rels).unwrap();
        assert!(sql.contains("json_array(") && sql.contains("sprf_sym_intern"), "json_array native: {sql}");

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
        assert!(sql.contains("ax0.\"name\" = sprf_sym(replace(") && sql.contains("callee_q"),
            "negation must join the bind's expression: {sql}");
    }
}
