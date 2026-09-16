//! `check_resolved_rules/5`, the second door. Port of `1_checker.pl:237-414`.
//!
//! Relation references are already canonical here, so there is no name
//! resolution: declaration, arity, mode, safety and stratification only.
//! `Relations` is the caller's raw list and `memberchk/2` at `:397` reads it
//! in that order.

use super::api::{prolog_sort, Stop};
use super::mode::{call_parts, goal_call, goal_failures, head_safety, head_variables};
use super::strata::{depends_rows, strata_rows, stratify_rules};
use crate::_6_eval::term::{Term, TermId, Universe};
use std::collections::HashMap;

pub struct Resolved {
    pub depends: Vec<TermId>,
    pub strata: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// First row per relation reference, list order.
fn relation_arities(u: &Universe, relations: &[TermId]) -> HashMap<TermId, i64> {
    let mut out = HashMap::new();
    for row in relations {
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

/// `:245`.
pub fn check_resolved_rules(
    u: &mut Universe,
    relations: &[TermId],
    rules: &[TermId],
) -> Result<Resolved, Stop> {
    let arities = relation_arities(u, relations);
    let diagnostics = rule_diagnostics(u, rules, &arities)?;
    if !diagnostics.is_empty() {
        return Ok(Resolved {
            depends: vec![],
            strata: vec![],
            diagnostics: prolog_sort(u, diagnostics),
        });
    }
    let stratified = stratify_rules(u, rules)?;
    if !stratified.diagnostics.is_empty() {
        return Ok(Resolved {
            depends: vec![],
            strata: vec![],
            diagnostics: stratified.diagnostics,
        });
    }
    Ok(Resolved {
        depends: depends_rows(u, rules),
        strata: strata_rows(u, relations, &stratified.levels),
        diagnostics: vec![],
    })
}

fn diagnostic(u: &mut Universe, reason: TermId) -> TermId {
    let check = u.atom("check");
    let none = u.atom("none");
    u.compound("diagnostic", vec![check, none, reason])
}

/// `:354`.
fn rule_diagnostics(
    u: &mut Universe,
    rules: &[TermId],
    arities: &HashMap<TermId, i64>,
) -> Result<Vec<TermId>, Stop> {
    let mut out = Vec::new();
    for rule in rules {
        out.extend(rule_diagnostic(u, *rule, arities)?);
    }
    Ok(out)
}

/// `:360`.
fn rule_diagnostic(
    u: &mut Universe,
    rule: TermId,
    arities: &HashMap<TermId, i64>,
) -> Result<Vec<TermId>, Stop> {
    let parts = match u.functor(rule) {
        Some(("rule", args)) if args.len() == 2 => (args[0], args[1]),
        _ => {
            let reason = u.compound("invalid_generated_rule", vec![rule]);
            return Ok(vec![diagnostic(u, reason)]);
        }
    };
    let (head, body_term) = parts;
    let Some(body) = u.as_list(body_term) else {
        return Err(Stop::Fail("rule body list expected"));
    };
    let mut out = call_diagnostics(u, head, arities);
    out.extend(goal_diagnostics(u, &body, arities));
    // `head_variables/2` and `check_goal_sequence_failures/7` both fail on the
    // shapes that produce `invalid_generated_polarity`, `invalid_generated_goal`
    // and `invalid_generated_call`, which is why those three are unreachable
    // (defect D2 in the PLAN).
    let Some(head_vars) = head_variables(u, head) else {
        return Err(Stop::Fail("head call/2 expected"));
    };
    let Some(fold) = goal_failures(u, &body, &head_vars, &[]) else {
        return Err(Stop::Fail("goal transition"));
    };
    for (_, reason) in &fold.failures {
        out.push(diagnostic(u, *reason));
    }
    let none = u.atom("none");
    out.extend(head_safety(u, &head_vars, &body, none));
    Ok(out)
}

/// `:376`.
fn goal_diagnostics(
    u: &mut Universe,
    body: &[TermId],
    arities: &HashMap<TermId, i64>,
) -> Vec<TermId> {
    let mut out = Vec::new();
    for goal in body {
        match goal_call(u, *goal) {
            Some((polarity, call)) => {
                let known = matches!(u.get(polarity),
                    Term::Atom(s) if matches!(u.sym_str(*s), "positive" | "negative"));
                if !known {
                    let reason = u.compound("invalid_generated_polarity", vec![polarity]);
                    out.push(diagnostic(u, reason));
                }
                out.extend(call_diagnostics(u, call, arities));
            }
            None => {
                let reason = u.compound("invalid_generated_goal", vec![*goal]);
                out.push(diagnostic(u, reason));
            }
        }
    }
    out
}

/// `:396`.
fn call_diagnostics(u: &mut Universe, call: TermId, arities: &HashMap<TermId, i64>) -> Vec<TermId> {
    let Some((relation, arguments)) = call_parts(u, call) else {
        let reason = u.compound("invalid_generated_call", vec![call]);
        return vec![diagnostic(u, reason)];
    };
    match arities.get(&relation).copied() {
        Some(arity) if arity == arguments.len() as i64 => vec![],
        Some(arity) => {
            let declared = u.int(arity);
            let observed = u.int(arguments.len() as i64);
            let reason = u.compound(
                "generated_arity_mismatch",
                vec![relation, declared, observed],
            );
            vec![diagnostic(u, reason)]
        }
        None => {
            let reason = u.compound("generated_undeclared_relation", vec![relation]);
            vec![diagnostic(u, reason)]
        }
    }
}
