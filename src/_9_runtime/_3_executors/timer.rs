//! `(timer ?PeriodMs ?Tick)`: a source. A goal with the period bound arms one
//! clock per distinct period on a single thread; ticks count from 1 per period.

use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, Term, TermId, Universe};
use crate::_9_runtime::reconcile::{application_values, Cadence, IExecutor};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const RELATION: &str = "timer";

pub struct Timer {
    relation: TermId,
    /// Next tick number per armed period.
    next: BTreeMap<i64, i64>,
    clock: Option<Clock>,
}

/// The timer thread. Dropping `arm` disconnects it, and `Drop` joins it.
struct Clock {
    arm: Option<Sender<u64>>,
    fires: Receiver<u64>,
    thread: Option<JoinHandle<()>>,
}

impl Clock {
    fn start() -> Clock {
        let (arm, periods) = channel::<u64>();
        let (fire, fires) = channel::<u64>();
        let thread = std::thread::spawn(move || tick_forever(periods, fire));
        Clock {
            arm: Some(arm),
            fires,
            thread: Some(thread),
        }
    }
}

impl Drop for Clock {
    fn drop(&mut self) {
        self.arm.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A fire missed while the loop was late is skipped, never replayed, so a slow
/// tick never produces a burst.
fn tick_forever(periods: Receiver<u64>, fire: Sender<u64>) {
    let mut due: BTreeMap<u64, Instant> = BTreeMap::new();
    loop {
        let command = match due.values().min() {
            None => periods.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(earliest) => {
                periods.recv_timeout(earliest.saturating_duration_since(Instant::now()))
            }
        };
        match command {
            Ok(period) => {
                due.entry(period)
                    .or_insert_with(|| Instant::now() + Duration::from_millis(period));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        for (period, at) in due.iter_mut() {
            if *at <= now {
                if fire.send(*period).is_err() {
                    return;
                }
                *at = now + Duration::from_millis(*period);
            }
        }
    }
}

impl Timer {
    pub fn new(relation: TermId) -> Timer {
        Timer {
            relation,
            next: BTreeMap::new(),
            clock: None,
        }
    }

    /// A reloaded db already holds ticks; numbering continues past them.
    fn first_tick(&self, u: &Universe, rows: &Store, period: i64) -> i64 {
        let int = |cell: TermId| u.unary(cell, "const").and_then(|v| u.as_int(v));
        rows.table(self.relation)
            .map(|table| {
                table
                    .rows
                    .iter()
                    .filter(|row| row.len() == 2 && int(row[0]) == Some(period))
                    .filter_map(|row| int(row[1]))
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0)
            + 1
    }

    fn rows_for(&mut self, u: &mut Universe, fired: BTreeSet<u64>) -> Vec<Row> {
        let mut rows = Vec::with_capacity(fired.len());
        for period in fired {
            let Some(next) = self.next.get_mut(&(period as i64)) else {
                continue;
            };
            let tick = *next;
            *next += 1;
            let period = u.int(period as i64);
            let tick = u.int(tick);
            rows.push(Row {
                rel: self.relation,
                args: vec![
                    u.compound("const", vec![period]),
                    u.compound("const", vec![tick]),
                ],
            });
        }
        rows
    }
}

impl IExecutor for Timer {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Continuing
    }

    fn answer(&mut self, u: &mut Universe, rows: &Store, pending: &[TermId]) -> Vec<Row> {
        for &application in pending {
            let period = application_values(u, application)
                .and_then(|values| values.first().copied().flatten())
                .and_then(|value| match u.get(value) {
                    Term::Int(period) if *period > 0 => Some(*period),
                    _ => None,
                });
            let Some(period) = period else {
                continue;
            };
            if self.next.contains_key(&period) {
                continue;
            }
            let first = self.first_tick(u, rows, period);
            self.next.insert(period, first);
            let clock = self.clock.get_or_insert_with(Clock::start);
            if let Some(arm) = &clock.arm {
                let _ = arm.send(period as u64);
            }
            tracing::info!(target: "dl8::timer", period, first, "armed");
        }
        Vec::new()
    }

    fn poll(&mut self, u: &mut Universe, timeout: Duration) -> Vec<Row> {
        let Some(clock) = &self.clock else {
            return Vec::new();
        };
        let mut fired: BTreeSet<u64> = clock.fires.try_iter().collect();
        if fired.is_empty() && !timeout.is_zero() {
            if let Ok(period) = clock.fires.recv_timeout(timeout) {
                fired.insert(period);
                fired.extend(clock.fires.try_iter());
            }
        }
        self.rows_for(u, fired)
    }

    fn armed(&self) -> bool {
        !self.next.is_empty()
    }
}
