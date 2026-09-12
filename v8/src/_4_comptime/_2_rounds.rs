//! `evaluate_compiler_rounds/11`, the inner fixpoint. Port of
//! `2_compiler.pl:1173-1206`, `:1384-1467` and `:1505-1517`.

use super::api::{
    colon_rows, diagnostic, intern_rows, is_kernel_ref, strip_intern_rows, strip_snapshot_rows,
    Generated,
};
use super::assemble::assemble_generated_program;
use super::finish::{derived_bind_diagnostics, validate_functional_rows};
use crate::_3_check::api::prolog_sort;
use crate::_3_check::{check_resolved_rules, Stop};
use crate::_6_eval::program::{Arg, Goal, Polarity, Program, Row, Rule, VarId};
use crate::_6_eval::term::{TermId, Universe};
use crate::_6_eval::{evaluate, Trace};

/// `debug_round_decision/2` at `:1376` prints exactly these three.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Outcome {
    Stable,
    Continue,
    LimitExhausted,
}

/// The inert per-round record; effects leave the reducer through `fx`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Round {
    Evaluate {
        outer: i64,
        round: i64,
        rules: usize,
        seeds: usize,
        closure: usize,
    },
    Assemble {
        outer: i64,
        round: i64,
        relations: usize,
        rules: usize,
    },
    Decision {
        outer: i64,
        round: i64,
        outcome: Outcome,
    },
    Refreeze {
        outer: i64,
        deferred: bool,
    },
}

/// The four frozen lists plus the counter; one struct, reduced per round.
pub struct RoundState {
    pub frozen_edges: Vec<TermId>,
    pub frozen_requests: Vec<TermId>,
    pub frozen_generated_relations: Vec<TermId>,
    pub frozen_generated_rules: Vec<TermId>,
    pub round: i64,
}

pub struct RoundOutcome {
    pub closure: Vec<TermId>,
    pub generated: Generated,
    pub diagnostics: Vec<TermId>,
}

impl RoundOutcome {
    pub fn failed(diagnostics: Vec<TermId>) -> Self {
        RoundOutcome {
            closure: vec![],
            generated: Generated::default(),
            diagnostics,
        }
    }
}

/// `:1467`. One constant serves both the inner and the outer loop; fork F4.
pub const COMPILER_ROUND_LIMIT: i64 = 16;

/// `:1173`. `outer` is the source-refreeze pass, carried for the trace only.
#[allow(clippy::too_many_arguments)]
pub fn rounds(
    u: &mut Universe,
    authored_rules: &[TermId],
    base_relations: &[TermId],
    base_seeds: &[TermId],
    frozen_edges: Vec<TermId>,
    frozen_requests: Vec<TermId>,
    frozen_generated_relations: Vec<TermId>,
    frozen_generated_rules: Vec<TermId>,
    outer: i64,
    fx: &mut dyn FnMut(Round),
) -> Result<RoundOutcome, Stop> {
    let mut st = RoundState {
        frozen_edges,
        frozen_requests,
        frozen_generated_relations,
        frozen_generated_rules,
        round: 1,
    };
    loop {
        let mut relations = base_relations.to_vec();
        relations.extend_from_slice(&st.frozen_generated_relations);
        let relations = prolog_sort(u, relations);
        let mut rules = authored_rules.to_vec();
        rules.extend_from_slice(&st.frozen_generated_rules);
        let rules = prolog_sort(u, rules);

        let resolved = check_resolved_rules(u, &relations, &rules)?;
        let seeds = compiler_round_seeds(u, base_seeds, &st.frozen_edges, &st.frozen_requests);
        if !resolved.diagnostics.is_empty() {
            return Ok(RoundOutcome::failed(resolved.diagnostics));
        }

        let (closure, evaluation_diagnostics) = evaluate_round(u, &rules, &seeds)?;
        if !evaluation_diagnostics.is_empty() {
            return Ok(RoundOutcome::failed(evaluation_diagnostics));
        }
        let closure = strip_snapshot_rows(u, &closure);
        fx(Round::Evaluate {
            outer,
            round: st.round,
            rules: rules.len(),
            seeds: seeds.len(),
            closure: closure.len(),
        });

        let next_edges = colon_rows(u, &closure);
        let next_requests = intern_rows(u, &closure);
        let assembled = assemble_generated_program(u, &closure, base_relations);
        fx(Round::Assemble {
            outer,
            round: st.round,
            relations: assembled.relations.len(),
            rules: assembled.rules.len(),
        });
        if !assembled.diagnostics.is_empty() {
            return Ok(RoundOutcome::failed(assembled.diagnostics));
        }

        if next_edges == st.frozen_edges
            && next_requests == st.frozen_requests
            && assembled.relations == st.frozen_generated_relations
            && assembled.rules == st.frozen_generated_rules
        {
            fx(Round::Decision {
                outer,
                round: st.round,
                outcome: Outcome::Stable,
            });
            let mut all = base_relations.to_vec();
            all.extend_from_slice(&assembled.relations);
            let all = prolog_sort(u, all);
            let mut stable = derived_bind_diagnostics(u, authored_rules, &closure);
            stable.extend(validate_functional_rows(u, &all, &closure));
            let stable = prolog_sort(u, stable);
            if !stable.is_empty() {
                return Ok(RoundOutcome::failed(stable));
            }
            return Ok(RoundOutcome {
                closure: strip_intern_rows(u, &closure),
                generated: Generated {
                    relations: assembled.relations,
                    rules: assembled.rules,
                    depends: resolved.depends,
                    strata: resolved.strata,
                },
                diagnostics: vec![],
            });
        }
        if st.round >= COMPILER_ROUND_LIMIT {
            fx(Round::Decision {
                outer,
                round: st.round,
                outcome: Outcome::LimitExhausted,
            });
            let limit = u.int(COMPILER_ROUND_LIMIT);
            let reason = u.compound("compiler_round_limit_exhausted", vec![limit]);
            let d = diagnostic(u, "compile", reason);
            return Ok(RoundOutcome::failed(vec![d]));
        }
        fx(Round::Decision {
            outer,
            round: st.round,
            outcome: Outcome::Continue,
        });
        st.frozen_edges = next_edges;
        st.frozen_requests = next_requests;
        st.frozen_generated_relations = assembled.relations;
        st.frozen_generated_rules = assembled.rules;
        st.round += 1;
    }
}

/// `:1505`. Last round's edges and intern requests re-enter as read-only rows.
pub fn compiler_round_seeds(
    u: &mut Universe,
    base_seeds: &[TermId],
    frozen_edges: &[TermId],
    frozen_requests: &[TermId],
) -> Vec<TermId> {
    let mut out = base_seeds.to_vec();
    for edge in frozen_edges {
        out.push(rename_kernel_call(u, *edge, ":", "edge_snapshot"));
    }
    for request in frozen_requests {
        out.push(rename_kernel_call(u, *request, "intern", "intern_snapshot"));
    }
    prolog_sort(u, out)
}

/// `:1514` and `:1517`: same arguments, different kernel relation.
pub fn rename_kernel_call(u: &mut Universe, row: TermId, from: &str, to: &str) -> TermId {
    let Some(("call", args)) = u.functor(row) else {
        return row;
    };
    let (rel, arguments) = (args[0], args[1]);
    if !is_kernel_ref(u, rel, from) {
        return row;
    }
    let atom = u.atom(to);
    let kernel = u.compound("kernel", vec![atom]);
    let rel = u.compound("ref", vec![kernel]);
    u.compound("call", vec![rel, arguments])
}

/// `evaluate_compiler_program/5` at `:1203`, over the landed kernel.
pub fn evaluate_round(
    u: &mut Universe,
    rules: &[TermId],
    seeds: &[TermId],
) -> Result<(Vec<TermId>, Vec<TermId>), Stop> {
    let program = round_program(u, rules, seeds)?;
    let closure = evaluate(u, &program, &mut |_: Trace| {});
    let mut diagnostics = Vec::with_capacity(closure.diagnostics.len());
    for d in &closure.diagnostics {
        let phase = d.phase;
        let payload = d.payload;
        diagnostics.push(diagnostic(u, phase, payload));
    }
    let rows = closure
        .rows
        .iter()
        .map(|row| {
            let args = u.list(&row.args);
            u.compound("call", vec![row.rel, args])
        })
        .collect();
    Ok((rows, diagnostics))
}

/// Checked-IR terms to the evaluator's `Program`.
pub fn round_program(
    u: &mut Universe,
    rules: &[TermId],
    seeds: &[TermId],
) -> Result<Program, Stop> {
    let mut program = Program::default();
    for rule in rules {
        program.rules.push(rule_from_term(u, *rule)?);
    }
    for seed in seeds {
        let Some(("call", args)) = u.functor(*seed).map(|(n, a)| (n, a.to_vec())) else {
            return Err(Stop::Fail("seed is not a call/2"));
        };
        let Some(items) = u.as_list(args[1]) else {
            return Err(Stop::Fail("seed arguments are not a list"));
        };
        program.seeds.push(Row {
            rel: args[0],
            args: items,
        });
    }
    Ok(program)
}

fn rule_from_term(u: &mut Universe, rule: TermId) -> Result<Rule, Stop> {
    let Some(("rule", parts)) = u.functor(rule).map(|(n, a)| (n, a.to_vec())) else {
        return Err(Stop::Fail("rule/2 expected"));
    };
    let mut vars: Vec<TermId> = Vec::new();
    let (rel, head) = call_from_term(u, parts[0], &mut vars)?;
    let Some(goals) = u.as_list(parts[1]) else {
        return Err(Stop::Fail("rule body is not a list"));
    };
    let mut body = Vec::with_capacity(goals.len());
    for goal in goals {
        let Some(("checked_goal", g)) = u.functor(goal).map(|(n, a)| (n, a.to_vec())) else {
            return Err(Stop::Fail("checked_goal/2 expected"));
        };
        let polarity = match u.functor_or_atom(g[0]).map(|(n, _)| n) {
            Some("positive") => Polarity::Positive,
            Some("negative") => Polarity::Negative,
            _ => return Err(Stop::Fail("goal polarity expected")),
        };
        let (grel, args) = call_from_term(u, g[1], &mut vars)?;
        body.push(Goal {
            polarity,
            rel: grel,
            args,
        });
    }
    Ok(Rule {
        rel,
        head,
        body,
        vars,
    })
}

fn call_from_term(
    u: &mut Universe,
    term: TermId,
    vars: &mut Vec<TermId>,
) -> Result<(TermId, Vec<Arg>), Stop> {
    let Some(("call", parts)) = u.functor(term).map(|(n, a)| (n, a.to_vec())) else {
        return Err(Stop::Fail("call/2 expected"));
    };
    let Some(items) = u.as_list(parts[1]) else {
        return Err(Stop::Fail("call arguments are not a list"));
    };
    let args = items.iter().map(|a| arg_from_term(u, *a, vars)).collect();
    Ok((parts[0], args))
}

fn arg_from_term(u: &Universe, term: TermId, vars: &mut Vec<TermId>) -> Arg {
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
    if let Some(("aggregate", args)) = u.functor(term) {
        if args.len() == 2 && u.functor_or_atom(args[0]).map(|(n, _)| n) == Some("count") {
            let inner = args[1];
            return Arg::Count(Box::new(arg_from_term(u, inner, vars)));
        }
    }
    Arg::Ground(term)
}
