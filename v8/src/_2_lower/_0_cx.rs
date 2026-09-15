//! The lowering frame: the term arena plus the two indexes and the relation
//! dictionary, all built once per `lower_datalog/5` boundary.

use super::index::{EdgeIndex, ReservationIndex};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashMap;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CallPolicy {
    Strict,
    DeferUnknownCalls,
}

pub struct Cx<'a> {
    pub u: &'a mut Universe,
    pub reservations: &'a ReservationIndex,
    pub edges: &'a EdgeIndex,
    /// `memberchk(relation(Callable, Arity, KeySets), Relations)`: first wins.
    pub relations: &'a HashMap<TermId, (i64, TermId)>,
    pub policy: CallPolicy,
}

impl Cx<'_> {
    pub fn atom(&mut self, name: &str) -> TermId {
        self.u.atom(name)
    }
    pub fn int(&mut self, value: i64) -> TermId {
        self.u.int(value)
    }
    pub fn compound(&mut self, name: &str, args: Vec<TermId>) -> TermId {
        self.u.compound(name, args)
    }
    pub fn diagnostic(&mut self, node: TermId, reason: TermId) -> TermId {
        let lower = self.u.atom("lower");
        self.u.compound("diagnostic", vec![lower, node, reason])
    }
    pub fn plain(&mut self, node: TermId, reason: &str) -> TermId {
        let r = self.u.atom(reason);
        self.diagnostic(node, r)
    }
    /// `:1040`. Only an unknown relation is deferrable.
    pub fn deferrable(&self, diagnostic: TermId) -> bool {
        let Some((name, args)) = self.u.functor(diagnostic) else {
            return false;
        };
        if name != "diagnostic" || args.len() != 3 {
            return false;
        }
        self.u.functor(args[2]).is_some_and(|(r, a)| {
            a.len() == 1 && (r == "undeclared_relation" || r == "not_relation")
        })
    }
    /// `:661`. Structural rewrite of every occurrence of `from`.
    pub fn replace(&mut self, from: TermId, to: TermId, term: TermId) -> TermId {
        if term == from {
            return to;
        }
        let Some((name, args)) = self.u.functor(term) else {
            return term;
        };
        let name = name.to_string();
        let args = args.to_vec();
        let replaced: Vec<TermId> = args
            .into_iter()
            .map(|arg| self.replace(from, to, arg))
            .collect();
        self.u.compound(&name, replaced)
    }
}
