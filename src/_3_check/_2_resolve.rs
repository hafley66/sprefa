//! Name resolution and the three resolution passes.
//! Port of `1_checker.pl:570-612`, `:727-800` and `:990-1035`.

use super::api::Stop;
use super::graph::{CheckerGraph, OriginArena};
use super::instance::instance_rows;
use super::kernel::primitive_name;
use super::mode::{call_parts, goal_failures, head_safety, head_variables};
use crate::_2_lower::index::Edge;
use crate::_2_lower::kernel::{kernel_relation, kernel_variadic};
use crate::_6_eval::program::AggregateKind;
use crate::_6_eval::term::{Term, TermId, Universe};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

/// What one rule's resolution consulted: a graph owner whose rows a name
/// walk read, or a relation target whose arity it matched. A later refreeze
/// keys rule reuse on these.
#[derive(Clone, PartialEq, Eq)]
pub enum Note {
    Owner(TermId),
    Arity(TermId, i64),
}

pub struct Cx<'a> {
    pub graph: &'a CheckerGraph,
    /// `memberchk(relation(Target, Arity, _), Relations)` at `:994` over the
    /// sorted, deduplicated list: first row for a target wins.
    pub relations: &'a HashMap<TermId, i64>,
    /// Scratch consult notes for the keep; `None` records nothing. The rule
    /// pass snapshots the cell once per rule.
    pub reads: Option<&'a RefCell<Vec<Note>>>,
}

impl Cx<'_> {
    fn note(&self, note: Note) {
        if let Some(cell) = self.reads {
            let mut notes = cell.borrow_mut();
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
    }
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
        ("name", 2) => resolve_path(u, cx, args[0], args[1], visited).ok(),
        _ => None,
    }
}

/// A dotted path: `owner` may itself be a `name/2` chain, one level per segment.
/// The inner level resolves first, and every level but the last must land on a
/// `ref`. `Err` carries the segment that did not resolve.
pub fn resolve_path(
    u: &mut Universe,
    cx: &Cx,
    owner: TermId,
    label: TermId,
    visited: &mut Vec<(TermId, TermId)>,
) -> Result<TermId, TermId> {
    let owner = path_owner(u, cx, owner, visited)?;
    resolve_name(u, cx, owner, label, visited).ok_or(label)
}

fn path_owner(
    u: &mut Universe,
    cx: &Cx,
    owner: TermId,
    visited: &mut Vec<(TermId, TermId)>,
) -> Result<TermId, TermId> {
    let parts = match u.functor(owner) {
        Some(("name", args)) if args.len() == 2 => (args[0], args[1]),
        _ => return Ok(owner),
    };
    let (inner_owner, inner_label) = parts;
    let inner_owner = path_owner(u, cx, inner_owner, visited)?;
    let target = resolve_name(u, cx, inner_owner, inner_label, visited).ok_or(inner_label)?;
    u.unary(target, "ref").ok_or(inner_label)
}

/// `:599`. A forward edge commits. Typed kernel members belong to the primitive
/// owner itself, before lookup continues through its parent scope.
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
    cx.note(Note::Owner(owner));
    if let Some(target) = cx.graph.forward(owner, name) {
        return resolve_target(u, cx, target, visited);
    }
    if let Some(primitive) = u.unary(owner, "primitive") {
        let typed = atom_name(u, primitive).map(str::to_owned);
        let label = atom_name(u, name).map(str::to_owned);
        if let (Some(typed), Some(label)) = (typed, label) {
            if kernel_relation(Some(&typed), &label).is_some() {
                let inner = u.compound("kernel", vec![primitive, name]);
                return Some(u.compound("ref", vec![inner]));
            }
        }
    }
    // A kernel name a module does not declare stays the kernel's; an importer's
    // declaration shadows it only for the importer's own goals.
    if cx.graph.module_member(owner) {
        if let Some(label) = atom_name(u, name).map(str::to_owned) {
            if kernel_relation(None, &label).is_some() || kernel_variadic(None, &label) {
                let inner = u.compound("kernel", vec![name]);
                return Some(u.compound("ref", vec![inner]));
            }
        }
    }
    if let Some(parent) = cx.graph.parent(owner) {
        if let Some(resolved) = resolve_name(u, cx, parent, name, visited) {
            return Some(resolved);
        }
    }
    if cx.graph.module_member(owner) {
        let label = atom_name(u, name).map(|s| s.to_string())?;
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
    if name == "variable_call" && parts.len() == 2 {
        let resolved = resolve_argument(u, cx, parts[0])
            .and_then(|value| Ok((value, resolve_args(u, cx, &args)?)));
        return Ok(match resolved {
            Ok((value, resolved)) => {
                let relation = u.compound("variable_call", vec![value, parts[1]]);
                let list = u.list(&resolved);
                let term = u.compound("call", vec![relation, list]);
                CallResult::Ok {
                    term,
                    args: resolved,
                }
            }
            Err(reason) => CallResult::Error(reason),
        });
    }
    let (target, label) = match (name.as_str(), &parts[..]) {
        // A generated goal names its kernel or prelude relation by identity.
        ("ref", &[identity]) => {
            let label = u.functor(identity).and_then(|(_, args)| args.last().copied());
            (callable, label.unwrap_or(identity))
        }
        ("name", &[owner, label]) => {
            let mut visited = Vec::new();
            match resolve_path(u, cx, owner, label, &mut visited) {
                // `vec` names the primitive that owns `vec.iter`; called, it is the kernel.
                Ok(target)
                    if u
                        .unary(target, "ref")
                        .and_then(|inner| u.unary(inner, "primitive"))
                        .and_then(|primitive| atom_name(u, primitive))
                        .is_some_and(|primitive| kernel_variadic(None, primitive)) =>
                {
                    let primitive = u.unary(u.unary(target, "ref").unwrap(), "primitive").unwrap();
                    let inner = u.compound("kernel", vec![primitive]);
                    (u.compound("ref", vec![inner]), label)
                }
                Ok(target) => (target, label),
                Err(segment) => {
                    let reason = u.compound("unresolved_name", vec![segment]);
                    return Ok(CallResult::Error(reason));
                }
            }
        }
        _ => return Err(Stop::Fail("name/2 callable expected")),
    };
    if u.unary(target, "ref").is_none() {
        let reason = u.compound("not_relation", vec![label]);
        return Ok(CallResult::Error(reason));
    }
    let variadic = u
        .unary(target, "ref")
        .and_then(|inner| u.unary(inner, "kernel"))
        .is_some_and(|name| atom_name(u, name).is_some_and(|label| kernel_variadic(None, label)));
    if variadic {
        return match resolve_args(u, cx, &args) {
            Ok(resolved) => {
                let list = u.list(&resolved);
                let term = u.compound("call", vec![target, list]);
                Ok(CallResult::Ok {
                    term,
                    args: resolved,
                })
            }
            Err(reason) => Ok(CallResult::Error(reason)),
        };
    }
    let Some(arity) = cx.relations.get(&target).copied() else {
        let reason = u.compound("undeclared_relation", vec![label]);
        return Ok(CallResult::Error(reason));
    };
    cx.note(Note::Arity(target, arity));
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
pub fn resolve_argument(u: &mut Universe, cx: &Cx, argument: TermId) -> Result<TermId, TermId> {
    if let Some((name, parts)) = u
        .functor(argument)
        .map(|(n, a)| (n.to_string(), a.to_vec()))
    {
        if name == "name" && parts.len() == 2 {
            let mut visited = Vec::new();
            return match resolve_path(u, cx, parts[0], parts[1], &mut visited) {
                Ok(resolved) => Ok(resolved),
                Err(segment) => Err(u.compound("unresolved_name", vec![segment])),
            };
        }
        if name == "application" && parts.len() == 2 && atom_name(u, parts[0]) == Some("vec") {
            // The term the `vec` kernel builds: the tagged items under `ref`.
            let items = u.as_list(parts[1]).unwrap_or_default();
            let mut tagged = Vec::with_capacity(items.len());
            for item in items {
                tagged.push(resolve_argument(u, cx, item)?);
            }
            let list = u.list(&tagged);
            let node = u.compound("application", vec![parts[0], list]);
            return Ok(u.compound("ref", vec![node]));
        }
        if name == "application" && parts.len() == 2 {
            let constructor = resolve_argument(u, cx, parts[0])?;
            let items = u.as_list(parts[1]).unwrap_or_default();
            let mut plain = Vec::with_capacity(items.len());
            for item in items {
                let tagged = resolve_argument(u, cx, item)?;
                plain.push(
                    u.unary(tagged, "ref")
                        .or_else(|| u.unary(tagged, "const"))
                        .unwrap_or(tagged),
                );
            }
            let (Some(inner), list) = (u.unary(constructor, "ref"), u.list(&plain)) else {
                return Ok(argument);
            };
            let node = u.compound("application", vec![inner, list]);
            return Ok(u.compound("ref", vec![node]));
        }
        if name == "aggregate"
            && parts.len() == 2
            && atom_name(u, parts[0]).and_then(AggregateKind::of).is_some()
        {
            let inner = resolve_argument(u, cx, parts[1])?;
            return Ok(u.compound("aggregate", vec![parts[0], inner]));
        }
        if name == "fold" && parts.len() == 3 {
            let mut resolved = Vec::with_capacity(3);
            for part in &parts {
                resolved.push(resolve_argument(u, cx, *part)?);
            }
            return Ok(u.compound("fold", resolved));
        }
    }
    Ok(argument)
}

/// `:570`. Deferred targets are dropped at `:571` and `:575`. The middle list
/// is the `intern` rows of the instances generic binds build.
pub fn resolve_edges(
    u: &mut Universe,
    cx: &Cx,
    origins: &OriginArena,
    parts: &[Edge],
    built: &mut HashSet<TermId>,
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let check = u.atom("check");
    let mut edges = Vec::new();
    let mut minted = Vec::new();
    let mut diagnostics = Vec::new();
    let rows: Vec<(TermId, TermId, TermId, i64)> = parts
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
        // A body application's instance owns its edges and mints its intern
        // rows once.
        if let Some([callee, list]) = u.args::<2>(owner, "application") {
            if built.insert(owner) {
                let tagged: Vec<TermId> = u
                    .as_list(list)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| match u.functor(item) {
                        Some(_) => u.compound("ref", vec![item]),
                        None => u.compound("const", vec![item]),
                    })
                    .collect();
                let instance = u.compound("ref", vec![owner]);
                intern_calls(u, callee, &tagged, instance, &mut minted);
            }
        }
        if u.functor(target)
            .is_some_and(|(n, a)| n == "application" && a.len() == 2)
        {
            match instance_rows(u, cx, owner, name, index, target, built, &mut minted) {
                Ok(rows) => edges.extend(rows),
                Err(reason) => {
                    diagnostics.push(u.compound("diagnostic", vec![check, node, reason]))
                }
            }
            continue;
        }
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
    (edges, minted, diagnostics)
}

/// `:727`. The resolved seeds and their value mints come back apart: the
/// delta path concatenates both groups in order, the single pass appends the
/// mints of the whole list.
pub fn resolve_seeds(
    u: &mut Universe,
    cx: &Cx,
    seeds: &[TermId],
    origins: &OriginArena,
    seed_offset: i64,
) -> Result<(Vec<TermId>, Vec<TermId>, Vec<TermId>), Stop> {
    let check = u.atom("check");
    let mut resolved = Vec::with_capacity(seeds.len());
    let mut diagnostics = Vec::new();
    let mut minted = Vec::new();
    for (index, seed) in seeds.iter().enumerate() {
        let node = origins.seed_origin(seed_offset + index as i64);
        match resolve_call(u, cx, *seed)? {
            CallResult::Ok { term, args } => {
                if args.iter().any(|a| u.unary(*a, "var").is_some()) {
                    resolved.push(*seed);
                    let reason = u.atom("non_ground_seed");
                    diagnostics.push(u.compound("diagnostic", vec![check, node, reason]));
                } else {
                    resolved.push(term);
                    for value in &args {
                        mint_rows(u, *value, 0, &mut minted);
                    }
                }
            }
            CallResult::Error(reason) => {
                resolved.push(*seed);
                diagnostics.push(u.compound("diagnostic", vec![check, node, reason]));
            }
        }
    }
    Ok((resolved, minted, diagnostics))
}

/// Nesting of value constructions inside one fact argument.
const MINT_DEPTH: usize = 16;

/// A fact that builds a value inserts the `intern` row of its identity, so the
/// value reads back the way a rule-built one does.
fn mint_rows(u: &mut Universe, value: TermId, depth: usize, out: &mut Vec<TermId>) {
    let depth_exceeded = depth >= MINT_DEPTH;
    let Some(node) = u.unary(value, "ref") else {
        return;
    };
    let Some([constructor, list]) = u.args::<2>(node, "application") else {
        return;
    };
    // A `vec` is the `vec` kernel's term, never an `intern` instance.
    if depth_exceeded || atom_name(u, constructor) == Some("vec") {
        return;
    }
    let items = u.as_list(list).unwrap_or_default();
    let mut tagged = Vec::with_capacity(items.len());
    for item in items {
        if u.functor(item).is_some() {
            let reference = u.compound("ref", vec![item]);
            mint_rows(u, reference, depth + 1, out);
            tagged.push(reference);
        } else {
            tagged.push(u.compound("const", vec![item]));
        }
    }
    intern_calls(u, constructor, &tagged, value, out);
}

/// The `intern` and `intern_snapshot` rows of `value`, built by `constructor`
/// from the tagged arguments.
pub fn intern_calls(
    u: &mut Universe,
    constructor: TermId,
    tagged: &[TermId],
    value: TermId,
    out: &mut Vec<TermId>,
) {
    let arguments = u.list(tagged);
    let tuple = u.atom("tuple");
    let arguments = u.compound("application", vec![tuple, arguments]);
    let arguments = u.compound("ref", vec![arguments]);
    let constructor = u.compound("ref", vec![constructor]);
    let args = u.list(&[constructor, arguments, value]);
    for name in ["intern", "intern_snapshot"] {
        let atom = u.atom(name);
        let kernel = u.compound("kernel", vec![atom]);
        let relation = u.compound("ref", vec![kernel]);
        out.push(u.compound("call", vec![relation, args]));
    }
}

/// `:748`.
///
/// One pass over rules: the resolved terms, their diagnostics, and per rule
/// the consult notes a later refreeze keys reuse on.
pub struct RuleResolved {
    pub rules: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
    /// Aligned with `rules`.
    pub reads: Vec<Vec<Note>>,
}

pub fn resolve_rules(
    u: &mut Universe,
    cx: &Cx,
    rules: &[TermId],
    origins: &OriginArena,
    rule_offset: i64,
) -> Result<RuleResolved, Stop> {
    let check = u.atom("check");
    let mut resolved = Vec::with_capacity(rules.len());
    let mut diagnostics = Vec::new();
    let mut reads: Vec<Vec<Note>> = Vec::with_capacity(rules.len());
    for (index, rule) in rules.iter().enumerate() {
        if let Some(cell) = cx.reads {
            cell.borrow_mut().clear();
        }
        let rule_index = rule_offset + index as i64;
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
        // A `next` head resolves its inner relation and keeps the wrapper.
        let next = u.args::<2>(head, "call").and_then(|[relation, args]| Some((u.unary(relation, "next")?, args)));
        let head_result = match next {
            Some((inner, args)) => {
                let inner_call = u.compound("call", vec![inner, args]);
                match resolve_call(u, cx, inner_call)? {
                    CallResult::Ok { term, args } => {
                        let [relation, list] = u.args::<2>(term, "call").ok_or(Stop::Fail("call/2 expected"))?;
                        let relation = u.compound("next", vec![relation]);
                        CallResult::Ok { term: u.compound("call", vec![relation, list]), args }
                    }
                    error => error,
                }
            }
            None => resolve_call(u, cx, head)?,
        };
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
        if let Some(cell) = cx.reads {
            reads.push(cell.borrow().clone());
        }
    }
    Ok(RuleResolved {
        rules: resolved,
        diagnostics,
        reads,
    })
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
