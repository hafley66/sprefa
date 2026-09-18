//! `evaluate_compiler_rounds/11`, the inner fixpoint. Port of
//! `2_compiler.pl:1173-1206`, `:1384-1467` and `:1505-1517`.

use super::api::{
    colon_call_parts, colon_rows, diagnostic, intern_rows, is_kernel_ref, strip_intern_rows, strip_snapshot_rows,
    Generated,
};
use super::assemble::{assemble_generated_program, Assembled};
use super::finish::{derived_bind_diagnostics, validate_functional_rows};
use crate::_3_check::api::prolog_sort;
use crate::_3_check::resolved::Resolved;
use crate::_3_check::{check_resolved_rules, Stop};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::program::{
    AggregateKind, Arg, Fold, Goal, Order, Polarity, Program, Row, Rule, Seed, VarId,
};
use crate::_6_eval::term::{Term, TermId, Universe};
use crate::_6_eval::{evaluate, Trace};
use crate::_7_effect::Slice;
use crate::_9_runtime::executors::once_executors;
use crate::_9_runtime::Answerer;
use std::collections::HashMap;
use std::marker::PhantomData;

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
    /// `effects` applications went to Once executors; `rows` of their answers
    /// were new.
    Answer {
        outer: i64,
        round: i64,
        effects: usize,
        rows: usize,
    },
}

/// The frozen lists plus the counter; one struct, reduced per round.
pub struct RoundState {
    pub frozen_edges: Vec<TermId>,
    pub frozen_requests: Vec<TermId>,
    pub frozen_generated_relations: Vec<TermId>,
    pub frozen_generated_rules: Vec<TermId>,
    /// Executor answers as `call/2` rows, re-seeded every round.
    pub frozen_answers: Vec<TermId>,
    /// Built on the first round from the module binds in the base seeds;
    /// `None` when no declared relation has a Once executor.
    pub answerer: Option<Answerer>,
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

/// The read-only side of one inner fixpoint; `outer` is the source-refreeze
/// pass, carried for the trace only.
pub struct RoundInput<'a> {
    pub u: &'a mut Universe,
    pub authored_rules: &'a [TermId],
    pub base_relations: &'a [TermId],
    pub base_seeds: &'a [TermId],
    pub outer: i64,
}

impl RoundState {
    /// `:1173`. The first round of a fixpoint, seeded from frozen rows.
    pub fn seeded(
        frozen_edges: Vec<TermId>,
        frozen_requests: Vec<TermId>,
        frozen_generated_relations: Vec<TermId>,
        frozen_generated_rules: Vec<TermId>,
    ) -> Self {
        RoundState {
            frozen_edges,
            frozen_requests,
            frozen_generated_relations,
            frozen_generated_rules,
            frozen_answers: Vec::new(),
            answerer: None,
            round: 1,
        }
    }
}

/// One base list and the frozen generated tail, sorted as one set.
pub fn merged_sorted(u: &mut Universe, base: &[TermId], generated: &[TermId]) -> Vec<TermId> {
    let mut all = base.to_vec();
    all.extend_from_slice(generated);
    prolog_sort(u, all)
}

/// `:1376`. The decision reaches the sink and the log together.
pub fn decide(fx: &mut dyn FnMut(Round), outer: i64, round: i64, outcome: Outcome) {
    fx(Round::Decision {
        outer,
        round,
        outcome,
    });
    tracing::debug!(target: "dl8::comptime", outer, round, outcome = ?outcome);
}

pub fn round_limit_diagnostic(u: &mut Universe) -> TermId {
    let limit = u.int(COMPILER_ROUND_LIMIT);
    let reason = u.compound("compiler_round_limit_exhausted", vec![limit]);
    diagnostic(u, "compile", reason)
}

/// `:1384`. The frozen lists stopped moving, so this closure is the answer
/// unless the derived-bind or functional-key check rejects it.
pub fn stable_outcome(
    u: &mut Universe,
    authored_rules: &[TermId],
    base_relations: &[TermId],
    closure: &[TermId],
    assembled: Assembled,
    resolved: Resolved,
) -> RoundOutcome {
    let all = merged_sorted(u, base_relations, &assembled.relations);
    let mut stable = derived_bind_diagnostics(u, authored_rules, closure);
    stable.extend(validate_functional_rows(u, &all, closure));
    let stable = prolog_sort(u, stable);
    if !stable.is_empty() {
        return RoundOutcome::failed(stable);
    }
    RoundOutcome {
        closure: strip_intern_rows(u, closure),
        generated: Generated {
            relations: assembled.relations,
            rules: assembled.rules,
            depends: resolved.depends,
            strata: resolved.strata,
        },
        diagnostics: vec![],
    }
}

/// One round's merged program, resolved and evaluated.
pub struct Pass {
    pub resolved: Resolved,
    pub closure: Vec<TermId>,
    pub rules: usize,
    pub seeds: usize,
}

/// `:1203`. `Err` carries the outcome that ends the fixpoint before anything
/// is assembled.
pub fn round_closure(
    u: &mut Universe,
    st: &RoundState,
    authored_rules: &[TermId],
    base_relations: &[TermId],
    base_seeds: &[TermId],
) -> Result<Result<Pass, RoundOutcome>, Stop> {
    let relations = merged_sorted(u, base_relations, &st.frozen_generated_relations);
    let rules = merged_sorted(u, authored_rules, &st.frozen_generated_rules);
    let resolved = check_resolved_rules(u, &relations, &rules)?;
    let seeds = compiler_round_seeds(
        u,
        base_seeds,
        &st.frozen_edges,
        &st.frozen_requests,
        &st.frozen_answers,
    );
    if !resolved.diagnostics.is_empty() {
        return Ok(Err(RoundOutcome::failed(resolved.diagnostics)));
    }
    let served: Vec<TermId> = st
        .answerer
        .as_ref()
        .map(|answerer| answerer.relations().collect())
        .unwrap_or_default();
    let (closure, evaluation_diagnostics) = evaluate_round(u, &rules, &seeds, &served)?;
    if !evaluation_diagnostics.is_empty() {
        return Ok(Err(RoundOutcome::failed(evaluation_diagnostics)));
    }
    Ok(Ok(Pass {
        resolved,
        closure: strip_snapshot_rows(u, &closure),
        rules: rules.len(),
        seeds: seeds.len(),
    }))
}

/// `:1173`. The inner compiler fixpoint as a reducer over its frozen lists.
pub struct Rounds<'a>(PhantomData<&'a ()>);

impl<'a> Slice for Rounds<'a> {
    type State = RoundState;
    type Event = RoundInput<'a>;
    type Output = Result<RoundOutcome, Stop>;
    type Effect = Round;

    #[tracing::instrument(skip_all)]
    fn reduce(
        st: &mut RoundState,
        ev: RoundInput<'a>,
        fx: &mut dyn FnMut(Round),
    ) -> Result<RoundOutcome, Stop> {
        let RoundInput {
            u,
            authored_rules,
            base_relations,
            base_seeds,
            outer,
        } = ev;
        if st.answerer.is_none() {
            st.answerer = comptime_answerer(u, base_seeds);
        }
        loop {
            let pass = match round_closure(u, st, authored_rules, base_relations, base_seeds)? {
                Ok(pass) => pass,
                Err(outcome) => return Ok(outcome),
            };
            let (resolved, closure) = (pass.resolved, pass.closure);
            fx(Round::Evaluate {
                outer,
                round: st.round,
                rules: pass.rules,
                seeds: pass.seeds,
                closure: closure.len(),
            });
            let answered = answer_round(u, st, &closure);
            if let Some((effects, rows)) = answered {
                fx(Round::Answer {
                    outer,
                    round: st.round,
                    effects,
                    rows,
                });
            }
            let answered = answered.map_or(0, |(_, rows)| rows);

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

            if answered == 0
                && next_edges == st.frozen_edges
                && next_requests == st.frozen_requests
                && assembled.relations == st.frozen_generated_relations
                && assembled.rules == st.frozen_generated_rules
            {
                decide(fx, outer, st.round, Outcome::Stable);
                let stable = stable_outcome(
                    u,
                    authored_rules,
                    base_relations,
                    &closure,
                    assembled,
                    resolved,
                );
                return Ok(stable);
            }
            if st.round >= COMPILER_ROUND_LIMIT {
                decide(fx, outer, st.round, Outcome::LimitExhausted);
                let d = round_limit_diagnostic(u);
                return Ok(RoundOutcome::failed(vec![d]));
            }
            decide(fx, outer, st.round, Outcome::Continue);
            st.frozen_edges = next_edges;
            st.frozen_requests = next_requests;
            st.frozen_generated_relations = assembled.relations;
            st.frozen_generated_rules = assembled.rules;
            st.round += 1;
        }
    }
}

/// The Once executors of every relation the program binds at module level.
fn comptime_answerer(u: &mut Universe, base_seeds: &[TermId]) -> Option<Answerer> {
    let names = module_names(u, base_seeds);
    let executors = once_executors(u, &names);
    if executors.is_empty() {
        return None;
    }
    Answerer::new(u, &names, executors).ok()
}

/// The `:(module(M), Name, ref(Relation), _)` binds among the graph seeds, the
/// in-memory twin of `program.names` (`_6_eval/_6_json.rs` `names_to_json`).
/// A `@std/<space>` member carries its namespace; a file bind shadows the
/// prelude's.
fn module_names(u: &mut Universe, base_seeds: &[TermId]) -> HashMap<String, TermId> {
    let mut out: HashMap<String, (bool, TermId)> = HashMap::new();
    for seed in base_seeds {
        let Some([owner, name, relation, _]) = colon_call_parts(u, *seed) else {
            continue;
        };
        let Some(module) = u.unary(owner, "ref").and_then(|o| u.unary(o, "module")) else {
            continue;
        };
        match u.unary(relation, "ref") {
            Some(target) if u.unary(target, "module").is_none() => {}
            _ => continue,
        }
        let Some(Term::Atom(s)) = u.unary(name, "const").map(|n| u.get(n)) else {
            continue;
        };
        let name = u.sym_str(*s).to_string();
        let name = match u.unary(module, "std").map(|space| u.get(space)) {
            Some(Term::Atom(space)) => format!("{}.{name}", u.sym_str(*space)),
            _ => name,
        };
        let prelude = matches!(u.get(module), Term::Atom(m) if u.sym_str(*m) == "prelude");
        match out.get(&name) {
            Some((true, _)) if !prelude => {
                out.insert(name, (prelude, relation));
            }
            Some(_) => {}
            None => {
                out.insert(name, (prelude, relation));
            }
        }
    }
    out.into_iter().map(|(name, (_, rel))| (name, rel)).collect()
}

/// Hands this round's new `effect` rows to the Once executors and freezes the
/// answers not seen before. `None` when nothing was asked.
fn answer_round(
    u: &mut Universe,
    st: &mut RoundState,
    closure: &[TermId],
) -> Option<(usize, usize)> {
    let answerer = st.answerer.as_mut()?;
    let mut store = Store::default();
    for row in closure {
        let Some([rel, args]) = u.args::<2>(*row, "call") else {
            continue;
        };
        if let Some(items) = u.as_list(args) {
            store.insert(rel, items.into_boxed_slice());
        }
    }
    let (answers, effects) = answerer.answer(u, &store);
    if effects == 0 {
        return None;
    }
    let mut rows = 0;
    for answer in answers {
        let args = u.list(&answer.args);
        let row = u.compound("call", vec![answer.rel, args]);
        if !st.frozen_answers.contains(&row) {
            st.frozen_answers.push(row);
            rows += 1;
        }
    }
    Some((effects, rows))
}

/// `:1505`. Last round's edges, intern requests and executor answers re-enter
/// as read-only rows.
pub fn compiler_round_seeds(
    u: &mut Universe,
    base_seeds: &[TermId],
    frozen_edges: &[TermId],
    frozen_requests: &[TermId],
    frozen_answers: &[TermId],
) -> Vec<TermId> {
    let mut out = base_seeds.to_vec();
    out.extend_from_slice(frozen_answers);
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
    served: &[TermId],
) -> Result<(Vec<TermId>, Vec<TermId>), Stop> {
    let mut program = round_program(u, rules, seeds)?;
    program.served.extend(served.iter().copied());
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
        let Some(args) = u.args::<2>(*seed, "call") else {
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
    let Some(parts) = u.args::<2>(rule, "rule") else {
        return Err(Stop::Fail("rule/2 expected"));
    };
    let mut vars: Vec<TermId> = Vec::new();
    let (rel, head) = call_from_term(u, parts[0], &mut vars)?;
    let Some(goals) = u.as_list(parts[1]) else {
        return Err(Stop::Fail("rule body is not a list"));
    };
    let mut body = Vec::with_capacity(goals.len());
    for goal in goals {
        let Some(g) = u.args::<2>(goal, "checked_goal") else {
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
    let Some(parts) = u.args::<2>(term, "call") else {
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
        if args.len() == 2 {
            if let Some(aggregation) = u
                .functor_or_atom(args[0])
                .and_then(|(name, _)| AggregateKind::of(name))
            {
                return Arg::Aggregate(aggregation, Box::new(arg_from_term(u, args[1], vars)));
            }
        }
    }
    if let Some(("fold", args)) = u.functor(term) {
        if args.len() == 3 {
            let fold = Fold {
                step: args[0],
                seed: Seed::Term(args[1]),
                order: Order::TermLt,
            };
            let subject = arg_from_term(u, args[2], vars);
            return Arg::Fold(fold, Box::new(subject));
        }
    }
    Arg::Ground(term)
}
