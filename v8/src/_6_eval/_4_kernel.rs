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
    IntAdd,
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
            "int_add" => Kernel::IntAdd,
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

/// Every kernel is a partial function: at most one solution, as the full
/// argument vector, with the caller unifying the unbound positions.
pub fn solve(u: &mut Universe, k: Kernel, args: &[Option<TermId>]) -> Vec<Vec<TermId>> {
    let solution = match k {
        Kernel::Nil => nil_row(u, args),
        Kernel::Cons => cons_row(u, args),
        Kernel::EdgeRef => edge_ref_row(u, args),
        Kernel::Intern => intern_row(u, args),
        Kernel::Int(cmp) => int_row(u, cmp, args),
        Kernel::IntAdd => int_add_row(u, args),
    };
    solution.map(|row| vec![row]).unwrap_or_default()
}

pub fn nil_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    (args.len() == 1).then(|| {
        let empty = u.empty_list();
        vec![u.compound("const", vec![empty])]
    })
}

/// Deconstruct when the list is bound, otherwise construct from head and tail.
pub fn cons_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    if let Some(list) = args[2] {
        let items = u.as_list(u.unary(list, "const")?)?;
        let head = *items.first()?;
        let rest = u.list(&items[1..]);
        let tail = u.compound("const", vec![rest]);
        return Some(vec![head, tail, list]);
    }
    let (head, tail) = (args[0]?, args[1]?);
    let mut items = u.as_list(u.unary(tail, "const")?)?;
    items.insert(0, head);
    let full = u.list(&items);
    let list = u.compound("const", vec![full]);
    Some(vec![head, tail, list])
}

pub fn edge_ref_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    let (owner_tagged, label) = (args[0]?, args[1]?);
    let owner = u.unary(owner_tagged, "ref")?;
    let sem = semantic(u, label)?;
    let edge = u.compound("edge", vec![owner, sem]);
    let result = u.compound("ref", vec![edge]);
    Some(vec![owner_tagged, label, result])
}

pub fn intern_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    let (ctor_tagged, arguments) = (args[0]?, args[1]?);
    let ctor = u.unary(ctor_tagged, "ref")?;
    let tagged_list = u.as_list(u.unary(arguments, "const")?)?;
    let plain: Vec<TermId> = tagged_list
        .iter()
        .map(|t| semantic(u, *t))
        .collect::<Option<_>>()?;
    let plain_list = u.list(&plain);
    let app = u.compound("application", vec![ctor, plain_list]);
    let result = u.compound("ref", vec![app]);
    Some(vec![ctor_tagged, arguments, result])
}

pub fn int_row(u: &Universe, cmp: IntCmp, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    let (l, r) = int_pair(u, args)?;
    cmp.holds(l, r)
        .then(|| vec![args[0].unwrap(), args[1].unwrap()])
}

/// `int_add(Left, Right, Sum)`: the sum is `const(Left + Right)`, and an
/// overflowing sum has no row. The return is not a key, so a bound wrong sum
/// fails at unification.
pub fn int_add_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    let (left, right) = (args[0]?, args[1]?);
    let left_value = u.as_int(u.unary(left, "const")?)?;
    let right_value = u.as_int(u.unary(right, "const")?)?;
    let sum = left_value.checked_add(right_value)?;
    let sum = u.int(sum);
    let result = u.compound("const", vec![sum]);
    Some(vec![left, right, result])
}

/// The three bound `const` integers of `int_add`, all present.
fn int_add_ground(u: &Universe, args: &[Option<TermId>]) -> Option<(i64, i64, i64)> {
    if args.len() != 3 {
        return None;
    }
    let left = u.as_int(u.unary(args[0]?, "const")?)?;
    let right = u.as_int(u.unary(args[1]?, "const")?)?;
    let sum = u.as_int(u.unary(args[2]?, "const")?)?;
    Some((left, right, sum))
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
        Kernel::IntAdd => match int_add_ground(u, args) {
            Some((left, right, sum)) => left.checked_add(right) != Some(sum),
            None => true,
        },
        _ => true,
    }
}
