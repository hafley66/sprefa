//! Name resolution and the three resolution passes.
//! Port of `1_checker.pl:570-612`, `:727-800` and `:990-1035`.

use super::api::Stop;
use super::graph::{CheckerGraph, OriginArena};
use super::kernel::primitive_name;
use super::mode::{call_parts, goal_failures, head_safety, head_variables};
use crate::_2_lower::kernel::kernel_relation;
use crate::_6_eval::term::{Term, TermId, Universe};
use std::collections::HashMap;

pub struct Cx<'a> {
    pub graph: &'a CheckerGraph,
    /// `memberchk(relation(Target, Arity, _), Relations)` at `:994` over the
    /// sorted, deduplicated list: first row for a target wins.
    pub relations: &'a HashMap<TermId, i64>,
}

fn atom_name(u: &Universe, id: TermId) -> Option<&str> {
    match u.get(id) {
        Term::Atom(s) => Some(u.sym_str(*s)),
        _ => None,
    }
}

/// `:590`. `None` is a Prolog failure, not a diagnostic.
pub fn resolve_target(
    u: &mut Universe,
    cx: &Cx,
    target: TermId,
    visited: &mut Vec<(TermId, TermId)>,
) -> Option<TermId> {
    let (name, args) = u
        .functor(target)
        .map(|(n, a)| (n.to_string(), a.to_vec()))?;
    match (name.as_str(), args.len()) {
        ("target", 1) => Some(u.compound("ref", vec![args[0]])),
        ("const", 1) => Some(target),
        ("name", 2) => resolve_name(u, cx, args[0], args[1], visited),
        _ => None,
    }
}

/// `:599`. The four alternatives are `(Forward -> Target ; Parent ; Kernel ;
/// Primitive)`: a forward edge COMMITS, a parent does not.
pub fn resolve_name(
    u: &mut Universe,
    cx: &Cx,
    owner: TermId,
    name: TermId,
    visited: &mut Vec<(TermId, TermId)>,
) -> Option<TermId> {
    if visited.contains(&(owner, name)) {
        return None;
    }
    visited.push((owner, name));
    let resolved = resolve_name_body(u, cx, owner, name, visited);
    visited.pop();
    resolved
}

fn resolve_name_body(
    u: &mut Universe,
    cx: &Cx,
    owner: TermId,
    name: TermId,
    visited: &mut Vec<(TermId, TermId)>,
) -> Option<TermId> {
    if let Some(target) = cx.graph.forward(owner, name) {
        return resolve_target(u, cx, target, visited);
    }
    if let Some(parent) = cx.graph.parent(owner) {
        if let Some(resolved) = resolve_name(u, cx, parent, name, visited) {
            return Some(resolved);
        }
    }
    if cx.graph.module_member(owner) {
        let label = atom_name(u, name).map(|s| s.to_string())?;
        if kernel_relation(&label).is_some() {
            let inner = u.compound("kernel", vec![name]);
            return Some(u.compound("ref", vec![inner]));
        }
        if primitive_name(&label) {
            let inner = u.compound("primitive", vec![name]);
            return Some(u.compound("ref", vec![inner]));
        }
    }
    None
}

pub enum CallResult {
    /// `call(Target, ResolvedArgs)` plus its pieces.
    Ok {
        term: TermId,
        args: Vec<TermId>,
    },
    Error(TermId),
}

/// `:990`. One clause only: a callable that is not `name(Owner, Name)` makes
/// v7 fail outright (defect D1).
pub fn resolve_call(u: &mut Universe, cx: &Cx, call: TermId) -> Result<CallResult, Stop> {
    let Some((callable, args)) = call_parts(u, call) else {
        return Err(Stop::Fail("call/2 expected"));
    };
    let Some((name, parts)) = u
        .functor(callable)
        .map(|(n, a)| (n.to_string(), a.to_vec()))
    else {
        return Err(Stop::Fail("name/2 callable expected"));
    };
    if name != "name" || parts.len() != 2 {
        return Err(Stop::Fail("name/2 callable expected"));
    }
    let (owner, label) = (parts[0], parts[1]);
    let mut visited = Vec::new();
    let Some(target) = resolve_name(u, cx, owner, label, &mut visited) else {
        let reason = u.compound("unresolved_name", vec![label]);
        return Ok(CallResult::Error(reason));
    };
    if u.unary(target, "ref").is_none() {
        let reason = u.compound("not_relation", vec![label]);
        return Ok(CallResult::Error(reason));
    }
    let Some(arity) = cx.relations.get(&target).copied() else {
        let reason = u.compound("undeclared_relation", vec![label]);
        return Ok(CallResult::Error(reason));
    };
    if args.len() as i64 != arity {
        let declared = u.int(arity);
        let observed = u.int(args.len() as i64);
        let reason = u.compound("arity_mismatch", vec![label, declared, observed]);
        return Ok(CallResult::Error(reason));
    }
    match resolve_args(u, cx, &args) {
        Ok(resolved) => {
            let list = u.list(&resolved);
            let term = u.compound("call", vec![target, list]);
            Ok(CallResult::Ok {
                term,
                args: resolved,
            })
        }
        Err(reason) => Ok(CallResult::Error(reason)),
    }
}

/// `:1010`.
fn resolve_args(u: &mut Universe, cx: &Cx, args: &[TermId]) -> Result<Vec<TermId>, TermId> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        out.push(resolve_argument(u, cx, *arg)?);
    }
    Ok(out)
}

/// `:1022`.
fn resolve_argument(u: &mut Universe, cx: &Cx, argument: TermId) -> Result<TermId, TermId> {
    if let Some((name, parts)) = u
        .functor(argument)
        .map(|(n, a)| (n.to_string(), a.to_vec()))
    {
        if name == "name" && parts.len() == 2 {
            let mut visited = Vec::new();
            return match resolve_name(u, cx, parts[0], parts[1], &mut visited) {
                Some(resolved) => Ok(resolved),
                None => Err(u.compound("unresolved_name", vec![parts[1]])),
            };
        }
        if name == "aggregate" && parts.len() == 2 && atom_name(u, parts[0]) == Some("count") {
            let inner = resolve_argument(u, cx, parts[1])?;
            return Ok(u.compound("aggregate", vec![parts[0], inner]));
        }
    }
    Ok(argument)
}

/// `:570`. Deferred targets are dropped at `:571` and `:575`.
pub fn resolve_edges(
    u: &mut Universe,
    cx: &Cx,
    origins: &OriginArena,
) -> (Vec<TermId>, Vec<TermId>) {
    let check = u.atom("check");
    let mut edges = Vec::new();
    let mut diagnostics = Vec::new();
    let rows: Vec<(TermId, TermId, TermId, i64)> = cx
        .graph
        .edges
        .iter()
        .map(|e| (e.owner, e.name, e.target, e.index))
        .collect();
    for (owner, name, target, index) in rows {
        if let Some((functor, args)) = u.functor(target) {
            let deferred = (functor == "deferred_expression" && args.len() == 1)
                || (functor == "deferred_compound_edge" && args.len() == 2);
            if deferred {
                continue;
            }
        }
        let node = origins.edge_origin(owner, name, index);
        let mut visited = Vec::new();
        let resolved = match resolve_target(u, cx, target, &mut visited) {
            Some(resolved) => resolved,
            None => {
                let reason = u.compound("unresolved_name", vec![name]);
                diagnostics.push(u.compound("diagnostic", vec![check, node, reason]));
                target
            }
        };
        let index = u.int(index);
        edges.push(u.compound(":", vec![owner, name, resolved, index]));
    }
    (edges, diagnostics)
}

/// `:727`.
pub fn resolve_seeds(
    u: &mut Universe,
    cx: &Cx,
    seeds: &[TermId],
    origins: &OriginArena,
) -> Result<(Vec<TermId>, Vec<TermId>), Stop> {
    let check = u.atom("check");
    let mut resolved = Vec::with_capacity(seeds.len());
    let mut diagnostics = Vec::new();
    for (index, seed) in seeds.iter().enumerate() {
        let node = origins.seed_origin(index as i64);
        match resolve_call(u, cx, *seed)? {
            CallResult::Ok { term, args } => {
                if args.iter().any(|a| u.unary(*a, "var").is_some()) {
                    resolved.push(*seed);
                    let reason = u.atom("non_ground_seed");
                    diagnostics.push(u.compound("diagnostic", vec![check, node, reason]));
                } else {
                    resolved.push(term);
                }
            }
            CallResult::Error(reason) => {
                resolved.push(*seed);
                diagnostics.push(u.compound("diagnostic", vec![check, node, reason]));
            }
        }
    }
    Ok((resolved, diagnostics))
}

/// `:748`.
pub fn resolve_rules(
    u: &mut Universe,
    cx: &Cx,
    rules: &[TermId],
    origins: &OriginArena,
) -> Result<(Vec<TermId>, Vec<TermId>), Stop> {
    let check = u.atom("check");
    let mut resolved = Vec::with_capacity(rules.len());
    let mut diagnostics = Vec::new();
    for (index, rule) in rules.iter().enumerate() {
        let rule_index = index as i64;
        let Some(("rule", parts)) = u.functor(*rule) else {
            return Err(Stop::Fail("rule/2 expected"));
        };
        if parts.len() != 2 {
            return Err(Stop::Fail("rule/2 expected"));
        }
        let (head, body) = (parts[0], parts[1]);
        let Some(body) = u.as_list(body) else {
            return Err(Stop::Fail("rule body list expected"));
        };
        let node = origins.rule_origin(rule_index);
        let head_result = resolve_call(u, cx, head)?;
        let (goals, goal_diagnostics) = resolve_goals(u, cx, &body, rule_index, origins)?;
        match (&head_result, &goals) {
            (CallResult::Ok { term, .. }, Some(goals)) => {
                let goal_list = u.list(goals);
                resolved.push(u.compound("rule", vec![*term, goal_list]));
                let Some(head_vars) = head_variables(u, *term) else {
                    return Err(Stop::Fail("head call/2 expected"));
                };
                let Some(fold) = goal_failures(u, goals, &head_vars, &[]) else {
                    return Err(Stop::Fail("goal transition"));
                };
                diagnostics.extend(goal_diagnostics);
                for (goal_index, reason) in &fold.failures {
                    let node = origins.goal_origin(rule_index, *goal_index);
                    diagnostics.push(u.compound("diagnostic", vec![check, node, *reason]));
                }
                diagnostics.extend(head_safety(u, &head_vars, goals, node));
            }
            _ => {
                resolved.push(*rule);
                if let CallResult::Error(reason) = head_result {
                    diagnostics.push(u.compound("diagnostic", vec![check, node, reason]));
                }
                diagnostics.extend(goal_diagnostics);
            }
        }
    }
    Ok((resolved, diagnostics))
}

/// `:778`. `None` is v7's `error(rule)` at `:795`.
fn resolve_goals(
    u: &mut Universe,
    cx: &Cx,
    body: &[TermId],
    rule_index: i64,
    origins: &OriginArena,
) -> Result<(Option<Vec<TermId>>, Vec<TermId>), Stop> {
    let check = u.atom("check");
    let mut goals = Vec::with_capacity(body.len());
    let mut diagnostics = Vec::new();
    let mut ok = true;
    for (index, pending) in body.iter().enumerate() {
        let Some(("pending_goal", parts)) = u.functor(*pending) else {
            return Err(Stop::Fail("pending_goal/2 expected"));
        };
        if parts.len() != 2 {
            return Err(Stop::Fail("pending_goal/2 expected"));
        }
        let (polarity, call) = (parts[0], parts[1]);
        let node = origins.goal_origin(rule_index, index as i64);
        match resolve_call(u, cx, call)? {
            CallResult::Ok { term, .. } => {
                goals.push(u.compound("checked_goal", vec![polarity, term]));
            }
            CallResult::Error(reason) => {
                ok = false;
                diagnostics.push(u.compound("diagnostic", vec![check, node, reason]));
            }
        }
    }
    Ok((if ok { Some(goals) } else { None }, diagnostics))
}
