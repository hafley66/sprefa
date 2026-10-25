//! Every named failure of every end. `rule` and `goal` are indexes into `Program::rules` and
//! `Rule::body`; `goal: None` is the head.

use crate::_2_ir::{Col, RelKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diag {
    /// `what` is "rel" or "service".
    Unknown {
        what: &'static str,
        name: String,
        rule: Option<usize>,
    },
    /// `what` is "rel", "column", "request service" or "answer service".
    Duplicate {
        what: &'static str,
        name: String,
    },
    /// `name` is a rel or a builtin.
    Arity {
        rule: usize,
        name: String,
        expected: usize,
        found: usize,
    },
    /// `term` is a var name or a constant as written.
    Type {
        rule: usize,
        term: String,
        expected: Col,
        found: Col,
    },
    Unbound {
        rule: usize,
        goal: Option<usize>,
        var: String,
    },
    /// A rule head into a Base or Answer rel.
    HeadKind {
        rule: usize,
        rel: String,
        kind: RelKind,
    },
    /// The Answer rel's cols do not start with its Request rel's cols.
    AnswerShape {
        answer: String,
        request: String,
    },
    NegationCycle {
        rule: usize,
        head: String,
        negated: String,
    },
    FixpointLimit {
        stratum: usize,
        max_rounds: u32,
    },
    /// The dataflow did not pass `time` within `steps` worker steps.
    StepLimit {
        time: u64,
        steps: usize,
    },
    Mismatch {
        stored_recipe: String,
        stored_hash: u64,
        recipe: String,
        hash: u64,
    },
    Store(String),
    /// A client change the runtime does not take; `change` indexes the commit's changes.
    /// `problem` is "unknown rel", "kind", "arity" or "cell type".
    Commit {
        change: usize,
        rel: String,
        problem: &'static str,
        found: String,
    },
    /// A commit's request, answer, request chain had answers left after `max_rounds` rounds.
    ServiceRounds {
        max_rounds: u32,
    },
    Service {
        service: String,
        message: String,
    },
    /// `problem` is "unknown key", "unknown value", "duplicate key" or "no '='".
    Recipe {
        problem: &'static str,
        found: String,
        allowed: &'static [&'static str],
    },
}
