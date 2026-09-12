//! `logical_program_rows/2`. Port of `0_logical_program_reifier.pl:15-31` and
//! `:190-281`.
//!
//! Occurrence ids are structural terms, never counters, so the same checked
//! program always reifies to the same row set: the checker has already fixed
//! relation, seed, rule and goal order (`:12-14`).

use super::api::Stop;
use crate::_3_check::api::prolog_sort;
use crate::_3_check::Checked;
use crate::_6_eval::term::{TermId, Universe};

/// `:15`. The `Checked` fields are the `checked_datalog/4` arguments.
#[tracing::instrument(skip_all)]
pub fn logical_program_rows(u: &mut Universe, checked: &Checked) -> Result<Vec<TermId>, Stop> {
    rows(
        u,
        &checked.relations,
        &checked.seeds,
        &checked.rules,
        &checked.depends,
        &checked.strata,
    )
}

/// The same body reached from a `checked_datalog/4` term.
pub fn logical_program_rows_term(u: &mut Universe, checked: TermId) -> Result<Vec<TermId>, Stop> {
    let parts = checked_parts(u, checked)?;
    rows(
        u,
        &parts.relations,
        &parts.seeds,
        &parts.rules,
        &parts.depends,
        &parts.strata,
    )
}

pub struct CheckedParts {
    pub relations: Vec<TermId>,
    pub seeds: Vec<TermId>,
    pub rules: Vec<TermId>,
    pub depends: Vec<TermId>,
    pub strata: Vec<TermId>,
}

/// `checked_datalog(_, datalog_program(Relations, Seeds, Rules), Depends,
/// Strata)` at `:16-17`.
pub fn checked_parts(u: &Universe, checked: TermId) -> Result<CheckedParts, Stop> {
    let Some(("checked_datalog", args)) = u.functor(checked) else {
        return Err(Stop::Fail("checked_datalog/4 expected"));
    };
    if args.len() != 4 {
        return Err(Stop::Fail("checked_datalog/4 expected"));
    }
    let (program, depends, strata) = (args[1], args[2], args[3]);
    let Some(("datalog_program", program_args)) = u.functor(program) else {
        return Err(Stop::Fail("datalog_program/3 expected"));
    };
    if program_args.len() != 3 {
        return Err(Stop::Fail("datalog_program/3 expected"));
    }
    let (Some(relations), Some(seeds), Some(rules), Some(depends), Some(strata)) = (
        u.as_list(program_args[0]),
        u.as_list(program_args[1]),
        u.as_list(program_args[2]),
        u.as_list(depends),
        u.as_list(strata),
    ) else {
        return Err(Stop::Fail("checked_datalog lists expected"));
    };
    Ok(CheckedParts {
        relations,
        seeds,
        rules,
        depends,
        strata,
    })
}

/// `:24-31`.
pub fn rows(
    u: &mut Universe,
    relations: &[TermId],
    seeds: &[TermId],
    rules: &[TermId],
    depends: &[TermId],
    strata: &[TermId],
) -> Result<Vec<TermId>, Stop> {
    let mut out = Vec::new();
    relation_rows(u, relations, &mut out)?;
    seed_rows(u, seeds, &mut out)?;
    rule_rows(u, rules, &mut out)?;
    dependency_rows(u, depends, &mut out)?;
    stratum_rows(u, strata, &mut out)?;
    Ok(prolog_sort(u, out))
}

/// `ref(Inner)`, or a stop naming the caller's clause head.
fn reference(u: &Universe, term: TermId, what: &'static str) -> Result<TermId, Stop> {
    u.unary(term, "ref").ok_or(Stop::Fail(what))
}

/// `:191-208`.
fn relation_rows(
    u: &mut Universe,
    relations: &[TermId],
    out: &mut Vec<TermId>,
) -> Result<(), Stop> {
    for row in relations {
        let Some(("relation", args)) = u.functor(*row) else {
            return Err(Stop::Fail("relation/3 expected"));
        };
        if args.len() != 3 {
            return Err(Stop::Fail("relation/3 expected"));
        }
        let (target, arity, key_sets) = (args[0], args[1], args[2]);
        let relation = reference(u, target, "relation(ref(_), _, _) expected")?;
        out.push(u.compound("program_relation", vec![relation, arity]));
        let Some(key_sets) = u.as_list(key_sets) else {
            return Err(Stop::Fail("relation key sets expected"));
        };
        for (ordinal, key_set) in key_sets.iter().enumerate() {
            let ordinal = u.int(ordinal as i64);
            out.push(u.compound("program_key", vec![relation, ordinal]));
            let Some(positions) = u.as_list(*key_set) else {
                return Err(Stop::Fail("key set positions expected"));
            };
            for position in positions {
                out.push(u.compound("program_key_position", vec![relation, ordinal, position]));
            }
        }
    }
    Ok(())
}

/// `:210-217`.
fn seed_rows(u: &mut Universe, seeds: &[TermId], out: &mut Vec<TermId>) -> Result<(), Stop> {
    for (index, seed) in seeds.iter().enumerate() {
        let index_term = u.int(index as i64);
        let seed_id = u.compound("seed_id", vec![index_term]);
        let seed_atom = u.atom("seed");
        let call_id = u.compound("call_id", vec![seed_atom, index_term]);
        out.push(u.compound("program_seed", vec![seed_id, call_id]));
        call_rows(u, call_id, *seed, out)?;
    }
    Ok(())
}

/// `:220-230`. `program_rule_kind` is the atom `level` for every rule (`:228`).
fn rule_rows(u: &mut Universe, rules: &[TermId], out: &mut Vec<TermId>) -> Result<(), Stop> {
    for (index, rule) in rules.iter().enumerate() {
        let Some(("rule", parts)) = u.functor(*rule) else {
            return Err(Stop::Fail("rule/2 expected"));
        };
        if parts.len() != 2 {
            return Err(Stop::Fail("rule/2 expected"));
        }
        let (head, goals) = (parts[0], parts[1]);
        let index_term = u.int(index as i64);
        let rule_id = u.compound("rule_id", vec![index_term]);
        let rule_index = u.compound("rule", vec![index_term]);
        let head_atom = u.atom("head");
        let head_call_id = u.compound("call_id", vec![rule_index, head_atom]);
        out.push(u.compound("program_rule", vec![rule_id, head_call_id]));
        let level = u.atom("level");
        out.push(u.compound("program_rule_kind", vec![rule_id, level]));
        call_rows(u, head_call_id, head, out)?;
        let Some(goals) = u.as_list(goals) else {
            return Err(Stop::Fail("rule goals expected"));
        };
        goal_rows(u, &goals, rule_id, rule_index, out)?;
    }
    Ok(())
}

/// `:233-240`.
fn goal_rows(
    u: &mut Universe,
    goals: &[TermId],
    rule_id: TermId,
    rule_index: TermId,
    out: &mut Vec<TermId>,
) -> Result<(), Stop> {
    for (position, goal) in goals.iter().enumerate() {
        let Some(("checked_goal", parts)) = u.functor(*goal) else {
            return Err(Stop::Fail("checked_goal/2 expected"));
        };
        if parts.len() != 2 {
            return Err(Stop::Fail("checked_goal/2 expected"));
        }
        let (polarity, call) = (parts[0], parts[1]);
        let position_term = u.int(position as i64);
        let goal_index = u.compound("goal", vec![position_term]);
        let call_id = u.compound("call_id", vec![rule_index, goal_index]);
        out.push(u.compound(
            "program_goal",
            vec![rule_id, position_term, polarity, call_id],
        ));
        call_rows(u, call_id, call, out)?;
    }
    Ok(())
}

/// `:242-244`.
fn call_rows(
    u: &mut Universe,
    call_id: TermId,
    call: TermId,
    out: &mut Vec<TermId>,
) -> Result<(), Stop> {
    let Some(("call", parts)) = u.functor(call) else {
        return Err(Stop::Fail("call/2 expected"));
    };
    if parts.len() != 2 {
        return Err(Stop::Fail("call/2 expected"));
    }
    let (target, arguments) = (parts[0], parts[1]);
    let relation = reference(u, target, "call(ref(_), _) expected")?;
    out.push(u.compound("program_apply", vec![call_id, relation]));
    let Some(arguments) = u.as_list(arguments) else {
        return Err(Stop::Fail("call arguments expected"));
    };
    argument_rows(u, &arguments, call_id, out)
}

/// `:247-253`.
fn argument_rows(
    u: &mut Universe,
    arguments: &[TermId],
    call_id: TermId,
    out: &mut Vec<TermId>,
) -> Result<(), Stop> {
    for (position, argument) in arguments.iter().enumerate() {
        let position_term = u.int(position as i64);
        let argument_id = u.compound("argument_id", vec![call_id, position_term]);
        out.push(u.compound(
            "program_argument",
            vec![call_id, position_term, argument_id],
        ));
        argument_value_rows(u, *argument, argument_id, out)?;
    }
    Ok(())
}

/// `:255-271`. An aggregate input receives its own node identity and recurses.
fn argument_value_rows(
    u: &mut Universe,
    argument: TermId,
    argument_id: TermId,
    out: &mut Vec<TermId>,
) -> Result<(), Stop> {
    let zero = u.int(0);
    let Some((name, args)) = u
        .functor(argument)
        .map(|(n, a)| (n.to_string(), a.to_vec()))
    else {
        return Err(Stop::Fail("argument value expected"));
    };
    match (name.as_str(), args.len()) {
        ("var", 1) => {
            let variable = u.compound("const", vec![args[0]]);
            let label = u.atom("variable");
            out.push(u.compound("program_edge", vec![argument_id, label, variable, zero]));
        }
        ("ref", 1) => {
            let label = u.atom("reference");
            out.push(u.compound("program_edge", vec![argument_id, label, argument, zero]));
        }
        ("const", 1) => {
            let label = u.atom("literal");
            out.push(u.compound("program_edge", vec![argument_id, label, argument, zero]));
        }
        ("aggregate", 2) => {
            let (operator, input) = (args[0], args[1]);
            let operator = u.compound("const", vec![operator]);
            let label = u.atom("aggregate");
            out.push(u.compound("program_edge", vec![argument_id, label, operator, zero]));
            let input_atom = u.atom("input");
            let child = u.compound("argument_child", vec![argument_id, input_atom]);
            let target = u.compound("ref", vec![child]);
            let one = u.int(1);
            out.push(u.compound("program_edge", vec![argument_id, input_atom, target, one]));
            argument_value_rows(u, input, child, out)?;
        }
        _ => return Err(Stop::Fail("argument value expected")),
    }
    Ok(())
}

/// `:274-276`.
fn dependency_rows(
    u: &mut Universe,
    depends: &[TermId],
    out: &mut Vec<TermId>,
) -> Result<(), Stop> {
    for row in depends {
        let Some(("depends", args)) = u.functor(*row) else {
            return Err(Stop::Fail("depends/3 expected"));
        };
        if args.len() != 3 {
            return Err(Stop::Fail("depends/3 expected"));
        }
        let (head, body, polarity) = (args[0], args[1], args[2]);
        let head = reference(u, head, "depends(ref(_), _, _) expected")?;
        let body = reference(u, body, "depends(_, ref(_), _) expected")?;
        out.push(u.compound("program_dependency", vec![head, body, polarity]));
    }
    Ok(())
}

/// `:279-281`.
fn stratum_rows(u: &mut Universe, strata: &[TermId], out: &mut Vec<TermId>) -> Result<(), Stop> {
    for row in strata {
        let Some(("stratum", args)) = u.functor(*row) else {
            return Err(Stop::Fail("stratum/2 expected"));
        };
        if args.len() != 2 {
            return Err(Stop::Fail("stratum/2 expected"));
        }
        let (target, level) = (args[0], args[1]);
        let relation = reference(u, target, "stratum(ref(_), _) expected")?;
        out.push(u.compound("program_stratum", vec![relation, level]));
    }
    Ok(())
}
