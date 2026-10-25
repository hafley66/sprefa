//! The traits between the ends. Each impl is picked by enum dispatch from the `Recipe`.

use crate::_0_id::Dict;
pub use crate::_2a_diag::Diag;
use crate::_3_plan::{RelId, Row};

pub type Event = u64;
pub type Diff = isize;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Change {
    pub rel: RelId,
    pub row: Row,
    pub diff: Diff,
}

/// Consolidated and sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    pub event: Event,
    pub changes: Vec<Change>,
}

/// A fixpoint that has not settled after `max_rounds` rounds is `Diag::FixpointLimit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub max_rounds: u32,
}

/// One SQL value.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Cell>>,
}

pub trait Engine {
    /// Base and Answer row changes in; every relation's changes at this event out.
    /// Err leaves the engine as it was before the call.
    fn step(&mut self, input: Vec<Change>) -> Result<Delta, Diag>;
}

pub trait Store {
    /// Base and Answer rows stored, with the dict entries they use.
    fn replay(&mut self, dict: &mut Dict) -> Result<Vec<Change>, Diag>;
    /// One event's delta, one transaction, one statement per table and sign; `delta.event`
    /// becomes the stored last event in the same transaction.
    fn write(&mut self, delta: &Delta, fresh: &[(u64, &[u8])]) -> Result<usize, Diag>;
    /// Read-only SQL over the relation tables and their `<rel>_text` views; at most a bounded
    /// row count and run time (`Diag::QueryRows`, `Diag::QueryTime`).
    fn query(&self, sql: &str) -> Result<Table, Diag>;
    /// The `rows` of `rel` that are stored, in one statement.
    fn present(&self, rel: RelId, rows: &[&Row]) -> Result<Vec<Row>, Diag>;
    /// Every stored row of `rel`.
    fn rows(&self, rel: RelId) -> Result<Vec<Row>, Diag>;
    /// The last event written; 0 when nothing was ever written.
    fn event(&self) -> Event;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    Once,
    Continuing,
}

pub trait IService {
    fn service(&self) -> &str;
    fn cadence(&self) -> Cadence;
    /// Every request change of one event at once.
    fn call(&mut self, requests: &[Change], dict: &Dict);
    /// Answer rows ready now (Answer rel changes); never blocks.
    fn poll(&mut self, dict: &mut Dict, out: &mut Vec<Change>);
    /// The stored Answer rows, when the runtime opens the store and after a commit fails. They
    /// replace the service's memory; answers and errors not yet polled are dropped. A service
    /// that answers a re-ask with a difference starts its memory here.
    fn seed(&mut self, _answers: &[Change]) {}
    /// Failures since the last drain; the runtime drains after every `poll` and stops the commit
    /// with `Diag::Service`.
    fn errors(&mut self) -> Vec<String> {
        Vec::new()
    }
}
