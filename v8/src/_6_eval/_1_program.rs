//! The checked-goal program shape from v7: `rule(call(Rel, Args), [checked_goal(Polarity, call(Rel, Args))])`.
//! Relations are terms (`ref(source)`, `ref(kernel(cons))`). Arguments are a
//! variable, a ground term, or `aggregate(count, Arg)` in a head.

use super::term::TermId;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct VarId(pub u32);

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Arg {
    Var(VarId),
    Ground(TermId),
    Count(Box<Arg>),
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
        self.head.iter().any(|a| matches!(a, Arg::Count(_)))
    }

    pub fn count_args(&self) -> usize {
        self.head
            .iter()
            .filter(|a| matches!(a, Arg::Count(_)))
            .count()
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
}
