//! The checked-goal program shape from v7: `rule(call(Rel, Args), [checked_goal(Polarity, call(Rel, Args))])`.
//! Relations are terms (`ref(source)`, `ref(kernel(cons))`). Arguments are a
//! variable, a ground term, `aggregate(Kind, Arg)` or `fold(Step, Seed, Arg)`
//! in a head, where `Kind` is one of `count`, `sum`, `min`, `max`.

use super::kernel::op_ref;
use super::term::{TermId, Universe};
use std::collections::HashSet;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct VarId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AggregateKind {
    Count,
    Sum,
    Min,
    Max,
}

impl AggregateKind {
    pub fn of(name: &str) -> Option<Self> {
        match name {
            "count" => Some(Self::Count),
            "sum" => Some(Self::Sum),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Sum => "sum",
            Self::Min => "min",
            Self::Max => "max",
        }
    }

    /// The step and seed each builtin folds with; `fold_group` is the Rust
    /// fast path for the same four and must agree with folding these.
    pub fn as_fold(self, u: &mut Universe) -> Fold {
        let (step, seed) = match self {
            Self::Count => ((None, "count_step"), Seed::Zero),
            Self::Sum => ((Some("int"), "add"), Seed::Zero),
            Self::Min => ((None, "min_step"), Seed::FirstValue),
            Self::Max => ((None, "max_step"), Seed::FirstValue),
        };
        Fold {
            step: op_ref(u, step),
            seed,
            order: Order::TermLt,
        }
    }
}

/// The order values are visited in, so a step that is not commutative still
/// folds to one answer.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Order {
    TermLt,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seed {
    /// `const(0)`, minted at fold time.
    Zero,
    /// The first value in `order`; the rest of the group folds onto it.
    FirstValue,
    Term(TermId),
}

/// `step` is called as `(Step Acc Value Next)` and must yield exactly one
/// `Next` per value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fold {
    pub step: TermId,
    pub seed: Seed,
    pub order: Order,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Arg {
    Var(VarId),
    Ground(TermId),
    Aggregate(AggregateKind, Box<Arg>),
    Fold(Fold, Box<Arg>),
}

/// Which fold a head position carries: one of the four builtins, or a fold the
/// program declared.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Folding {
    Builtin(AggregateKind),
    Declared(Fold),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Polarity {
    Positive,
    Negative,
}

#[derive(Clone, Debug)]
pub struct Goal {
    pub polarity: Polarity,
    pub rel: TermId,
    pub args: Vec<Arg>,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub rel: TermId,
    pub head: Vec<Arg>,
    pub body: Vec<Goal>,
    /// Variable identities in first-occurrence order; `VarId(i)` indexes here.
    pub vars: Vec<TermId>,
}

impl Rule {
    pub fn is_aggregate(&self) -> bool {
        self.head
            .iter()
            .any(|a| matches!(a, Arg::Aggregate(..) | Arg::Fold(..)))
    }

    pub fn aggregate_args(&self) -> usize {
        self.head
            .iter()
            .filter(|a| matches!(a, Arg::Aggregate(..) | Arg::Fold(..)))
            .count()
    }

    /// Position, folding and subject argument of the single folding head,
    /// when the head carries one.
    pub fn folding_head(&self) -> Option<(usize, Folding, &Arg)> {
        self.head
            .iter()
            .enumerate()
            .find_map(|(position, a)| match a {
                Arg::Aggregate(aggregation, subject) => {
                    Some((position, Folding::Builtin(*aggregation), subject.as_ref()))
                }
                Arg::Fold(fold, subject) => {
                    Some((position, Folding::Declared(*fold), subject.as_ref()))
                }
                _ => None,
            })
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Row {
    pub rel: TermId,
    pub args: Vec<TermId>,
}

/// `diagnostic(Phase, none, Payload)`; the payload is a term.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Diagnostic {
    pub phase: &'static str,
    pub payload: TermId,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub rules: Vec<Rule>,
    pub seeds: Vec<Row>,
    /// Relation refs the outside settles; a miss on one writes an `effect` row.
    pub served: HashSet<TermId>,
}
