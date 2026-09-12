//! `logical_program_graph_rows/2` and `logical_program_graph_calls/2,3`. Port
//! of `0a_logical_program_grapher.pl:16-120`.
//!
//! Head and body edges point to distinct occurrence nodes, so later clock,
//! trigger, read and write annotations have an owner without changing calls
//! (`:12-14`).

use super::api::Stop;
use super::rows::logical_program_rows_term;
use crate::_3_check::api::prolog_sort;
use crate::_6_eval::term::{TermId, Universe};

/// `:110`.
fn logical_id(u: &mut Universe, id: TermId) -> TermId {
    u.compound("logical_program", vec![id])
}

/// `:112-113`.
fn goal_id(u: &mut Universe, rule_id: TermId, position: TermId) -> TermId {
    let occurrence = u.compound("goal_occurrence", vec![rule_id, position]);
    logical_id(u, occurrence)
}

fn reference(u: &mut Universe, id: TermId) -> TermId {
    u.compound("ref", vec![id])
}

/// `':'(Owner, Label, Target, Index)`.
fn edge(u: &mut Universe, owner: TermId, label: &str, target: TermId, index: i64) -> TermId {
    let label = u.atom(label);
    let index = u.int(index);
    u.compound(":", vec![owner, label, target, index])
}

/// The functor and arguments of one reified row.
fn row_parts(u: &Universe, row: TermId) -> Option<(String, Vec<TermId>)> {
    u.functor(row).map(|(n, a)| (n.to_string(), a.to_vec()))
}

/// `:16-20`. Clause order does not reach the caller: `sort/2` at `:20` fixes
/// the order of the whole set.
pub fn logical_program_graph_rows(u: &mut Universe, checked: TermId) -> Result<Vec<TermId>, Stop> {
    let rows = logical_program_rows_term(u, checked)?;
    let mut out = Vec::new();

    // :90-108, then :51-52. Nodes and products carry the same occurrence set.
    let mut occurrences = Vec::new();
    for row in &rows {
        let Some((name, args)) = row_parts(u, *row) else {
            continue;
        };
        match (name.as_str(), args.len()) {
            ("program_seed", 2) | ("program_rule", 2) => occurrences.push(logical_id(u, args[0])),
            ("program_goal", 4) => occurrences.push(goal_id(u, args[0], args[1])),
            ("program_apply", 2) => occurrences.push(logical_id(u, args[0])),
            ("program_argument", 3) => occurrences.push(logical_id(u, args[2])),
            ("program_edge", 4) => {
                let input = u.atom("input");
                if args[1] == input {
                    if let Some(id) = u.unary(args[2], "ref") {
                        occurrences.push(logical_id(u, id));
                    }
                }
            }
            _ => {}
        }
    }
    for id in &occurrences {
        out.push(u.compound("node", vec![*id]));
        out.push(u.compound("product", vec![*id]));
    }

    for row in &rows {
        let Some((name, args)) = row_parts(u, *row) else {
            continue;
        };
        match (name.as_str(), args.len()) {
            // :54-57
            ("program_rule", 2) => {
                let rule = logical_id(u, args[0]);
                let head_call = logical_id(u, args[1]);
                let target = reference(u, head_call);
                out.push(edge(u, rule, "head", target, 0));
            }
            // :58-69
            ("program_goal", 4) => {
                let rule = logical_id(u, args[0]);
                let goal = goal_id(u, args[0], args[1]);
                let position = u
                    .as_int(args[1])
                    .ok_or(Stop::Fail("goal position expected"))?;
                let goal_reference = reference(u, goal);
                out.push(edge(u, rule, "body", goal_reference, position + 1));
                let polarity = u.compound("const", vec![args[2]]);
                out.push(edge(u, goal, "polarity", polarity, 0));
                let call = logical_id(u, args[3]);
                let call_reference = reference(u, call);
                out.push(edge(u, goal, "call", call_reference, 1));
            }
            // :71-74
            ("program_seed", 2) => {
                let seed = logical_id(u, args[0]);
                let call = logical_id(u, args[1]);
                let target = reference(u, call);
                out.push(edge(u, seed, "call", target, 0));
            }
            // :76-78. The relation is the raw identity, never wrapped.
            ("program_apply", 2) => {
                let call = logical_id(u, args[0]);
                let target = reference(u, args[1]);
                out.push(edge(u, call, "apply", target, 0));
            }
            // :79-83
            ("program_argument", 3) => {
                let call = logical_id(u, args[0]);
                let argument = logical_id(u, args[2]);
                let position = u
                    .as_int(args[1])
                    .ok_or(Stop::Fail("argument position expected"))?;
                let target = reference(u, argument);
                out.push(edge(u, call, "argument", target, position + 1));
            }
            // :85-88. Label and index ride along from the reified row.
            ("program_edge", 4) => {
                let Some(target) = argument_target(u, args[1], args[2]) else {
                    continue;
                };
                let argument = logical_id(u, args[0]);
                out.push(u.compound(":", vec![argument, args[1], target, args[3]]));
            }
            _ => {}
        }
    }
    Ok(prolog_sort(u, out))
}

/// `:115-120`.
fn argument_target(u: &mut Universe, label: TermId, raw: TermId) -> Option<TermId> {
    let reference_label = u.atom("reference");
    let input_label = u.atom("input");
    if label == reference_label {
        return u.unary(raw, "ref").map(|_| raw);
    }
    if label == input_label {
        let identity = u.unary(raw, "ref")?;
        let wrapped = logical_id(u, identity);
        return Some(u.compound("ref", vec![wrapped]));
    }
    u.unary(raw, "const").map(|_| raw)
}

/// `:40-46`. `None` is v7's atom `all` (`:48`).
pub fn logical_program_graph_calls(
    u: &mut Universe,
    checked: TermId,
    relations: Option<&[TermId]>,
) -> Result<Vec<TermId>, Stop> {
    let rows = logical_program_graph_rows(u, checked)?;
    let mut calls = Vec::new();
    for row in &rows {
        let Some((name, args)) = row_parts(u, *row) else {
            continue;
        };
        let (relation, arguments) = match (name.as_str(), args.len()) {
            ("node", 1) | ("product", 1) => {
                let kernel_name = u.atom(&name);
                let relation = u.compound("kernel", vec![kernel_name]);
                let target = reference(u, args[0]);
                (relation, vec![target])
            }
            (":", 4) => {
                let colon = u.atom(":");
                let relation = u.compound("kernel", vec![colon]);
                let owner = reference(u, args[0]);
                let label = u.compound("const", vec![args[1]]);
                let index = u.compound("const", vec![args[3]]);
                (relation, vec![owner, label, args[2], index])
            }
            _ => continue,
        };
        if relations.is_some_and(|wanted| !wanted.contains(&relation)) {
            continue;
        }
        let target = reference(u, relation);
        let arguments = u.list(&arguments);
        calls.push(u.compound("call", vec![target, arguments]));
    }
    Ok(prolog_sort(u, calls))
}
