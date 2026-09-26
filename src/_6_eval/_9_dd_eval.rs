//! Differential-dataflow evaluation over one worker and one dataflow.

use super::evaluate::{Closure, Trace};
use super::program::{Arg, Diagnostic, Folding, Polarity, Program, Row, Rule};
use super::stratify::{stratify, Strata};
use super::term::{TermId, Universe};
use differential_dataflow::input::{Input, InputSession};
use differential_dataflow::lattice::Lattice;
use differential_dataflow::operators::arrange::{Arranged, TraceAgent};
use differential_dataflow::operators::Iterate;
use differential_dataflow::trace::implementations::ValSpine;
use differential_dataflow::VecCollection;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use timely::dataflow::operators::probe::Handle as ProbeHandle;
use timely::logging::{TimelyEvent, TimelyEventBuilder};
use timely::progress::Timestamp;
use timely::worker::Worker;

type DdRow = Vec<u32>;
type Weight = isize;
type Relations<'scope, T> = HashMap<(u32, usize), VecCollection<'scope, T, DdRow, Weight>>;
type Arrangements<'scope, T> = HashMap<
    (u32, usize, Vec<usize>),
    Arranged<'scope, TraceAgent<ValSpine<Vec<u32>, DdRow, T, Weight>>>,
>;
type TaggedArrangement<'scope, T> = Arranged<'scope, TraceAgent<ValSpine<u32, DdRow, T, Weight>>>;
type RuleSources<'a, 'scope, T> = (
    &'a VecCollection<'scope, T, DdRow, Weight>,
    &'a VecCollection<'scope, T, (), Weight>,
    &'a TaggedArrangement<'scope, T>,
);
const UNBOUND: u32 = u32::MAX;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
enum DeclaredFoldInput {
    Subject(DdRow, u32),
    Stored(DdRow),
}

#[derive(Clone, Copy)]
struct ServedRows<'a> {
    intern: TermId,
    effect: TermId,
    snapshot: TermId,
    none: TermId,
    stored: &'a HashSet<TermId>,
}

struct StratumProfile {
    level: u32,
    operator_start: usize,
    operator_end: usize,
    rounds: Arc<AtomicU64>,
    rows: Arc<Mutex<HashMap<DdRow, Weight>>>,
    round_inspect: Option<usize>,
}

struct CachedDd {
    key: String,
    syms: Vec<String>,
    terms: Vec<super::term::Term>,
    input: InputSession<u64, DdRow, Weight>,
    worker: Worker,
    level_probes: Vec<(u32, ProbeHandle<u64>)>,
    probe: ProbeHandle<u64>,
    arena: Arc<Mutex<Universe>>,
    fold_program: Arc<Mutex<Program>>,
    captured: Arc<Mutex<Vec<(DdRow, Weight)>>>,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    seeds: HashSet<DdRow>,
    rows: HashMap<DdRow, Weight>,
    profiles: Vec<StratumProfile>,
    operator_ids: Arc<Mutex<Vec<usize>>>,
}

// Macrotime and comptime alternate rule sets, so retain one graph for each.
const CACHE_LIMIT: usize = 2;

thread_local! {
    static CACHE: std::cell::RefCell<Vec<CachedDd>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Evaluates positive products and recursive strata as DD collections.
pub fn evaluate_dd(u: &mut Universe, program: &Program, fx: &mut dyn FnMut(Trace)) -> Closure {
    let evaluation_started = Instant::now();
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
    let fold_program = Arc::new(Mutex::new(program.clone()));
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
    let stored_rels: Arc<HashSet<TermId>> = Arc::new(
        program
            .seeds
            .iter()
            .map(|row| row.rel)
            .chain(program.rules.iter().map(|rule| rule.rel))
            .chain([nil_rel, intern_rel, effect_rel, snapshot_rel])
            .collect(),
    );
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

    let mut served_key: Vec<u32> = served.iter().map(|rel| rel.0).collect();
    served_key.sort_unstable();
    let mut seeded_rels: Vec<u32> = program.seeds.iter().map(|row| row.rel.0).collect();
    seeded_rels.sort_unstable();
    seeded_rels.dedup();
    let key = format!("{:?}|{served_key:?}|{seeded_rels:?}", program.rules);
    let seeds: HashSet<DdRow> = program
        .seeds
        .iter()
        .map(|row| tagged(row.rel, &row.args))
        .chain([nil.clone()])
        .collect();
    let mut cache = CACHE.with(|cache| std::mem::take(&mut *cache.borrow_mut()));
    // Reuse only an equal graph with insertion-only seed changes. The arena
    // prefix also guards captured TermIds when a different program is evaluated.
    let reusable = cache.iter().position(|entry| {
        entry.key == key
            && entry.seeds.is_subset(&seeds)
            && entry.syms.len() <= u.syms.len()
            && entry.terms.len() <= u.terms.len()
            && entry.syms.iter().zip(&u.syms).all(|(old, new)| old == new)
            && entry
                .terms
                .iter()
                .zip(&u.terms)
                .all(|(old, new)| old == new)
    });
    let reused = reusable.is_some();
    let mut entry = if let Some(index) = reusable {
        let selected = cache.remove(index);
        *selected.arena.lock().unwrap() = std::mem::take(u);
        *selected.fold_program.lock().unwrap() = fold_program.lock().unwrap().clone();
        selected
    } else {
        let syms = u.syms.iter().cloned().collect();
        let terms = u.terms.iter().cloned().collect();
        let arena = Arc::new(Mutex::new(std::mem::take(u)));
        let captured: Arc<Mutex<Vec<(DdRow, Weight)>>> = Arc::default();
        let aggregate_diagnostics: Arc<Mutex<Vec<Diagnostic>>> = Arc::default();
        let program = program.clone();
        let arena_worker = Arc::clone(&arena);
        let captured_worker = Arc::clone(&captured);
        let diagnostics_worker = Arc::clone(&aggregate_diagnostics);
        let alloc = timely::communication::Allocator::Thread(
            timely::communication::allocator::thread::Thread::default(),
        );
        let mut worker = Worker::new(timely::WorkerConfig::default(), alloc, Some(Instant::now()));
        let profile = std::env::var_os("DL8_DD_PROFILE").is_some();
        let operator_ids = Arc::new(Mutex::new(Vec::new()));
        if profile {
            let operator_ids = Arc::clone(&operator_ids);
            worker
                .log_register()
                .unwrap()
                .insert::<TimelyEventBuilder, _>("timely", move |_, data| {
                    if let Some(events) = data.as_ref() {
                        let mut ids = operator_ids.lock().unwrap();
                        ids.extend(events.iter().filter_map(|(_, event)| match event {
                            TimelyEvent::Operates(operator) => Some(operator.id),
                            _ => None,
                        }));
                    }
                });
        }
        let probe = ProbeHandle::new();
        let build_started = Instant::now();
        let (input, level_probes, profiles) = worker.dataflow::<u64, _, _>(|scope| {
            let (input, seeds) = scope.new_collection::<DdRow, Weight>();
            let mut known = seeds.clone().filter(move |row| row[0] == nil_rel.0);
            let mut level_probes = Vec::new();
            let mut profiles = Vec::new();
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
            for (level, rels, rules) in levels {
                let level_started = Instant::now();
                let operator_start = scope.worker().peek_identifier();
                let rounds = Arc::new(AtomicU64::new(0));
                let profile_rows: Arc<Mutex<HashMap<DdRow, Weight>>> = Arc::default();
                let at_level = seeds.clone().filter(move |row| rels.contains(&row[0]));
                let (aggregate, plain): (Vec<Rule>, Vec<Rule>) =
                    rules.into_iter().partition(Rule::is_aggregate);
                let mut aggregate_rows = known.clone().filter(|_| false);
                let mut relations = Relations::new();
                let mut arrangements = Arrangements::new();
                let unit = known
                    .clone()
                    .filter(move |row| row[0] == nil_rel.0)
                    .map(|_| ())
                    .distinct();
                let tagged_rows = known
                    .clone()
                    .map(|row| (row[0], row[1..].to_vec()))
                    .arrange_by_key();
                for rule in &aggregate {
                    let rows = lower_aggregate_rule(
                        (&known, &unit, &tagged_rows),
                        &mut relations,
                        &mut arrangements,
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
                    if profile {
                        let operator_end = scope.worker().peek_identifier();
                        let rows = Arc::clone(&profile_rows);
                        known = known.inspect(move |(row, _, diff)| {
                            *rows.lock().unwrap().entry(row.clone()).or_default() += diff;
                        });
                        profiles.push(StratumProfile {
                            level,
                            operator_start,
                            operator_end,
                            rounds,
                            rows: profile_rows,
                            round_inspect: None,
                        });
                    }
                    let level_probe = ProbeHandle::new();
                    known = known.probe_with(&level_probe);
                    level_probes.push((level, level_probe));
                    tracing::info!(
                        level,
                        build_ms = level_started.elapsed().as_millis(),
                        "dd stratum build"
                    );
                    continue;
                }
                let arena = Arc::clone(&arena_worker);
                let served = Arc::clone(&served);
                let stored_rels = Arc::clone(&stored_rels);
                let round_counter = Arc::clone(&rounds);
                let round_inspect = Arc::new(Mutex::new(None));
                let round_inspect_worker = Arc::clone(&round_inspect);
                known = base.iterate(move |_scope, current| {
                    let current = if profile {
                        let operator_id = _scope.worker().peek_identifier();
                        let rounds = Arc::clone(&round_counter);
                        let inspected = current.inspect(move |(_, time, _)| {
                            rounds.fetch_max(time.inner + 1, Ordering::Relaxed);
                        });
                        *round_inspect_worker.lock().unwrap() = Some(operator_id);
                        inspected
                    } else {
                        current
                    };
                    let mut derived = current.clone().filter(|_| false);
                    let mut relations = Relations::new();
                    let mut arrangements = Arrangements::new();
                    let unit = current
                        .clone()
                        .filter(move |row| row[0] == nil_rel.0)
                        .map(|_| ())
                        .distinct();
                    let tagged_rows = current
                        .clone()
                        .map(|row| (row[0], row[1..].to_vec()))
                        .arrange_by_key();
                    for rule in &plain {
                        let rows = lower_rule(
                            (&current, &unit, &tagged_rows),
                            &mut relations,
                            &mut arrangements,
                            rule,
                            Arc::clone(&arena),
                            Arc::clone(&served),
                            ServedRows {
                                intern: intern_rel,
                                effect: effect_rel,
                                snapshot: snapshot_rel,
                                none,
                                stored: &stored_rels,
                            },
                        );
                        derived = derived.concat(rows);
                    }
                    current.concat(derived).distinct()
                });
                if profile {
                    let operator_end = scope.worker().peek_identifier();
                    let rows = Arc::clone(&profile_rows);
                    known = known.inspect(move |(row, _, diff)| {
                        *rows.lock().unwrap().entry(row.clone()).or_default() += diff;
                    });
                    profiles.push(StratumProfile {
                        level,
                        operator_start,
                        operator_end,
                        rounds,
                        rows: profile_rows,
                        round_inspect: *round_inspect.lock().unwrap(),
                    });
                }
                let level_probe = ProbeHandle::new();
                known = known.probe_with(&level_probe);
                level_probes.push((level, level_probe));
                tracing::info!(
                    level,
                    build_ms = level_started.elapsed().as_millis(),
                    "dd stratum build"
                );
            }
            let sink = Arc::clone(&captured_worker);
            known
                .inspect(move |(row, _, diff)| sink.lock().unwrap().push((row.clone(), *diff)))
                .probe_with(&probe);
            (input, level_probes, profiles)
        });
        tracing::info!(
            build_ms = build_started.elapsed().as_millis(),
            "dd dataflow build"
        );
        CachedDd {
            key,
            syms,
            terms,
            worker,
            input,
            level_probes,
            probe,
            arena,
            fold_program,
            captured,
            diagnostics: aggregate_diagnostics,
            seeds: HashSet::new(),
            rows: HashMap::new(),
            profiles,
            operator_ids,
        }
    };
    tracing::info!(
        reused,
        inserted = seeds.len() - entry.seeds.len(),
        "dd dataflow reuse"
    );
    if seeds != entry.seeds {
        for row in seeds.difference(&entry.seeds) {
            entry.input.insert(row.clone());
        }
        let next = *entry.input.time() + 1;
        entry.input.advance_to(next);
        entry.input.flush();
        for (level, level_probe) in &entry.level_probes {
            let step_started = Instant::now();
            entry.worker.step_while(|| level_probe.less_than(&next));
            tracing::info!(
                level,
                step_ms = step_started.elapsed().as_millis(),
                "dd stratum step"
            );
        }
        let step_started = Instant::now();
        entry.worker.step_while(|| entry.probe.less_than(&next));
        tracing::info!(step_ms = step_started.elapsed().as_millis(), "dd sink step");
        entry.seeds = seeds;
    }
    if std::env::var_os("DL8_DD_PROFILE").is_some() && !entry.profiles.is_empty() {
        entry.worker.logging().unwrap().flush();
        let ids = entry.operator_ids.lock().unwrap();
        let mut previous_rows = 0;
        for stratum in &entry.profiles {
            let operators = if reused {
                0
            } else {
                ids.iter()
                    .filter(|&&id| {
                        id >= stratum.operator_start
                            && id < stratum.operator_end
                            && Some(id) != stratum.round_inspect
                    })
                    .count()
            };
            let rows = stratum
                .rows
                .lock()
                .unwrap()
                .values()
                .filter(|&&diff| diff > 0)
                .count();
            tracing::info!(
                level = stratum.level,
                rounds = stratum.rounds.load(Ordering::Relaxed),
                operators,
                new_rows = rows.saturating_sub(previous_rows),
                cumulative_rows = rows,
                "dd stratum profile"
            );
            previous_rows = rows;
        }
    }

    let extraction_started = Instant::now();
    for (row, diff) in entry.captured.lock().unwrap().drain(..) {
        *entry.rows.entry(row).or_default() += diff;
    }
    entry.rows.retain(|_, weight| *weight != 0);
    let mut rows: Vec<Row> = entry
        .rows
        .iter()
        .filter(|(_, weight)| **weight > 0)
        .map(|(row, _)| Row {
            rel: TermId(row[0]),
            args: row[1..].iter().copied().map(TermId).collect(),
        })
        .collect();
    rows.retain(|row| !hidden.contains(&row.rel));
    let mut diagnostics = entry.diagnostics.lock().unwrap().clone();
    let mut arena = entry.arena.lock().unwrap();
    entry.syms = arena.syms.iter().cloned().collect();
    entry.terms = arena.terms.iter().cloned().collect();
    if !diagnostics.is_empty() {
        diagnostics.sort_by(|left, right| arena.cmp(left.payload, right.payload));
        diagnostics.dedup();
    } else {
        rows.sort_by(|left, right| {
            arena
                .cmp(left.rel, right.rel)
                .then_with(|| arena.cmp_rows(&left.args, &right.args))
        });
        rows.dedup();
    }
    let result = if diagnostics.is_empty() {
        fx(Trace::Closure { rows: rows.len() });
        tracing::info!(
            extract_ms = extraction_started.elapsed().as_millis(),
            total_ms = evaluation_started.elapsed().as_millis(),
            rows = rows.len(),
            "dd row extraction"
        );
        Closure { rows, diagnostics }
    } else {
        Closure {
            rows: Vec::new(),
            diagnostics,
        }
    };
    *u = std::mem::take(&mut *arena);
    drop(arena);
    cache.push(entry);
    if cache.len() > CACHE_LIMIT {
        cache.remove(0);
    }
    CACHE.with(|stored| *stored.borrow_mut() = cache);
    result
}

fn lower_aggregate_rule<'scope, T>(
    sources: RuleSources<'_, 'scope, T>,
    relations: &mut Relations<'scope, T>,
    arrangements: &mut Arrangements<'scope, T>,
    rule: &Rule,
    arena: Arc<Mutex<Universe>>,
    diagnostics: Arc<Mutex<Vec<Diagnostic>>>,
    program: Arc<Mutex<Program>>,
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord + Hash,
{
    let (all, unit, tagged_rows) = sources;
    let vars = rule.vars.len();
    let mut bound = vec![false; vars];
    let mut envs = unit.clone().map(move |_| vec![UNBOUND; vars]);
    for goal in &rule.body {
        let kernel = super::kernel::Kernel::of(&arena.lock().unwrap(), goal.rel);
        if goal.polarity == Polarity::Negative {
            envs = match kernel {
                Some(kernel) => negative_kernel(envs, &goal.args, kernel, Arc::clone(&arena)),
                None => {
                    let right = relation((unit, tagged_rows), relations, goal.rel, goal.args.len());
                    anti_goal(envs, right, &goal.args)
                }
            };
            continue;
        }
        envs = join_goal(
            envs,
            (unit, tagged_rows),
            relations,
            arrangements,
            goal,
            &bound,
        );
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
            let inputs = proofs
                .map(|(key, value)| ((), DeclaredFoldInput::Subject(key, value)))
                .concat(
                    all.clone()
                        .distinct()
                        .map(|row| ((), DeclaredFoldInput::Stored(row))),
                );
            let diagnostics = Arc::clone(&diagnostics);
            inputs
                .reduce(move |_key, input, output| {
                    let mut groups: BTreeMap<DdRow, Vec<TermId>> = BTreeMap::new();
                    let mut rows = Vec::new();
                    for (item, weight) in input {
                        for _ in 0..(*weight).max(0) as usize {
                            match item {
                                DeclaredFoldInput::Subject(key, value) => {
                                    groups.entry(key.clone()).or_default().push(TermId(*value));
                                }
                                DeclaredFoldInput::Stored(row) => rows.push(Row {
                                    rel: TermId(row[0]),
                                    args: row[1..].iter().copied().map(TermId).collect(),
                                }),
                            }
                        }
                    }
                    if groups.is_empty() {
                        return;
                    }
                    let values: Vec<&[TermId]> = groups.values().map(Vec::as_slice).collect();
                    let results = super::evaluate::fold_generic_row_groups(
                        &mut arena_for_reduce.lock().unwrap(),
                        &program.lock().unwrap(),
                        &rows,
                        &fold,
                        &values,
                    );
                    for ((key, _), result) in groups.into_iter().zip(results) {
                        match result {
                            Ok(term) => output.push(((key, term.0), 1)),
                            Err(diagnostic) => diagnostics.lock().unwrap().push(diagnostic),
                        }
                    }
                })
                .map(|(_, row)| row)
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
    sources: RuleSources<'_, 'scope, T>,
    relations: &mut Relations<'scope, T>,
    arrangements: &mut Arrangements<'scope, T>,
    rule: &Rule,
    arena: Arc<Mutex<Universe>>,
    served: Arc<HashSet<TermId>>,
    served_rows: ServedRows<'_>,
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord + Hash,
{
    let (_all, unit, tagged_rows) = sources;
    let vars = rule.vars.len();
    let mut bound = vec![false; vars];
    let mut envs = unit.clone().map(move |_| vec![UNBOUND; vars]);
    let mut side_rows: Option<VecCollection<'scope, T, DdRow, Weight>> = None;
    for goal in &rule.body {
        let kernel = super::kernel::Kernel::of(&arena.lock().unwrap(), goal.rel);
        if goal.polarity == Polarity::Negative {
            envs = match kernel {
                Some(kernel) => negative_kernel(envs, &goal.args, kernel, Arc::clone(&arena)),
                None => {
                    let right = relation((unit, tagged_rows), relations, goal.rel, goal.args.len());
                    anti_goal(envs, right, &goal.args)
                }
            };
            continue;
        }
        if served.contains(&goal.rel) {
            let args = goal.args.clone();
            let rel = goal.rel;
            let arena_for_effect = Arc::clone(&arena);
            let effects = envs.clone().flat_map(move |env| {
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
            });
            side_rows = Some(match side_rows {
                Some(rows) => rows.concat(effects),
                None => effects,
            });
        }
        envs = match kernel {
            Some(kernel) => {
                let stored = served_rows.stored.contains(&goal.rel).then(|| {
                    join_goal(
                        envs.clone(),
                        (unit, tagged_rows),
                        relations,
                        arrangements,
                        goal,
                        &bound,
                    )
                });
                let solved = positive_kernel(envs, &goal.args, kernel, Arc::clone(&arena));
                if kernel == super::kernel::Kernel::Intern {
                    let rel = goal.rel.0;
                    let interned = solved.clone().map(move |(_, mut row)| {
                        row.insert(0, rel);
                        row
                    });
                    side_rows = Some(match side_rows {
                        Some(rows) => rows.concat(interned),
                        None => interned,
                    });
                }
                let solved = solved.map(|(env, _)| env);
                match stored {
                    Some(rows) => solved.concat(rows).distinct(),
                    None => solved.distinct(),
                }
            }
            None => join_goal(
                envs,
                (unit, tagged_rows),
                relations,
                arrangements,
                goal,
                &bound,
            ),
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
    match side_rows {
        Some(rows) => heads.concat(rows),
        None => heads,
    }
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
    (unit, tagged): (
        &VecCollection<'scope, T, (), Weight>,
        &TaggedArrangement<'scope, T>,
    ),
    relations: &mut Relations<'scope, T>,
    rel: TermId,
    arity: usize,
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord,
{
    relations
        .entry((rel.0, arity))
        .or_insert_with(|| {
            unit.clone()
                .map(move |_| (rel.0, ()))
                .join_core(tagged.clone(), move |_rel, _unit, row| {
                    (row.len() == arity).then(|| row.clone())
                })
        })
        .clone()
}

fn join_goal<'scope, T>(
    envs: VecCollection<'scope, T, DdRow, Weight>,
    source: (
        &VecCollection<'scope, T, (), Weight>,
        &TaggedArrangement<'scope, T>,
    ),
    relations: &mut Relations<'scope, T>,
    arrangements: &mut Arrangements<'scope, T>,
    goal: &super::program::Goal,
    bound: &[bool],
) -> VecCollection<'scope, T, DdRow, Weight>
where
    T: Timestamp + Lattice + Ord,
{
    let keyed: Vec<(usize, Arg)> = goal
        .args
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
    let positions: Vec<usize> = keyed.iter().map(|(position, _)| *position).collect();
    let right = arrangements
        .entry((goal.rel.0, goal.args.len(), positions.clone()))
        .or_insert_with(|| {
            relation(source, relations, goal.rel, goal.args.len())
                .map(move |row| {
                    let key = positions.iter().map(|position| row[*position]).collect();
                    (key, row)
                })
                .arrange_by_key()
        })
        .clone();
    let args = goal.args.clone();
    left.join_core(right, move |_key, env, row| {
        let mut env = env.clone();
        unify(&args, row, &mut env).then_some(env)
    })
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

/// Runs synthetic demand rules only where a rule at that level reads them.
fn schedule_rules(
    program: &Program,
    strata: &Strata,
    hidden: &HashSet<TermId>,
) -> HashMap<u32, Vec<Rule>> {
    let mut by_level: HashMap<u32, Vec<Rule>> = HashMap::new();
    let mut hidden_rules: HashMap<TermId, Vec<(&Rule, u32)>> = HashMap::new();
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
        hidden_rules
            .entry(rule.rel)
            .or_default()
            .push((rule, first));
    }
    for level in 0..=strata.max_level {
        let mut seen = HashSet::new();
        let mut queue = Vec::new();
        for rule in by_level.get(&level).into_iter().flatten() {
            for goal in &rule.body {
                if hidden.contains(&goal.rel) && seen.insert(goal.rel) {
                    queue.push(goal.rel);
                }
            }
        }
        // Each hidden relation enters the queue once, so this bound covers its transitive reads.
        for at in 0..hidden.len() {
            let Some(rel) = queue.get(at) else {
                break;
            };
            for (rule, first) in hidden_rules.get(rel).into_iter().flatten() {
                if *first > level {
                    continue;
                }
                for goal in &rule.body {
                    if hidden.contains(&goal.rel) && seen.insert(goal.rel) {
                        queue.push(goal.rel);
                    }
                }
                by_level.entry(level).or_default().push((*rule).clone());
            }
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
