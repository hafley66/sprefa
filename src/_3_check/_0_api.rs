//! `check_datalog/4`. Port of `1_checker.pl:182-230` and `:416-431`.
//!
//! Pure: no file IO, no clock, no globals. v7's origin arena and graph store
//! are plain structs built per call and dropped with the frame.

use super::bind::bind_diagnostics;
use super::graph::{CheckerGraph, OriginArena};
use super::kernel::{kernel_graph, kernel_relation_rows};
use super::resolve::{resolve_edges, resolve_rules, resolve_seeds, Cx};
use super::strata::{depends_rows, locate_strata_diagnostics, strata_rows, stratify_rules};
use crate::_2_lower::Lowered;
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashMap;

/// Where v7's `check_datalog/4` fails instead of returning a diagnostic
/// (defect D1 in the PLAN).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Fail(&'static str),
}

/// `checked_datalog(root_graph(Nodes, Edges),
///                  datalog_program(Relations, Seeds, Rules),
///                  Depends, Strata)` at `:427`, one field per argument.
pub struct Checked {
    pub nodes: Vec<TermId>,
    pub edges: Vec<TermId>,
    pub relations: Vec<TermId>,
    pub seeds: Vec<TermId>,
    pub rules: Vec<TermId>,
    pub depends: Vec<TermId>,
    pub strata: Vec<TermId>,
}

impl Checked {
    pub fn to_term(&self, u: &mut Universe) -> TermId {
        let nodes = u.list(&self.nodes);
        let edges = u.list(&self.edges);
        let graph = u.compound("root_graph", vec![nodes, edges]);
        let relations = u.list(&self.relations);
        let seeds = u.list(&self.seeds);
        let rules = u.list(&self.rules);
        let program = u.compound("datalog_program", vec![relations, seeds, rules]);
        let depends = u.list(&self.depends);
        let strata = u.list(&self.strata);
        u.compound("checked_datalog", vec![graph, program, depends, strata])
    }
}

/// `sort/2`: standard order, duplicates removed.
pub fn prolog_sort(u: &Universe, mut rows: Vec<TermId>) -> Vec<TermId> {
    rows.sort_by(|a, b| u.cmp(*a, *b));
    rows.dedup();
    rows
}

/// `msort/2`: standard order, duplicates kept.
pub fn prolog_msort(u: &Universe, mut rows: Vec<TermId>) -> Vec<TermId> {
    rows.sort_by(|a, b| u.cmp(*a, *b));
    rows
}

struct Basement {
    nodes: Vec<TermId>,
    edges: Vec<TermId>,
    relations: Vec<TermId>,
    seeds: Vec<TermId>,
    rules: Vec<TermId>,
}

/// `:190`.
fn basement_parts(u: &Universe, program: TermId) -> Option<Basement> {
    let (name, args) = u.functor(program)?;
    if name != "basement_program" || args.len() != 2 {
        return None;
    }
    let (graph, datalog) = (args[0], args[1]);
    let (graph_name, graph_args) = u.functor(graph)?;
    if graph_name != "root_graph" || graph_args.len() != 2 {
        return None;
    }
    let (nodes, edges) = (graph_args[0], graph_args[1]);
    let (datalog_name, datalog_args) = u.functor(datalog)?;
    if datalog_name != "datalog_program" || datalog_args.len() != 3 {
        return None;
    }
    Some(Basement {
        nodes: u.as_list(nodes)?,
        edges: u.as_list(edges)?,
        relations: u.as_list(datalog_args[0])?,
        seeds: u.as_list(datalog_args[1])?,
        rules: u.as_list(datalog_args[2])?,
    })
}

/// The `@std/dl6` seam: one `return` edge gives one key set of every other
/// position, zero or many give none, and no other site derives keys.
pub fn return_key_sets(u: &mut Universe, arity: i64, return_indices: &[i64]) -> TermId {
    let [only] = return_indices else {
        return u.empty_list();
    };
    let positions: Vec<TermId> = (0..arity).filter(|i| i != only).map(|i| u.int(i)).collect();
    let inner = u.list(&positions);
    u.list(&[inner])
}

/// Arity is the product's own `:` edge count, keys its one `return` edge.
/// `origin(relation(Id), _)` declares; `product(Id)` only classifies a type.
fn product_relation_rows(u: &mut Universe, graph: &CheckerGraph, origins: &[TermId]) -> Vec<TermId> {
    let mut owners: Vec<TermId> = origins
        .iter()
        .filter_map(|row| match u.functor(*row) {
            Some(("origin", args)) if args.len() == 2 => u.unary(args[0], "relation"),
            _ => None,
        })
        .collect();
    owners.sort_by(|a, b| u.cmp(*a, *b));
    owners.dedup();
    let mut out = Vec::with_capacity(owners.len());
    for owner in owners {
        let arity = graph.owner_slots.get(&owner).map_or(0, |s| s.len() as i64);
        let indices = graph
            .owner_return_indices
            .get(&owner)
            .cloned()
            .unwrap_or_default();
        let keys = return_key_sets(u, arity, &indices);
        let reference = u.compound("ref", vec![owner]);
        let arity = u.int(arity);
        out.push(u.compound("relation", vec![reference, arity, keys]));
    }
    out
}

/// `:1099`. `relation(Target, A, K)` becomes `relation(ref(Target), A, K)`.
/// Left on this channel: loader families and the comptime `def` rows only.
fn relations_refs(u: &mut Universe, rows: &[TermId], covered: &[TermId]) -> Option<Vec<TermId>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let (name, args) = u.functor(*row).map(|(n, a)| (n.to_string(), a.to_vec()))?;
        if name != "relation" || args.len() != 3 {
            return None;
        }
        let reference = u.compound("ref", vec![args[0]]);
        if covered.contains(&reference) {
            continue;
        }
        out.push(u.compound("relation", vec![reference, args[1], args[2]]));
    }
    Some(out)
}

/// First arity per reference over the sorted, deduplicated relation list.
fn relation_arities(u: &Universe, rows: &[TermId]) -> HashMap<TermId, i64> {
    let mut out = HashMap::new();
    for row in rows {
        let Some(("relation", args)) = u.functor(*row) else {
            continue;
        };
        if args.len() != 3 {
            continue;
        }
        if let Some(arity) = u.as_int(args[1]) {
            out.entry(args[0]).or_insert(arity);
        }
    }
    out
}

fn invalid(u: &mut Universe) -> Vec<TermId> {
    let check = u.atom("check");
    let none = u.atom("none");
    let reason = u.atom("invalid_basement_program");
    vec![u.compound("diagnostic", vec![check, none, reason])]
}

/// `:182`. `None` is v7's `Checked = []`.
#[tracing::instrument(skip_all)]
pub fn check_datalog(
    u: &mut Universe,
    lowered: &Lowered,
) -> Result<(Option<Checked>, Vec<TermId>), Stop> {
    let Some(program) = lowered.program else {
        return Err(Stop::Fail("unbound basement program"));
    };
    let Some(basement) = basement_parts(u, program) else {
        return Ok((None, invalid(u)));
    };
    let Some(graph) = CheckerGraph::build(u, &basement.edges, &basement.nodes) else {
        return Err(Stop::Fail("pending_edge/4 expected"));
    };
    let origins = OriginArena::build(u, &lowered.origins);

    let declared = product_relation_rows(u, &graph, &lowered.origins);
    let covered: Vec<TermId> = declared
        .iter()
        .filter_map(|row| match u.functor(*row) {
            Some(("relation", args)) => Some(args[0]),
            _ => None,
        })
        .collect();
    let Some(supplied) = relations_refs(u, &basement.relations, &covered) else {
        return Err(Stop::Fail("relation/3 expected"));
    };
    let mut all = declared;
    all.extend(supplied);
    all.extend(kernel_relation_rows(u));
    let relations = prolog_sort(u, all);
    let arities = relation_arities(u, &relations);
    let cx = Cx {
        graph: &graph,
        relations: &arities,
    };

    let mut diagnostics = bind_diagnostics(u, &graph, &origins);
    let (colon_edges, edge_diagnostics) = resolve_edges(u, &cx, &origins);
    let (seeds, seed_diagnostics) = resolve_seeds(u, &cx, &basement.seeds, &origins)?;
    let (rules, rule_diagnostics) = resolve_rules(u, &cx, &basement.rules, &origins)?;
    diagnostics.extend(edge_diagnostics);
    diagnostics.extend(seed_diagnostics);
    diagnostics.extend(rule_diagnostics);
    if !diagnostics.is_empty() {
        return Ok((None, prolog_sort(u, diagnostics)));
    }

    let stratified = stratify_rules(u, &rules)?;
    let located = locate_strata_diagnostics(u, &stratified.diagnostics, &rules, &origins);
    if !located.is_empty() {
        // `:431` returns the located list unsorted.
        return Ok((None, located));
    }

    let depends = depends_rows(u, &rules);
    let strata = strata_rows(u, &relations, &stratified.levels);
    let (kernel_nodes, kernel_edges) = kernel_graph(u);
    let mut nodes = basement.nodes;
    nodes.extend(kernel_nodes);
    let mut edges = colon_edges;
    edges.extend(kernel_edges);
    let edges = prolog_msort(u, edges);
    let sorted_relations = prolog_msort(u, relations);
    Ok((
        Some(Checked {
            nodes,
            edges,
            relations: sorted_relations,
            seeds,
            rules,
            depends,
            strata,
        }),
        vec![],
    ))
}
