//! Bound-variable fold over a checked body, and head safety.
//! Port of `1_checker.pl:802-982` and `:1038-1072`.
//!
//! Two variable collectors disagree on purpose: `argument_variables/2`
//! (`:939`) walks every `var(_)` subterm and sorts, `goal_variables/2`
//! (`:1058`) reads top-level `var(_)` arguments only and keeps order and
//! duplicates. Head safety compares the first against the second.

use super::kernel::is_comparison;
use crate::_6_eval::term::{Term, TermId, Universe};
use std::collections::HashSet;

/// `add_variables/3` at `:960` prepends each unseen variable, so the list
/// grows `[Vn, ..., V1 | Bound0]`. `check_goal_sequence/4` exports it.
#[derive(Clone, Default)]
pub struct Bound {
    pub order: Vec<TermId>,
    pub members: HashSet<TermId>,
}

impl Bound {
    pub fn from(items: &[TermId]) -> Bound {
        Bound {
            order: items.to_vec(),
            members: items.iter().copied().collect(),
        }
    }

    pub fn holds(&self, id: TermId) -> bool {
        self.members.contains(&id)
    }

    pub fn add_all(&mut self, items: &[TermId]) {
        for item in items {
            if self.members.insert(*item) {
                self.order.insert(0, *item);
            }
        }
    }
}

/// `:939`. Every `var(_)` subterm, sorted and deduplicated.
pub fn argument_variables(u: &Universe, argument: TermId) -> Vec<TermId> {
    let mut out = Vec::new();
    collect_variables(u, argument, &mut out);
    out.sort_by(|a, b| u.cmp(*a, *b));
    out.dedup();
    out
}

fn collect_variables(u: &Universe, id: TermId, out: &mut Vec<TermId>) {
    if let Some(identity) = u.unary(id, "var") {
        out.push(identity);
    }
    if let Term::Compound(_, args) = u.get(id) {
        for arg in args.clone() {
            collect_variables(u, arg, out);
        }
    }
}

/// `goal_call/3` at `:1055`.
pub fn goal_call(u: &Universe, goal: TermId) -> Option<(TermId, TermId)> {
    let (name, args) = u.functor(goal)?;
    if name != "checked_goal" || args.len() != 2 {
        return None;
    }
    Some((args[0], args[1]))
}

/// `call(Relation, Arguments)`.
pub fn call_parts(u: &Universe, call: TermId) -> Option<(TermId, Vec<TermId>)> {
    let (name, args) = u.functor(call)?;
    if name != "call" || args.len() != 2 {
        return None;
    }
    Some((args[0], u.as_list(args[1])?))
}

/// `:1058`. Top-level `var(_)` arguments, in order, duplicates kept.
pub fn goal_variables(u: &Universe, goal: TermId) -> Option<Vec<TermId>> {
    let (_, call) = goal_call(u, goal)?;
    let (_, arguments) = call_parts(u, call)?;
    Some(
        arguments
            .iter()
            .filter_map(|a| u.unary(*a, "var"))
            .collect(),
    )
}

/// `:978`.
pub fn head_variables(u: &Universe, head: TermId) -> Option<Vec<TermId>> {
    let (_, arguments) = call_parts(u, head)?;
    let mut out: Vec<TermId> = arguments
        .iter()
        .flat_map(|a| argument_variables(u, *a))
        .collect();
    out.sort_by(|a, b| u.cmp(*a, *b));
    out.dedup();
    Some(out)
}

fn polarity(u: &Universe, id: TermId) -> Option<&'static str> {
    match u.get(id) {
        Term::Atom(s) => match u.sym_str(*s) {
            "positive" => Some("positive"),
            "negative" => Some("negative"),
            _ => None,
        },
        _ => None,
    }
}

/// `ref(kernel(Name))`.
fn kernel_name(u: &Universe, relation: TermId) -> Option<String> {
    let inner = u.unary(relation, "ref")?;
    let name = u.unary(inner, "kernel")?;
    match u.get(name) {
        Term::Atom(s) => Some(u.sym_str(*s).to_string()),
        _ => None,
    }
}

/// `:935`.
fn argument_is_bound(u: &Universe, argument: TermId, bound: &Bound) -> bool {
    argument_variables(u, argument)
        .iter()
        .all(|v| bound.holds(*v))
}

/// `:932`.
fn int_argument(u: &Universe, argument: TermId) -> bool {
    if u.unary(argument, "var").is_some() {
        return true;
    }
    match u.unary(argument, "const") {
        Some(value) => u.as_int(value).is_some(),
        None => false,
    }
}

/// `:924`. `none` when every argument is an int shape.
fn comparison_argument_types(u: &mut Universe, name: &str, arguments: &[TermId]) -> Option<TermId> {
    let bad = arguments.iter().position(|a| !int_argument(u, *a))?;
    let name = u.atom(name);
    let position = u.int(bad as i64);
    let kind = u.atom("int");
    Some(u.compound(
        "kernel_argument_type_mismatch",
        vec![name, position, kind, arguments[bad]],
    ))
}

fn modes(u: &mut Universe, sets: &[&[i64]]) -> TermId {
    let rows: Vec<TermId> = sets
        .iter()
        .map(|set| {
            let items: Vec<TermId> = set.iter().map(|i| u.int(*i)).collect();
            u.list(&items)
        })
        .collect();
    u.list(&rows)
}

fn underconstrained(u: &mut Universe, name: &str, sets: &[&[i64]]) -> TermId {
    let name = u.atom(name);
    let list = modes(u, sets);
    u.compound("underconstrained_kernel_goal", vec![name, list])
}

/// `:840-889`. `None` is v7's `none` reason.
fn check_positive(
    u: &mut Universe,
    relation: TermId,
    arguments: &[TermId],
    variables: &[TermId],
    bound: &mut Bound,
) -> Option<TermId> {
    match kernel_name(u, relation).as_deref() {
        Some(name) if is_comparison(name) => {
            if arguments.iter().all(|a| argument_is_bound(u, *a, bound)) {
                comparison_argument_types(u, name, arguments)
            } else {
                Some(underconstrained(u, name, &[&[0, 1]]))
            }
        }
        Some("cons") if arguments.len() == 3 => {
            let ready = argument_is_bound(u, arguments[2], bound)
                || (argument_is_bound(u, arguments[0], bound)
                    && argument_is_bound(u, arguments[1], bound));
            if ready {
                bound.add_all(variables);
                None
            } else {
                Some(underconstrained(u, "cons", &[&[2], &[0, 1]]))
            }
        }
        Some("edge_ref") if arguments.len() == 3 => {
            if argument_is_bound(u, arguments[0], bound)
                && argument_is_bound(u, arguments[1], bound)
            {
                bound.add_all(variables);
                None
            } else {
                Some(underconstrained(u, "edge_ref", &[&[0, 1]]))
            }
        }
        Some("intern") if arguments.len() == 3 => {
            if argument_is_bound(u, arguments[0], bound)
                && argument_is_bound(u, arguments[1], bound)
            {
                bound.add_all(variables);
                None
            } else {
                Some(underconstrained(u, "intern", &[&[0, 1]]))
            }
        }
        _ => {
            bound.add_all(variables);
            None
        }
    }
}

/// `:891-919`.
fn check_negative(
    u: &mut Universe,
    relation: TermId,
    arguments: &[TermId],
    variables: &[TermId],
    bound: &Bound,
) -> Option<TermId> {
    if let Some(name) = kernel_name(u, relation) {
        if matches!(name.as_str(), "cons" | "edge_ref" | "intern" | "nil") {
            let name = u.atom(&name);
            return Some(u.compound("negative_constructive_kernel_goal", vec![name]));
        }
        if is_comparison(&name) {
            let unbound = unbound_variables(variables, bound);
            return if unbound.is_empty() {
                comparison_argument_types(u, &name, arguments)
            } else {
                let list = u.list(&unbound);
                Some(u.compound("unbound_negative_goal", vec![list]))
            };
        }
    }
    let unbound = unbound_variables(variables, bound);
    if unbound.is_empty() {
        None
    } else {
        let list = u.list(&unbound);
        Some(u.compound("unbound_negative_goal", vec![list]))
    }
}

/// `:952`. Order and duplicates are kept.
fn unbound_variables(variables: &[TermId], bound: &Bound) -> Vec<TermId> {
    variables
        .iter()
        .copied()
        .filter(|v| !bound.holds(*v))
        .collect()
}

pub struct Fold {
    pub available: Bound,
    pub produced: Bound,
    /// `goal_failure(GoalIndex, Reason)` at `:819`.
    pub failures: Vec<(i64, TermId)>,
}

/// `:807`. `None` where v7's `check_goal_transition/6` has no matching
/// clause and the whole predicate fails.
pub fn goal_failures(
    u: &mut Universe,
    goals: &[TermId],
    available: &[TermId],
    produced: &[TermId],
) -> Option<Fold> {
    let mut fold = Fold {
        available: Bound::from(available),
        produced: Bound::from(produced),
        failures: Vec::new(),
    };
    for (index, goal) in goals.iter().enumerate() {
        let (polarity_term, call) = goal_call(u, *goal)?;
        let sign = polarity(u, polarity_term)?;
        let (relation, arguments) = call_parts(u, call)?;
        let variables = goal_variables(u, *goal)?;
        let reason = if sign == "negative" {
            check_negative(u, relation, &arguments, &variables, &fold.produced)
        } else {
            let reason = check_positive(u, relation, &arguments, &variables, &mut fold.available);
            if reason.is_none() {
                fold.produced.add_all(&variables);
            }
            reason
        };
        if let Some(reason) = reason {
            fold.failures.push((index as i64, reason));
        }
    }
    Some(fold)
}

/// `:802`. The exported helper the comptime lane calls.
pub fn check_goal_sequence(
    u: &mut Universe,
    goals: &[TermId],
    bound: &[TermId],
) -> Option<(Vec<TermId>, Vec<TermId>)> {
    let fold = goal_failures(u, goals, bound, bound)?;
    let check = u.atom("check");
    let none = u.atom("none");
    let diagnostics = fold
        .failures
        .iter()
        .map(|(_, reason)| u.compound("diagnostic", vec![check, none, *reason]))
        .collect();
    Some((fold.available.order, diagnostics))
}

/// `:968`.
pub fn unlocated_diagnostics(u: &mut Universe, failures: &[(i64, TermId)]) -> Vec<TermId> {
    let check = u.atom("check");
    let none = u.atom("none");
    failures
        .iter()
        .map(|(_, reason)| u.compound("diagnostic", vec![check, none, *reason]))
        .collect()
}

/// `:1048`. Body variables come from `goal_variables/2`, head variables from
/// `argument_variables/2`; the asymmetry is v7's (fork F1 in the PLAN).
pub fn head_safety(
    u: &mut Universe,
    head_variables: &[TermId],
    body: &[TermId],
    node: TermId,
) -> Vec<TermId> {
    let body_variables: HashSet<TermId> = body
        .iter()
        .filter_map(|g| goal_variables(u, *g))
        .flatten()
        .collect();
    let check = u.atom("check");
    head_variables
        .iter()
        .filter(|v| !body_variables.contains(v))
        .map(|v| {
            let reason = u.compound("unsafe_head_var", vec![*v]);
            u.compound("diagnostic", vec![check, node, reason])
        })
        .collect()
}
