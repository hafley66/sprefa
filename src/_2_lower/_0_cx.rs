//! The lowering frame: the term arena plus the two indexes, built once per
//! `lower_datalog/5` boundary.

use super::index::{EdgeIndex, ReservationIndex};
use super::kernel::{kernel_relation, kernel_variadic};
use crate::_6_eval::term::{TermId, Universe};

/// The prelude relations compiler-generated rules call.
pub const PRELUDE_RELATIONS: [&str; 6] =
    ["obligation", "discharged", "Apply", "PartialCall", "Literal", "Curry"];

/// Each of `PRELUDE_RELATIONS` as the callable node of the prelude module's own
/// edge, resolved once per lowering frame. A program's declarations never reach it.
#[derive(Default)]
pub struct Prelude {
    pub callables: Vec<(&'static str, TermId)>,
}

impl Prelude {
    pub fn resolve(u: &mut Universe, edges: &EdgeIndex) -> Self {
        let prelude = u.atom("prelude");
        let module = u.compound("module", vec![prelude]);
        let mut callables = Vec::with_capacity(PRELUDE_RELATIONS.len());
        for name in PRELUDE_RELATIONS {
            let atom = u.atom(name);
            let Some(edge) = edges.forward(module, atom) else {
                continue;
            };
            if let Some(callable) = u.unary(edge.target, "target") {
                callables.push((name, callable));
            }
        }
        Prelude { callables }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CallPolicy {
    Strict,
    DeferUnknownCalls,
}

pub struct Cx<'a> {
    pub u: &'a mut Universe,
    pub reservations: &'a ReservationIndex,
    pub edges: &'a EdgeIndex,
    pub policy: CallPolicy,
    /// Constructions with ground arguments lower to one term, not intern goals.
    pub ground_values: bool,
    pub prelude: Prelude,
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
    /// `ref(kernel(Label))`, or `ref(kernel(Owner, Label))` for a typed op: the
    /// kernel relation by identity, whatever the calling scope declares.
    pub fn kernel_ref(&mut self, owner: Option<&str>, label: &str) -> TermId {
        let label = self.u.atom(label);
        let kernel = match owner {
            Some(owner) => {
                let owner = self.u.atom(owner);
                self.u.compound("kernel", vec![owner, label])
            }
            None => self.u.compound("kernel", vec![label]),
        };
        self.u.compound("ref", vec![kernel])
    }
    /// `call(ref(kernel(..)), Arguments)`.
    pub fn kernel_call(&mut self, owner: Option<&str>, label: &str, arguments: &[TermId]) -> TermId {
        let relation = self.kernel_ref(owner, label);
        let list = self.u.list(arguments);
        self.u.compound("call", vec![relation, list])
    }
    /// `pending_goal(positive, call(ref(kernel(..)), Arguments))`.
    pub fn kernel_goal(&mut self, owner: Option<&str>, label: &str, arguments: &[TermId]) -> TermId {
        let call = self.kernel_call(owner, label, arguments);
        let positive = self.u.atom("positive");
        self.u.compound("pending_goal", vec![positive, call])
    }
    /// `call(ref(Callable), Arguments)` of a `PRELUDE_RELATIONS` entry; `Err` is
    /// the reason `missing_prelude_relation(Name)`.
    pub fn prelude_call(&mut self, name: &str, arguments: &[TermId]) -> Result<TermId, TermId> {
        let callable = self.prelude_callable(name)?;
        let relation = self.u.compound("ref", vec![callable]);
        let list = self.u.list(arguments);
        Ok(self.u.compound("call", vec![relation, list]))
    }
    /// The prelude's callable node for `name`; `Err` is `missing_prelude_relation(Name)`.
    pub fn prelude_callable(&mut self, name: &str) -> Result<TermId, TermId> {
        match self.prelude.callables.iter().find(|(n, _)| *n == name) {
            Some((_, callable)) => Ok(*callable),
            None => {
                let atom = self.u.atom(name);
                Err(self.u.compound("missing_prelude_relation", vec![atom]))
            }
        }
    }
    /// The untyped kernel label a goal callable names: `ref(kernel(Label))`, or a
    /// source-written `name(Scope, Label)` no declaration in scope shadows.
    pub fn kernel_label(&self, callable: TermId) -> Option<String> {
        if let Some(inner) = self.u.unary(callable, "ref") {
            let label = self.u.unary(inner, "kernel")?;
            return self.u.functor_or_atom(label).map(|(n, _)| n.to_string());
        }
        let [scope, label] = self.u.args::<2>(callable, "name")?;
        let text = self.u.functor_or_atom(label)?.0.to_string();
        let kernel = kernel_relation(None, &text).is_some() || kernel_variadic(None, &text);
        (kernel && self.reservations.scoped(scope, label).is_none()).then_some(text)
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
