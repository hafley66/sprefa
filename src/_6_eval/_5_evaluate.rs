//! `evaluate(program) -> closure + diagnostics` through the IR engine
//! `DL8_ENGINE` selects, the row store a continuing run seeds from, and the
//! goal solver a declared fold runs its step through.

use super::ir_eval::{configured_backend, evaluate_ir, unknown_engine};
use super::kernel::{self, Kernel};
use super::program::{
    Arg, Diagnostic, Fold, Goal, Order, Polarity, Program, Row, Rule, Seed, VarId,
};
use super::table::{Range, Table};
use super::term::{TermId, Universe};
use crate::_7_effect::Slice;
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
    Demand {
        calls: usize,
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
            Arg::Aggregate(_, subject) | Arg::Fold(_, subject) => self.value(subject),
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
            Arg::Aggregate(_, subject) | Arg::Fold(_, subject) => self.unify(subject, t),
        }
    }
}

/// Which row range each body goal reads in one rule variant.
type Plan = Vec<Range>;

type Pattern = Vec<Option<TermId>>;
type DemandKey = (TermId, Pattern);

/// One round's demand memo: rows proved per `(relation, pattern)` and the
/// entry count the trace reports.
#[derive(Default)]
struct Memo {
    proved: HashMap<DemandKey, Vec<Vec<TermId>>>,
    calls: usize,
}

/// The relations the effect branch writes into, minted once per evaluation.
#[derive(Copy, Clone)]
struct Effects {
    intern_snapshot: TermId,
    /// `_1_slots.rs:13`: `const(none)` fills an argument the goal left unbound.
    none: TermId,
}

#[derive(Copy, Clone)]
struct Context<'a> {
    store: &'a Store,
    rules_by_rel: &'a HashMap<TermId, Vec<&'a Rule>>,
    served: &'a HashSet<TermId>,
    demanded: &'a HashSet<TermId>,
    effects: Effects,
    sum_readbacks: &'a HashSet<TermId>,
}

/// One body evaluation.
struct Eval<'a> {
    u: &'a mut Universe,
    cx: Context<'a>,
    intern_requests: Vec<Box<[TermId]>>,
    effect_rows: Vec<Box<[TermId]>>,
    snapshot_rows: Vec<Box<[TermId]>>,
    memo: &'a mut Memo,
    in_progress: HashSet<DemandKey>,
    cycle_hits: Vec<DemandKey>,
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
        if let Some(k) = kernel {
            solutions = kernel::solve(self.u, k, &pattern);
            if k == Kernel::Intern {
                for row in &solutions {
                    self.intern_requests.push(row.clone().into_boxed_slice());
                    if self.cx.sum_readbacks.contains(&row[0]) {
                        self.snapshot_rows.push(row.clone().into_boxed_slice());
                    }
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
        let served =
            self.cx.served.contains(&goal.rel) && !self.cx.rules_by_rel.contains_key(&goal.rel);
        if served {
            self.write_effect(goal.rel, &pattern);
        }
        let demand = kernel.is_none()
            && pattern.iter().any(|t| t.is_some())
            && self.cx.demanded.contains(&goal.rel);
        if demand {
            let extra = self.demand(goal.rel, &pattern);
            solutions.extend(extra);
        }
        solutions
    }

    /// `effect(Relation, Application)` on every evaluation of a served goal,
    /// hit or miss: the row is live interest, not a miss report.
    fn write_effect(&mut self, rel: TermId, pattern: &Pattern) {
        let Some(rows) = effect_rows(self.u, self.cx.effects.none, rel, pattern) else {
            return;
        };
        self.intern_requests.push(rows.intern.into_boxed_slice());
        self.snapshot_rows.push(rows.snapshot.into_boxed_slice());
        self.effect_rows.push(rows.effect.into_boxed_slice());
    }

    /// Top-down proof of a derived relation under bindings, the part of v7's
    /// tabled `proves/2` that bottom-up evaluation cannot reach: a rule whose
    /// body needs its head arguments bound (kernel constructors, negation).
    /// Rows found here are used, never stored, matching v7's closure. A
    /// re-entrant call with the same key sees the rows found so far.
    ///
    /// The memo is shared across the round's firings, so an entry must be
    /// reproducible on its own. A proof cut short by a re-entrant key read
    /// that key's partial rows and is not: such an entry is dropped and the
    /// next firing re-proves it. The cycle target's own entry is complete
    /// because its final pass sees the same partial rows any fresh pass
    /// would, so it stays.
    fn demand(&mut self, rel: TermId, pattern: &Pattern) -> Vec<Vec<TermId>> {
        let key = (rel, pattern.clone());
        self.memo.calls += 1;
        if let Some(rows) = self.memo.proved.get(&key) {
            if self.in_progress.contains(&key) {
                self.cycle_hits.push(key);
            }
            return rows.clone();
        }
        self.in_progress.insert(key.clone());
        self.memo.proved.insert(key.clone(), Vec::new());
        let rules: Vec<&Rule> = self.cx.rules_by_rel[&rel].clone();
        let mut found: Vec<Vec<TermId>> = Vec::new();
        let base = self.cycle_hits.len();
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
        let mut hits: Vec<DemandKey> = self.cycle_hits.drain(base..).collect();
        hits.retain(|hit| *hit != key);
        self.in_progress.remove(&key);
        if hits.is_empty() {
            self.memo.proved.insert(key, found.clone());
        } else {
            self.cycle_hits.extend(hits);
            self.memo.proved.remove(&key);
        }
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

/// Sum-variant patterns mark their readback variable during lowering. Only
/// those constructors read newly minted `intern` rows as snapshots.
pub(super) fn sum_readback_constructors(
    u: &Universe,
    program: &Program,
    snapshot_rel: TermId,
) -> HashSet<TermId> {
    program
        .rules
        .iter()
        .flat_map(|rule| {
            rule.body.iter().filter_map(move |goal| {
                if goal.rel != snapshot_rel {
                    return None;
                }
                let (Some(Arg::Ground(constructor)), Some(Arg::Var(value))) =
                    (goal.args.first(), goal.args.get(2))
                else {
                    return None;
                };
                let key = *rule.vars.get(value.0 as usize)?;
                (u.functor_or_atom(key).map(|(name, _)| name) == Some("sum_pattern"))
                    .then_some(*constructor)
            })
        })
        .collect()
}

/// The three rows one served-goal evaluation writes.
pub(super) struct EffectRows {
    pub intern: Vec<TermId>,
    pub effect: Vec<TermId>,
    pub snapshot: Vec<TermId>,
}

/// Builds the intern request, effect row, and intern snapshot for one pattern.
pub(super) fn effect_rows(
    u: &mut Universe,
    none: TermId,
    rel: TermId,
    pattern: &[Option<TermId>],
) -> Option<EffectRows> {
    let values: Vec<TermId> = pattern.iter().map(|term| term.unwrap_or(none)).collect();
    let list = u.list(&values);
    let tuple = u.atom("tuple");
    let application = u.compound("application", vec![tuple, list]);
    let arguments = u.compound("ref", vec![application]);
    let intern = kernel::intern_row(u, &[Some(rel), Some(arguments), None])?;
    let application = intern[2];
    Some(EffectRows {
        snapshot: intern.clone(),
        intern,
        effect: vec![rel, application],
    })
}

fn head_row(env: &Env, rule: &Rule) -> Option<Vec<TermId>> {
    rule.head.iter().map(|a| env.value(a)).collect()
}

fn arg_vars(arg: &Arg) -> Vec<VarId> {
    match arg {
        Arg::Var(v) => vec![*v],
        Arg::Ground(_) => Vec::new(),
        Arg::Aggregate(_, subject) | Arg::Fold(_, subject) => arg_vars(subject),
    }
}

/// The rule proves a row only under a caller's binding: some kernel or
/// negative goal reads a head variable no earlier goal binds, so bottom-up
/// derivation cannot reach its rows. A positive kernel goal never blocks
/// when the op computes its row from the arguments bound at that point,
/// matching each op's row function: comparisons need both sides, `str.cons`
/// takes head and tail or the whole text, `str.nil` needs nothing, every other op takes its first two arguments.
/// That readiness argument only holds while every table the rule reads is
/// present when its stratum starts. A sum readback's `intern_snapshot` rows
/// arrive inside the evaluation that interns them, and the snapshot relation
/// never joins the delta, so a rule with such a goal keeps the plain blocking
/// walk and demand answers it in place. Every other snapshot row is a seed of
/// the evaluation.
pub(crate) fn needs_bound_head(u: &Universe, rule: &Rule, snapshots: TermId) -> bool {
    if rule.is_aggregate() {
        return false;
    }
    let anchored = rule.body.iter().all(|g| {
        let sum_readback = matches!(
            g.args.get(2),
            Some(Arg::Var(value)) if rule
                .vars
                .get(value.0 as usize)
                .and_then(|key| u.functor_or_atom(*key))
                .is_some_and(|(name, _)| name == "sum_pattern")
        );
        g.polarity != Polarity::Positive || g.rel != snapshots || !sum_readback
    });
    let head: HashSet<VarId> = rule.head.iter().flat_map(arg_vars).collect();
    let mut bound: HashSet<VarId> = HashSet::new();
    for goal in &rule.body {
        let op = Kernel::of(u, goal.rel);
        let vars: HashSet<VarId> = goal.args.iter().flat_map(arg_vars).collect();
        if let (Some(op), Polarity::Positive) = (op, goal.polarity) {
            if anchored {
                let at = |i: usize| {
                    goal.args
                        .get(i)
                        .map(|a| arg_vars(a).into_iter().all(|v| bound.contains(&v)))
                        .unwrap_or(false)
                };
                let ready = match op {
                    Kernel::StrNil => true,
                    Kernel::StrCons => at(2) || (at(0) && at(1)),
                    Kernel::Tuple => at(goal.args.len() - 1) || (0..goal.args.len() - 1).all(at),
                    Kernel::Str(op) => (0..op.args().len()).all(at),
                    _ => at(0) && at(1),
                };
                if !ready {
                    return true;
                }
            } else if vars.iter().any(|v| head.contains(v) && !bound.contains(v)) {
                return true;
            }
            bound.extend(vars);
        } else if op.is_none() && goal.polarity == Polarity::Positive {
            bound.extend(vars);
        } else {
            let blocking = op.is_some() || goal.polarity == Polarity::Negative;
            if blocking && vars.iter().any(|v| head.contains(v) && !bound.contains(v)) {
                return true;
            }
        }
    }
    false
}

pub(crate) fn demanded_relations(
    u: &Universe,
    rules_by_rel: &HashMap<TermId, Vec<&Rule>>,
    snapshots: TermId,
) -> HashSet<TermId> {
    let mut demanded: HashSet<TermId> = rules_by_rel
        .iter()
        .filter(|(_, rules)| {
            rules
                .iter()
                .any(|rule| needs_bound_head(u, rule, snapshots))
        })
        .map(|(relation, _)| *relation)
        .collect();
    loop {
        let before = demanded.len();
        for (relation, rules) in rules_by_rel {
            if rules.iter().any(|rule| {
                rule.body.iter().any(|goal| {
                    goal.polarity == Polarity::Positive
                        && Kernel::of(u, goal.rel).is_none()
                        && demanded.contains(&goal.rel)
                })
            }) {
                demanded.insert(*relation);
            }
        }
        if demanded.len() == before {
            return demanded;
        }
    }
}

/// The generic path: values in `order`, then the step run as a goal per value.
fn fold_generic(
    u: &mut Universe,
    cx: Context,
    fold: &Fold,
    values: &[TermId],
) -> Result<TermId, Diagnostic> {
    let mut sorted = values.to_vec();
    match fold.order {
        Order::TermLt => sorted.sort_by(|left, right| u.cmp(*left, *right)),
    }
    let (mut accumulator, rest) = match fold.seed {
        Seed::Zero => {
            let zero = u.int(0);
            (u.compound("const", vec![zero]), &sorted[..])
        }
        Seed::Term(term) => (term, &sorted[..]),
        Seed::FirstValue => match sorted.split_first() {
            Some((first, rest)) => (*first, rest),
            None => {
                let payload = u.compound("fold_empty_group", vec![fold.step]);
                return Err(Diagnostic {
                    phase: "evaluate",
                    payload,
                });
            }
        },
    };
    for &value in rest {
        accumulator = fold_step(u, cx, fold.step, accumulator, value)?;
    }
    Ok(accumulator)
}

/// Runs a declared fold against a completed row snapshot, one result per group; the row
/// snapshot and rule index are built once for every group.
pub(super) fn fold_generic_row_groups(
    u: &mut Universe,
    program: &Program,
    rows: &[Row],
    fold: &Fold,
    groups: &[&[TermId]],
) -> Vec<Result<TermId, Diagnostic>> {
    let mut store = Store::default();
    for row in rows {
        store.insert(row.rel, row.args.clone().into_boxed_slice());
    }
    let mut rules_by_rel: HashMap<TermId, Vec<&Rule>> = HashMap::new();
    for rule in &program.rules {
        rules_by_rel.entry(rule.rel).or_default().push(rule);
    }
    let kernel_rel = |u: &mut Universe, name: &str| {
        let name = u.atom(name);
        let kernel = u.compound("kernel", vec![name]);
        u.compound("ref", vec![kernel])
    };
    let effects = Effects {
        intern_snapshot: kernel_rel(u, "intern_snapshot"),
        none: {
            let none = u.atom("none");
            u.compound("const", vec![none])
        },
    };
    let sum_readbacks = sum_readback_constructors(u, program, effects.intern_snapshot);
    let demanded = demanded_relations(u, &rules_by_rel, effects.intern_snapshot);
    let cx = Context {
        store: &store,
        rules_by_rel: &rules_by_rel,
        served: &program.served,
        demanded: &demanded,
        effects,
        sum_readbacks: &sum_readbacks,
    };
    groups
        .iter()
        .map(|values| fold_generic(u, cx, fold, values))
        .collect()
}

/// `(Step Acc Value ?Next)` as a one-goal rule; exactly one distinct `Next` is
/// the only outcome that folds. A step that constructs its `Next` (a `Doc`
/// accumulator) yields the constructed term, the one the engine mints.
fn fold_step(
    u: &mut Universe,
    cx: Context,
    step: TermId,
    accumulator: TermId,
    value: TermId,
) -> Result<TermId, Diagnostic> {
    let identity = u.atom("fold_next");
    let rule = Rule {
        rel: step,
        head: vec![Arg::Var(VarId(0))],
        body: vec![Goal {
            polarity: Polarity::Positive,
            rel: step,
            args: vec![
                Arg::Ground(accumulator),
                Arg::Ground(value),
                Arg::Var(VarId(0)),
            ],
        }],
        vars: vec![identity],
    };
    let plan: Plan = vec![Range::All];
    let mut nexts: Vec<TermId> = Vec::new();
    {
        let mut demand_memo = Memo::default();
        let mut env = Env::new(1);
        let mut eval = Eval {
            u: &mut *u,
            cx,
            intern_requests: Vec::new(),
            effect_rows: Vec::new(),
            snapshot_rows: Vec::new(),
            memo: &mut demand_memo,
            in_progress: HashSet::new(),
            cycle_hits: Vec::new(),
        };
        let mut out = |_u: &mut Universe, env: &Env| {
            if let Some(next) = env.vars[0] {
                nexts.push(next);
            }
        };
        eval.solve(&rule, 0, &plan, &mut env, &mut out);
    }
    nexts.sort_by(|left, right| u.cmp(*left, *right));
    nexts.dedup();
    let reason = match nexts.len() {
        1 => return Ok(nexts[0]),
        0 => "fold_step_no_row",
        _ => "fold_step_ambiguous",
    };
    let payload = u.compound(reason, vec![step, accumulator, value]);
    Err(Diagnostic {
        phase: "evaluate",
        payload,
    })
}

/// IR evaluation as a reducer over its own row store.
pub struct Evaluate<'a>(PhantomData<&'a ()>);
impl<'a> Slice for Evaluate<'a> {
    type State = Store;
    type Event = (&'a mut Universe, &'a Program);
    type Output = Closure;
    type Effect = Trace;

    fn reduce(store: &mut Store, (u, program): Self::Event, fx: &mut dyn FnMut(Trace)) -> Closure {
        // Rows already in the store (a continuing run) are seeds to the
        // engine; every closure row lands back in the store.
        let mut seeded = program.clone();
        for (rel, table) in &store.tables {
            for id in 0..table.len() as u32 {
                seeded.seeds.push(Row {
                    rel: *rel,
                    args: table.row(id).to_vec(),
                });
            }
        }
        let closure = match configured_backend() {
            Ok(backend) => evaluate_ir(u, &seeded, backend, fx),
            Err(message) => Closure {
                rows: Vec::new(),
                diagnostics: vec![unknown_engine(u, &message)],
            },
        };
        for row in &closure.rows {
            store.insert(row.rel, row.args.clone().into_boxed_slice());
        }
        closure
    }
}

#[tracing::instrument(skip_all, fields(rules = program.rules.len(), seeds = program.seeds.len()))]
pub fn evaluate(u: &mut Universe, program: &Program, fx: &mut dyn FnMut(Trace)) -> Closure {
    Evaluate::reduce(&mut Store::default(), (u, program), fx)
}
