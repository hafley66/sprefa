//! The tick loop over a served program. Tick 0 evaluates the program as it
//! stands; tick N inserts executor answers, evaluates, and persists.

use super::store::{IRowStore, StoreError};
use crate::_6_eval::evaluate::{Closure, Evaluate, Store};
use crate::_6_eval::kernel::{kernel_ref, semantic};
use crate::_6_eval::{Diagnostic, Program, Row, Term, TermId, Trace, Universe};
use crate::_7_effect::Slice;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Cadence {
    /// One answer per application, never asked again.
    Once,
    /// Armed by an application, then rows arrive on the executor's own clock.
    Continuing,
}

/// One executor answers the `effect` rows of one served relation.
pub trait IExecutor {
    fn relation(&self) -> &str;
    fn cadence(&self) -> Cadence;
    /// Applications this executor has not seen; returns rows to insert, possibly
    /// for another relation (`fetch_json_error`). `rows` is the closure so far.
    fn answer(&mut self, u: &mut Universe, rows: &Store, pending: &[TermId]) -> Vec<Row>;
    /// Rows that arrived on the executor's own clock. Blocks up to `timeout`
    /// when nothing has arrived; `Duration::ZERO` never blocks.
    fn poll(&mut self, u: &mut Universe, timeout: Duration) -> Vec<Row>;
    /// A Continuing executor with at least one live clock.
    fn armed(&self) -> bool;
}

/// How long one idle wait lasts before the loop polls the next armed executor.
const IDLE_SLICE: Duration = Duration::from_millis(50);

pub struct Reconciled {
    pub ticks: usize,
    pub closure: Closure,
}

pub struct Reconciler {
    executors: Vec<(TermId, Box<dyn IExecutor>)>,
    effect: TermId,
    /// Effect rows below this index were already handed out. The effect table
    /// is append-only and deduplicated, so each application passes once.
    effects_seen: usize,
}

impl Reconciler {
    /// One executor per served name. A served name with no executor, or one
    /// the program does not declare, is a diagnostic.
    pub fn new(
        u: &mut Universe,
        names: &HashMap<String, TermId>,
        executors: Vec<Box<dyn IExecutor>>,
    ) -> Result<Reconciler, Vec<Diagnostic>> {
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
        Ok(Reconciler {
            executors: bound,
            effect: kernel_ref(u, "effect"),
            effects_seen: 0,
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
            let mut answers = self.answer(u, rows);
            answers.extend(self.poll(u, Duration::ZERO));
            if answers.is_empty() && self.armed() {
                answers = self.poll(u, IDLE_SLICE);
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

    fn armed(&self) -> bool {
        self.executors.iter().any(|(_, e)| e.armed())
    }

    /// Hands each new effect row to its relation's executor. A Once application
    /// whose data row already exists (a reloaded db) is answered.
    fn answer(&mut self, u: &mut Universe, rows: &Store) -> Vec<Row> {
        let mut pending: Vec<Vec<TermId>> = vec![Vec::new(); self.executors.len()];
        if let Some(table) = rows.table(self.effect) {
            for index in self.effects_seen..table.len() {
                let row = table.row(index as u32);
                let [rel, application] = [row[0], row[1]];
                let Some(slot) = self.executors.iter().position(|(r, _)| *r == rel) else {
                    continue;
                };
                let once = self.executors[slot].1.cadence() == Cadence::Once;
                if once && data_row_exists(u, rows, rel, application) {
                    continue;
                }
                pending[slot].push(application);
            }
            self.effects_seen = table.len();
        }
        let mut answers = Vec::new();
        for ((_, executor), applications) in self.executors.iter_mut().zip(pending) {
            if !applications.is_empty() {
                answers.extend(executor.answer(u, rows, &applications));
            }
        }
        answers
    }

    fn poll(&mut self, u: &mut Universe, timeout: Duration) -> Vec<Row> {
        let armed = self.executors.iter().filter(|(_, e)| e.armed()).count();
        let slice = timeout / armed.max(1) as u32;
        let mut rows = Vec::new();
        for (_, executor) in self.executors.iter_mut() {
            if executor.cadence() == Cadence::Continuing {
                rows.extend(executor.poll(u, slice));
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
