//! The bridge to `_6_eval::stratify`, plus the dependency and stratum rows.
//! Port of `1_checker.pl:433-474` and `:1075-1097`.

use super::api::{prolog_sort, Stop};
use super::graph::OriginArena;
use super::mode::{call_parts, goal_call};
use crate::_6_eval::program::{Arg, Goal, Polarity, Program, Rule, VarId};
use crate::_6_eval::stratify::stratify;
use crate::_6_eval::term::{Term, TermId, Universe};

fn arg_of(u: &Universe, term: TermId, vars: &mut Vec<TermId>) -> Arg {
    if let Some(identity) = u.unary(term, "var") {
        let position = match vars.iter().position(|x| *x == identity) {
            Some(p) => p,
            None => {
                vars.push(identity);
                vars.len() - 1
            }
        };
        return Arg::Var(VarId(position as u32));
    }
    if let Some((name, args)) = u.functor(term) {
        if name == "aggregate" && args.len() == 2 {
            if let Term::Atom(s) = u.get(args[0]) {
                if u.sym_str(*s) == "count" {
                    let inner = args[1];
                    return Arg::Count(Box::new(arg_of(u, inner, vars)));
                }
            }
        }
    }
    Arg::Ground(term)
}

/// `rule(call(Relation, Arguments), [checked_goal(Polarity, call(R, A))])`.
pub fn eval_program(u: &Universe, rules: &[TermId]) -> Result<Program, Stop> {
    let mut program = Program::default();
    for rule in rules {
        let Some(("rule", parts)) = u.functor(*rule) else {
            return Err(Stop::Fail("rule/2 expected"));
        };
        let (Some((rel, head_args)), Some(body)) = (call_parts(u, parts[0]), u.as_list(parts[1]))
        else {
            return Err(Stop::Fail("rule head or body"));
        };
        let mut vars = Vec::new();
        let head: Vec<Arg> = head_args.iter().map(|a| arg_of(u, *a, &mut vars)).collect();
        let mut goals = Vec::with_capacity(body.len());
        for item in &body {
            let Some((polarity, call)) = goal_call(u, *item) else {
                return Err(Stop::Fail("checked_goal/2 expected"));
            };
            let Some((goal_rel, args)) = call_parts(u, call) else {
                return Err(Stop::Fail("goal call/2 expected"));
            };
            let polarity = match u.get(polarity) {
                Term::Atom(s) if u.sym_str(*s) == "negative" => Polarity::Negative,
                _ => Polarity::Positive,
            };
            goals.push(Goal {
                polarity,
                rel: goal_rel,
                args: args.iter().map(|a| arg_of(u, *a, &mut vars)).collect(),
            });
        }
        program.rules.push(Rule {
            rel,
            head,
            body: goals,
            vars,
        });
    }
    Ok(program)
}

pub struct Stratified {
    /// `stratum(Relation, Level)` for every derived relation.
    pub levels: std::collections::HashMap<TermId, u32>,
    /// `diagnostic(stratify, none, Reason)` rows.
    pub diagnostics: Vec<TermId>,
}

/// `stratify_rules/3` at `v7/src/1_libtime/0_evaluator.pl:654`.
pub fn stratify_rules(u: &mut Universe, rules: &[TermId]) -> Result<Stratified, Stop> {
    let program = eval_program(u, rules)?;
    let (strata, diagnostics) = stratify(u, &program);
    let none = u.atom("none");
    let diagnostics = diagnostics
        .iter()
        .map(|d| {
            let phase = u.atom(d.phase);
            u.compound("diagnostic", vec![phase, none, d.payload])
        })
        .collect();
    Ok(Stratified {
        levels: strata.levels,
        diagnostics,
    })
}

/// `:433`. Only the two cycle reasons move; everything else passes through.
pub fn locate_strata_diagnostics(
    u: &mut Universe,
    diagnostics: &[TermId],
    rules: &[TermId],
    origins: &OriginArena,
) -> Vec<TermId> {
    let mut out = Vec::with_capacity(diagnostics.len());
    for diagnostic in diagnostics {
        let Some(("diagnostic", args)) = u.functor(*diagnostic) else {
            out.push(*diagnostic);
            continue;
        };
        let (phase, node, reason) = (args[0], args[1], args[2]);
        let stratify_phase = matches!(u.get(phase), Term::Atom(s) if u.sym_str(*s) == "stratify");
        let unlocated = matches!(u.get(node), Term::Atom(s) if u.sym_str(*s) == "none");
        if !stratify_phase || !unlocated {
            out.push(*diagnostic);
            continue;
        }
        let Some((name, payload)) = u.functor(reason).map(|(n, a)| (n.to_string(), a.to_vec()))
        else {
            out.push(*diagnostic);
            continue;
        };
        if payload.len() != 1 {
            out.push(*diagnostic);
            continue;
        }
        let Some(relations) = u.as_list(payload[0]) else {
            out.push(*diagnostic);
            continue;
        };
        let node = match name.as_str() {
            "strict_dependency_cycle" => cycle_origin(u, rules, &relations, origins),
            "aggregate_dependency_cycle" => aggregate_cycle_origin(u, rules, &relations, origins),
            _ => {
                out.push(*diagnostic);
                continue;
            }
        };
        out.push(u.compound("diagnostic", vec![phase, node, reason]));
    }
    out
}

/// `:456`. The first rule whose head is in the cycle and whose first negative
/// goal is also in the cycle.
fn cycle_origin(
    u: &Universe,
    rules: &[TermId],
    relations: &[TermId],
    origins: &OriginArena,
) -> TermId {
    for (rule_index, rule) in rules.iter().enumerate() {
        let Some(("rule", parts)) = u.functor(*rule) else {
            continue;
        };
        let (Some((head_rel, _)), Some(body)) = (call_parts(u, parts[0]), u.as_list(parts[1]))
        else {
            continue;
        };
        if !relations.contains(&head_rel) {
            continue;
        }
        for (goal_index, goal) in body.iter().enumerate() {
            let Some((polarity, call)) = goal_call(u, *goal) else {
                continue;
            };
            if !matches!(u.get(polarity), Term::Atom(s) if u.sym_str(*s) == "negative") {
                continue;
            }
            let Some((body_rel, _)) = call_parts(u, call) else {
                continue;
            };
            if relations.contains(&body_rel) {
                return origins.goal_origin(rule_index as i64, goal_index as i64);
            }
        }
    }
    origins.none
}

/// `:467`.
fn aggregate_cycle_origin(
    u: &Universe,
    rules: &[TermId],
    relations: &[TermId],
    origins: &OriginArena,
) -> TermId {
    for (rule_index, rule) in rules.iter().enumerate() {
        let Some(("rule", parts)) = u.functor(*rule) else {
            continue;
        };
        let Some((head_rel, head_args)) = call_parts(u, parts[0]) else {
            continue;
        };
        if !relations.contains(&head_rel) {
            continue;
        }
        let counted = head_args.iter().any(|a| {
            u.functor(*a)
                .is_some_and(|(n, args)| n == "aggregate" && args.len() == 2)
                && u.functor(*a).is_some_and(
                    |(_, args)| matches!(u.get(args[0]), Term::Atom(s) if u.sym_str(*s) == "count"),
                )
        });
        if counted {
            return origins.rule_origin(rule_index as i64);
        }
    }
    origins.none
}

/// `:1075`. One `depends(HeadRef, BodyRef, Polarity)` per goal, then sorted.
pub fn depends_rows(u: &mut Universe, rules: &[TermId]) -> Vec<TermId> {
    let mut out = Vec::new();
    for rule in rules {
        let Some(("rule", parts)) = u.functor(*rule) else {
            continue;
        };
        let (Some((head_rel, _)), Some(body)) = (call_parts(u, parts[0]), u.as_list(parts[1]))
        else {
            continue;
        };
        for goal in &body {
            let Some((polarity, call)) = goal_call(u, *goal) else {
                continue;
            };
            let Some((body_rel, _)) = call_parts(u, call) else {
                continue;
            };
            out.push(u.compound("depends", vec![head_rel, body_rel, polarity]));
        }
    }
    prolog_sort(u, out)
}

/// `:1088`. One stratum row per declared relation; relations without a derived
/// head stay at zero.
pub fn strata_rows(
    u: &mut Universe,
    relations: &[TermId],
    levels: &std::collections::HashMap<TermId, u32>,
) -> Vec<TermId> {
    let mut out = Vec::with_capacity(relations.len());
    for relation in relations {
        let Some(("relation", args)) = u.functor(*relation) else {
            continue;
        };
        let target = args[0];
        let level = levels.get(&target).copied().unwrap_or(0);
        let level = u.int(level as i64);
        out.push(u.compound("stratum", vec![target, level]));
    }
    prolog_sort(u, out)
}
