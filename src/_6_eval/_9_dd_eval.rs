//! Differential-dataflow evaluation over one worker and one dataflow.

use super::evaluate::{Closure, Trace};
use super::program::{Arg, Diagnostic, Folding, Polarity, Program, Row, Rule};
use super::stratify::{stratify, Strata};
use super::term::{TermId, Universe};
use differential_dataflow::input::Input;
use differential_dataflow::lattice::Lattice;
use differential_dataflow::operators::Iterate;
use differential_dataflow::VecCollection;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use timely::dataflow::operators::probe::Handle as ProbeHandle;
use timely::progress::Timestamp;

type DdRow = Vec<u32>;
type Weight = isize;
const UNBOUND: u32 = u32::MAX;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
enum DeclaredFoldInput {
    Subject(u32),
    Stored(DdRow),
}

#[derive(Clone, Copy)]
struct ServedRows {
    intern: TermId,
    effect: TermId,
    snapshot: TermId,
    none: TermId,
}

/// Evaluates positive products and recursive strata as DD collections.
pub fn evaluate_dd(u: &mut Universe, program: &Program, fx: &mut dyn FnMut(Trace)) -> Closure {
    let (strata, diagnostics) = stratify(u, program);
    if !diagnostics.is_empty() {
        return Closure {
            rows: Vec::new(),
            diagnostics,
        };
    }
    let ruled: HashSet<TermId> = program.rules.iter().map(|rule| rule.rel).collect();
    let served: Arc<HashSet<TermId>> = Arc::new(
        program
            .served
            .iter()
            .filter(|rel| !ruled.contains(rel))
            .copied()
            .collect(),
    );
    let fold_program = Arc::new(program.clone());
    let (program, hidden) = super::sqlite_eval::with_demand(u, program);
    let program = &program;

    let mut aggregate_diagnostics = Vec::new();
    for rule in program.rules.iter().filter(|rule| rule.is_aggregate()) {
        let count = rule.aggregate_args();
        if count != 1 {
            let count = u.int(count as i64);
            aggregate_diagnostics.push(Diagnostic {
                phase: "evaluate",
                payload: u.compound("malformed_aggregate_head", vec![count]),
            });
        }
    }
    if !aggregate_diagnostics.is_empty() {
        aggregate_diagnostics.sort_by(|left, right| u.cmp(left.payload, right.payload));
        aggregate_diagnostics.dedup();
        return Closure {
            rows: Vec::new(),
            diagnostics: aggregate_diagnostics,
        };
    }

    let nil_rel = kernel_ref(u, "nil");
    let intern_rel = kernel_ref(u, "intern");
    let effect_rel = kernel_ref(u, "effect");
    let snapshot_rel = kernel_ref(u, "intern_snapshot");
    let none = {
        let none = u.atom("none");
        u.compound("const", vec![none])
    };
    let empty = u.empty_list();
    let nil_arg = u.compound("const", vec![empty]);
    let nil = tagged(nil_rel, &[nil_arg]);
    let mut rules_by_level = schedule_rules(program, &strata, &hidden);
    for level in 0..=strata.max_level {
        let rules = rules_by_level.get(&level).map_or(0, Vec::len);
        let seeds = program
            .seeds
            .iter()
            .filter(|row| strata.level(row.rel) == level)
            .count();
        fx(Trace::Stratum {
            level,
            rules,
            seeds,
        });
    }

    let arena = Arc::new(Mutex::new(std::mem::take(u)));
    let captured: Arc<Mutex<Vec<(DdRow, Weight)>>> = Arc::default();
    let aggregate_diagnostics: Arc<Mutex<Vec<Diagnostic>>> = Arc::default();
    let program = program.clone();
    let arena_worker = Arc::clone(&arena);
    let captured_worker = Arc::clone(&captured);
    let diagnostics_worker = Arc::clone(&aggregate_diagnostics);
    timely::execute_directly(move |worker| {
        let probe = ProbeHandle::new();
        let (mut input, ()) = worker.dataflow::<u64, _, _>(|scope| {
            let (input, seeds) = scope.new_collection::<DdRow, Weight>();
            let mut known = seeds.clone().filter(move |row| row[0] == nil_rel.0);
            let levels: Vec<(u32, Vec<u32>, Vec<Rule>)> = (0..=strata.max_level)
                .map(|level| {
                    let rels = program
                        .seeds
                        .iter()
                        .filter(|row| strata.level(row.rel) == level)
                        .map(|row| row.rel.0)
                        .collect();
                    let rules = rules_by_level.remove(&level).unwrap_or_default();
                    (level, rels, rules)
                })
                .collect();
            for (_, rels, rules) in levels {
                let at_level = seeds.clone().filter(move |row| rels.contains(&row[0]));
                let (aggregate, plain): (Vec<Rule>, Vec<Rule>) =
                    rules.into_iter().partition(Rule::is_aggregate);
                let mut aggregate_rows = known.clone().filter(|_| false);
                for rule in &aggregate {
                    let rows = lower_aggregate_rule(
                        &known,
                        rule,
                        Arc::clone(&arena_worker),
                        Arc::clone(&diagnostics_worker),
                        Arc::clone(&fold_program),
                    );
                    aggregate_rows = aggregate_rows.concat(rows);
                }
                let base = known.concat(at_level).concat(aggregate_rows).distinct();
                if plain.is_empty() {
                    known = base;
                    continue;
                }
                let arena = Arc::clone(&arena_worker);
                let served = Arc::clone(&served);
                known = base.iterate(move |_scope, current| {
                    let mut derived = current.clone().filter(|_| false);
                    for rule in &plain {
                        let rows = lower_rule(
                            &current,
                            rule,
                            Arc::clone(&arena),
                            Arc::clone(&served),
                            ServedRows {
                                intern: intern_rel,
                                effect: effect_rel,
                                snapshot: snapshot_rel,
                                none,
                            },
                        );
                        derived = derived.concat(rows);
                    }
                    current.concat(derived).distinct()
                });
            }
            let sink = Arc::clone(&captured_worker);
            known
                .inspect(move |(row, _, diff)| sink.lock().unwrap().push((row.clone(), *diff)))
                .probe_with(&probe);
            (input, ())
        });
        input.insert(nil);
        for row in &program.seeds {
            input.insert(tagged(row.rel, &row.args));
        }
        input.advance_to(1);
        input.flush();
        worker.step_while(|| probe.less_than(&1));
    });

    let mut rows: Vec<Row> = captured
        .lock()
        .unwrap()
        .drain(..)
        .filter(|(_, diff)| *diff > 0)
        .map(|(row, _)| Row {
            rel: TermId(row[0]),
            args: row[1..].iter().copied().map(TermId).collect(),
        })
        .collect();
    rows.retain(|row| !hidden.contains(&row.rel));
    let mut diagnostics = aggregate_diagnostics.lock().unwrap().clone();
    let mutex = match Arc::try_unwrap(arena) {
        Ok(mutex) => mutex,
        Err(_) => panic!("DD arena still has an operator owner"),
    };
    let mut arena = mutex.into_inner().unwrap();
    if !diagnostics.is_empty() {
        diagnostics.sort_by(|left, right| arena.cmp(left.payload, right.payload));
        diagnostics.dedup();
        *u = std::mem::take(&mut arena);
        return Closure {
            rows: Vec::new(),
            diagnostics,
        };
    }
    rows.sort_by(|left, right| {
        arena
            .cmp(left.rel, right.rel)
            .then_with(|| arena.cmp_rows(&left.args, &right.args))
    });
    rows.dedup();
    fx(Trace::Closure { rows: rows.len() });
    *u = std::mem::take(&mut arena);
    Closure {
        rows,
        diagnostics: Vec::new(),
    }
}

fn lower_aggregate_rule<'scope, T>(
    all: &VecCollection<'scope, T, DdRow, Weight>,
    rule: &Rule,
    arena: Arc<Mutex<Universe>>,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    program: Arc<Program>,
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord + Hash,
{
    let vars = rule.vars.len();
    let mut bound = vec![false; vars];
    // The nil bootstrap makes this one empty environment without a separate DD input.
    let mut envs = all.clone().map(move |_| vec![UNBOUND; vars]).distinct();
    for goal in &rule.body {
        let kernel = super::kernel::Kernel::of(&arena.lock().unwrap(), goal.rel);
        if goal.polarity == Polarity::Negative {
            envs = match kernel {
                Some(kernel) => negative_kernel(envs, &goal.args, kernel, Arc::clone(&arena)),
                None => {
                    let right = relation(all, goal.rel, goal.args.len());
                    anti_goal(envs, right, &goal.args)
                }
            };
            continue;
        }
        let right = relation(all, goal.rel, goal.args.len());
        envs = join_goal(envs, right, &goal.args, &bound);
        for arg in &goal.args {
            if let Arg::Var(var) = arg {
                bound[var.0 as usize] = true;
            }
        }
    }

    let (position, folding, _) = rule.folding_head().unwrap();
    let head = rule.head.clone();
    let arena_for_proofs = Arc::clone(&arena);
    let diagnostics_for_proofs = Arc::clone(&diagnostics);
    let proofs = envs.flat_map(move |env| {
        let row: Option<Vec<u32>> = head.iter().map(|arg| value(arg, &env)).collect();
        match row {
            Some(mut row) => {
                let subject = row.remove(position);
                Some((row, subject))
            }
            None => {
                let payload = arena_for_proofs
                    .lock()
                    .unwrap()
                    .atom("non_ground_aggregate_proof");
                diagnostics_for_proofs.lock().unwrap().push(Diagnostic {
                    phase: "evaluate",
                    payload,
                });
                None
            }
        }
    });
    let reduce = move |input: &[(&u32, Weight)]| {
        let mut values = Vec::new();
        for (value, weight) in input {
            for _ in 0..(*weight).max(0) as usize {
                values.push(TermId(**value));
            }
        }
        values
    };
    let folded = match folding {
        Folding::Builtin(kind) => {
            let arena_for_reduce = Arc::clone(&arena);
            let diagnostics = Arc::clone(&diagnostics);
            proofs.reduce(move |_key, input, output| {
                let values = reduce(input);
                if values.is_empty() {
                    return;
                }
                match super::evaluate::fold_group(
                    &mut arena_for_reduce.lock().unwrap(),
                    kind,
                    &values,
                ) {
                    Ok(term) => output.push((term.0, 1)),
                    Err(diagnostic) => diagnostics.lock().unwrap().push(diagnostic),
                }
            })
        }
        Folding::Declared(fold) => {
            let arena_for_reduce = Arc::clone(&arena);
            let groups = proofs.clone().map(|(key, _)| ((), key)).distinct();
            let stored = groups
                .join(all.clone().distinct().map(|row| ((), row)))
                .map(|(_, (key, row))| (key, DeclaredFoldInput::Stored(row)));
            let inputs = proofs
                .map(|(key, value)| (key, DeclaredFoldInput::Subject(value)))
                .concat(stored);
            let diagnostics = Arc::clone(&diagnostics);
            inputs.reduce(move |_key, input, output| {
                let mut values = Vec::new();
                let mut rows = Vec::new();
                for (item, weight) in input {
                    for _ in 0..(*weight).max(0) as usize {
                        match item {
                            DeclaredFoldInput::Subject(value) => values.push(TermId(*value)),
                            DeclaredFoldInput::Stored(row) => rows.push(Row {
                                rel: TermId(row[0]),
                                args: row[1..].iter().copied().map(TermId).collect(),
                            }),
                        }
                    }
                }
                if values.is_empty() {
                    return;
                }
                match super::evaluate::fold_generic_rows(
                    &mut arena_for_reduce.lock().unwrap(),
                    &program,
                    &rows,
                    &fold,
                    &values,
                ) {
                    Ok(term) => output.push((term.0, 1)),
                    Err(diagnostic) => diagnostics.lock().unwrap().push(diagnostic),
                }
            })
        }
    };
    let rel = rule.rel.0;
    folded.map(move |(mut key, result)| {
        key.insert(position, result);
        key.insert(0, rel);
        key
    })
}

fn lower_rule<'scope, T>(
    all: &VecCollection<'scope, T, DdRow, Weight>,
    rule: &Rule,
    arena: Arc<Mutex<Universe>>,
    served: Arc<HashSet<TermId>>,
    served_rows: ServedRows,
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord + Hash,
{
    let vars = rule.vars.len();
    let mut bound = vec![false; vars];
    // The nil bootstrap makes this one empty environment without a separate DD input.
    let mut envs = all.clone().map(move |_| vec![UNBOUND; vars]).distinct();
    let mut side_rows = all.clone().filter(|_| false);
    for goal in &rule.body {
        let kernel = super::kernel::Kernel::of(&arena.lock().unwrap(), goal.rel);
        if goal.polarity == Polarity::Negative {
            envs = match kernel {
                Some(kernel) => negative_kernel(envs, &goal.args, kernel, Arc::clone(&arena)),
                None => {
                    let right = relation(all, goal.rel, goal.args.len());
                    anti_goal(envs, right, &goal.args)
                }
            };
            continue;
        }
        if served.contains(&goal.rel) {
            let args = goal.args.clone();
            let rel = goal.rel;
            let arena_for_effect = Arc::clone(&arena);
            side_rows = side_rows.concat(envs.clone().flat_map(move |env| {
                let pattern: Vec<Option<TermId>> = args
                    .iter()
                    .map(|arg| value(arg, &env).map(TermId))
                    .collect();
                let Some(rows) = super::evaluate::effect_rows(
                    &mut arena_for_effect.lock().unwrap(),
                    served_rows.none,
                    rel,
                    &pattern,
                ) else {
                    return Vec::new();
                };
                vec![
                    tagged(served_rows.intern, &rows.intern),
                    tagged(served_rows.effect, &rows.effect),
                    tagged(served_rows.snapshot, &rows.snapshot),
                ]
            }));
        }
        envs = match kernel {
            Some(kernel) => {
                let stored = join_goal(
                    envs.clone(),
                    relation(all, goal.rel, goal.args.len()),
                    &goal.args,
                    &bound,
                );
                let solved = positive_kernel(envs, &goal.args, kernel, Arc::clone(&arena));
                if kernel == super::kernel::Kernel::Intern {
                    let rel = goal.rel.0;
                    side_rows = side_rows.concat(solved.clone().map(move |(_, mut row)| {
                        row.insert(0, rel);
                        row
                    }));
                }
                solved.map(|(env, _)| env).concat(stored).distinct()
            }
            None => {
                let right = relation(all, goal.rel, goal.args.len());
                join_goal(envs, right, &goal.args, &bound)
            }
        };
        for arg in &goal.args {
            if let Arg::Var(var) = arg {
                bound[var.0 as usize] = true;
            }
        }
    }
    let rel = rule.rel.0;
    let head = rule.head.clone();
    let heads = envs.flat_map(move |env| {
        let mut row = Vec::with_capacity(head.len() + 1);
        row.push(rel);
        for arg in &head {
            let value = value(arg, &env)?;
            row.push(value);
        }
        Some(row)
    });
    heads.concat(side_rows)
}

fn positive_kernel<'scope, T>(
    envs: VecCollection<'scope, T, DdRow, Weight>,
    args: &[Arg],
    kernel: super::kernel::Kernel,
    arena: Arc<Mutex<Universe>>,
) -> VecCollection<'scope, T, (DdRow, DdRow), Weight>
where
    T: Timestamp + Lattice + Ord,
{
    let args = args.to_vec();
    envs.flat_map(move |env| {
        let pattern: Vec<Option<TermId>> = args
            .iter()
            .map(|arg| value(arg, &env).map(TermId))
            .collect();
        let solutions = super::kernel::solve(&mut arena.lock().unwrap(), kernel, &pattern);
        solutions.into_iter().filter_map({
            let args = args.clone();
            let env = env.clone();
            move |row| {
                let row_u32: Vec<u32> = row.iter().map(|term| term.0).collect();
                let mut next = env.clone();
                unify(&args, &row_u32, &mut next).then_some((next, row_u32))
            }
        })
    })
}

fn anti_goal<'scope, T>(
    envs: VecCollection<'scope, T, DdRow, Weight>,
    rows: VecCollection<'scope, T, DdRow, Weight>,
    args: &[Arg],
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord,
{
    let args = args.to_vec();
    envs.flat_map(move |env| {
        let key: Option<Vec<u32>> = args.iter().map(|arg| value(arg, &env)).collect();
        key.map(|key| (key, env))
    })
    .antijoin(rows.distinct())
    .map(|(_, env)| env)
}

fn negative_kernel<'scope, T>(
    envs: VecCollection<'scope, T, DdRow, Weight>,
    args: &[Arg],
    kernel: super::kernel::Kernel,
    arena: Arc<Mutex<Universe>>,
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord,
{
    let args = args.to_vec();
    envs.filter(move |env| {
        let values: Vec<Option<TermId>> =
            args.iter().map(|arg| value(arg, env).map(TermId)).collect();
        values.iter().all(Option::is_some)
            && super::kernel::negative_holds(&arena.lock().unwrap(), kernel, &values)
    })
}

fn relation<'scope, T>(
    all: &VecCollection<'scope, T, DdRow, Weight>,
    rel: TermId,
    arity: usize,
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord,
{
    // Each goal scans the tagged known collection; a later arrangement pass can share this work.
    all.clone().flat_map(move |row| {
        (row.first() == Some(&rel.0) && row.len() == arity + 1).then(|| row[1..].to_vec())
    })
}

fn join_goal<'scope, T>(
    envs: VecCollection<'scope, T, DdRow, Weight>,
    rows: VecCollection<'scope, T, DdRow, Weight>,
    args: &[Arg],
    bound: &[bool],
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord,
{
    let keyed: Vec<(usize, Arg)> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| match arg {
            Arg::Ground(_) => true,
            Arg::Var(var) => bound[var.0 as usize],
            Arg::Aggregate(_, subject) | Arg::Fold(_, subject) => match subject.as_ref() {
                Arg::Ground(_) => true,
                Arg::Var(var) => bound[var.0 as usize],
                _ => false,
            },
        })
        .map(|(position, arg)| (position, arg.clone()))
        .collect();
    let left_keyed = keyed.clone();
    let left = envs.map(move |env| {
        let key = left_keyed
            .iter()
            .map(|(_, arg)| value(arg, &env).unwrap())
            .collect::<Vec<_>>();
        (key, env)
    });
    let right_keyed = keyed;
    let right = rows.map(move |row| {
        let key = right_keyed
            .iter()
            .map(|(position, _)| row[*position])
            .collect::<Vec<_>>();
        (key, row)
    });
    let args = args.to_vec();
    left.join(right)
        .flat_map(move |(_, (mut env, row))| unify(&args, &row, &mut env).then_some(env))
}

fn value(arg: &Arg, env: &[u32]) -> Option<u32> {
    match arg {
        Arg::Ground(term) => Some(term.0),
        Arg::Var(var) => (env[var.0 as usize] != UNBOUND).then_some(env[var.0 as usize]),
        Arg::Aggregate(_, subject) | Arg::Fold(_, subject) => value(subject, env),
    }
}

fn unify(args: &[Arg], row: &[u32], env: &mut [u32]) -> bool {
    if args.len() != row.len() {
        return false;
    }
    for (arg, cell) in args.iter().zip(row) {
        match arg {
            Arg::Ground(term) if term.0 != *cell => return false,
            Arg::Ground(_) => {}
            Arg::Var(var) => {
                let slot = &mut env[var.0 as usize];
                if *slot == UNBOUND {
                    *slot = *cell;
                } else if *slot != *cell {
                    return false;
                }
            }
            Arg::Aggregate(_, subject) | Arg::Fold(_, subject) => {
                if !unify(
                    std::slice::from_ref(subject),
                    std::slice::from_ref(cell),
                    env,
                ) {
                    return false;
                }
            }
        }
    }
    true
}

/// Runs synthetic demand rules from their first safe stratum onward.
fn schedule_rules(
    program: &Program,
    strata: &Strata,
    hidden: &HashSet<TermId>,
) -> HashMap<u32, Vec<Rule>> {
    let mut by_level: HashMap<u32, Vec<Rule>> = HashMap::new();
    for rule in &program.rules {
        if !hidden.contains(&rule.rel) {
            by_level
                .entry(strata.level(rule.rel))
                .or_default()
                .push(rule.clone());
            continue;
        }
        let first = rule
            .body
            .iter()
            .filter(|goal| !hidden.contains(&goal.rel))
            .map(|goal| strata.level(goal.rel) + u32::from(goal.polarity == Polarity::Negative))
            .max()
            .unwrap_or(0)
            .min(strata.max_level);
        for level in first..=strata.max_level {
            by_level.entry(level).or_default().push(rule.clone());
        }
    }
    by_level
}

fn tagged(rel: TermId, args: &[TermId]) -> DdRow {
    std::iter::once(rel.0)
        .chain(args.iter().map(|term| term.0))
        .collect()
}

fn kernel_ref(u: &mut Universe, name: &str) -> TermId {
    let name = u.atom(name);
    let kernel = u.compound("kernel", vec![name]);
    u.compound("ref", vec![kernel])
}
