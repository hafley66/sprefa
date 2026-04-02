/// Compile QueryDef (Datalog horn clauses) to SQL.
///
/// Each QueryDef becomes a CTE. Recursive queries use WITH RECURSIVE.
/// Variable unification across body atoms becomes JOIN ON conditions.
///
/// Relations resolve to:
///   1. Known query → CTE reference
///   2. Known rule  → group_id JOIN over matches (rule-as-relation)
///   3. Built-in    → SQL over base tables
///   4. Fallback    → link_kind in match_links
///
/// The final query resolves match IDs back through strings/files for display.
use std::collections::HashSet;
use std::collections::HashMap;

use crate::types::{QueryAtom, QueryDef};

/// Schema for an extraction rule: ordered list of capture kinds.
/// Built from `Rule.create_matches` during lowering.
///
/// Example: rule `deploy_config` with matches `[svc, repo, tag]`
/// → `RuleSchema { kinds: ["svc", "repo", "tag"] }`
#[derive(Debug, Clone)]
pub struct RuleSchema {
    pub kinds: Vec<String>,
}

/// Compile a query and all its transitive query dependencies into SQL.
///
/// Topologically sorts dependent queries so each CTE is defined before
/// it is referenced. Detects non-self-referencing cycles and returns Err.
pub fn compile_query_with_deps(
    def: &QueryDef,
    all_queries: &HashMap<String, &QueryDef>,
    known_queries: &HashMap<String, usize>,
    known_rules: &HashMap<String, RuleSchema>,
) -> Result<String, String> {
    let mut order = Vec::new();
    let mut visited = HashSet::new();
    let mut in_stack = HashSet::new();
    topo_sort(def, all_queries, &mut visited, &mut in_stack, &mut order)?;

    // Build chained CTEs for all dependencies, then the target query last
    let mut cte_parts = Vec::new();
    let mut seen_recursive = false;
    for q in &order {
        let cte_body = compile_cte_body(q, known_queries, known_rules);
        if q.is_recursive {
            seen_recursive = true;
        }
        let col_list: Vec<String> = (0..q.arity).map(|i| format!("a{i}")).collect();
        let cols = col_list.join(", ");
        cte_parts.push(format!("{name}({cols}) AS (\n  {body}\n)", name = q.name, cols = cols, body = cte_body));
    }

    let with_keyword = if seen_recursive { "WITH RECURSIVE" } else { "WITH" };
    let ctes = cte_parts.join(",\n");

    // Final SELECT from the target query
    let final_select = compile_final_select(def);
    Ok(format!("{with_keyword} {ctes}\n{final_select}"))
}

fn topo_sort<'a>(
    def: &'a QueryDef,
    all_queries: &HashMap<String, &'a QueryDef>,
    visited: &mut HashSet<String>,
    in_stack: &mut HashSet<String>,
    order: &mut Vec<&'a QueryDef>,
) -> Result<(), String> {
    if visited.contains(&def.name) {
        return Ok(());
    }
    if in_stack.contains(&def.name) {
        return Err(format!("cycle detected: query '{}' has a non-self-referencing cycle", def.name));
    }
    in_stack.insert(def.name.clone());

    for atom in &def.body {
        // Skip self-references (handled by WITH RECURSIVE)
        if atom.relation == def.name {
            continue;
        }
        if let Some(dep) = all_queries.get(&atom.relation) {
            topo_sort(dep, all_queries, visited, in_stack, order)?;
        }
    }

    in_stack.remove(&def.name);
    visited.insert(def.name.clone());
    order.push(def);
    Ok(())
}

/// Compile a single query definition into a complete SQL SELECT statement.
///
/// `known_queries` maps query names to their arity, used to distinguish
/// query-derived relations from link_kind base relations.
///
/// For queries with no dependencies on other queries, this is sufficient.
/// For queries that reference other queries, use `compile_query_with_deps`.
pub fn compile_query(
    def: &QueryDef,
    known_queries: &HashMap<String, usize>,
    known_rules: &HashMap<String, RuleSchema>,
) -> String {
    let cte = compile_cte(def, known_queries, known_rules);
    let final_select = compile_final_select(def);
    format!("{cte}\n{final_select}")
}

/// Build the final SELECT that resolves match IDs to human-readable values.
fn compile_final_select(def: &QueryDef) -> String {
    let mut select_cols = Vec::new();
    let mut joins = Vec::new();
    for i in 0..def.arity {
        let alias = format!("m{i}");
        let r_alias = format!("r{i}");
        let rr_alias = format!("rr{i}");
        let s_alias = format!("s{i}");
        let f_alias = format!("f{i}");
        joins.push(format!(
            "JOIN matches {alias} ON q.a{i} = {alias}.id\n\
             LEFT JOIN refs {r_alias} ON {alias}.ref_id = {r_alias}.id\n\
             LEFT JOIN repo_refs {rr_alias} ON {alias}.repo_ref_id = {rr_alias}.id\n\
             JOIN strings {s_alias} ON COALESCE({r_alias}.string_id, {rr_alias}.string_id) = {s_alias}.id\n\
             LEFT JOIN files {f_alias} ON {r_alias}.file_id = {f_alias}.id"
        ));
        let col_name = &def.head_args[i];
        let label = if col_name.starts_with('=') || col_name == "_" {
            format!("col{i}")
        } else {
            col_name.to_lowercase()
        };
        select_cols.push(format!("{s_alias}.value AS {label}"));
        select_cols.push(format!("{f_alias}.path AS {label}_file"));
        select_cols.push(format!("{alias}.kind AS {label}_kind"));
    }
    format!(
        "SELECT {cols}\nFROM {name} q\n{joins}",
        cols = select_cols.join(", "),
        name = def.name,
        joins = joins.join("\n"),
    )
}

/// Compile the CTE body (the SELECT inside the AS (...)).
fn compile_cte_body(def: &QueryDef, known_queries: &HashMap<String, usize>, known_rules: &HashMap<String, RuleSchema>) -> String {
    if def.is_recursive {
        let base_atoms: Vec<&QueryAtom> = def
            .body
            .iter()
            .filter(|a| a.relation != def.name)
            .collect();
        let recursive_atoms: Vec<&QueryAtom> = def
            .body
            .iter()
            .filter(|a| a.relation == def.name)
            .collect();

        let base_sql = compile_body_select(&base_atoms, &def.head_args, def, known_queries, known_rules);
        let rec_sql =
            compile_recursive_step(def, &base_atoms, &recursive_atoms, known_queries, known_rules);

        format!("{base_sql}\n  UNION\n  {rec_sql}")
    } else {
        compile_body_select(&def.body.iter().collect::<Vec<_>>(), &def.head_args, def, known_queries, known_rules)
    }
}

/// Compile the CTE portion (WITH [RECURSIVE] name AS (...)).
fn compile_cte(def: &QueryDef, known_queries: &HashMap<String, usize>, known_rules: &HashMap<String, RuleSchema>) -> String {
    let col_list: Vec<String> = (0..def.arity).map(|i| format!("a{i}")).collect();
    let cols = col_list.join(", ");
    let body = compile_cte_body(def, known_queries, known_rules);

    if def.is_recursive {
        format!(
            "WITH RECURSIVE {name}({cols}) AS (\n  {body}\n)",
            name = def.name,
        )
    } else {
        format!(
            "WITH {name}({cols}) AS (\n  {body}\n)",
            name = def.name,
        )
    }
}

/// Compile a non-recursive body into a SELECT.
///
/// Positive atoms become subquery sources joined via shared variables.
/// Negated atoms compile to NOT EXISTS subqueries that reference
/// variables already bound by positive atoms.
fn compile_body_select(
    atoms: &[&QueryAtom],
    head_args: &[String],
    _def: &QueryDef,
    known_queries: &HashMap<String, usize>,
    known_rules: &HashMap<String, RuleSchema>,
) -> String {
    let positive: Vec<&QueryAtom> = atoms.iter().filter(|a| !a.negated).copied().collect();
    let negated: Vec<&QueryAtom> = atoms.iter().filter(|a| a.negated).copied().collect();

    if positive.is_empty() && negated.is_empty() {
        return "SELECT NULL WHERE 0".to_string();
    }

    // Each positive atom gets a table alias: t0, t1, ...
    let mut from_parts = Vec::new();
    let mut where_parts = Vec::new();

    // Track which variable maps to which table.column
    let mut var_bindings: HashMap<String, Vec<String>> = HashMap::new();

    for (idx, atom) in positive.iter().enumerate() {
        let alias = format!("t{idx}");
        let builtin = builtin_relation(&atom.relation);
        let subquery = relation_source(&atom.relation, atom.args.len(), known_queries, known_rules);
        from_parts.push(format!("({subquery}) AS {alias}"));

        for (col_idx, arg) in atom.args.iter().enumerate() {
            let col_ref = format!("{alias}.a{col_idx}");

            if arg == "_" {
                // wildcard, no binding
            } else if arg.starts_with('=') {
                let lit = arg[1..].replace('\'', "''");
                let is_string_col = builtin.as_ref().map_or(false, |b| b.all_string || col_idx > 0);
                if is_string_col {
                    // Column is a string value -- compare directly.
                    where_parts.push(format!("{col_ref} = '{lit}'"));
                } else {
                    // Column is a match ID -- resolve through strings.
                    where_parts.push(format!(
                        "{col_ref} IN (SELECT m.id FROM matches m \
                         LEFT JOIN refs r ON m.ref_id = r.id \
                         LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
                         JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id \
                         WHERE s.norm = '{lit}')"
                    ));
                }
            } else {
                // variable: record binding for unification
                var_bindings.entry(arg.clone()).or_default().push(col_ref);
            }
        }
    }

    // Unify shared variables: all refs to the same var must be equal
    for (_var, refs) in &var_bindings {
        for pair in refs.windows(2) {
            where_parts.push(format!("{} = {}", pair[0], pair[1]));
        }
    }

    // Negated atoms: NOT EXISTS subqueries referencing bound variables
    for atom in &negated {
        let subquery = relation_source(&atom.relation, atom.args.len(), known_queries, known_rules);
        let mut sub_conditions = Vec::new();
        let builtin = builtin_relation(&atom.relation);

        for (col_idx, arg) in atom.args.iter().enumerate() {
            let inner_col = format!("neg.a{col_idx}");
            if arg == "_" {
                // wildcard, no constraint
            } else if arg.starts_with('=') {
                let lit = arg[1..].replace('\'', "''");
                let is_string_col = builtin.as_ref().map_or(false, |b| b.all_string || col_idx > 0);
                if is_string_col {
                    sub_conditions.push(format!("{inner_col} = '{lit}'"));
                } else {
                    sub_conditions.push(format!(
                        "{inner_col} IN (SELECT m.id FROM matches m \
                         LEFT JOIN refs r ON m.ref_id = r.id \
                         LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
                         JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id \
                         WHERE s.norm = '{lit}')"
                    ));
                }
            } else if let Some(refs) = var_bindings.get(arg.as_str()) {
                // Bind to the positive atom's column
                sub_conditions.push(format!("{inner_col} = {}", refs[0]));
            }
        }

        let sub_where = if sub_conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", sub_conditions.join(" AND "))
        };
        where_parts.push(format!(
            "NOT EXISTS (SELECT 1 FROM ({subquery}) AS neg{sub_where})"
        ));
    }

    // Build SELECT columns from head_args
    let mut select_cols = Vec::new();
    for (i, head_arg) in head_args.iter().enumerate() {
        if head_arg == "_" || head_arg.starts_with('=') {
            select_cols.push(format!("NULL AS a{i}"));
        } else if let Some(refs) = var_bindings.get(head_arg.as_str()) {
            select_cols.push(format!("{} AS a{i}", refs[0]));
        } else {
            select_cols.push(format!("NULL AS a{i}"));
        }
    }

    let select = select_cols.join(", ");
    let from = from_parts.join("\nJOIN ");
    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("\nWHERE {}", where_parts.join("\n  AND "))
    };

    format!("SELECT {select}\nFROM {from}{where_clause}")
}

/// Compile the recursive step of a recursive query.
fn compile_recursive_step(
    def: &QueryDef,
    base_atoms: &[&QueryAtom],
    recursive_atoms: &[&QueryAtom],
    known_queries: &HashMap<String, usize>,
    known_rules: &HashMap<String, RuleSchema>,
) -> String {
    // For simplicity, handle the common pattern:
    //   head($A, $C) :- base($A, $B), head($B, $C)
    // which becomes:
    //   SELECT base.a0, rec.a1 FROM head rec JOIN (base_source) base ON rec.a1 = base.a0
    //
    // General case: build full join with variable unification.

    let mut from_parts = Vec::new();
    let mut where_parts = Vec::new();
    let mut var_bindings: HashMap<String, Vec<String>> = HashMap::new();
    let mut table_idx = 0;

    // Add recursive CTE references
    for atom in recursive_atoms {
        let alias = format!("rec{table_idx}");
        from_parts.push(format!("{} AS {alias}", def.name));
        for (col_idx, arg) in atom.args.iter().enumerate() {
            let col_ref = format!("{alias}.a{col_idx}");
            if arg != "_" && !arg.starts_with('=') {
                var_bindings.entry(arg.clone()).or_default().push(col_ref);
            }
        }
        table_idx += 1;
    }

    // Add positive base relation references
    let pos_base: Vec<&&QueryAtom> = base_atoms.iter().filter(|a| !a.negated).collect();
    let neg_base: Vec<&&QueryAtom> = base_atoms.iter().filter(|a| a.negated).collect();

    for atom in &pos_base {
        let alias = format!("b{table_idx}");
        let builtin = builtin_relation(&atom.relation);
        let subquery = relation_source(&atom.relation, atom.args.len(), known_queries, known_rules);
        from_parts.push(format!("({subquery}) AS {alias}"));
        for (col_idx, arg) in atom.args.iter().enumerate() {
            let col_ref = format!("{alias}.a{col_idx}");
            if arg == "_" {
                // skip
            } else if arg.starts_with('=') {
                let lit = arg[1..].replace('\'', "''");
                let is_string_col = builtin.as_ref().map_or(false, |b| b.all_string || col_idx > 0);
                if is_string_col {
                    where_parts.push(format!("{col_ref} = '{lit}'"));
                } else {
                    where_parts.push(format!(
                        "{col_ref} IN (SELECT m.id FROM matches m \
                         LEFT JOIN refs r ON m.ref_id = r.id \
                         LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
                         JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id \
                         WHERE s.norm = '{lit}')"
                    ));
                }
            } else {
                var_bindings.entry(arg.clone()).or_default().push(col_ref);
            }
        }
        table_idx += 1;
    }

    // Unify shared variables
    for (_var, refs) in &var_bindings {
        for pair in refs.windows(2) {
            where_parts.push(format!("{} = {}", pair[0], pair[1]));
        }
    }

    // Negated base atoms: NOT EXISTS subqueries
    for atom in &neg_base {
        let subquery = relation_source(&atom.relation, atom.args.len(), known_queries, known_rules);
        let builtin = builtin_relation(&atom.relation);
        let mut sub_conditions = Vec::new();
        for (col_idx, arg) in atom.args.iter().enumerate() {
            let inner_col = format!("neg.a{col_idx}");
            if arg == "_" {
                // wildcard
            } else if arg.starts_with('=') {
                let lit = arg[1..].replace('\'', "''");
                let is_string_col = builtin.as_ref().map_or(false, |b| b.all_string || col_idx > 0);
                if is_string_col {
                    sub_conditions.push(format!("{inner_col} = '{lit}'"));
                } else {
                    sub_conditions.push(format!(
                        "{inner_col} IN (SELECT m.id FROM matches m \
                         LEFT JOIN refs r ON m.ref_id = r.id \
                         LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
                         JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id \
                         WHERE s.norm = '{lit}')"
                    ));
                }
            } else if let Some(refs) = var_bindings.get(arg.as_str()) {
                sub_conditions.push(format!("{inner_col} = {}", refs[0]));
            }
        }
        let sub_where = if sub_conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", sub_conditions.join(" AND "))
        };
        where_parts.push(format!(
            "NOT EXISTS (SELECT 1 FROM ({subquery}) AS neg{sub_where})"
        ));
    }

    // Build SELECT from head args
    let mut select_cols = Vec::new();
    for (i, head_arg) in def.head_args.iter().enumerate() {
        if head_arg == "_" || head_arg.starts_with('=') {
            select_cols.push(format!("NULL AS a{i}"));
        } else if let Some(refs) = var_bindings.get(head_arg.as_str()) {
            select_cols.push(format!("{} AS a{i}", refs[0]));
        } else {
            select_cols.push(format!("NULL AS a{i}"));
        }
    }

    let select = select_cols.join(", ");
    let from = from_parts.join("\nJOIN ");
    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("\nWHERE {}", where_parts.join("\n  AND "))
    };

    format!("SELECT {select}\nFROM {from}{where_clause}")
}

/// SQL source for a relation name.
///
/// Resolution order:
/// 1. Known query → CTE reference
/// 2. Known rule  → group_id JOIN over matches (rule-as-relation)
/// 3. Built-in    → SQL over base tables
/// 4. Fallback    → link_kind in match_links
fn relation_source(
    name: &str,
    arity: usize,
    known_queries: &HashMap<String, usize>,
    known_rules: &HashMap<String, RuleSchema>,
) -> String {
    if known_queries.contains_key(name) {
        let cols: Vec<String> = (0..arity).map(|i| format!("a{i}")).collect();
        format!("SELECT {} FROM {}", cols.join(", "), name)
    } else if let Some(schema) = known_rules.get(name) {
        rule_relation_source(name, schema)
    } else if let Some(info) = builtin_relation(name) {
        info.sql
    } else {
        let escaped = name.replace('\'', "''");
        format!(
            "SELECT source_match_id AS a0, target_match_id AS a1 \
             FROM match_links WHERE link_kind = '{escaped}'"
        )
    }
}

/// Generate SQL for a rule-as-relation: JOIN matches within the same group_id.
///
/// Each capture kind becomes a column. The anchor match (first kind) provides
/// the group_id and rule_name filter. Remaining kinds JOIN on group_id.
///
/// Returns match IDs so `compile_final_select` can resolve to strings.
fn rule_relation_source(rule_name: &str, schema: &RuleSchema) -> String {
    if schema.kinds.is_empty() {
        return "SELECT NULL AS a0 WHERE 0".to_string();
    }

    let escaped_rule = rule_name.replace('\'', "''");
    let escaped_kind0 = schema.kinds[0].replace('\'', "''");

    let mut select_cols = vec![format!("m0.id AS a0")];
    let mut joins = Vec::new();

    for (i, kind) in schema.kinds.iter().enumerate().skip(1) {
        let escaped_kind = kind.replace('\'', "''");
        select_cols.push(format!("m{i}.id AS a{i}"));
        joins.push(format!(
            "JOIN matches m{i} ON m{i}.group_id = m0.group_id AND m{i}.kind = '{escaped_kind}'"
        ));
    }

    let select = select_cols.join(", ");
    let join_clause = if joins.is_empty() {
        String::new()
    } else {
        format!("\n{}", joins.join("\n"))
    };

    format!(
        "SELECT {select} FROM matches m0{join_clause} \
         WHERE m0.rule_name = '{escaped_rule}' AND m0.kind = '{escaped_kind0}'"
    )
}

/// Info about a built-in relation.
struct BuiltinInfo {
    /// The SQL subquery.
    sql: String,
    /// If true, ALL columns are string-valued (not match IDs).
    /// If false, a0 is a match ID and a1+ are string-valued.
    all_string: bool,
}

/// Built-in relations that query base tables directly.
///
/// These compile to SQL subqueries over matches/refs/strings/files/repos
/// without requiring materialized link edges.
///
/// | Relation              | Arity | Semantics                                      |
/// |-----------------------|-------|-------------------------------------------------|
/// | has_kind($M, "kind")  | 2     | match M has this kind                           |
/// | has_norm($M, "val")   | 2     | match M's string has this normalized value      |
/// | same_norm($A, $B)     | 2     | matches A and B share the same norm              |
/// | same_repo($A, $B)     | 2     | matches A and B are in the same repo             |
/// | same_file($A, $B)     | 2     | matches A and B are in the same file             |
/// | in_repo($M, "name")   | 2     | match M is in a repo with this name              |
/// | in_file($M, "glob")   | 2     | match M is in a file matching this glob          |
/// | has_value($M, "val")  | 2     | match M's raw string value (not normalized)      |
/// | repo_has_tag($R, $T)  | 2     | repo R has tag T (all-string)                    |
/// | repo_has_branch($R,$B)| 2     | repo R has branch B (all-string)                 |
/// | repo_has_rev($R, $V)  | 2     | repo R has rev V (all-string)                    |
fn builtin_relation(name: &str) -> Option<BuiltinInfo> {
    let (sql, all_string) = match name {
        "has_kind" => (
            "SELECT m.id AS a0, m.kind AS a1 FROM matches m".to_string(),
            false,
        ),
        "has_norm" => (
            "SELECT m.id AS a0, s.norm AS a1 \
             FROM matches m \
             LEFT JOIN refs r ON m.ref_id = r.id \
             LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
             JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id"
                .to_string(),
            false,
        ),
        "has_value" => (
            "SELECT m.id AS a0, s.value AS a1 \
             FROM matches m \
             LEFT JOIN refs r ON m.ref_id = r.id \
             LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
             JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id"
                .to_string(),
            false,
        ),
        "same_norm" => (
            "SELECT m1.id AS a0, m2.id AS a1 \
             FROM matches m1 \
             LEFT JOIN refs r1 ON m1.ref_id = r1.id \
             LEFT JOIN repo_refs rr1 ON m1.repo_ref_id = rr1.id \
             JOIN strings s1 ON COALESCE(r1.string_id, rr1.string_id) = s1.id \
             JOIN strings s2 ON s1.norm = s2.norm \
             LEFT JOIN refs r2 ON r2.string_id = s2.id \
             LEFT JOIN repo_refs rr2 ON rr2.string_id = s2.id \
             JOIN matches m2 ON (m2.ref_id = r2.id OR m2.repo_ref_id = rr2.id) \
             WHERE m1.id != m2.id"
                .to_string(),
            false,
        ),
        "same_repo" => (
            "SELECT m1.id AS a0, m2.id AS a1 \
             FROM matches m1 \
             LEFT JOIN refs r1 ON m1.ref_id = r1.id \
             LEFT JOIN repo_refs rr1 ON m1.repo_ref_id = rr1.id \
             LEFT JOIN files f1 ON r1.file_id = f1.id \
             JOIN files f2 ON COALESCE(f1.repo_id, rr1.repo_id) = f2.repo_id \
             JOIN refs r2 ON r2.file_id = f2.id \
             JOIN matches m2 ON m2.ref_id = r2.id \
             WHERE m1.id != m2.id \
             UNION ALL \
             SELECT m1.id AS a0, m2.id AS a1 \
             FROM matches m1 \
             LEFT JOIN refs r1 ON m1.ref_id = r1.id \
             LEFT JOIN repo_refs rr1 ON m1.repo_ref_id = rr1.id \
             LEFT JOIN files f1 ON r1.file_id = f1.id \
             JOIN repo_refs rr2 ON COALESCE(f1.repo_id, rr1.repo_id) = rr2.repo_id \
             JOIN matches m2 ON m2.repo_ref_id = rr2.id \
             WHERE m1.id != m2.id"
                .to_string(),
            false,
        ),
        "same_file" => (
            "SELECT m1.id AS a0, m2.id AS a1 \
             FROM matches m1 \
             JOIN refs r1 ON m1.ref_id = r1.id \
             JOIN refs r2 ON r1.file_id = r2.file_id \
             JOIN matches m2 ON m2.ref_id = r2.id \
             WHERE m1.id != m2.id"
                .to_string(),
            false,
        ),
        "in_repo" => (
            "SELECT m.id AS a0, rp.name AS a1 \
             FROM matches m \
             LEFT JOIN refs r ON m.ref_id = r.id \
             LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
             LEFT JOIN files f ON r.file_id = f.id \
             JOIN repos rp ON COALESCE(f.repo_id, rr.repo_id) = rp.id"
                .to_string(),
            false,
        ),
        "in_file" => (
            "SELECT m.id AS a0, f.path AS a1 \
             FROM matches m \
             JOIN refs r ON m.ref_id = r.id \
             JOIN files f ON r.file_id = f.id"
                .to_string(),
            false,
        ),
        // All-string builtins: both columns are string values, not match IDs.
        "repo_has_tag" => (
            "SELECT rp.name AS a0, rv.rev AS a1 \
             FROM repo_revs rv \
             JOIN repos rp ON rv.repo_id = rp.id \
             WHERE rv.is_semver = 1 OR rv.is_working_tree = 0"
                .to_string(),
            true,
        ),
        "repo_has_branch" => (
            "SELECT rp.name AS a0, rv.rev AS a1 \
             FROM repo_revs rv \
             JOIN repos rp ON rv.repo_id = rp.id \
             WHERE rv.is_working_tree = 0"
                .to_string(),
            true,
        ),
        "repo_has_rev" => (
            "SELECT rp.name AS a0, rv.rev AS a1 \
             FROM repo_revs rv \
             JOIN repos rp ON rv.repo_id = rp.id"
                .to_string(),
            true,
        ),
        _ => return None,
    };
    Some(BuiltinInfo { sql, all_string })
}

/// Compile a goal expression (CLI query) into WHERE clauses to append.
///
/// A goal is an atom like `all_deps($WHO, "lodash")`.
/// Variables become output columns, literals become WHERE filters.
pub fn compile_goal_filter(goal_args: &[String], _def: &QueryDef) -> (Vec<String>, String) {
    let mut output_cols = Vec::new();
    let mut where_parts = Vec::new();

    for (i, arg) in goal_args.iter().enumerate() {
        if arg == "_" {
            // wildcard, skip
        } else if arg.starts_with('=') {
            let lit = arg[1..].replace('\'', "''");
            // Filter: resolve this position's match to a string value
            where_parts.push(format!(
                "s{i}.norm = '{lit}'"
            ));
        } else {
            // Variable: include in output
            output_cols.push(arg.to_lowercase());
        }
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!("\nWHERE {}", where_parts.join(" AND "))
    };

    (output_cols, where_clause)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_def(name: &str, head_args: &[&str], body: Vec<QueryAtom>, is_recursive: bool) -> QueryDef {
        QueryDef {
            name: name.to_string(),
            arity: head_args.len(),
            head_args: head_args.iter().map(|s| s.to_string()).collect(),
            body,
            is_recursive,
            is_check: false,
        }
    }

    fn atom(rel: &str, args: &[&str]) -> QueryAtom {
        QueryAtom {
            relation: rel.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            negated: false,
        }
    }

    fn no_rules() -> HashMap<String, RuleSchema> {
        HashMap::new()
    }

    #[test]
    fn nonrecursive_join() {
        let def = make_def(
            "same_eco",
            &["A", "B"],
            vec![
                atom("dep_to_package", &["A", "X"]),
                atom("dep_to_package", &["B", "X"]),
            ],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        // Should contain WITH (not RECURSIVE)
        assert!(sql.starts_with("WITH same_eco"), "sql: {}", sql);
        assert!(!sql.contains("RECURSIVE"), "sql: {}", sql);
        // Should JOIN two instances of match_links
        assert!(sql.contains("link_kind = 'dep_to_package'"), "sql: {}", sql);
        // Should have unification: t0.a1 = t1.a1 (shared X)
        assert!(sql.contains("t0.a1 = t1.a1"), "sql: {}", sql);
    }

    #[test]
    fn recursive_transitive() {
        let def = make_def(
            "all_deps",
            &["A", "C"],
            vec![
                atom("dep_to_package", &["A", "B"]),
                atom("all_deps", &["B", "C"]),
            ],
            true,
        );
        let known: HashMap<String, usize> = [("all_deps".to_string(), 2)].into();
        let sql = compile_query(&def, &known, &no_rules());
        assert!(sql.contains("WITH RECURSIVE all_deps"), "sql: {}", sql);
        // Base case should reference dep_to_package
        assert!(sql.contains("link_kind = 'dep_to_package'"), "sql: {}", sql);
        // Recursive step should reference all_deps CTE
        assert!(sql.contains("FROM all_deps AS"), "sql: {}", sql);
    }

    #[test]
    fn literal_in_body() {
        let def = make_def(
            "who_uses",
            &["WHO"],
            vec![atom("dep_to_package", &["WHO", "=lodash"])],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        assert!(sql.contains("s.norm = 'lodash'"), "sql: {}", sql);
    }

    #[test]
    fn goal_filter() {
        let def = make_def("all_deps", &["A", "C"], vec![], false);
        let goal_args = vec!["WHO".to_string(), "=lodash".to_string()];
        let (cols, where_clause) = compile_goal_filter(&goal_args, &def);
        assert_eq!(cols, vec!["who"]);
        assert!(where_clause.contains("s1.norm = 'lodash'"), "where: {}", where_clause);
    }

    #[test]
    fn wildcard_in_body() {
        let def = make_def(
            "has_dep",
            &["A"],
            vec![atom("dep_to_package", &["A", "_"])],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        // Wildcard should not generate any WHERE condition for that column
        assert!(!sql.contains("t0.a1 ="), "sql: {}", sql);
    }

    #[test]
    fn builtin_has_kind() {
        let def = make_def(
            "find_images",
            &["M"],
            vec![atom("has_kind", &["M", "=image_repo"])],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        // Should use direct string comparison, not match ID resolution
        assert!(sql.contains("= 'image_repo'"), "sql: {}", sql);
        assert!(!sql.contains("s.norm"), "should not resolve through strings: {}", sql);
    }

    #[test]
    fn builtin_same_norm_unifies() {
        let def = make_def(
            "norm_match",
            &["A", "B"],
            vec![
                atom("has_kind", &["A", "=image_repo"]),
                atom("has_kind", &["B", "=repo_name"]),
                atom("same_norm", &["A", "B"]),
            ],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        // same_norm should produce a subquery with norm join
        assert!(sql.contains("s1.norm = s2.norm"), "sql: {}", sql);
        // Variable A should unify across has_kind and same_norm
        assert!(sql.contains("t0.a0") && sql.contains("t2.a0"), "sql: {}", sql);
    }

    #[test]
    fn builtin_in_repo_literal() {
        let def = make_def(
            "repo_matches",
            &["M"],
            vec![atom("in_repo", &["M", "=myorg/frontend"])],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        assert!(sql.contains("= 'myorg/frontend'"), "sql: {}", sql);
    }

    #[test]
    fn query_with_deps_topo_sorts() {
        let base = make_def(
            "direct",
            &["A", "B"],
            vec![atom("dep_link", &["A", "B"])],
            false,
        );
        let transitive = make_def(
            "all",
            &["A", "C"],
            vec![
                atom("direct", &["A", "B"]),
                atom("all", &["B", "C"]),
            ],
            true,
        );
        let all_queries: HashMap<String, &QueryDef> = [
            ("direct".to_string(), &base),
            ("all".to_string(), &transitive),
        ].into();
        let known: HashMap<String, usize> = [
            ("direct".to_string(), 2),
            ("all".to_string(), 2),
        ].into();

        let sql = compile_query_with_deps(&transitive, &all_queries, &known, &no_rules()).unwrap();
        // "direct" CTE should appear before "all" CTE
        let direct_pos = sql.find("direct(").unwrap();
        let all_pos = sql.find("all(").unwrap();
        assert!(direct_pos < all_pos, "direct should come before all in CTE chain: {}", sql);
    }

    #[test]
    fn cycle_detection() {
        let a = make_def(
            "a",
            &["X"],
            vec![atom("b", &["X"])],
            false,
        );
        let b = make_def(
            "b",
            &["X"],
            vec![atom("a", &["X"])],
            false,
        );
        let all_queries: HashMap<String, &QueryDef> = [
            ("a".to_string(), &a),
            ("b".to_string(), &b),
        ].into();
        let known: HashMap<String, usize> = [
            ("a".to_string(), 1),
            ("b".to_string(), 1),
        ].into();

        let result = compile_query_with_deps(&a, &all_queries, &known, &no_rules());
        assert!(result.is_err(), "should detect cycle");
        assert!(result.unwrap_err().contains("cycle"), "error should mention cycle");
    }

    fn neg_atom(rel: &str, args: &[&str]) -> QueryAtom {
        QueryAtom {
            relation: rel.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            negated: true,
        }
    }

    #[test]
    fn negated_atom_compiles_to_not_exists() {
        let def = make_def(
            "orphan_dep",
            &["X"],
            vec![
                atom("has_kind", &["X", "=dep_name"]),
                neg_atom("dep_to_package", &["X", "_"]),
            ],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        assert!(sql.contains("NOT EXISTS"), "sql: {}", sql);
        assert!(sql.contains("link_kind = 'dep_to_package'"), "sql: {}", sql);
        // The negated atom should not appear in FROM
        assert!(!sql.contains("AS t1"), "negated atom should not be a FROM source: {}", sql);
    }

    #[test]
    fn negated_atom_binds_to_positive_var() {
        let def = make_def(
            "unlinked",
            &["A"],
            vec![
                atom("has_kind", &["A", "=dep_name"]),
                neg_atom("dep_to_package", &["A", "_"]),
            ],
            false,
        );
        let known = HashMap::new();
        let sql = compile_query(&def, &known, &no_rules());
        // The NOT EXISTS subquery should reference the outer binding for A
        assert!(sql.contains("neg.a0 = t0.a0"), "should bind negated var to positive: {}", sql);
    }

    #[test]
    fn check_query_compiles_same_as_query() {
        let query_def = make_def(
            "orphan",
            &["X"],
            vec![
                atom("has_kind", &["X", "=dep_name"]),
                neg_atom("dep_to_package", &["X", "_"]),
            ],
            false,
        );
        let mut check_def = query_def.clone();
        check_def.is_check = true;

        let known = HashMap::new();
        let query_sql = compile_query(&query_def, &known, &no_rules());
        let check_sql = compile_query(&check_def, &known, &no_rules());
        // SQL is identical -- is_check is a CLI-level concern
        assert_eq!(query_sql, check_sql);
    }

    #[test]
    fn rule_as_relation_single_capture() {
        let def = make_def(
            "find_pkgs",
            &["X"],
            vec![atom("pkg_manifest", &["X"])],
            false,
        );
        let known_queries = HashMap::new();
        let known_rules: HashMap<String, RuleSchema> = [(
            "pkg_manifest".to_string(),
            RuleSchema { kinds: vec!["name".to_string()] },
        )].into();
        let sql = compile_query(&def, &known_queries, &known_rules);
        assert!(sql.contains("rule_name = 'pkg_manifest'"), "sql: {}", sql);
        assert!(sql.contains("m0.kind = 'name'"), "sql: {}", sql);
    }

    #[test]
    fn rule_as_relation_multi_capture() {
        let def = make_def(
            "find_deps",
            &["N", "V"],
            vec![atom("dep_source", &["N", "V"])],
            false,
        );
        let known_queries = HashMap::new();
        let known_rules: HashMap<String, RuleSchema> = [(
            "dep_source".to_string(),
            RuleSchema { kinds: vec!["dep".to_string(), "version".to_string()] },
        )].into();
        let sql = compile_query(&def, &known_queries, &known_rules);
        assert!(sql.contains("rule_name = 'dep_source'"), "sql: {}", sql);
        assert!(sql.contains("m0.kind = 'dep'"), "sql: {}", sql);
        assert!(sql.contains("m1.group_id = m0.group_id"), "sql: {}", sql);
        assert!(sql.contains("m1.kind = 'version'"), "sql: {}", sql);
    }

    #[test]
    fn rule_relation_with_negated_builtin() {
        // check missing_tag($SVC, $REPO, $TAG) :-
        //     deploy_config($SVC, $REPO, $TAG),
        //     not repo_has_tag($REPO, $TAG);
        let def = make_def(
            "missing_tag",
            &["SVC", "REPO", "TAG"],
            vec![
                atom("deploy_config", &["SVC", "REPO", "TAG"]),
                neg_atom("repo_has_tag", &["REPO", "TAG"]),
            ],
            false,
        );
        let known_queries = HashMap::new();
        let known_rules: HashMap<String, RuleSchema> = [(
            "deploy_config".to_string(),
            RuleSchema { kinds: vec!["svc".to_string(), "repo".to_string(), "tag".to_string()] },
        )].into();
        let sql = compile_query(&def, &known_queries, &known_rules);
        // Should use rule relation for deploy_config
        assert!(sql.contains("rule_name = 'deploy_config'"), "sql: {}", sql);
        assert!(sql.contains("group_id"), "sql: {}", sql);
        // Should use NOT EXISTS for repo_has_tag
        assert!(sql.contains("NOT EXISTS"), "sql: {}", sql);
        assert!(sql.contains("repo_revs"), "sql: {}", sql);
    }
}
