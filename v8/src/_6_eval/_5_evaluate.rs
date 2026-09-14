//! `evaluate(program) -> closure + diagnostics`, the port of `evaluate/4`.
//! Strata in order; inside a stratum, aggregate rules first over the completed
//! lower rows, then semi-naive rounds of the plain rules until a round adds
//! nothing. Every state change is reported to the trace sink.

use super::kernel::{self, Kernel};
use super::program::{AggregateKind, Arg, Diagnostic, Goal, Polarity, Program, Row, Rule, VarId};
use super::stratify::{stratify, Strata};
use super::table::{Range, Table};
use super::term::{TermId, Universe};
use crate::_7_effect::Slice;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trace {
    Stratum {
        level: u32,
        rules: usize,
        seeds: usize,
    },
    Aggregate {
        level: u32,
        rules: usize,
        rows: usize,
    },
    Round {
        level: u32,
        round: u32,
        new: usize,
    },
    Closure {
        rows: usize,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Closure {
    pub rows: Vec<Row>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Default)]
pub struct Store {
    pub tables: HashMap<TermId, Table>,
}

impl Store {
    pub fn insert(&mut self, rel: TermId, args: Box<[TermId]>) -> bool {
        self.tables.entry(rel).or_default().insert(args)
    }

    pub fn table(&self, rel: TermId) -> Option<&Table> {
        self.tables.get(&rel)
    }

    pub fn mark_all(&mut self) {
        for t in self.tables.values_mut() {
            t.frontier = t.len();
        }
    }
}

struct Env {
    vars: Vec<Option<TermId>>,
    trail: Vec<VarId>,
}

impl Env {
    fn new(n: usize) -> Self {
        Env {
            vars: vec![None; n],
            trail: Vec::new(),
        }
    }

    fn bind(&mut self, v: VarId, t: TermId) {
        self.vars[v.0 as usize] = Some(t);
        self.trail.push(v);
    }

    fn checkpoint(&self) -> usize {
        self.trail.len()
    }

    fn undo(&mut self, to: usize) {
        while self.trail.len() > to {
            let v = self.trail.pop().unwrap();
            self.vars[v.0 as usize] = None;
        }
    }

    fn value(&self, a: &Arg) -> Option<TermId> {
        match a {
            Arg::Var(v) => self.vars[v.0 as usize],
            Arg::Ground(t) => Some(*t),
            Arg::Aggregate(_, subject) => self.value(subject),
        }
    }

    /// Unify one argument with a ground value; returns false and leaves the
    /// trail for the caller to undo on mismatch.
    fn unify(&mut self, a: &Arg, t: TermId) -> bool {
        match a {
            Arg::Var(v) => match self.vars[v.0 as usize] {
                Some(bound) => bound == t,
                None => {
                    self.bind(*v, t);
                    true
                }
            },
            Arg::Ground(g) => *g == t,
            Arg::Aggregate(_, subject) => self.unify(subject, t),
        }
    }
}

/// Which row range each body goal reads in one rule variant.
type Plan = Vec<Range>;

/// Derived rows waiting to be inserted after a round: `(relation, args)`.
type Sink = Vec<(TermId, Box<[TermId]>)>;

type Pattern = Vec<Option<TermId>>;

/// The relations the effect branch writes into, minted once per evaluation.
#[derive(Copy, Clone)]
struct Effects {
    effect: TermId,
    intern_snapshot: TermId,
    /// `_1_slots.rs:13`: `const(none)` fills an argument the goal left unbound.
    none: TermId,
}

#[derive(Copy, Clone)]
struct Context<'a> {
    store: &'a Store,
    rules_by_rel: &'a HashMap<TermId, Vec<&'a Rule>>,
    served: &'a HashSet<TermId>,
    effects: Effects,
}

/// One body evaluation. `tables_only` is the aggregate mode of v7
/// (`completed_body_holds/2`): goals match stored rows only, no kernel
/// functions, no demanded rules, no effects.
struct Eval<'a> {
    u: &'a mut Universe,
    cx: Context<'a>,
    tables_only: bool,
    intern_requests: Vec<Box<[TermId]>>,
    effect_rows: Vec<Box<[TermId]>>,
    snapshot_rows: Vec<Box<[TermId]>>,
    memo: HashMap<(TermId, Pattern), Vec<Vec<TermId>>>,
}

impl<'a> Eval<'a> {
    fn solve(
        &mut self,
        rule: &Rule,
        i: usize,
        plan: &Plan,
        env: &mut Env,
        out: &mut dyn FnMut(&mut Universe, &Env),
    ) {
        if i == rule.body.len() {
            out(self.u, env);
            return;
        }
        let goal = &rule.body[i];
        let kernel = Kernel::of(self.u, goal.rel);
        if goal.polarity == Polarity::Negative {
            if self.negative_goal_holds(goal, kernel, env) {
                self.solve(rule, i + 1, plan, env, out);
            }
            return;
        }
        for solution in self.positive_solutions(goal, kernel, plan[i], env) {
            let cp = env.checkpoint();
            if self.unify_row(goal, &solution, env) {
                self.solve(rule, i + 1, plan, env, out);
            }
            env.undo(cp);
        }
    }

    /// A negative goal with an unbound argument never holds; v7's `\+` is
    /// checked against ground rows only.
    #[inline]
    fn negative_goal_holds(&self, goal: &Goal, kernel: Option<Kernel>, env: &Env) -> bool {
        let args: Vec<Option<TermId>> = goal.args.iter().map(|a| env.value(a)).collect();
        if args.iter().any(|a| a.is_none()) {
            return false;
        }
        match kernel {
            Some(k) => kernel::negative_holds(self.u, k, &args),
            None => {
                let row: Vec<TermId> = args.iter().map(|a| a.unwrap()).collect();
                !self
                    .cx
                    .store
                    .table(goal.rel)
                    .is_some_and(|t| t.contains(&row))
            }
        }
    }

    /// Kernel rows, then stored rows in the goal's plan range, then the
    /// top-down rows a bound pattern demands.
    #[inline]
    fn positive_solutions(
        &mut self,
        goal: &Goal,
        kernel: Option<Kernel>,
        range: Range,
        env: &Env,
    ) -> Vec<Vec<TermId>> {
        let pattern: Pattern = goal.args.iter().map(|a| env.value(a)).collect();
        let mut solutions: Vec<Vec<TermId>> = Vec::new();
        if let (Some(k), false) = (kernel, self.tables_only) {
            solutions = kernel::solve(self.u, k, &pattern);
            if k == Kernel::Intern {
                for row in &solutions {
                    self.intern_requests.push(row.clone().into_boxed_slice());
                }
            }
        }
        if let Some(table) = self.cx.store.table(goal.rel) {
            let bound: Vec<(usize, TermId)> = pattern
                .iter()
                .enumerate()
                .filter_map(|(c, t)| t.map(|t| (c, t)))
                .collect();
            for id in table.candidates(&bound, range) {
                let row = table.row(id);
                if row.len() == goal.args.len() {
                    solutions.push(row.to_vec());
                }
            }
        }
        let served = !self.tables_only
            && self.cx.served.contains(&goal.rel)
            && !self.cx.rules_by_rel.contains_key(&goal.rel);
        if served {
            self.write_effect(goal.rel, &pattern);
        }
        let demand = !self.tables_only
            && kernel.is_none()
            && pattern.iter().any(|t| t.is_some())
            && self.cx.rules_by_rel.contains_key(&goal.rel);
        if demand {
            let extra = self.demand(goal.rel, &pattern);
            solutions.extend(extra);
        }
        solutions
    }

    /// `effect(Relation, Application)` on every evaluation of a served goal,
    /// hit or miss: the row is live interest, not a miss report.
    fn write_effect(&mut self, rel: TermId, pattern: &Pattern) {
        let none = self.cx.effects.none;
        let values: Vec<TermId> = pattern.iter().map(|t| t.unwrap_or(none)).collect();
        let list = self.u.list(&values);
        let arguments = self.u.compound("const", vec![list]);
        let Some(row) = kernel::intern_row(self.u, &[Some(rel), Some(arguments), None]) else {
            return;
        };
        let application = row[2];
        self.intern_requests.push(row.clone().into_boxed_slice());
        self.snapshot_rows.push(row.into_boxed_slice());
        self.effect_rows
            .push(vec![rel, application].into_boxed_slice());
    }

    /// Top-down proof of a derived relation under bindings, the part of v7's
    /// tabled `proves/2` that bottom-up evaluation cannot reach: a rule whose
    /// body needs its head arguments bound (kernel constructors, negation).
    /// Rows found here are used, never stored, matching v7's closure. A
    /// re-entrant call with the same key sees the rows found so far.
    fn demand(&mut self, rel: TermId, pattern: &Pattern) -> Vec<Vec<TermId>> {
        let key = (rel, pattern.clone());
        if let Some(rows) = self.memo.get(&key) {
            return rows.clone();
        }
        self.memo.insert(key.clone(), Vec::new());
        let rules: Vec<&Rule> = self.cx.rules_by_rel[&rel].clone();
        let mut found: Vec<Vec<TermId>> = Vec::new();
        for rule in rules {
            if rule.is_aggregate() || rule.head.len() != pattern.len() {
                continue;
            }
            let mut env = Env::new(rule.vars.len());
            let mut ok = true;
            for (a, t) in rule.head.iter().zip(pattern.iter()) {
                if let Some(t) = t {
                    if !env.unify(a, *t) {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                continue;
            }
            let plan: Plan = rule.body.iter().map(|_| Range::All).collect();
            let mut rows: Vec<Vec<TermId>> = Vec::new();
            let mut out = |_u: &mut Universe, env: &Env| {
                if let Some(row) = head_row(env, rule) {
                    rows.push(row);
                }
            };
            self.solve(rule, 0, &plan, &mut env, &mut out);
            found.extend(rows);
        }
        found.sort_by(|a, b| self.u.cmp_rows(a, b));
        found.dedup();
        self.memo.insert(key, found.clone());
        found
    }

    fn unify_row(&self, goal: &Goal, row: &[TermId], env: &mut Env) -> bool {
        if row.len() != goal.args.len() {
            return false;
        }
        for (a, t) in goal.args.iter().zip(row.iter()) {
            if !env.unify(a, *t) {
                return false;
            }
        }
        true
    }
}

fn head_row(env: &Env, rule: &Rule) -> Option<Vec<TermId>> {
    rule.head.iter().map(|a| env.value(a)).collect()
}

/// Neither relation the branch writes heads a rule, so `Strata` never levels
/// them, yet their rows arrive inside a round: a goal on one joins the delta.
fn current_goal_positions(
    rule: &Rule,
    strata: &Strata,
    level: u32,
    effects: Effects,
    u: &Universe,
) -> Vec<usize> {
    rule.body
        .iter()
        .enumerate()
        .filter(|(_, g)| {
            g.polarity == Polarity::Positive
                && Kernel::of(u, g.rel).is_none()
                && (strata.levels.get(&g.rel) == Some(&level) || g.rel == effects.effect)
        })
        .map(|(i, _)| i)
        .collect()
}

#[derive(Default)]
struct Pending {
    sink: Sink,
    requests: Vec<Box<[TermId]>>,
    effects: Vec<Box<[TermId]>>,
    snapshots: Vec<Box<[TermId]>>,
}

fn fire(u: &mut Universe, cx: Context, rule: &Rule, plan: &Plan, out: &mut Pending) {
    let mut env = Env::new(rule.vars.len());
    let mut eval = Eval {
        u,
        cx,
        tables_only: false,
        intern_requests: Vec::new(),
        effect_rows: Vec::new(),
        snapshot_rows: Vec::new(),
        memo: HashMap::new(),
    };
    let rel = rule.rel;
    let mut derived = |_u: &mut Universe, env: &Env| {
        if let Some(row) = head_row(env, rule) {
            out.sink.push((rel, row.into_boxed_slice()));
        }
    };
    eval.solve(rule, 0, plan, &mut env, &mut derived);
    out.requests.append(&mut eval.intern_requests);
    out.effects.append(&mut eval.effect_rows);
    out.snapshots.append(&mut eval.snapshot_rows);
}

/// `completed_body_holds/2`: one bag entry per body proof over the stored
/// rows, with a non-ground head rejecting the whole rule.
fn aggregate_proofs(
    u: &mut Universe,
    cx: Context,
    rule: &Rule,
) -> Result<Vec<Vec<TermId>>, Diagnostic> {
    let plan: Plan = rule.body.iter().map(|_| Range::All).collect();
    let mut proofs: Vec<Vec<TermId>> = Vec::new();
    let mut non_ground = false;
    {
        let mut env = Env::new(rule.vars.len());
        let mut eval = Eval {
            u: &mut *u,
            cx,
            tables_only: true,
            intern_requests: Vec::new(),
            effect_rows: Vec::new(),
            snapshot_rows: Vec::new(),
            memo: HashMap::new(),
        };
        let mut out = |_u: &mut Universe, env: &Env| match head_row(env, rule) {
            Some(row) => proofs.push(row),
            None => non_ground = true,
        };
        eval.solve(rule, 0, &plan, &mut env, &mut out);
    }
    if non_ground {
        let payload = u.atom("non_ground_aggregate_proof");
        return Err(Diagnostic {
            phase: "evaluate",
            payload,
        });
    }
    Ok(proofs)
}

/// Port of `derive_aggregate_rows/4`: every body proof over the completed
/// lower rows is one bag entry; plain head positions form the group key, and
/// the aggregate position is folded by kind.
fn aggregate_rows(u: &mut Universe, cx: Context, rule: &Rule) -> Result<Sink, Diagnostic> {
    let aggregate_args = rule.aggregate_args();
    if aggregate_args != 1 {
        let n = u.int(aggregate_args as i64);
        let payload = u.compound("malformed_aggregate_head", vec![n]);
        return Err(Diagnostic {
            phase: "evaluate",
            payload,
        });
    }
    let proofs = aggregate_proofs(u, cx, rule)?;
    let (position, aggregation, _) = rule.aggregate_head().unwrap();
    let mut groups: HashMap<Vec<TermId>, Vec<TermId>> = HashMap::new();
    let mut order: Vec<Vec<TermId>> = Vec::new();
    for proof in proofs {
        let mut key = proof;
        let value = key.remove(position);
        groups
            .entry(key.clone())
            .or_insert_with(|| {
                order.push(key);
                Vec::new()
            })
            .push(value);
    }
    let mut rows = Vec::new();
    for key in order {
        let result = fold_group(u, aggregation, &groups[&key])?;
        let mut row = key;
        row.insert(position, result);
        rows.push((rule.rel, row.into_boxed_slice()));
    }
    Ok(rows)
}

/// One group's folded value. `count` and `sum` produce `const(Int)`; `min`
/// and `max` return the winning subject term unchanged.
fn fold_group(
    u: &mut Universe,
    aggregation: AggregateKind,
    values: &[TermId],
) -> Result<TermId, Diagnostic> {
    match aggregation {
        AggregateKind::Count => {
            let n = u.int(values.len() as i64);
            Ok(u.compound("const", vec![n]))
        }
        AggregateKind::Sum => {
            let mut running: i64 = 0;
            for &value in values {
                let Some(addend) = u
                    .unary(value, "const")
                    .and_then(|payload| u.as_int(payload))
                else {
                    let payload = u.compound("aggregate_type_mismatch", vec![value]);
                    return Err(Diagnostic {
                        phase: "evaluate",
                        payload,
                    });
                };
                let Some(next) = running.checked_add(addend) else {
                    let payload = u.atom("aggregate_overflow");
                    return Err(Diagnostic {
                        phase: "evaluate",
                        payload,
                    });
                };
                running = next;
            }
            let n = u.int(running);
            Ok(u.compound("const", vec![n]))
        }
        AggregateKind::Min | AggregateKind::Max => {
            let mut best = values[0];
            for &value in &values[1..] {
                let ordering = u.cmp(value, best);
                let wins = match aggregation {
                    AggregateKind::Min => ordering == Ordering::Less,
                    _ => ordering == Ordering::Greater,
                };
                if wins {
                    best = value;
                }
            }
            Ok(best)
        }
    }
}

/// The semi-naive fixpoint as a reducer over its own row store.
pub struct Evaluate<'a>(PhantomData<&'a ()>);

impl<'a> Slice for Evaluate<'a> {
    type State = Store;
    type Event = (&'a mut Universe, &'a Program);
    type Output = Closure;
    type Effect = Trace;

    fn reduce(store: &mut Store, (u, program): Self::Event, fx: &mut dyn FnMut(Trace)) -> Closure {
        evaluate_into(store, u, program, fx)
    }
}

#[tracing::instrument(skip_all, fields(rules = program.rules.len(), seeds = program.seeds.len()))]
pub fn evaluate(u: &mut Universe, program: &Program, fx: &mut dyn FnMut(Trace)) -> Closure {
    Evaluate::reduce(&mut Store::default(), (u, program), fx)
}

fn evaluate_into(
    store: &mut Store,
    u: &mut Universe,
    program: &Program,
    fx: &mut dyn FnMut(Trace),
) -> Closure {
    let (strata, diagnostics) = stratify(u, program);
    if !diagnostics.is_empty() {
        return Closure {
            rows: Vec::new(),
            diagnostics,
        };
    }
    let kernel_rel = |u: &mut Universe, name: &str| {
        let n = u.atom(name);
        let k = u.compound("kernel", vec![n]);
        u.compound("ref", vec![k])
    };
    let effects = Effects {
        effect: kernel_rel(u, "effect"),
        intern_snapshot: kernel_rel(u, "intern_snapshot"),
        none: {
            let n = u.atom("none");
            u.compound("const", vec![n])
        },
    };
    let nil_rel = {
        let n = u.atom("nil");
        let k = u.compound("kernel", vec![n]);
        u.compound("ref", vec![k])
    };
    let intern_rel = {
        let n = u.atom("intern");
        let k = u.compound("kernel", vec![n]);
        u.compound("ref", vec![k])
    };
    let nil_row = {
        let e = u.empty_list();
        u.compound("const", vec![e])
    };
    store.insert(nil_rel, vec![nil_row].into_boxed_slice());

    let mut seeds_by_level: HashMap<u32, Vec<&Row>> = HashMap::new();
    for seed in &program.seeds {
        seeds_by_level
            .entry(strata.level(seed.rel))
            .or_default()
            .push(seed);
    }
    let mut rules_by_level: HashMap<u32, Vec<&Rule>> = HashMap::new();
    let mut rules_by_rel: HashMap<TermId, Vec<&Rule>> = HashMap::new();
    for rule in &program.rules {
        rules_by_level
            .entry(strata.level(rule.rel))
            .or_default()
            .push(rule);
        rules_by_rel.entry(rule.rel).or_default().push(rule);
    }

    for level in 0..=strata.max_level {
        let rules = rules_by_level.get(&level).cloned().unwrap_or_default();
        let seeds = seeds_by_level.get(&level).cloned().unwrap_or_default();
        fx(Trace::Stratum {
            level,
            rules: rules.len(),
            seeds: seeds.len(),
        });

        let (aggregate, plain): (Vec<&Rule>, Vec<&Rule>) =
            rules.iter().partition(|r| r.is_aggregate());
        let mut aggregate_diags = Vec::new();
        let mut aggregate_seeds = Vec::new();
        for rule in &aggregate {
            let cx = Context {
                store,
                rules_by_rel: &rules_by_rel,
                served: &program.served,
                effects,
            };
            match aggregate_rows(u, cx, rule) {
                Ok(rows) => aggregate_seeds.extend(rows),
                Err(d) => aggregate_diags.push(d),
            }
        }
        if !aggregate_diags.is_empty() {
            aggregate_diags.sort_by(|a, b| u.cmp(a.payload, b.payload));
            aggregate_diags.dedup();
            return Closure {
                rows: Vec::new(),
                diagnostics: aggregate_diags,
            };
        }
        fx(Trace::Aggregate {
            level,
            rules: aggregate.len(),
            rows: aggregate_seeds.len(),
        });

        for seed in &seeds {
            store.insert(seed.rel, seed.args.clone().into_boxed_slice());
        }
        for (rel, row) in aggregate_seeds {
            store.insert(rel, row);
        }

        let mut round: u32 = 0;
        loop {
            let mut pending = Pending::default();
            for rule in &plain {
                let cx = Context {
                    store,
                    rules_by_rel: &rules_by_rel,
                    served: &program.served,
                    effects,
                };
                if round == 0 {
                    let plan: Plan = rule.body.iter().map(|_| Range::All).collect();
                    fire(u, cx, rule, &plan, &mut pending);
                } else {
                    let positions = current_goal_positions(rule, &strata, level, effects, u);
                    for &delta_at in &positions {
                        let plan: Plan = rule
                            .body
                            .iter()
                            .enumerate()
                            .map(|(j, _)| {
                                if !positions.contains(&j) {
                                    Range::All
                                } else if j < delta_at {
                                    Range::Old
                                } else if j == delta_at {
                                    Range::Delta
                                } else {
                                    Range::All
                                }
                            })
                            .collect();
                        fire(u, cx, rule, &plan, &mut pending);
                    }
                }
            }
            store.mark_all();
            let mut new = 0;
            for (rel, row) in pending.sink {
                if store.insert(rel, row) {
                    new += 1;
                }
            }
            for r in pending.requests {
                if store.insert(intern_rel, r) {
                    new += 1;
                }
            }
            for row in pending.effects {
                if store.insert(effects.effect, row) {
                    new += 1;
                }
            }
            for row in pending.snapshots {
                if store.insert(effects.intern_snapshot, row) {
                    new += 1;
                }
            }
            fx(Trace::Round { level, round, new });
            tracing::trace!(target: "dl8::eval", level, round, new);
            round += 1;
            if new == 0 {
                break;
            }
        }
    }

    let mut rows: Vec<Row> = Vec::new();
    for (rel, table) in &store.tables {
        for row in table.rows.iter() {
            rows.push(Row {
                rel: *rel,
                args: row.to_vec(),
            });
        }
    }
    rows.sort_by(|a, b| {
        u.cmp(a.rel, b.rel)
            .then_with(|| u.cmp_rows(&a.args, &b.args))
    });
    rows.dedup();
    fx(Trace::Closure { rows: rows.len() });
    Closure {
        rows,
        diagnostics: Vec::new(),
    }
}
