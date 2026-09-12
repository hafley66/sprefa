//! `validate_hosted_relations/4` and `erase_host_planning_rows/7`. Port of
//! `1d_host_planner.pl:1-183`.

use super::api::diagnostic;
use crate::_3_check::api::prolog_sort;
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashSet;

/// `host_schema(HostedIds, HostPortIds, InputIds, OutputIds)` at `:21`.
pub struct Schema {
    pub hosted: Vec<TermId>,
    pub host_port: Vec<TermId>,
    pub input: Vec<TermId>,
    pub output: Vec<TermId>,
}

/// `:29`. A `:/4` edge named `Name` whose owner is a `module/1` graph node.
pub fn named_module_targets(
    u: &Universe,
    nodes: &[TermId],
    edges: &[TermId],
    name: &str,
) -> Vec<TermId> {
    let modules: HashSet<TermId> = nodes.iter().filter_map(|n| u.unary(*n, "module")).collect();
    let mut out = Vec::new();
    for edge in edges {
        let Some((":", args)) = u.functor(*edge) else {
            continue;
        };
        if args.len() != 4 || !modules.contains(&args[0]) {
            continue;
        }
        if u.functor_or_atom(args[1]).map(|(n, a)| (n, a.len())) != Some((name, 0)) {
            continue;
        }
        if let Some(target) = u.unary(args[2], "ref") {
            out.push(target);
        }
    }
    out.sort_by(|a, b| u.cmp(*a, *b));
    out.dedup();
    out
}

pub fn host_schema(u: &Universe, nodes: &[TermId], edges: &[TermId]) -> Schema {
    Schema {
        hosted: named_module_targets(u, nodes, edges, "Hosted"),
        host_port: named_module_targets(u, nodes, edges, "HostPort"),
        input: named_module_targets(u, nodes, edges, "Input"),
        output: named_module_targets(u, nodes, edges, "Output"),
    }
}

/// `hosted(Relation, Implementation)` and `host_port(Relation, Label, Direction)`.
pub struct HostRows {
    pub hosted: Vec<(TermId, TermId)>,
    pub ports: Vec<(TermId, TermId, TermId)>,
}

/// `:36`.
pub fn host_rows(u: &Universe, schema: &Schema, compiler_facts: &[TermId]) -> HostRows {
    let hosted_ids: HashSet<TermId> = schema.hosted.iter().copied().collect();
    let port_ids: HashSet<TermId> = schema.host_port.iter().copied().collect();
    let mut hosted = Vec::new();
    let mut ports = Vec::new();
    for row in compiler_facts {
        let Some(("call", parts)) = u.functor(*row) else {
            continue;
        };
        let Some(rel) = u.unary(parts[0], "ref") else {
            continue;
        };
        let Some(args) = u.as_list(parts[1]) else {
            continue;
        };
        if hosted_ids.contains(&rel) && args.len() == 2 {
            if let (Some(relation), Some(implementation)) =
                (u.unary(args[0], "ref"), u.unary(args[1], "ref"))
            {
                hosted.push((relation, implementation));
            }
        }
        if port_ids.contains(&rel) && args.len() == 3 {
            if let (Some(relation), Some(label), Some(direction)) = (
                u.unary(args[0], "ref"),
                u.unary(args[1], "const"),
                u.unary(args[2], "ref"),
            ) {
                ports.push((relation, label, direction));
            }
        }
    }
    hosted.sort_by(|a, b| u.cmp(a.0, b.0).then_with(|| u.cmp(a.1, b.1)));
    hosted.dedup();
    ports.sort_by(|a, b| {
        u.cmp(a.0, b.0)
            .then_with(|| u.cmp(a.1, b.1))
            .then_with(|| u.cmp(a.2, b.2))
    });
    ports.dedup();
    HostRows { hosted, ports }
}

/// `:11`.
pub fn validate_hosted_relations(
    u: &mut Universe,
    nodes: &[TermId],
    edges: &[TermId],
    relations: &[TermId],
    compiler_facts: &[TermId],
) -> Vec<TermId> {
    let schema = host_schema(u, nodes, edges);
    let rows = host_rows(u, &schema, compiler_facts);
    let mut ids: Vec<TermId> = rows.hosted.iter().map(|r| r.0).collect();
    ids.extend(rows.ports.iter().map(|r| r.0));
    ids.sort_by(|a, b| u.cmp(*a, *b));
    ids.dedup();

    let declared: HashSet<TermId> = relations
        .iter()
        .filter_map(|r| match u.functor(*r) {
            Some(("relation", args)) if args.len() == 3 => u.unary(args[0], "ref"),
            _ => None,
        })
        .collect();
    let directions: HashSet<TermId> = schema
        .input
        .iter()
        .chain(schema.output.iter())
        .copied()
        .collect();

    let mut out = Vec::new();
    for relation in &ids {
        let implementations = rows.hosted.iter().filter(|r| r.0 == *relation).count();
        if implementations != 1 {
            let count = u.int(implementations as i64);
            let reason = u.compound("hosted_implementation_count", vec![*relation, count]);
            out.push(diagnostic(u, "host", reason));
        }
        if !declared.contains(relation) {
            let reason = u.compound("hosted_relation_not_declared", vec![*relation]);
            out.push(diagnostic(u, "host", reason));
        }
        let mut relation_edges: Vec<(TermId, TermId)> = edges
            .iter()
            .filter_map(|e| match u.functor(*e) {
                Some((":", args)) if args.len() == 4 && args[0] == *relation => {
                    Some((args[1], args[3]))
                }
                _ => None,
            })
            .collect();
        relation_edges.sort_by(|a, b| u.cmp(a.0, b.0).then_with(|| u.cmp(a.1, b.1)));
        relation_edges.dedup();

        for (label, index) in &relation_edges {
            let count = rows
                .ports
                .iter()
                .filter(|p| p.0 == *relation && p.1 == *label)
                .count();
            if count != 1 {
                let count = u.int(count as i64);
                let reason = u.compound(
                    "hosted_port_direction_count",
                    vec![*relation, *label, *index, count],
                );
                out.push(diagnostic(u, "host", reason));
            }
        }
        for port in rows.ports.iter().filter(|p| p.0 == *relation) {
            if !relation_edges.iter().any(|(label, _)| *label == port.1) {
                let reason = u.compound("hosted_port_unknown_edge", vec![*relation, port.1]);
                out.push(diagnostic(u, "host", reason));
            }
        }
        for port in rows.ports.iter().filter(|p| p.0 == *relation) {
            if !directions.contains(&port.2) {
                let reason = u.compound(
                    "hosted_port_invalid_direction",
                    vec![*relation, port.1, port.2],
                );
                out.push(diagnostic(u, "host", reason));
            }
        }
    }
    prolog_sort(u, out)
}

pub struct Erased {
    pub relations: Vec<TermId>,
    pub seeds: Vec<TermId>,
    pub rules: Vec<TermId>,
}

/// `:161`. Hosted and HostPort rows are compiler-only and never reach runtime.
pub fn erase_host_planning_rows(
    u: &Universe,
    nodes: &[TermId],
    edges: &[TermId],
    relations: &[TermId],
    seeds: &[TermId],
    rules: &[TermId],
) -> Erased {
    let schema = host_schema(u, nodes, edges);
    let planning: HashSet<TermId> = schema
        .hosted
        .iter()
        .chain(schema.host_port.iter())
        .copied()
        .collect();
    let relation_target = |row: &TermId| match u.functor(*row) {
        Some(("relation", args)) if args.len() == 3 => u.unary(args[0], "ref"),
        _ => None,
    };
    let call_target = |row: &TermId| match u.functor(*row) {
        Some(("call", args)) if args.len() == 2 => u.unary(args[0], "ref"),
        _ => None,
    };
    Erased {
        relations: relations
            .iter()
            .copied()
            .filter(|r| !relation_target(r).is_some_and(|id| planning.contains(&id)))
            .collect(),
        seeds: seeds
            .iter()
            .copied()
            .filter(|s| !call_target(s).is_some_and(|id| planning.contains(&id)))
            .collect(),
        rules: rules
            .iter()
            .copied()
            .filter(|r| !rule_uses_any(u, *r, &planning, &call_target))
            .collect(),
    }
}

/// `:178`. The head first, then any checked goal in the body.
fn rule_uses_any(
    u: &Universe,
    rule: TermId,
    planning: &HashSet<TermId>,
    call_target: &dyn Fn(&TermId) -> Option<TermId>,
) -> bool {
    let Some(("rule", args)) = u.functor(rule) else {
        return false;
    };
    if call_target(&args[0]).is_some_and(|id| planning.contains(&id)) {
        return true;
    }
    let Some(goals) = u.as_list(args[1]) else {
        return false;
    };
    goals.iter().any(|goal| {
        matches!(u.functor(*goal), Some(("checked_goal", parts)) if parts.len() == 2
            && call_target(&parts[1]).is_some_and(|id| planning.contains(&id)))
    })
}
