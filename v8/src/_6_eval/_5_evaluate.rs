//! `evaluate(program) -> closure + diagnostics`, the port of `evaluate/4`.
//! Strata in order; inside a stratum, aggregate rules first over the completed
//! lower rows, then semi-naive rounds of the plain rules until a round adds
//! nothing. Every state change is reported to the trace sink.

use super::kernel::{self, Kernel};
use super::program::{Arg, Diagnostic, Goal, Polarity, Program, Row, Rule, VarId};
use super::stratify::{stratify, Strata};
use super::table::{Range, Table};
use super::term::{TermId, Universe};
use std::collections::HashMap;

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
            Arg::Count(inner) => self.value(inner),
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
            Arg::Count(inner) => self.unify(inner, t),
        }
    }
}

/// Which row range each body goal reads in one rule variant.
type Plan = Vec<Range>;

/// Derived rows waiting to be inserted after a round: `(relation, args)`.
type Sink = Vec<(TermId, Box<[TermId]>)>;

type Pattern = Vec<Option<TermId>>;

/// One body evaluation. `tables_only` is the aggregate mode of v7
/// (`completed_body_holds/2`): goals match stored rows only, no kernel
/// functions, no demanded rules.
struct Eval<'a> {
    u: &'a mut Universe,
    store: &'a Store,
    rules_by_rel: &'a HashMap<TermId, Vec<&'a Rule>>,
    tables_only: bool,
    intern_requests: Vec<Box<[TermId]>>,
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
        match goal.polarity {
            Polarity::Negative => {
                let args: Vec<Option<TermId>> = goal.args.iter().map(|a| env.value(a)).collect();
                if args.iter().any(|a| a.is_none()) {
                    return;
                }
                let holds = match kernel {
                    Some(k) => kernel::negative_holds(self.u, k, &args),
                    None => {
                        let row: Vec<TermId> = args.iter().map(|a| a.unwrap()).collect();
                        !self.store.table(goal.rel).is_some_and(|t| t.contains(&row))
                    }
                };
                if holds {
                    self.solve(rule, i + 1, plan, env, out);
                }
            }
            Polarity::Positive => {
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
                if let Some(table) = self.store.table(goal.rel) {
                    let bound: Vec<(usize, TermId)> = pattern
                        .iter()
                        .enumerate()
                        .filter_map(|(c, t)| t.map(|t| (c, t)))
                        .collect();
                    for id in table.candidates(&bound, plan[i]) {
                        let row = table.row(id);
                        if row.len() == goal.args.len() {
                            solutions.push(row.to_vec());
                        }
                    }
                }
                let demand = !self.tables_only
                    && kernel.is_none()
                    && pattern.iter().any(|t| t.is_some())
                    && self.rules_by_rel.contains_key(&goal.rel);
                if demand {
                    let extra = self.demand(goal.rel, &pattern);
                    solutions.extend(extra);
                }
                for solution in solutions {
                    let cp = env.checkpoint();
                    if self.unify_row(goal, &solution, env) {
                        self.solve(rule, i + 1, plan, env, out);
                    }
                    env.undo(cp);
                }
            }
        }
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
        let rules: Vec<&Rule> = self.rules_by_rel[&rel].clone();
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

fn current_goal_positions(rule: &Rule, strata: &Strata, level: u32, u: &Universe) -> Vec<usize> {
    rule.body
        .iter()
        .enumerate()
        .filter(|(_, g)| {
            g.polarity == Polarity::Positive
                && Kernel::of(u, g.rel).is_none()
                && strata.levels.get(&g.rel) == Some(&level)
        })
        .map(|(i, _)| i)
        .collect()
}

fn fire(
    u: &mut Universe,
    store: &Store,
    rules_by_rel: &HashMap<TermId, Vec<&Rule>>,
    rule: &Rule,
    plan: &Plan,
    sink: &mut Sink,
    requests: &mut Vec<Box<[TermId]>>,
) {
    let mut env = Env::new(rule.vars.len());
    let mut eval = Eval {
        u,
        store,
        rules_by_rel,
        tables_only: false,
        intern_requests: Vec::new(),
        memo: HashMap::new(),
    };
    let rel = rule.rel;
    let mut out = |_u: &mut Universe, env: &Env| {
        if let Some(row) = head_row(env, rule) {
            sink.push((rel, row.into_boxed_slice()));
        }
    };
    eval.solve(rule, 0, plan, &mut env, &mut out);
    requests.append(&mut eval.intern_requests);
}

/// Port of `derive_aggregate_rows/4`: every body proof over the completed
/// lower rows is one bag entry; plain head positions form the group key.
fn aggregate_rows(
    u: &mut Universe,
    store: &Store,
    rules_by_rel: &HashMap<TermId, Vec<&Rule>>,
    rule: &Rule,
) -> Result<Sink, Diagnostic> {
    let count_args = rule.count_args();
    if count_args != 1 {
        let n = u.int(count_args as i64);
        let payload = u.compound("malformed_aggregate_head", vec![n]);
        return Err(Diagnostic {
            phase: "evaluate",
            payload,
        });
    }
    let plan: Plan = rule.body.iter().map(|_| Range::All).collect();
    let mut proofs: Vec<Vec<TermId>> = Vec::new();
    let mut non_ground = false;
    {
        let mut env = Env::new(rule.vars.len());
        let mut eval = Eval {
            u: &mut *u,
            store,
            rules_by_rel,
            tables_only: true,
            intern_requests: Vec::new(),
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
    let count_pos = rule
        .head
        .iter()
        .position(|a| matches!(a, Arg::Count(_)))
        .unwrap();
    let mut groups: HashMap<Vec<TermId>, i64> = HashMap::new();
    let mut order: Vec<Vec<TermId>> = Vec::new();
    for proof in proofs {
        let mut key = proof.clone();
        key.remove(count_pos);
        let e = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key);
            0
        });
        *e += 1;
    }
    let mut rows = Vec::new();
    for key in order {
        let count = groups[&key];
        let n = u.int(count);
        let c = u.compound("const", vec![n]);
        let mut row = key;
        row.insert(count_pos, c);
        rows.push((rule.rel, row.into_boxed_slice()));
    }
    Ok(rows)
}

pub fn evaluate(u: &mut Universe, program: &Program, fx: &mut dyn FnMut(Trace)) -> Closure {
    let (strata, diagnostics) = stratify(u, program);
    if !diagnostics.is_empty() {
        return Closure {
            rows: Vec::new(),
            diagnostics,
        };
    }
    let mut store = Store::default();
    let mut requests: Vec<Box<[TermId]>> = Vec::new();

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
            match aggregate_rows(u, &store, &rules_by_rel, rule) {
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
            let mut sink: Sink = Vec::new();
            for rule in &plain {
                if round == 0 {
                    let plan: Plan = rule.body.iter().map(|_| Range::All).collect();
                    fire(
                        u,
                        &store,
                        &rules_by_rel,
                        rule,
                        &plan,
                        &mut sink,
                        &mut requests,
                    );
                } else {
                    let positions = current_goal_positions(rule, &strata, level, u);
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
                        fire(
                            u,
                            &store,
                            &rules_by_rel,
                            rule,
                            &plan,
                            &mut sink,
                            &mut requests,
                        );
                    }
                }
            }
            store.mark_all();
            let mut new = 0;
            for (rel, row) in sink {
                if store.insert(rel, row) {
                    new += 1;
                }
            }
            for r in requests.drain(..) {
                if store.insert(intern_rel, r) {
                    new += 1;
                }
            }
            fx(Trace::Round { level, round, new });
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
