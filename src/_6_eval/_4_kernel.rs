//! Kernel relations: `ref(kernel(Label))` and `ref(kernel(Owner, Label))`. Ported from the `proves/2` clauses
//! and `integer_comparison/3` in `v7/src/1_libtime/0_evaluator.pl`. Each is a
//! function over bound arguments, never a stored table, except that `intern`
//! records every request as an output row.

use super::term::{Term, TermId, Universe};
use ivm_ir::{StrKind, StrOp, StrOut, StrVal};
use std::cmp::Ordering;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Kernel {
    StrNil,
    StrCons,
    Str(StrOp),
    EdgeRef,
    Intern,
    Int(IntCmp),
    IntAdd,
    TermLt,
    CountStep,
    MinStep,
    MaxStep,
    Tuple,
    Vec,
    VecIter,
    VecLen,
    VecGet,
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
            (None, "edge_ref") => Kernel::EdgeRef,
            (None, "intern") => Kernel::Intern,
            (None, "tuple") => Kernel::Tuple,
            (None, "vec") => Kernel::Vec,
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
            (Some("vec"), "iter") => Kernel::VecIter,
            (Some("vec"), "len") => Kernel::VecLen,
            (Some("vec"), "get") => Kernel::VecGet,
            (Some("str"), "cons") => Kernel::StrCons,
            (Some("str"), "nil") => Kernel::StrNil,
            (Some(owner), label) => Kernel::Str(
                super::str_ops::STR_OPS
                    .iter()
                    .find(|(op_owner, name, ..)| *op_owner == owner && *name == label)?
                    .3,
            ),
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
        Kernel::VecIter => return vec_iter_rows(u, args),
        Kernel::StrNil => str_nil_row(u, args),
        Kernel::StrCons => str_cons_row(u, args),
        Kernel::Str(op) => str_op_row(u, op, args),
        Kernel::EdgeRef => edge_ref_row(u, args),
        Kernel::Intern => intern_row(u, args),
        Kernel::Int(cmp) => int_row(u, cmp, args),
        Kernel::IntAdd => int_dot_add_row(u, args),
        Kernel::TermLt => any_lt_row(u, args),
        Kernel::CountStep => count_step_row(u, args),
        Kernel::MinStep => extremum_step_row(u, args, Ordering::Less),
        Kernel::MaxStep => extremum_step_row(u, args, Ordering::Greater),
        Kernel::Tuple => tuple_row(u, args),
        Kernel::Vec => vec_row(u, args),
        Kernel::VecLen => vec_len_row(u, args),
        Kernel::VecGet => vec_get_row(u, args),
        Kernel::VecIter => unreachable!("vec.iter returned above"),
    };
    solution.map(|row| vec![row]).unwrap_or_default()
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

/// The bound `const` inputs of a string op, as owned text and integer slots.
fn str_inputs(u: &Universe, op: StrOp, args: &[Option<TermId>]) -> Option<(Vec<String>, Vec<i64>)> {
    let kinds = op.args();
    let mut texts = Vec::with_capacity(kinds.len());
    let mut ints = Vec::with_capacity(kinds.len());
    for (arg, kind) in args.iter().zip(kinds) {
        let tagged = (*arg)?;
        match kind {
            StrKind::Text => {
                texts.push(text(u, tagged)?);
                ints.push(0);
            }
            StrKind::Int => {
                texts.push(String::new());
                ints.push(u.as_int(u.unary(tagged, "const")?)?);
            }
        }
    }
    (texts.len() == kinds.len()).then_some((texts, ints))
}

fn str_values<'a>(op: StrOp, texts: &'a [String], ints: &[i64]) -> Vec<StrVal<'a>> {
    op.args()
        .iter()
        .enumerate()
        .map(|(at, kind)| match kind {
            StrKind::Text => StrVal::Text(&texts[at]),
            StrKind::Int => StrVal::Int(ints[at]),
        })
        .collect()
}

/// `str.<op>(Input.., Result)`: every input bound, `Result` computed by the one
/// `StrOp::apply`; a miss has no row. A filter op has no `Result` position.
pub fn str_op_row(u: &mut Universe, op: StrOp, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    let kinds = op.args();
    if args.len() != kinds.len() + usize::from(op.out().is_some()) {
        return None;
    }
    let (texts, ints) = str_inputs(u, op, args)?;
    let values = str_values(op, &texts, &ints);
    let result = match op.apply(&values)? {
        StrOut::Text(value) => Some(u.string(&value)),
        StrOut::Int(value) => Some(u.int(value)),
        StrOut::Holds => None,
    };
    let mut row: Vec<TermId> = args[..kinds.len()].iter().map(|arg| arg.unwrap()).collect();
    if let Some(result) = result {
        row.push(u.compound("const", vec![result]));
    }
    Some(row)
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

/// A tuple element as stored in the argument vector: its `ref` or `const` tag
/// is dropped.
fn tuple_element(u: &Universe, tagged: TermId) -> TermId {
    u.unary(tagged, "ref")
        .or_else(|| u.unary(tagged, "const"))
        .unwrap_or(tagged)
}

/// `tuple(Element.., Value)`: with every element bound, `Value` is
/// `ref(application(tuple, [elements]))`; with `Value` bound, its elements.
pub fn tuple_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    sequence_row(u, "tuple", args)
}

/// `vec(Element.., Value)`: `tuple_row` under the head atom `vec`.
pub fn vec_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    sequence_row(u, "vec", args)
}

fn sequence_row(u: &mut Universe, head: &str, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    let (value, elements) = args.split_last()?;
    if let Some(whole) = *value {
        let application = u.unary(whole, "ref")?;
        let [constructor, list] = u.args::<2>(application, "application")?;
        let expected = u.atom(head);
        if u.get(constructor) != u.get(expected) {
            return None;
        }
        let items = u.as_list(list)?;
        if items.len() != elements.len() {
            return None;
        }
        let mut row = items;
        row.push(whole);
        return Some(row);
    }
    let mut tagged = Vec::with_capacity(elements.len());
    for element in elements {
        tagged.push((*element)?);
    }
    let list = u.list(&tagged);
    let constructor = u.atom(head);
    let application = u.compound("application", vec![constructor, list]);
    let whole = u.compound("ref", vec![application]);
    let mut row: Vec<TermId> = elements.iter().filter_map(|e| *e).collect();
    row.push(whole);
    Some(row)
}

/// `ref(application(vec, Items))`: the tagged items, in order.
pub fn vec_items(u: &Universe, whole: TermId) -> Option<Vec<TermId>> {
    let application = u.unary(whole, "ref")?;
    let [constructor, list] = u.args::<2>(application, "application")?;
    let (name, args) = u.functor_or_atom(constructor)?;
    (name == "vec" && args.is_empty()).then_some(())?;
    u.as_list(list)
}

fn int_const(u: &mut Universe, value: i64) -> TermId {
    let value = u.int(value);
    u.compound("const", vec![value])
}

/// `vec.iter(Vec, Index, Item)`: one row per item, `Index` from 0. A bound
/// `Index` narrows to that item.
pub fn vec_iter_rows(u: &mut Universe, args: &[Option<TermId>]) -> Vec<Vec<TermId>> {
    let [Some(whole), index, _] = args[..] else {
        return Vec::new();
    };
    let Some(items) = vec_items(u, whole) else {
        return Vec::new();
    };
    if let Some(index) = index {
        return vec_get_row(u, args).into_iter().map(|row| vec![row[0], index, row[2]]).collect();
    }
    let mut rows = Vec::with_capacity(items.len());
    for (at, item) in items.into_iter().enumerate() {
        let index = int_const(u, at as i64);
        rows.push(vec![whole, index, item]);
    }
    rows
}

/// `vec.len(Vec, Len)`: the item count.
pub fn vec_len_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    let [Some(whole), _] = args[..] else {
        return None;
    };
    let count = vec_items(u, whole)?.len();
    Some(vec![whole, int_const(u, count as i64)])
}

/// `vec.get(Vec, Index, Item)`: the item at `Index`; an index past the end or
/// below zero has no row.
pub fn vec_get_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    let [Some(whole), Some(index), _] = args[..] else {
        return None;
    };
    let at = u.as_int(u.unary(index, "const")?)?;
    let item = *vec_items(u, whole)?.get(usize::try_from(at).ok()?)?;
    Some(vec![whole, index, item])
}

pub fn intern_row(u: &mut Universe, args: &[Option<TermId>]) -> Option<Vec<TermId>> {
    if args.len() != 3 {
        return None;
    }
    let (ctor_tagged, arguments) = (args[0]?, args[1]?);
    let ctor = u.unary(ctor_tagged, "ref")?;
    let plain: Vec<TermId> = match u.unary(arguments, "const").and_then(|list| u.as_list(list)) {
        Some(tagged_list) => tagged_list
            .iter()
            .map(|t| semantic(u, *t))
            .collect::<Option<_>>()?,
        None => tuple_arguments(u, arguments)?,
    };
    let plain_list = u.list(&plain);
    let app = u.compound("application", vec![ctor, plain_list]);
    let result = u.compound("ref", vec![app]);
    Some(vec![ctor_tagged, arguments, result])
}

/// `ref(application(tuple, Items))` as an argument vector, tags stripped.
fn tuple_arguments(u: &mut Universe, arguments: TermId) -> Option<Vec<TermId>> {
    let application = u.unary(arguments, "ref")?;
    let [constructor, list] = u.args::<2>(application, "application")?;
    let tuple = u.atom("tuple");
    if u.get(constructor) != u.get(tuple) {
        return None;
    }
    let items = u.as_list(list)?;
    Some(items.into_iter().map(|t| tuple_element(u, t)).collect())
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
        Kernel::Str(op) if op.out().is_none() => match str_inputs(u, op, args) {
            Some((texts, ints)) => op.apply(&str_values(op, &texts, &ints)).is_none(),
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
        assert!(!linear((None, "intern")));
    }

    #[test]
    fn vec_builds_and_reads_items_len_and_index() {
        let mut u = Universe::new();
        let text = |u: &mut Universe, value: &str| {
            let value = u.string(value);
            u.compound("const", vec![value])
        };
        let (a, b) = (text(&mut u, "a"), text(&mut u, "b"));
        let whole = solve(&mut u, Kernel::Vec, &[Some(a), Some(b), None]).remove(0)[2];
        let empty = solve(&mut u, Kernel::Vec, &[None]).remove(0)[0];
        let one = int_const(&mut u, 1);
        let two = int_const(&mut u, 2);
        let mut shown = Vec::new();
        for (label, kernel, args) in [
            ("vec", Kernel::Vec, vec![None, None, Some(whole)]),
            ("iter", Kernel::VecIter, vec![Some(whole), None, None]),
            ("iter at 1", Kernel::VecIter, vec![Some(whole), Some(one), None]),
            ("iter empty", Kernel::VecIter, vec![Some(empty), None, None]),
            ("len", Kernel::VecLen, vec![Some(whole), None]),
            ("len empty", Kernel::VecLen, vec![Some(empty), None]),
            ("get 1", Kernel::VecGet, vec![Some(whole), Some(one), None]),
            ("get 2", Kernel::VecGet, vec![Some(whole), Some(two), None]),
        ] {
            for row in solve(&mut u, kernel, &args) {
                let row: Vec<String> = row.iter().map(|id| u.display(*id).to_string()).collect();
                shown.push(format!("{label}: {}", row.join(", ")));
            }
        }
        assert_eq!(
            shown,
            [
                "vec: const(\"a\"), const(\"b\"), ref(application(vec,[const(\"a\"),const(\"b\")]))",
                "iter: ref(application(vec,[const(\"a\"),const(\"b\")])), const(0), const(\"a\")",
                "iter: ref(application(vec,[const(\"a\"),const(\"b\")])), const(1), const(\"b\")",
                "iter at 1: ref(application(vec,[const(\"a\"),const(\"b\")])), const(1), const(\"b\")",
                "len: ref(application(vec,[const(\"a\"),const(\"b\")])), const(2)",
                "len empty: ref(application(vec,[])), const(0)",
                "get 1: ref(application(vec,[const(\"a\"),const(\"b\")])), const(1), const(\"b\")",
            ]
        );
    }
}
