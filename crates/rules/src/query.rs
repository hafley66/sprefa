/// Compile QueryDef (Datalog horn clauses) to SQL.
///
/// Each QueryDef becomes a CTE. Recursive queries use WITH RECURSIVE.
/// Variable unification across body atoms becomes JOIN ON conditions.
///
/// Relations resolve to:
///   - link_kind in match_links: SELECT source_match_id AS a0, target_match_id AS a1
///   - another query name: reference to that CTE
///
/// The final query resolves match IDs back through strings/files for display.
use std::collections::HashMap;

use crate::types::{QueryAtom, QueryDef};

/// Compile a query definition into a complete SQL SELECT statement.
///
/// `known_queries` maps query names to their arity, used to distinguish
/// query-derived relations from link_kind base relations.
pub fn compile_query(
    def: &QueryDef,
    known_queries: &HashMap<String, usize>,
) -> String {
    let cte = compile_cte(def, known_queries);

    // Final SELECT: resolve match IDs to human-readable values.
    // Each column a0..aN is a match_id. Join through to strings for display.
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
        "{cte}\nSELECT {cols}\nFROM {name} q\n{joins}",
        cte = cte,
        cols = select_cols.join(", "),
        name = def.name,
        joins = joins.join("\n"),
    )
}

/// Compile the CTE portion (WITH [RECURSIVE] name AS (...)).
fn compile_cte(def: &QueryDef, known_queries: &HashMap<String, usize>) -> String {
    let col_list: Vec<String> = (0..def.arity).map(|i| format!("a{i}")).collect();
    let cols = col_list.join(", ");

    if def.is_recursive {
        // Split body into base atoms (not self-referencing) and recursive atoms
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

        let base_sql = compile_body_select(&base_atoms, &def.head_args, def, known_queries);

        // Recursive step: join the recursive CTE with base relations
        // For `all_deps($A, $C) :- link($A, $B), all_deps($B, $C)`:
        //   SELECT base.a0, rec.a1 FROM all_deps rec JOIN <link> base ON rec.a0 = base.a1
        let rec_sql =
            compile_recursive_step(def, &base_atoms, &recursive_atoms, known_queries);

        format!(
            "WITH RECURSIVE {name}({cols}) AS (\n  {base}\n  UNION\n  {rec}\n)",
            name = def.name,
            cols = cols,
            base = base_sql,
            rec = rec_sql,
        )
    } else {
        let body_sql =
            compile_body_select(&def.body.iter().collect::<Vec<_>>(), &def.head_args, def, known_queries);
        format!(
            "WITH {name}({cols}) AS (\n  {body}\n)",
            name = def.name,
            cols = cols,
            body = body_sql,
        )
    }
}

/// Compile a non-recursive body into a SELECT.
///
/// Each body atom becomes a subquery or table reference. Shared variables
/// across atoms become JOIN ON conditions.
fn compile_body_select(
    atoms: &[&QueryAtom],
    head_args: &[String],
    _def: &QueryDef,
    known_queries: &HashMap<String, usize>,
) -> String {
    if atoms.is_empty() {
        return "SELECT NULL WHERE 0".to_string();
    }

    // Each atom gets a table alias: t0, t1, ...
    let mut from_parts = Vec::new();
    let mut where_parts = Vec::new();

    // Track which variable maps to which table.column
    let mut var_bindings: HashMap<String, Vec<String>> = HashMap::new();

    for (idx, atom) in atoms.iter().enumerate() {
        let alias = format!("t{idx}");
        let subquery = relation_source(&atom.relation, atom.args.len(), known_queries);
        from_parts.push(format!("({subquery}) AS {alias}"));

        for (col_idx, arg) in atom.args.iter().enumerate() {
            let col_ref = format!("{alias}.a{col_idx}");

            if arg == "_" {
                // wildcard, no binding
            } else if arg.starts_with('=') {
                // literal: need to resolve through strings table
                let lit = arg[1..].replace('\'', "''");
                where_parts.push(format!(
                    "{col_ref} IN (SELECT m.id FROM matches m \
                     LEFT JOIN refs r ON m.ref_id = r.id \
                     LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
                     JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id \
                     WHERE s.norm = '{lit}')"
                ));
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

    // Build SELECT columns from head_args
    let mut select_cols = Vec::new();
    for (i, head_arg) in head_args.iter().enumerate() {
        if head_arg == "_" || head_arg.starts_with('=') {
            // Should not normally appear in head, but handle gracefully
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

    // Add base relation references
    for atom in base_atoms {
        let alias = format!("b{table_idx}");
        let subquery = relation_source(&atom.relation, atom.args.len(), known_queries);
        from_parts.push(format!("({subquery}) AS {alias}"));
        for (col_idx, arg) in atom.args.iter().enumerate() {
            let col_ref = format!("{alias}.a{col_idx}");
            if arg == "_" {
                // skip
            } else if arg.starts_with('=') {
                let lit = arg[1..].replace('\'', "''");
                where_parts.push(format!(
                    "{col_ref} IN (SELECT m.id FROM matches m \
                     LEFT JOIN refs r ON m.ref_id = r.id \
                     LEFT JOIN repo_refs rr ON m.repo_ref_id = rr.id \
                     JOIN strings s ON COALESCE(r.string_id, rr.string_id) = s.id \
                     WHERE s.norm = '{lit}')"
                ));
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
/// If the relation is a known query, reference it by name (CTE).
/// Otherwise, treat it as a link_kind in match_links.
fn relation_source(name: &str, arity: usize, known_queries: &HashMap<String, usize>) -> String {
    if known_queries.contains_key(name) {
        // Reference to another CTE -- just SELECT from it
        let cols: Vec<String> = (0..arity).map(|i| format!("a{i}")).collect();
        format!("SELECT {} FROM {}", cols.join(", "), name)
    } else {
        // Base relation: link_kind in match_links (binary, arity 2)
        let escaped = name.replace('\'', "''");
        format!(
            "SELECT source_match_id AS a0, target_match_id AS a1 \
             FROM match_links WHERE link_kind = '{escaped}'"
        )
    }
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
        }
    }

    fn atom(rel: &str, args: &[&str]) -> QueryAtom {
        QueryAtom {
            relation: rel.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        }
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
        let sql = compile_query(&def, &known);
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
        let sql = compile_query(&def, &known);
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
        let sql = compile_query(&def, &known);
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
        let sql = compile_query(&def, &known);
        // Wildcard should not generate any WHERE condition for that column
        assert!(!sql.contains("t0.a1 ="), "sql: {}", sql);
    }
}
