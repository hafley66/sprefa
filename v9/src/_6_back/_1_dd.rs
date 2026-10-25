//! `Engine` on differential dataflow. One worker, one dataflow per plan, one input for Base and
//! Answer rows. Outer time `t` is the input epoch; recursive strata loop on `Product<t, round>`.
//!
//! An update's round is the lattice join of the rounds it was derived from, so work at epoch `t`
//! that joins rows of earlier epochs starts at their round: a recursion through a service climbs
//! a few rounds per epoch, forever. `Bounds::max_rounds` therefore counts the distinct rounds a
//! stratum's loop updates at one epoch, and leaves the round numbers alone.

use crate::_2_ir::RelKind;
use crate::_3_plan::{PGoal, Plan, RelId, Row, Stratum};
use crate::_5_ends::{Bounds, Change, Delta, Diag, Engine, Event};
use crate::_6_back::_0_rule::{lower, set, Rows};
use differential_dataflow::collection::concatenate;
use differential_dataflow::input::{Input, InputSession};
use differential_dataflow::lattice::Lattice;
use differential_dataflow::operators::iterate::Variable;
use differential_dataflow::Collection;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use timely::communication::allocator::thread::Thread;
use timely::communication::Allocator;
use timely::dataflow::operators::vec::Filter;
use timely::dataflow::{ProbeHandle, Scope};
use timely::order::Product;
use timely::progress::Timestamp;
use timely::worker::Worker;
use timely::WorkerConfig;

/// The input rel id of the one empty row every rule body starts from.
const UNIT: RelId = RelId::MAX;
/// Worker steps per epoch; protects against a dataflow that never passes the probe.
const MAX_WORKER_STEPS: usize = 1 << 22;
/// Default `Bounds::max_rounds`: rounds one recursive stratum may update in one epoch. Protects
/// against a recursion that derives new rows every round (a counter); one epoch's legit depth is
/// its longest new derivation chain.
pub const MAX_ROUNDS: u32 = 1 << 16;

type Updates = Rc<RefCell<Vec<(u64, RelId, Row, isize)>>>;

/// Rounds each recursive stratum's loop updated, per epoch; shared by the loops' filters.
#[derive(Default)]
pub struct Rounds {
    /// `(stratum, outer time)` -> rounds let through the loop at that epoch, at most
    /// `bounds.max_rounds`. A round once let through stays let through; a round refused stays
    /// refused, so each `Product` time is kept or dropped whole.
    pub admitted: HashMap<(usize, u64), BTreeSet<u64>>,
    /// `(limited, undo)`: the undo epoch lets through exactly the rounds the limited epoch did,
    /// so its updates cancel the limited epoch's on both sides of the filter.
    pub mirror: Option<(u64, u64)>,
    /// `(outer time, stratum)` of each update refused.
    pub limits: Vec<(u64, usize)>,
}

impl Rounds {
    /// Lets `(outer, inner)` through stratum `index`'s loop or records a limit.
    fn admit(&mut self, index: usize, outer: u64, inner: u64, max: usize) -> bool {
        let (key, open) = match self.mirror {
            Some((limited, undo)) if undo == outer => (limited, false),
            _ => (outer, true),
        };
        let rounds = self.admitted.entry((index, key)).or_default();
        if rounds.contains(&inner) {
            return true;
        }
        if open && rounds.len() < max {
            rounds.insert(inner);
            return true;
        }
        self.limits.push((outer, index));
        false
    }

    fn clear(&mut self) {
        self.admitted.clear();
        self.mirror = None;
        self.limits.clear();
    }
}

pub struct Dd {
    pub worker: Worker,
    pub input: InputSession<u64, (RelId, Row), isize>,
    pub probe: ProbeHandle<u64>,
    /// Every rel's output updates not yet taken by a step.
    pub out: Updates,
    pub rounds: Rc<RefCell<Rounds>>,
    pub bounds: Bounds,
    pub event: Event,
    /// Rows the dataflow derived before any input (rules with constant bodies); the first
    /// successful step reports them.
    pub held: Vec<Change>,
    /// Set when the worker missed its step budget mid epoch; every later step returns it.
    pub broken: Option<Diag>,
}

impl Dd {
    pub fn new(plan: &Plan, bounds: Bounds) -> Result<Dd, Diag> {
        let mut worker = Worker::new(
            WorkerConfig::default(),
            Allocator::Thread(Thread::default()),
            None,
        );
        let probe = ProbeHandle::new();
        let out: Updates = Rc::default();
        let rounds = Rc::default();
        let input = worker.dataflow::<u64, _, _>(|scope| {
            let (input, all) = scope.new_collection::<(RelId, Row), isize>();
            let rels = build(plan, bounds, &rounds, scope, all);
            for (id, rows) in rels.into_iter().enumerate() {
                let out = Rc::clone(&out);
                let id = id as RelId;
                rows.inspect(move |(row, time, diff)| {
                    out.borrow_mut().push((*time, id, row.clone(), *diff))
                })
                .probe_with(&probe);
            }
            input
        });
        let mut dd = Dd {
            worker,
            input,
            probe,
            out,
            rounds,
            bounds,
            event: 0,
            held: Vec::new(),
            broken: None,
        };
        dd.input.insert((UNIT, Row::new()));
        let time = dd.advance()?;
        dd.held = dd.take(time, Vec::new());
        dd.rounds.borrow_mut().clear();
        Ok(dd)
    }

    /// Closes the current epoch and steps until the probe passes it.
    fn advance(&mut self) -> Result<u64, Diag> {
        let time = *self.input.time();
        self.input.advance_to(time + 1);
        self.input.flush();
        for _ in 0..MAX_WORKER_STEPS {
            if !self.probe.less_than(self.input.time()) {
                return Ok(time);
            }
            self.worker.step();
        }
        let diag = Diag::StepLimit {
            time,
            steps: MAX_WORKER_STEPS,
        };
        self.broken = Some(diag.clone());
        Err(diag)
    }

    /// Output updates at `time` plus `held`, consolidated and sorted.
    fn take(&mut self, time: u64, held: Vec<Change>) -> Vec<Change> {
        let mut sums: BTreeMap<(RelId, Row), isize> = held
            .into_iter()
            .map(|change| ((change.rel, change.row), change.diff))
            .collect();
        for (at, rel, row, diff) in self.out.borrow_mut().drain(..) {
            debug_assert_eq!(at, time, "an update outside the settled epoch");
            *sums.entry((rel, row)).or_default() += diff;
        }
        sums.into_iter()
            .filter(|(_, diff)| *diff != 0)
            .map(|((rel, row), diff)| Change { rel, row, diff })
            .collect()
    }
}

impl Engine for Dd {
    fn step(&mut self, input: Vec<Change>) -> Result<Delta, Diag> {
        if let Some(diag) = &self.broken {
            return Err(diag.clone());
        }
        for change in &input {
            self.input
                .update((change.rel, change.row.clone()), change.diff);
        }
        let time = self.advance()?;
        let changes = self.take(time, Vec::new());
        let limit = self
            .rounds
            .borrow()
            .limits
            .iter()
            .filter(|(at, _)| *at == time)
            .map(|(_, stratum)| *stratum)
            .min();
        if let Some(stratum) = limit {
            self.rounds.borrow_mut().mirror = Some((time, *self.input.time()));
            for change in input {
                self.input.update((change.rel, change.row), -change.diff);
            }
            let undo = self.advance()?;
            let undone = self.take(undo, Vec::new());
            debug_assert_eq!(
                undone,
                changes
                    .iter()
                    .map(|change| Change {
                        diff: -change.diff,
                        ..change.clone()
                    })
                    .collect::<Vec<_>>(),
                "rollback undoes the limited epoch"
            );
            self.rounds.borrow_mut().clear();
            return Err(Diag::FixpointLimit {
                stratum,
                max_rounds: self.bounds.max_rounds,
            });
        }
        self.rounds.borrow_mut().clear();
        self.event += 1;
        let held = std::mem::take(&mut self.held);
        let changes = if held.is_empty() {
            changes
        } else {
            self.take(time, [held, changes].concat())
        };
        Ok(Delta {
            event: self.event,
            changes,
        })
    }
}

/// Every rel's visible rows, indexed by `RelId`.
fn build<'s>(
    plan: &Plan,
    bounds: Bounds,
    rounds: &Rc<RefCell<Rounds>>,
    scope: Scope<'s, u64>,
    all: differential_dataflow::VecCollection<'s, u64, (RelId, Row)>,
) -> Vec<Rows<'s, u64>> {
    let pick = |id: RelId| {
        all.clone()
            .filter(move |(rel, _)| *rel == id)
            .map(|(_, row)| row)
    };
    let unit = pick(UNIT);
    let mut rels: Vec<Option<Rows<'s, u64>>> = vec![None; plan.rels.len()];
    for (id, rel) in plan.rels.iter().enumerate() {
        rels[id] = match rel.kind {
            RelKind::Base => Some(set(pick(id as RelId))),
            RelKind::Answer { .. } => Some(set(pick(id as RelId))),
            _ => None,
        };
    }
    let mut answers: HashMap<RelId, Rows<'s, u64>> = HashMap::new();
    for (id, rel) in plan.rels.iter().enumerate() {
        if let RelKind::Answer { .. } = rel.kind {
            answers.insert(id as RelId, rels[id].take().expect("answer input"));
        }
    }
    for (index, stratum) in plan.strata.iter().enumerate() {
        let reads = reads(plan, stratum);
        if !stratum.recursive {
            let env: HashMap<RelId, Rows<'s, u64>> = reads
                .iter()
                .map(|id| {
                    (
                        *id,
                        rels[*id as usize].clone().expect("read before written"),
                    )
                })
                .collect();
            for &id in &stratum.rels {
                let answer = answers.get(&id).cloned();
                rels[id as usize] =
                    Some(define(plan, id, stratum, &env, unit.clone(), answer, scope));
            }
            continue;
        }
        let entered: Vec<(RelId, Rows<'s, u64>)> = reads
            .iter()
            .map(|id| {
                (
                    *id,
                    rels[*id as usize].clone().expect("read before written"),
                )
            })
            .collect();
        let answers = &answers;
        let unit = unit.clone();
        let max = bounds.max_rounds as usize;
        let left = scope.iterative::<u64, _, _>(|inner| {
            let mut env: HashMap<RelId, Rows<'_, Product<u64, u64>>> = entered
                .into_iter()
                .map(|(id, rows)| (id, rows.enter(inner)))
                .collect();
            let unit = unit.enter(inner);
            let mut variables = Vec::new();
            for &id in &stratum.rels {
                let (variable, rows) = Variable::new(inner, Product::new(0, 1));
                env.insert(id, rows);
                variables.push((id, variable));
            }
            let mut left = Vec::new();
            for (id, variable) in variables {
                let answer = answers.get(&id).map(|rows| rows.clone().enter(inner));
                let rounds = Rc::clone(rounds);
                let result = define(plan, id, stratum, &env, unit.clone(), answer, inner);
                variable.set(Collection::new(result.inner.clone().filter(
                    move |(_, time, _)| {
                        rounds
                            .borrow_mut()
                            .admit(index, time.outer, time.inner, max)
                    },
                )));
                left.push((id, result.leave(scope)));
            }
            left
        });
        for (id, rows) in left {
            rels[id as usize] = Some(rows);
        }
    }
    rels.into_iter()
        .enumerate()
        .map(|(id, rows)| rows.unwrap_or_else(|| panic!("rel {id} in no stratum")))
        .collect()
}

/// Rels a stratum's rules and Answer rels read from earlier strata or Base.
fn reads(plan: &Plan, stratum: &Stratum) -> BTreeSet<RelId> {
    let mut reads = BTreeSet::new();
    for &rule in &stratum.rules {
        for goal in &plan.rules[rule].body {
            match goal {
                PGoal::Pos(atom) | PGoal::Neg(atom) => {
                    reads.insert(atom.rel);
                }
                PGoal::Call(..) => {}
            }
        }
    }
    for &id in &stratum.rels {
        reads.extend(plan.rels[id as usize].request);
    }
    for id in &stratum.rels {
        reads.remove(id);
    }
    reads
}

/// An Answer rel is its input semijoined on its Request rel's cols; any other rel is the set of
/// its rules' heads.
fn define<'s, T: Timestamp + Lattice + Ord>(
    plan: &Plan,
    id: RelId,
    stratum: &Stratum,
    env: &HashMap<RelId, Rows<'s, T>>,
    unit: Rows<'s, T>,
    answer: Option<Rows<'s, T>>,
    scope: Scope<'s, T>,
) -> Rows<'s, T> {
    if let (Some(answer), Some(request)) = (answer, plan.rels[id as usize].request) {
        let width = plan.rels[request as usize].cols.len();
        return answer
            .map(move |row| (row[..width].to_vec(), row))
            .semijoin(env[&request].clone())
            .map(|(_, row)| row);
    }
    let heads = stratum
        .rules
        .iter()
        .filter(|rule| plan.rules[**rule].head.rel == id)
        .map(|rule| lower(&plan.rules[*rule], env, unit.clone()));
    set(concatenate(scope, heads))
}
