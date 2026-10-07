//! The tick loop over a served program. Tick 0 evaluates the program as it
//! stands; tick N inserts executor answers, evaluates, and persists.

use super::store::{IRowStore, StoreError};
use crate::_6_eval::evaluate::{Closure, Evaluate, Store};
use crate::_6_eval::kernel::{kernel_ref, semantic};
use crate::_6_eval::{Diagnostic, Program, Row, Term, TermId, Trace, Universe};
use crate::_7_effect::Slice;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Cadence {
    /// One answer per application, never asked again.
    Once,
    /// Armed by an application, then rows arrive on the executor's own clock.
    Continuing,
}

/// One service answers the `effect` rows of one served relation.
pub trait IService {
    fn relation(&self) -> &str;
    fn cadence(&self) -> Cadence;
    /// Starts work for the applications this service has not seen. A
    /// synchronous service finishes here and buffers its rows; an asynchronous
    /// one spawns tasks holding plain data on the shared runtime. A service
    /// that watches the store (`http.serve` reads `http.response` rows) also
    /// sees it here: the answering loop calls with an empty `pending` once per
    /// tick.
    fn call(&mut self, u: &mut Universe, rows: &Store, pending: &[TermId]);
    /// Buffered and task-finished rows, drained without blocking. Terms are
    /// minted here, on the tick thread, so `Universe` never crosses threads.
    fn poll(&mut self, u: &mut Universe) -> Vec<Row>;
    /// Tasks started by `call` and not yet drained.
    fn in_flight(&self) -> usize {
        0
    }
    /// A Continuing service with at least one live clock.
    fn armed(&self) -> bool;
    /// Signed rows from a Continuing service's clock, retractions included.
    fn poll_signed(
        &mut self,
        u: &mut Universe,
        _timeout: Duration,
    ) -> Vec<crate::_6_eval::ir_eval::SignedRow> {
        self.poll(u)
            .into_iter()
            .map(|row| crate::_6_eval::ir_eval::SignedRow { row, weight: 1 })
            .collect()
    }
    /// Release an application whose request left the IR frontier.
    fn cancel(&mut self, _u: &Universe, _application: TermId) {}
}

/// How long one idle wait lasts before the loop polls the next armed executor.
const IDLE_SLICE: Duration = Duration::from_millis(50);

pub struct Reconciled {
    pub ticks: usize,
    pub closure: Closure,
}

/// The one answer step both loops take: new `effect` rows in, executor rows
/// out. The tick loop and the comptime round loop each hold one.
pub struct Answerer {
    executors: Vec<(TermId, Box<dyn IService>)>,
    effect: TermId,
    /// `(Relation, Application)` pairs already handed out. The comptime store is
    /// rebuilt every round, so an index into the effect table cannot be kept.
    seen: HashSet<(TermId, TermId)>,
}
impl Answerer {
    /// One executor per served name. A served name with no executor, or one
    /// the program does not declare, is a diagnostic.
    pub fn new(
        u: &mut Universe,
        names: &HashMap<String, TermId>,
        executors: Vec<Box<dyn IService>>,
    ) -> Result<Answerer, Vec<Diagnostic>> {
        let mut bound = Vec::with_capacity(executors.len());
        let mut diagnostics = Vec::new();
        for executor in executors {
            match names.get(executor.relation()) {
                Some(rel) if !bound.iter().any(|(seen, _)| seen == rel) => {
                    bound.push((*rel, executor))
                }
                Some(_) => diagnostics.push(eval_diagnostic(
                    u,
                    "served_relation_duplicate",
                    executor.relation(),
                )),
                None => diagnostics.push(eval_diagnostic(
                    u,
                    "served_relation_unknown",
                    executor.relation(),
                )),
            }
        }
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        Ok(Answerer {
            executors: bound,
            effect: kernel_ref(u, "effect"),
            seen: HashSet::new(),
        })
    }

    /// Hands each new effect row to its relation's service. A Once application
    /// whose data row already exists (a reloaded db) is answered. Returns the
    /// rows to insert and the number of applications handed out.
    pub fn answer(&mut self, u: &mut Universe, rows: &Store) -> (Vec<Row>, usize) {
        let mut requests = Vec::new();
        if let Some(table) = rows.table(self.effect) {
            for index in 0..table.len() {
                let row = table.row(index as u32);
                requests.push((row[0], row[1]));
            }
        }
        self.answer_requests(u, rows, requests)
    }
    /// Hands signed-IR request additions to services. Request removals are
    /// handled by the IR host because it owns the answer rows for each key.
    pub fn answer_requests(
        &mut self,
        u: &mut Universe,
        rows: &Store,
        requests: impl IntoIterator<Item = (TermId, TermId)>,
    ) -> (Vec<Row>, usize) {
        let mut pending: Vec<Vec<TermId>> = vec![Vec::new(); self.executors.len()];
        for (rel, application) in requests {
            let Some(slot) = self.executors.iter().position(|(r, _)| *r == rel) else {
                continue;
            };
            if !self.seen.insert((rel, application)) {
                continue;
            }
            let once = self.executors[slot].1.cadence() == Cadence::Once;
            if once && data_row_exists(u, rows, rel, application) {
                continue;
            }
            pending[slot].push(application);
        }
        let asked = pending.iter().map(Vec::len).sum();
        for ((_, service), applications) in self.executors.iter_mut().zip(pending) {
            if !applications.is_empty() {
                service.call(u, rows, &applications);
            }
        }
        let answers = self.drain(u);
        (answers, asked)
    }

    /// Rows every service has finished, drained without blocking.
    fn drain(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = Vec::new();
        for (_, service) in &mut self.executors {
            rows.extend(service.poll(u));
        }
        rows
    }

    pub fn cancel_request(&mut self, u: &Universe, relation: TermId, application: TermId) {
        self.seen.remove(&(relation, application));
        if let Some((_, executor)) = self.executors.iter_mut().find(|(rel, _)| *rel == relation) {
            executor.cancel(u, application);
        }
    }

    /// Signed rows from the Continuing services' clocks. The caller applies
    /// all returned changes in one frontier.
    pub fn poll_signed(
        &mut self,
        u: &mut Universe,
        _timeout: Duration,
    ) -> Vec<crate::_6_eval::ir_eval::SignedRow> {
        let mut changes = Vec::new();
        for (_, service) in &mut self.executors {
            if service.cadence() == Cadence::Continuing {
                changes.extend(service.poll_signed(u, Duration::ZERO));
            }
        }
        changes
    }

    pub fn armed(&self) -> bool {
        self.executors.iter().any(|(_, executor)| executor.armed())
    }

    /// Active requests whose bound arguments agree with an executor answer.
    /// The evaluator uses these keys to retract the current answer when a
    /// request later leaves the frontier.
    pub fn matching_requests(&self, u: &Universe, row: &Row) -> Vec<TermId> {
        self.seen
            .iter()
            .filter(|(relation, _)| *relation == row.rel)
            .filter_map(|(_, application)| {
                let values = application_values(u, *application)?;
                let matches = values.iter().zip(&row.args).all(|(wanted, actual)| {
                    wanted
                        .is_none_or(|wanted| u.unary(*actual, "const").unwrap_or(*actual) == wanted)
                });
                matches.then_some(*application)
            })
            .collect()
    }

    /// The served relations, for `Program.served`.
    pub fn relations(&self) -> impl Iterator<Item = TermId> + '_ {
        self.executors.iter().map(|(rel, _)| *rel)
    }
}

pub struct Reconciler {
    answerer: Answerer,
}

impl Reconciler {
    pub fn new(
        u: &mut Universe,
        names: &HashMap<String, TermId>,
        executors: Vec<Box<dyn IService>>,
    ) -> Result<Reconciler, Vec<Diagnostic>> {
        Ok(Reconciler {
            answerer: Answerer::new(u, names, executors)?,
        })
    }

    /// Runs ticks until an answer pass inserts nothing and no executor is armed,
    /// or `max_ticks` ticks after tick 0 have run.
    pub fn run(
        &mut self,
        u: &mut Universe,
        program: &Program,
        rows: &mut Store,
        mut store: Option<&mut dyn IRowStore>,
        max_ticks: usize,
        fx: &mut dyn FnMut(Trace),
    ) -> Result<Reconciled, StoreError> {
        let mut closure = evaluate_tick(u, program, rows, &mut store, fx)?;
        let mut ticks = 0;
        while closure.diagnostics.is_empty() && ticks < max_ticks {
            let (mut answers, _) = self.answerer.answer(u, rows);
            answers.extend(self.poll(u));
            if answers.is_empty() && self.armed() {
                answers = self.poll(u);
            }
            let mut new = 0;
            for row in answers {
                if rows.insert(row.rel, row.args.into_boxed_slice()) {
                    new += 1;
                }
            }
            tracing::debug!(target: "dl8::reconcile", tick = ticks + 1, new);
            if new == 0 {
                if self.armed() {
                    continue;
                }
                break;
            }
            closure = evaluate_tick(u, program, rows, &mut store, fx)?;
            ticks += 1;
        }
        Ok(Reconciled { ticks, closure })
    }

    pub fn run_frontiers(
        &mut self,
        u: &mut Universe,
        program: &Program,
        backend: crate::_6_eval::ir_eval::Backend,
        frontiers: &[Vec<crate::_6_eval::ir_eval::SignedRow>],
        public_names: &HashMap<TermId, String>,
        fx: &mut dyn FnMut(Trace),
    ) -> Result<(Closure, Vec<crate::_6_eval::ir_eval::FrontierResult>), Diagnostic> {
        crate::_6_eval::ir_eval::evaluate_ir_frontiers(
            u,
            program,
            backend,
            &mut self.answerer,
            frontiers,
            public_names,
            fx,
        )
    }

    pub fn has_continuing(&self) -> bool {
        self.answerer
            .executors
            .iter()
            .any(|(_, executor)| executor.cadence() == Cadence::Continuing)
    }

    pub fn run_live(
        &mut self,
        u: &mut Universe,
        program: &Program,
        backend: crate::_6_eval::ir_eval::Backend,
        public_names: &HashMap<TermId, String>,
        max_ticks: usize,
        external: &mut dyn FnMut(&mut Universe) -> Option<Vec<crate::_6_eval::ir_eval::SignedRow>>,
        on_settle: &mut dyn FnMut(
            &Universe,
            &Closure,
            &[crate::_6_eval::ir_eval::NamedDelta],
            &ivm_engine::Counters,
        ),
        fx: &mut dyn FnMut(Trace),
    ) -> Result<Closure, Diagnostic> {
        let mut poll = |answerer: &mut Answerer, u: &mut Universe, timeout: Duration| {
            let external_changes = external(u);
            if let Some(changes) = &external_changes {
                if !changes.is_empty() {
                    return external_changes;
                }
            }
            let changes = answerer.poll_signed(u, timeout);
            if !changes.is_empty() {
                return Some(changes);
            }
            if !answerer.armed() && external_changes.is_none() {
                None
            } else {
                Some(Vec::new())
            }
        };
        crate::_6_eval::ir_eval::evaluate_ir_live(
            u,
            program,
            backend,
            &mut self.answerer,
            public_names,
            max_ticks,
            &mut poll,
            on_settle,
            fx,
        )
    }

    fn armed(&self) -> bool {
        self.answerer.executors.iter().any(|(_, e)| e.armed())
    }

    fn poll(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = Vec::new();
        for (_, service) in &mut self.answerer.executors {
            if service.cadence() == Cadence::Continuing {
                rows.extend(service.poll(u));
            }
        }
        rows
    }
}

fn eval_diagnostic(u: &mut Universe, reason: &str, name: &str) -> Diagnostic {
    let atom = u.atom(name);
    let payload = u.compound(reason, vec![atom]);
    Diagnostic {
        phase: "eval",
        payload,
    }
}

fn evaluate_tick(
    u: &mut Universe,
    program: &Program,
    rows: &mut Store,
    store: &mut Option<&mut dyn IRowStore>,
    fx: &mut dyn FnMut(Trace),
) -> Result<Closure, StoreError> {
    rows.mark_all();
    let closure = Evaluate::reduce(rows, (u, program), fx);
    if let Some(store) = store.as_mut() {
        let written = persist(&mut **store, u, rows)?;
        tracing::info!(target: "dl8::store", phase = "commit", rows = written);
    }
    Ok(closure)
}

/// One transaction per tick: the arena and the rows never disagree after a kill.
pub fn persist(store: &mut dyn IRowStore, u: &Universe, rows: &Store) -> Result<usize, StoreError> {
    store.begin_tick()?;
    let from = store.watermark();
    let written = store
        .commit_arena(u, from)
        .and_then(|_| store.commit_rows(u, rows));
    match written {
        Ok(written) => {
            store.commit_tick()?;
            Ok(written)
        }
        Err(e) => {
            let _ = store.rollback_tick();
            Err(e)
        }
    }
}

/// The plain values of `ref(application(Relation, Values))`, with the `none`
/// the evaluator writes for an unbound position read back as `None`.
pub fn application_values(u: &Universe, application: TermId) -> Option<Vec<Option<TermId>>> {
    let inner = u.unary(application, "ref")?;
    let [_, values] = u.args::<2>(inner, "application")?;
    let values = u.as_list(values)?;
    Some(
        values
            .into_iter()
            .map(|value| match u.get(value) {
                Term::Atom(s) if u.sym_str(*s) == "none" => None,
                _ => Some(value),
            })
            .collect(),
    )
}

/// A row of `rel` agreeing with every bound position of the application.
fn data_row_exists(u: &Universe, rows: &Store, rel: TermId, application: TermId) -> bool {
    let (Some(table), Some(values)) = (rows.table(rel), application_values(u, application)) else {
        return false;
    };
    let probe = values
        .iter()
        .enumerate()
        .find_map(|(column, value)| value.map(|v| (column, v)));
    let candidates = match probe {
        Some((column, value)) => tagged(u, value)
            .into_iter()
            .flat_map(|cell| table.candidates(&[(column, cell)], crate::_6_eval::table::Range::All))
            .collect::<Vec<_>>(),
        None => table.candidates(&[], crate::_6_eval::table::Range::All),
    };
    candidates.into_iter().any(|id| {
        let row = table.row(id);
        row.len() == values.len()
            && row
                .iter()
                .zip(&values)
                .all(|(cell, value)| value.is_none_or(|v| semantic(u, *cell) == Some(v)))
    })
}

/// The `const(Value)` and `ref(Value)` cells already in the arena; looking them
/// up never mints a term.
fn tagged(u: &Universe, value: TermId) -> Vec<TermId> {
    ["const", "ref"]
        .iter()
        .filter_map(|name| {
            let sym = u.syms.get_index_of(*name)?;
            let term = Term::Compound(crate::_6_eval::Sym(sym as u32), vec![value]);
            u.terms.get_index_of(&term).map(|i| TermId(i as u32))
        })
        .collect()
}
