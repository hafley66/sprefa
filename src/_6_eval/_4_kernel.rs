//! Kernel relations: `ref(kernel(Label))` and `ref(kernel(Owner, Label))`. Ported from the `proves/2` clauses
//! and `integer_comparison/3` in `v7/src/1_libtime/0_evaluator.pl`. Each is a
//! function over bound arguments, never a stored table, except that `intern`
//! records every request as an output row.

use super::term::{Term, TermId, Universe};
use std::cmp::Ordering;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Kernel {
    Nil,
    Cons,
    StrNil,
    StrCons,
    EdgeRef,
    Intern,
    Int(IntCmp),
    IntAdd,
    TermLt,
    CountStep,
    MinStep,
    MaxStep,
}

/// `ref(kernel(Name))`.
pub fn kernel_ref(u: &mut Universe, name: &str) -> TermId {
    let atom = u.atom(name);
    let inner = u.compound("kernel", vec![atom]);
    u.compound("ref", vec![inner])
}

/// `ref(kernel(Owner, Label))` for a typed op, `ref(kernel(Label))` otherwise.
pub fn op_ref(u: &mut Universe, (owner, label): (Option<&str>, &str)) -> TermId {
    let Some(owner) = owner else {
        return kernel_ref(u, label);
    };
    let owner = u.atom(owner);
    let label = u.atom(label);
    let inner = u.compound("kernel", vec![owner, label]);
    u.compound("ref", vec![inner])
}

/// `linear(Step)`: the step admits an incremental lowering later. Facts only;
/// nothing reads them yet.
pub const LINEAR_STEPS: [(Option<&str>, &str); 2] = [(Some("int"), "add"), (None, "count_step")];

pub fn linear(step: (Option<&str>, &str)) -> bool {
    LINEAR_STEPS.contains(&step)
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
    /// `ref(kernel(..))` to the kernel, `None` for every other relation.
    pub fn of(u: &Universe, rel: TermId) -> Option<Kernel> {
        let inner = u.unary(rel, "ref")?;
        let atom = |id: TermId| match u.functor_or_atom(id)? {
            (name, []) => Some(name),
            _ => None,
        };
        let (owner, label) = match u.args::<2>(inner, "kernel") {
            Some([owner, label]) => (Some(atom(owner)?), atom(label)?),
            None => (None, atom(u.unary(inner, "kernel")?)?),
        };
        Some(match (owner, label) {
            (None, "nil") => Kernel::Nil,
            (None, "cons") => Kernel::Cons,
            (None, "edge_ref") => Kernel::EdgeRef,
            (None, "intern") => Kernel::Intern,
            (None, "count_step") => Kernel::CountStep,
            (None, "min_step") => Kernel::MinStep,
            (None, "max_step") => Kernel::MaxStep,
            (Some("int"), "lt") => Kernel::Int(IntCmp::Lt),
            (Some("int"), "le") => Kernel::Int(IntCmp::Le),
            (Some("int"), "eq") => Kernel::Int(IntCmp::Eq),
            (Some("int"), "ne") => Kernel::Int(IntCmp::Ne),
            (Some("int"), "ge") => Kernel::Int(IntCmp::Ge),
            (Some("int"), "gt") => Kernel::Int(IntCmp::Gt),
            (Some("int"), "add") => Kernel::IntAdd,
            (Some("any"), "lt") => Kernel::TermLt,
            (Some("str"), "cons") => Kernel::StrCons,
            (Some("str"), "nil") => Kernel::StrNil,
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

/// `const(Left)`, `const(Right)` with both terms of any kind.
pub fn term_pair(u: &Universe, args: &[Option<TermId>]) -> Option<(TermId, TermId)> {
    if args.len() != 2 {
        return None;
    }
    let l = u.unary(args[0]?, "const")?;
    let r = u.unary(args[1]?, "const")?;
    Some((l, r))
}

/// Every kernel is a partial function: at most one solution, as the full
/// argument vector, with the caller unifying the unbound positions.
pub fn solve(u: &mut Universe, k: Kernel, args: &[Option<TermId>]) -> Vec<Vec<TermId>> {
    let solution = match k {
        Kernel::Nil => nil_row(u, args),
        Kernel::Cons => cons_row(u, args),
        Kernel::StrNil => str_nil_row(u, args),
        Kernel::StrCons => str_cons_row(u, args),
        Kernel::EdgeRef => edge_ref_row(u, args),
        Kernel::Intern => intern_row(u, args),
        Kernel::Int(cmp) => int_row(u, cmp, args),
        Kernel::IntAdd => int_dot_add_row(u, args),
        Kernel::TermLt => any_lt_row(u, args),
        Kernel::CountStep => count_step_row(u, args),
        Kernel::MinStep => extremum_step_row(u, args, Ordering::Less),
        Kernel::MaxStep => extremum_step_row(u, args, Ordering::Greater),
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

/// `str.nil(Empty)`: `Empty` is `""`.
pub fn str_nil_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    (args.len() == 1).then(|| {
        let empty = u.string("");
        vec![u.compound("const", vec![empty])]
    })
}

fn text(u: &Universe, tagged: TermId) -> Option<String> {
    match u.get(u.unary(tagged, "const")?) {
        Term::Str(s) => Some(u.sym_str(*s).to_string()),
        _ => None,
    }
}

/// `str.cons(Head, Tail, Text)`: with head and tail bound, `Text` is their
/// concatenation; with only `Text` bound, `Head` is its first character and
/// `Tail` the rest, so `""` has no row.
pub fn str_cons_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    if let (Some(head), Some(tail)) = (args[0], args[1]) {
        let joined = text(u, head)? + &text(u, tail)?;
        let joined = u.string(&joined);
        let joined = u.compound("const", vec![joined]);
        return Some(vec![head, tail, joined]);
    }
    let whole = args[2]?;
    let full = text(u, whole)?;
    let first = full.chars().next()?;
    let (head, rest) = full.split_at(first.len_utf8());
    let head = u.string(head);
    let head = u.compound("const", vec![head]);
    let rest = u.string(rest);
    let rest = u.compound("const", vec![rest]);
    Some(vec![head, rest, whole])
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

/// `any.lt(Left, Right)`: holds when `Left` precedes `Right` in the store's
/// standard term order.
pub fn any_lt_row(u: &Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    let (l, r) = term_pair(u, args)?;
    (u.cmp(l, r) == std::cmp::Ordering::Less).then(|| vec![args[0].unwrap(), args[1].unwrap()])
}

/// `int.add(Left, Right, Sum)`: the sum is `const(Left + Right)`, and an
/// overflowing sum has no row. The return is not a key, so a bound wrong sum
/// fails at unification.
pub fn int_dot_add_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
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

/// `count_step(Acc, Value, Next)`: the value is ignored, `Next` is `Acc + 1`.
pub fn count_step_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    let (accumulator, value) = (args[0]?, args[1]?);
    let running = u.as_int(u.unary(accumulator, "const")?)?;
    let next = u.int(running.checked_add(1)?);
    let next = u.compound("const", vec![next]);
    Some(vec![accumulator, value, next])
}

/// `min_step` and `max_step`: `Next` is whichever of `Acc` and `Value` the
/// standard term order puts on `wins`, with `Acc` keeping a tie.
pub fn extremum_step_row(
    u: &mut Universe,
    args: &[Option<TermId>],
    wins: Ordering,
) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    let (accumulator, value) = (args[0]?, args[1]?);
    let next = if u.cmp(value, accumulator) == wins {
        value
    } else {
        accumulator
    };
    Some(vec![accumulator, value, next])
}

/// The three bound `const` integers of `int.add`, all present.
fn int_dot_add_ground(u: &Universe, args: &[Option<TermId>]) -> Option<(i64, i64, i64)> {
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
        Kernel::IntAdd => match int_dot_add_ground(u, args) {
            Some((left, right, sum)) => left.checked_add(right) != Some(sum),
            None => true,
        },
        Kernel::TermLt => match term_pair(u, args) {
            Some((left, right)) => u.cmp(left, right) != Ordering::Less,
            None => true,
        },
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_steps_are_int_dot_add_and_count_step() {
        assert_eq!(LINEAR_STEPS, [(Some("int"), "add"), (None, "count_step")]);
        assert!(linear((Some("int"), "add")));
        assert!(linear((None, "count_step")));
        assert!(!linear((None, "min_step")));
        assert!(!linear((None, "max_step")));
        assert!(!linear((None, "cons")));
    }
}
