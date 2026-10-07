//! `check_datalog/4`. Port of `1_checker.pl:182-230` and `:416-431`.
//!
//! Pure: no file IO, no clock, no globals. v7's origin arena and graph store
//! are plain structs built per call and dropped with the frame.

use super::bind::bind_diagnostics;
use super::graph::{CheckerGraph, OriginArena};
use super::kernel::{kernel_graph, kernel_relation_rows};
use super::mode::{call_parts, goal_call, variable_call};
use super::resolve::{resolve_edges, resolve_rules, resolve_seeds, Cx};
use super::strata::{depends_rows, locate_strata_diagnostics, strata_rows, stratify_rules};
use crate::_2_lower::Lowered;
use crate::_6_eval::term::{TermId, Universe};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// Where v7's `check_datalog/4` fails instead of returning a diagnostic
/// (defect D1 in the PLAN).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Fail(&'static str),
}

/// `checked_datalog(root_graph(Nodes, Edges),
///                  datalog_program(Relations, Seeds, Rules),
///                  Depends, Strata)` at `:427`, one field per argument.
#[derive(Clone)]
pub struct Checked {
    pub nodes: Vec<TermId>,
    pub edges: Vec<TermId>,
    pub relations: Vec<TermId>,
    pub seeds: Vec<TermId>,
    pub rules: Vec<TermId>,
    pub depends: Vec<TermId>,
    pub strata: Vec<TermId>,
    /// Rules with a call goal; comptime expands them once per relation.
    pub calls: Vec<TermId>,
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

pub(crate) struct Basement {
    pub(crate) nodes: Vec<TermId>,
    pub(crate) edges: Vec<TermId>,
    pub(crate) relations: Vec<TermId>,
    pub(crate) seeds: Vec<TermId>,
    pub(crate) rules: Vec<TermId>,
}

/// `:190`.
pub(crate) fn basement_parts(u: &Universe, program: TermId) -> Option<Basement> {
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
pub(crate) fn product_relation_rows(
    u: &mut Universe,
    graph: &CheckerGraph,
    origins: &[TermId],
) -> Vec<TermId> {
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
pub(crate) fn relations_refs(
    u: &mut Universe,
    rows: &[TermId],
    covered: &[TermId],
) -> Option<Vec<TermId>> {
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
pub(crate) fn relation_arities(u: &Universe, rows: &[TermId]) -> HashMap<TermId, i64> {
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
    tracing::info!(target: "dl8::a1", "check_run");
    let Some(program) = lowered.program else {
        return Err(Stop::Fail("unbound basement program"));
    };
    let Some(basement) = basement_parts(u, program) else {
        return Ok((None, invalid(u)));
    };
    let Some(graph) = CheckerGraph::build(u, &basement.edges, &basement.nodes) else {
        return Err(Stop::Fail("pending_edge/4 expected"));
    };
    let resolved = resolve_fresh(u, graph, basement, &lowered.origins)?;
    emit_checked(u, &resolved)
}

/// The middle of one full check, before stratification. The keep holds it so
/// a later program's delta resolves alone against the union graph.
pub(crate) struct Resolved {
    pub graph: CheckerGraph,
    pub origins: OriginArena,
    /// `prolog_sort` of the declared, supplied and kernel relation rows.
    pub relations: Vec<TermId>,
    /// `resolve_seeds` output: the resolved seeds, their value mints apart.
    pub resolved_seeds: Vec<TermId>,
    pub seed_mints: Vec<TermId>,
    /// All rules resolved, before the `calls` partition.
    pub resolved_rules: Vec<TermId>,
    /// `resolve_edges` output and its instance intern rows.
    pub colon_edges: Vec<TermId>,
    pub edge_mints: Vec<TermId>,
    /// Application owners whose intern rows are already minted.
    pub built: HashSet<TermId>,
    pub diagnostics: Vec<TermId>,
    // The raw basement shape: the delta prefix compares against it.
    pub nodes: Vec<TermId>,
    pub edge_rows: Vec<TermId>,
    pub relation_rows: Vec<TermId>,
    pub seed_rows: Vec<TermId>,
    pub rule_rows: Vec<TermId>,
    /// Aligned with `rule_rows`: the graph owners and relation arities each
    /// rule's resolution consulted, the key a refreeze reuses rules on.
    pub rule_reads: Vec<Vec<super::resolve::Note>>,
}

/// The four resolution passes over one whole basement, at offsets zero.
pub(crate) fn resolve_fresh(
    u: &mut Universe,
    graph: CheckerGraph,
    basement: Basement,
    origin_rows: &[TermId],
) -> Result<Resolved, Stop> {
    let origins = OriginArena::build(u, origin_rows);
    let declared = product_relation_rows(u, &graph, origin_rows);
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
    let notes = RefCell::new(Vec::new());
    let cx = Cx {
        graph: &graph,
        relations: &arities,
        reads: Some(&notes),
    };

    let mut diagnostics = bind_diagnostics(u, &graph, &origins, &graph.edges);
    let mut built = HashSet::new();
    let (colon_edges, edge_mints, edge_diagnostics) =
        resolve_edges(u, &cx, &origins, &graph.edges, &mut built);
    let (resolved_seeds, seed_mints, seed_diagnostics) =
        resolve_seeds(u, &cx, &basement.seeds, &origins, 0)?;
    let rule_pass = resolve_rules(u, &cx, &basement.rules, &origins, 0)?;
    let resolved_rules = rule_pass.rules;
    let rule_diagnostics = rule_pass.diagnostics;
    diagnostics.extend(edge_diagnostics);
    diagnostics.extend(seed_diagnostics);
    diagnostics.extend(rule_diagnostics);
    Ok(Resolved {
        graph,
        origins,
        relations,
        resolved_seeds,
        seed_mints,
        resolved_rules,
        colon_edges,
        edge_mints,
        built,
        diagnostics,
        nodes: basement.nodes,
        edge_rows: basement.edges,
        relation_rows: basement.relations,
        seed_rows: basement.seeds,
        rule_rows: basement.rules,
        rule_reads: rule_pass.reads,
    })
}

/// The tail of `check_datalog`: diagnostics, stratification over the union,
/// then the checked term.
pub(crate) fn emit_checked(
    u: &mut Universe,
    resolved: &Resolved,
) -> Result<(Option<Checked>, Vec<TermId>), Stop> {
    if !resolved.diagnostics.is_empty() {
        return Ok((None, prolog_sort(u, resolved.diagnostics.clone())));
    }

    let stratified = stratify_rules(u, &resolved.resolved_rules)?;
    let located = locate_strata_diagnostics(
        u,
        &stratified.diagnostics,
        &resolved.resolved_rules,
        &resolved.origins,
    );
    if !located.is_empty() {
        // `:431` returns the located list unsorted.
        return Ok((None, located));
    }

    let (calls, rules): (Vec<TermId>, Vec<TermId>) = resolved
        .resolved_rules
        .iter()
        .copied()
        .partition(|rule| rule_calls(u, *rule));
    let depends = depends_rows(u, &rules);
    let strata = strata_rows(u, &resolved.relations, &stratified.levels);
    let (kernel_nodes, kernel_edges) = kernel_graph(u);
    let mut nodes = resolved.nodes.clone();
    nodes.extend(kernel_nodes);
    let mut seeds = resolved.resolved_seeds.clone();
    seeds.extend(resolved.seed_mints.iter().copied());
    seeds.extend(resolved.edge_mints.iter().copied());
    let mut edges = resolved.colon_edges.clone();
    edges.extend(kernel_edges);
    let edges = prolog_msort(u, edges);
    let sorted_relations = prolog_msort(u, resolved.relations.clone());
    Ok((
        Some(Checked {
            nodes,
            edges,
            relations: sorted_relations,
            seeds,
            rules,
            depends,
            strata,
            calls,
        }),
        vec![],
    ))
}

/// A resolved rule with a `variable_call` goal.
pub fn rule_calls(u: &Universe, rule: TermId) -> bool {
    let body = u
        .args::<2>(rule, "rule")
        .and_then(|[_, body]| u.as_list(body));
    body.unwrap_or_default().iter().any(|goal| {
        goal_call(u, *goal)
            .and_then(|(_, call)| call_parts(u, call))
            .is_some_and(|(relation, _)| variable_call(u, relation).is_some())
    })
}
