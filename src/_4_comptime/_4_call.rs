//! Call goals `(?Checker ..)`, `(?Bound.check ..)`: one rule per relation that
//! reaches the value, the value fixed to that relation everywhere in the rule.

use crate::_3_check::api::prolog_sort;
use crate::_3_check::mode::{argument_variables, call_parts, goal_call, variable_call};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashMap;

/// Nesting of one rule argument a call value is substituted into.
const CALL_TERM_DEPTH: usize = 64;

pub struct CallRules {
    pub rules: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// The rules `templates` expand to over `rows`. A relation reaches a call when
/// the first positive goal binding the call's value holds a row naming it.
pub fn call_rules(
    u: &mut Universe,
    templates: &[TermId],
    rows: &[&[TermId]],
    relations: &[TermId],
) -> CallRules {
    let mut out = CallRules {
        rules: Vec::new(),
        diagnostics: Vec::new(),
    };
    if templates.is_empty() {
        return out;
    }
    let by_relation = rows_by_relation(u, rows);
    let arities = relation_arities(u, relations);
    for template in templates {
        let calls = rule_parts(u, *template).map_or(0, |(_, goals)| {
            goals.iter().filter(|g| call_goal(u, **g).is_some()).count()
        });
        let mut pending = vec![*template];
        for _ in 0..=calls {
            let mut next = Vec::new();
            for rule in pending {
                let Some((head, goals)) = rule_parts(u, rule) else {
                    continue;
                };
                let Some((at, value, site, arguments)) = first_call(u, &goals) else {
                    out.rules.push(rule);
                    continue;
                };
                for name in reaching(u, &goals[..at], value, &by_relation) {
                    let observed = arguments.len() as i64;
                    let reason = match arities.get(&name).copied() {
                        None => {
                            let shape = u.atom("call");
                            Some(u.compound("not_built_yet", vec![shape, name]))
                        }
                        Some(arity) if arity != observed => {
                            let declared = u.int(arity);
                            let observed = u.int(observed);
                            Some(u.compound("arity_mismatch", vec![name, declared, observed]))
                        }
                        Some(_) => None,
                    };
                    if let Some(reason) = reason {
                        let check = u.atom("check");
                        out.diagnostics
                            .push(u.compound("diagnostic", vec![check, site, reason]));
                        continue;
                    }
                    match fixed(u, head, &goals, at, value, name) {
                        Some(rule) => next.push(rule),
                        None => {
                            let check = u.atom("check");
                            let reason = u.atom("call_term_depth_exceeded");
                            out.diagnostics
                                .push(u.compound("diagnostic", vec![check, site, reason]));
                        }
                    }
                }
            }
            pending = next;
        }
    }
    CallRules {
        rules: prolog_sort(u, out.rules),
        diagnostics: prolog_sort(u, out.diagnostics),
    }
}

fn rows_by_relation(u: &Universe, rows: &[&[TermId]]) -> HashMap<TermId, Vec<Vec<TermId>>> {
    let mut out: HashMap<TermId, Vec<Vec<TermId>>> = HashMap::new();
    for row in rows.iter().flat_map(|rows| rows.iter()) {
        if let Some((relation, arguments)) = call_parts(u, *row) {
            out.entry(relation).or_default().push(arguments);
        }
    }
    out
}

fn relation_arities(u: &Universe, relations: &[TermId]) -> HashMap<TermId, i64> {
    let mut out = HashMap::new();
    for row in relations {
        let Some([relation, arity, _]) = u.args::<3>(*row, "relation") else {
            continue;
        };
        if let Some(arity) = u.as_int(arity) {
            out.entry(relation).or_insert(arity);
        }
    }
    out
}

fn rule_parts(u: &Universe, rule: TermId) -> Option<(TermId, Vec<TermId>)> {
    let [head, body] = u.args::<2>(rule, "rule")?;
    Some((head, u.as_list(body)?))
}

/// `(Value, Site, Arguments)` of a call goal.
fn call_goal(u: &Universe, goal: TermId) -> Option<(TermId, TermId, Vec<TermId>)> {
    let (_, call) = goal_call(u, goal)?;
    let (relation, arguments) = call_parts(u, call)?;
    let [value, site] = variable_call(u, relation)?;
    Some((value, site, arguments))
}

fn first_call(u: &Universe, goals: &[TermId]) -> Option<(usize, TermId, TermId, Vec<TermId>)> {
    goals.iter().enumerate().find_map(|(at, goal)| {
        call_goal(u, *goal).map(|(value, site, arguments)| (at, value, site, arguments))
    })
}

/// The relations that reach `value` through the first positive goal before
/// the call that binds it, read from rows matching that goal's ground columns.
fn reaching(
    u: &Universe,
    before: &[TermId],
    value: TermId,
    by_relation: &HashMap<TermId, Vec<Vec<TermId>>>,
) -> Vec<TermId> {
    if u.unary(value, "var").is_none() {
        return vec![value];
    }
    let binding = before.iter().find_map(|goal| {
        let (polarity, call) = goal_call(u, *goal)?;
        if u.functor_or_atom(polarity).map(|(p, _)| p) != Some("positive") {
            return None;
        }
        let (relation, arguments) = call_parts(u, call)?;
        if variable_call(u, relation).is_some() {
            return None;
        }
        let position = arguments.iter().position(|a| *a == value)?;
        Some((relation, arguments, position))
    });
    let Some((relation, arguments, position)) = binding else {
        return vec![];
    };
    let names: Vec<TermId> = by_relation
        .get(&relation)
        .into_iter()
        .flatten()
        .filter(|row| {
            row.len() == arguments.len()
                && arguments.iter().zip(row.iter()).all(|(argument, column)| {
                    !argument_variables(u, *argument).is_empty() || argument == column
                })
        })
        .map(|row| row[position])
        .collect();
    prolog_sort(u, names)
}

/// The rule with `value` fixed to `name` and goal `at` calling `name`.
fn fixed(
    u: &mut Universe,
    head: TermId,
    goals: &[TermId],
    at: usize,
    value: TermId,
    name: TermId,
) -> Option<TermId> {
    let head = substitute(u, value, name, head, 0)?;
    let mut body = Vec::with_capacity(goals.len());
    for (index, goal) in goals.iter().enumerate() {
        let goal = substitute(u, value, name, *goal, 0)?;
        if index != at {
            body.push(goal);
            continue;
        }
        let (polarity, call) = goal_call(u, goal)?;
        let (_, arguments) = call_parts(u, call)?;
        let list = u.list(&arguments);
        let call = u.compound("call", vec![name, list]);
        body.push(u.compound("checked_goal", vec![polarity, call]));
    }
    let body = u.list(&body);
    Some(u.compound("rule", vec![head, body]))
}

/// `None` past `CALL_TERM_DEPTH`; the caller reports `call_term_depth_exceeded`.
fn substitute(
    u: &mut Universe,
    from: TermId,
    to: TermId,
    term: TermId,
    depth: usize,
) -> Option<TermId> {
    if term == from {
        return Some(to);
    }
    let Some((functor, arguments)) = u.functor(term).map(|(f, a)| (f.to_string(), a.to_vec()))
    else {
        return Some(term);
    };
    if depth >= CALL_TERM_DEPTH {
        return None;
    }
    let mut replaced = Vec::with_capacity(arguments.len());
    for argument in arguments {
        replaced.push(substitute(u, from, to, argument, depth + 1)?);
    }
    Some(u.compound(&functor, replaced))
}
