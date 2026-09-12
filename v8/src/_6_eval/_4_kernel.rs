//! Kernel relations: `ref(kernel(Name))`. Ported from the `proves/2` clauses
//! and `integer_comparison/3` in `v7/src/1_libtime/0_evaluator.pl`. Each is a
//! function over bound arguments, never a stored table, except that `intern`
//! records every request as an output row.

use super::term::{TermId, Universe};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Kernel {
    Nil,
    Cons,
    EdgeRef,
    Intern,
    Int(IntCmp),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum IntCmp {
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
}

impl IntCmp {
    pub fn holds(self, l: i64, r: i64) -> bool {
        match self {
            IntCmp::Lt => l < r,
            IntCmp::Le => l <= r,
            IntCmp::Eq => l == r,
            IntCmp::Ne => l != r,
            IntCmp::Ge => l >= r,
            IntCmp::Gt => l > r,
        }
    }
}

impl Kernel {
    /// `ref(kernel(name))` to the kernel, `None` for every other relation.
    pub fn of(u: &Universe, rel: TermId) -> Option<Kernel> {
        let inner = u.unary(rel, "ref")?;
        let name = u.unary(inner, "kernel")?;
        let (name, args) = u.functor_or_atom(name)?;
        if !args.is_empty() {
            return None;
        }
        Some(match name {
            "nil" => Kernel::Nil,
            "cons" => Kernel::Cons,
            "edge_ref" => Kernel::EdgeRef,
            "intern" => Kernel::Intern,
            "int_lt" => Kernel::Int(IntCmp::Lt),
            "int_le" => Kernel::Int(IntCmp::Le),
            "int_eq" => Kernel::Int(IntCmp::Eq),
            "int_ne" => Kernel::Int(IntCmp::Ne),
            "int_ge" => Kernel::Int(IntCmp::Ge),
            "int_gt" => Kernel::Int(IntCmp::Gt),
            _ => return None,
        })
    }
}

/// `semantic_argument/2`: strip the `ref`/`const` tag.
pub fn semantic(u: &Universe, tagged: TermId) -> Option<TermId> {
    u.unary(tagged, "ref").or_else(|| u.unary(tagged, "const"))
}

/// `const(Left)`, `const(Right)` with both integers.
pub fn int_pair(u: &Universe, args: &[Option<TermId>]) -> Option<(i64, i64)> {
    if args.len() != 2 {
        return None;
    }
    let l = u.as_int(u.unary(args[0]?, "const")?)?;
    let r = u.as_int(u.unary(args[1]?, "const")?)?;
    Some((l, r))
}

/// A kernel call with some arguments bound. Returns every solution as the
/// full argument vector; the caller unifies the unbound positions.
pub fn solve(u: &mut Universe, k: Kernel, args: &[Option<TermId>]) -> Vec<Vec<TermId>> {
    match k {
        Kernel::Nil => {
            if args.len() != 1 {
                return vec![];
            }
            let empty = u.empty_list();
            let row = u.compound("const", vec![empty]);
            vec![vec![row]]
        }
        Kernel::Cons => {
            if args.len() != 3 {
                return vec![];
            }
            if let Some(list) = args[2] {
                // cons_deconstruct: const([H|T]) with T a proper list
                let inner = match u.unary(list, "const") {
                    Some(i) => i,
                    None => return vec![],
                };
                let items = match u.as_list(inner) {
                    Some(items) if !items.is_empty() => items,
                    _ => return vec![],
                };
                let head = items[0];
                let rest = u.list(&items[1..]);
                let tail = u.compound("const", vec![rest]);
                return vec![vec![head, tail, list]];
            }
            if let (Some(head), Some(tail)) = (args[0], args[1]) {
                let inner = match u.unary(tail, "const") {
                    Some(i) => i,
                    None => return vec![],
                };
                let mut items = match u.as_list(inner) {
                    Some(items) => items,
                    None => return vec![],
                };
                items.insert(0, head);
                let full = u.list(&items);
                let list = u.compound("const", vec![full]);
                return vec![vec![head, tail, list]];
            }
            vec![]
        }
        Kernel::EdgeRef => {
            if args.len() != 3 {
                return vec![];
            }
            let (owner_tagged, label) = match (args[0], args[1]) {
                (Some(o), Some(l)) => (o, l),
                _ => return vec![],
            };
            let owner = match u.unary(owner_tagged, "ref") {
                Some(o) => o,
                None => return vec![],
            };
            let sem = match semantic(u, label) {
                Some(s) => s,
                None => return vec![],
            };
            let edge = u.compound("edge", vec![owner, sem]);
            let result = u.compound("ref", vec![edge]);
            vec![vec![owner_tagged, label, result]]
        }
        Kernel::Intern => {
            if args.len() != 3 {
                return vec![];
            }
            let (ctor_tagged, arguments) = match (args[0], args[1]) {
                (Some(c), Some(a)) => (c, a),
                _ => return vec![],
            };
            let ctor = match u.unary(ctor_tagged, "ref") {
                Some(c) => c,
                None => return vec![],
            };
            let tagged_list = match u.unary(arguments, "const").and_then(|l| u.as_list(l)) {
                Some(l) => l,
                None => return vec![],
            };
            let mut plain = Vec::with_capacity(tagged_list.len());
            for t in tagged_list {
                match semantic(u, t) {
                    Some(s) => plain.push(s),
                    None => return vec![],
                }
            }
            let plain_list = u.list(&plain);
            let app = u.compound("application", vec![ctor, plain_list]);
            let result = u.compound("ref", vec![app]);
            vec![vec![ctor_tagged, arguments, result]]
        }
        Kernel::Int(cmp) => match int_pair(u, args) {
            Some((l, r)) if cmp.holds(l, r) => vec![vec![args[0].unwrap(), args[1].unwrap()]],
            _ => vec![],
        },
    }
}

/// Negative kernel goal, only integer comparisons: the complement. Any other
/// kernel relation under negation is checked against the (always empty) lower
/// store, so it holds.
pub fn negative_holds(u: &Universe, k: Kernel, args: &[Option<TermId>]) -> bool {
    match k {
        Kernel::Int(cmp) => match int_pair(u, args) {
            Some((l, r)) => !cmp.holds(l, r),
            None => true,
        },
        _ => true,
    }
}
