//! `DL8_ONE_ENGINE=1`: macro library comptime, macrotime cone and stdlib comptime rules as
//! parts of one install. Design: `plans/v8/2026-10-05-one-dd-engine.findings.md`.

use super::evaluate::{needs_bound_head, sum_readback_constructors, Closure, Trace};
use super::ir_eval::{merge_closures, put_answers, reset_last, take_answers, ApplyError, IrSession, SharedPool, SignedRow};
use super::ir_lower::{rule_key, Declared};
use super::kernel::{kernel_ref, Kernel};
use super::program::{Arg, Polarity, Program, Row, Rule};
use super::term::{TermId, Universe};
use crate::_9_runtime::Answerer;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

static MODE: OnceLock<Result<bool, String>> = OnceLock::new();

fn read_mode() -> Result<bool, String> {
    match std::env::var("DL8_ONE_ENGINE").as_deref() {
        Ok("0") => Ok(false),
        Err(std::env::VarError::NotPresent) | Ok("1") => Ok(true),
        Ok(other) => Err(format!("unknown DL8_ONE_ENGINE {other:?}; expected 0 or 1")),
        Err(error) => Err(format!("DL8_ONE_ENGINE {error}; expected 0 or 1")),
    }
}

/// The binary reads `DL8_ONE_ENGINE` here, once, at start; a library caller that never did reads
/// it on first use.
pub fn init_one_engine_mode() -> Result<bool, String> {
    MODE.get_or_init(read_mode).clone()
}

pub fn one_engine() -> bool {
    init_one_engine_mode().unwrap_or(true)
}

type Key = (u32, usize);

/// Passes of one stdlib loop before the round takes the whole-program path.
const LOOP_LIMIT: usize = 64;

/// What a comptime round loop's session attaches to in one-engine mode.
#[derive(Clone)]
pub enum Attach {
    /// Installs compose with the pool's pending programs or adopt its parts.
    Pool(SharedPool),
    Stdlib(Arc<Mutex<Stdlib>>),
}

impl Attach {
    pub fn session(&self) -> IrSession {
        match self {
            Attach::Pool(pool) => IrSession::configured().with_pool(pool.clone()),
            Attach::Stdlib(stdlib) => IrSession::configured().with_stdlib(stdlib.clone()),
        }
    }
}

fn goal_key(u: &Universe, rule: &Rule, goal: &super::program::Goal) -> Option<Key> {
    (Kernel::of(u, goal.rel).is_none()
        || (rule.is_aggregate() && goal.polarity == Polarity::Positive))
        .then_some((goal.rel.0, goal.args.len()))
}

/// The prelude's comptime rules on their part of the process engine. A program's comptime round
/// enters as the difference of its stdlib seed rows; its own rules run beside, on `rest`.
pub struct Stdlib {
    rules: Vec<Rule>,
    /// Relation keys of the stdlib rules: heads and the goals the lowering keys.
    keys: HashSet<Key>,
    reads: HashSet<Key>,
    /// Stdlib relations the demand rewrite drives from their callers' bound arguments.
    demanded: HashSet<Key>,
    reads_snapshots: bool,
    session: IrSession,
    resident: HashSet<Row>,
    lost: bool,
}

impl Stdlib {
    /// `rules` with the seeds of their relation keys: the program the stdlib part installs.
    pub fn program(u: &mut Universe, rules: Vec<Rule>, seeds: &[Row]) -> Program {
        let keys = Self::keys_of(u, &rules);
        let mut kept = HashSet::new();
        let seeds = seeds
            .iter()
            .filter(|row| keys.contains(&(row.rel.0, row.args.len())) && kept.insert((*row).clone()))
            .cloned()
            .collect();
        Program {
            rules,
            seeds,
            ..Program::default()
        }
    }

    fn keys_of(u: &Universe, rules: &[Rule]) -> HashSet<Key> {
        let mut keys = HashSet::new();
        for rule in rules {
            keys.insert(rule_key(u, rule).0);
            keys.extend(rule.body.iter().filter_map(|goal| goal_key(u, rule, goal)));
        }
        keys
    }

    /// Prepare `program` on a session attached to `pool`: the pool's stdlib part is adopted, no
    /// install and no settle.
    pub fn adopt(u: &mut Universe, pool: SharedPool, program: &Program) -> Result<Self, super::Diagnostic> {
        let mut session = IrSession::configured().with_pool(pool);
        session.prepare(u, program, &Declared::default())?;
        let snapshots = kernel_ref(u, "intern_snapshot");
        let mut reads = HashSet::new();
        let mut demanded = HashSet::new();
        for rule in &program.rules {
            reads.extend(rule.body.iter().filter_map(|goal| goal_key(u, rule, goal)));
            if needs_bound_head(u, rule, snapshots) {
                demanded.insert(rule_key(u, rule).0);
            }
        }
        Ok(Self {
            keys: Self::keys_of(u, &program.rules),
            reads,
            demanded,
            reads_snapshots: program
                .rules
                .iter()
                .any(|rule| rule.body.iter().any(|goal| goal.rel == snapshots)),
            rules: program.rules.clone(),
            session,
            resident: program.seeds.iter().cloned().collect(),
            lost: false,
        })
    }

    /// The stdlib-read relations program rules head (the loop input), or why the split fails; a
    /// rule on the loop must not negate or aggregate a relation the input reaches.
    fn split(&self, u: &mut Universe, program: &Program, theirs: &[Rule], mine: usize) -> Result<HashSet<Key>, String> {
        if self.lost {
            return Err("stdlib part lost".into());
        }
        if mine != self.rules.len() {
            return Err("stdlib rules absent".into());
        }
        if program.served.iter().any(|rel| self.keys.iter().any(|(key, _)| *key == rel.0)) {
            return Err("served relation in the stdlib".into());
        }
        let snapshots = kernel_ref(u, "intern_snapshot");
        let readbacks = if self.reads_snapshots {
            sum_readback_constructors(u, program, snapshots)
        } else {
            Default::default()
        };
        let mut input = HashSet::new();
        for rule in theirs {
            let head = rule_key(u, rule).0;
            if self.reads.contains(&head) {
                input.insert(head);
            }
            for goal in &rule.body {
                if goal.polarity == Polarity::Positive && self.demanded.contains(&(goal.rel.0, goal.args.len())) {
                    return Err("program rule calls a demand-driven stdlib relation".into());
                }
                if Kernel::of(u, goal.rel) == Some(Kernel::Intern)
                    && matches!(goal.args.first(), Some(Arg::Ground(ctor)) if readbacks.contains(ctor))
                {
                    return Err("program rule interns a constructor the stdlib reads back".into());
                }
            }
        }
        if input.is_empty() {
            return Ok(input);
        }
        let edges: Vec<(Key, Vec<(Key, bool)>, bool)> = program
            .rules
            .iter()
            .map(|rule| {
                let goals = rule
                    .body
                    .iter()
                    .filter_map(|goal| goal_key(u, rule, goal).map(|key| (key, goal.polarity == Polarity::Positive)))
                    .collect();
                (rule_key(u, rule).0, goals, rule.is_aggregate())
            })
            .collect();
        // Fixed points over the rule edges, explicit worklists: `down` depends on the input,
        // `up` feeds it.
        let mut down: HashSet<Key> = input.clone();
        let mut up: HashSet<Key> = input.clone();
        // Each pass adds a relation to `down` or `up`, or ends.
        let relations: usize = edges.iter().map(|(_, goals, _)| 1 + goals.len()).sum();
        for _ in 0..=2 * relations {
            let before = (down.len(), up.len());
            for (head, goals, _) in &edges {
                if goals.iter().any(|(goal, _)| down.contains(goal)) {
                    down.insert(*head);
                }
                if up.contains(head) {
                    up.extend(goals.iter().map(|(goal, _)| *goal));
                }
            }
            if (down.len(), up.len()) == before {
                break;
            }
        }
        for (head, goals, aggregate) in &edges {
            if !(down.contains(head) && up.contains(head)) {
                continue;
            }
            if goals.iter().any(|(goal, positive)| (!positive || *aggregate) && down.contains(goal)) {
                return Err(format!("non-monotone loop between stdlib and program rules at {}", u.display(TermId(head.0))));
            }
        }
        Ok(input)
    }

    /// One settle of the stdlib part to `wanted` seed rows, and its closure.
    fn settle_to(&mut self, u: &mut Universe, wanted: HashSet<Row>) -> Result<(Closure, usize, usize), Result<Closure, String>> {
        let mut changes: Vec<SignedRow> = self
            .resident
            .difference(&wanted)
            .map(|row| SignedRow { row: row.clone(), weight: -1 })
            .collect();
        changes.extend(wanted.difference(&self.resident).map(|row| SignedRow { row: row.clone(), weight: 1 }));
        let sent = changes.len();
        let applied = match self.session.apply(u, &changes) {
            Ok(applied) => applied,
            Err(ApplyError::Unprepared(reason)) => {
                self.lost = true;
                return Err(Err(reason.to_string()));
            }
            Err(ApplyError::Failed(diagnostic)) => {
                self.lost = true;
                return Err(Ok(Closure { rows: vec![], diagnostics: vec![diagnostic] }));
            }
        };
        self.resident = wanted;
        if !applied.diagnostics.is_empty() {
            return Err(Ok(Closure { rows: vec![], diagnostics: applied.diagnostics }));
        }
        match self.session.snapshot(u) {
            Ok(closure) if closure.diagnostics.is_empty() => Ok((closure, sent, applied.rows.len())),
            Ok(closure) => Err(Ok(closure)),
            Err(diagnostic) => {
                self.lost = true;
                Err(Ok(Closure { rows: vec![], diagnostics: vec![diagnostic] }))
            }
        }
    }

    /// One comptime round: stdlib seeds enter the stdlib part as signed rows, program rules run on
    /// `rest`; their rows of loop-input relations enter the stdlib part until none is new.
    pub fn evaluate(
        &mut self,
        u: &mut Universe,
        program: &Program,
        mut answerer: Option<&mut Answerer>,
        fx: &mut dyn FnMut(Trace),
        rest: &mut IrSession,
    ) -> Closure {
        let mut theirs = Vec::new();
        let mut mine = 0;
        for rule in &program.rules {
            if self.rules.contains(rule) {
                mine += 1;
            } else {
                theirs.push(rule.clone());
            }
        }
        let input = match self.split(u, program, &theirs, mine) {
            Ok(input) => input,
            Err(reason) => {
                tracing::info!(target: "dl8::one", %reason, rules = program.rules.len(), "one engine fallback");
                return rest.evaluate_plain(u, program, answerer, fx);
            }
        };
        reset_last();
        let mut seeds = HashSet::new();
        let mut others = Vec::new();
        for row in &program.seeds {
            if self.keys.contains(&(row.rel.0, row.args.len())) {
                seeds.insert(row.clone());
            } else {
                others.push(row.clone());
            }
        }
        let mut feeds = HashSet::new();
        for rule in &theirs {
            feeds.insert(rule_key(u, rule).0);
            feeds.extend(rule.body.iter().map(|goal| (goal.rel.0, goal.args.len())));
        }
        let mut extra: HashSet<Row> = HashSet::new();
        let (mut asked, mut answers) = (0, Vec::new());
        for pass in 0..LOOP_LIMIT {
            let wanted: HashSet<Row> = seeds.union(&extra).cloned().collect();
            let (stdlib, sent, changed) = match self.settle_to(u, wanted) {
                Ok(settled) => settled,
                Err(Ok(failed)) => return failed,
                Err(Err(reason)) => {
                    tracing::info!(target: "dl8::one", %reason, "one engine fallback");
                    return rest.evaluate_plain(u, program, answerer, fx);
                }
            };
            tracing::info!(
                target: "dl8::one",
                pass,
                sent,
                changed,
                closure = stdlib.rows.len(),
                program_rules = theirs.len(),
                "one engine stdlib"
            );
            if theirs.is_empty() {
                let seeds = Closure { rows: others, diagnostics: Vec::new() };
                return merge_closures(u, stdlib, seeds);
            }
            let mut part_seeds = others.clone();
            part_seeds.extend(
                stdlib
                    .rows
                    .iter()
                    .filter(|row| feeds.contains(&(row.rel.0, row.args.len())))
                    .cloned(),
            );
            // Answers of earlier passes stay seeds, as the next round's frozen answers would.
            part_seeds.extend(answers.iter().cloned());
            let part = Program {
                rules: theirs.clone(),
                seeds: part_seeds,
                served: program.served.clone(),
            };
            let closure = rest.evaluate_plain(u, &part, answerer.as_deref_mut(), fx);
            let (requests, rows) = take_answers();
            asked += requests;
            answers.extend(rows);
            put_answers(asked, answers.clone());
            if !closure.diagnostics.is_empty() {
                return closure;
            }
            let held: HashSet<&Row> = stdlib.rows.iter().collect();
            let new: Vec<Row> = closure
                .rows
                .iter()
                .filter(|row| input.contains(&(row.rel.0, row.args.len())) && !held.contains(row) && !extra.contains(row))
                .cloned()
                .collect();
            if new.is_empty() {
                return merge_closures(u, stdlib, closure);
            }
            extra.extend(new);
        }
        tracing::info!(target: "dl8::one", reason = "stdlib loop limit", "one engine fallback");
        rest.evaluate_plain(u, program, answerer, fx)
    }
}
