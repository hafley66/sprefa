//! One rule as differential operators, generic over the timestamp so it runs in and out of a loop.
//! A binding row holds the slots bound so far; goals extend or filter it in written order.

use crate::_2_ir::Builtin;
use crate::_3_plan::{PAtom, PGoal, PRule, PTerm, RelId, Row, Value};
use differential_dataflow::lattice::Lattice;
use differential_dataflow::VecCollection;
use std::collections::HashMap;
use timely::progress::Timestamp;

pub type Rows<'s, T> = VecCollection<'s, T, Row>;

/// Count above zero is one row; at zero or below the row is absent.
pub fn set<'s, T: Timestamp + Lattice + Ord>(rows: Rows<'s, T>) -> Rows<'s, T> {
    rows.threshold(|_, count| isize::from(*count > 0))
}

fn value(binding: &[Value], term: PTerm) -> Value {
    match term {
        PTerm::Var(slot) => binding[slot as usize],
        PTerm::Const(value) => value,
    }
}

/// `unit` holds the one empty binding row every body starts from.
pub fn lower<'s, T: Timestamp + Lattice + Ord>(
    rule: &PRule,
    env: &HashMap<RelId, Rows<'s, T>>,
    unit: Rows<'s, T>,
) -> Rows<'s, T> {
    let mut width = 0;
    let mut rows = unit;
    for goal in &rule.body {
        rows = match goal {
            PGoal::Pos(atom) => {
                let (joined, fresh) = pos(rows, atom, width, env[&atom.rel].clone());
                width += fresh;
                joined
            }
            PGoal::Neg(atom) => neg(rows, atom, env[&atom.rel].clone()),
            PGoal::Call(builtin, args) => {
                let (called, fresh) = call(rows, *builtin, args, width);
                width += fresh;
                called
            }
        };
    }
    let head = rule.head.args.clone();
    rows.map(move |binding| head.iter().map(|term| value(&binding, *term)).collect())
}

/// Joins on the slots already bound; new slots take their first position's value, in slot order.
fn pos<'s, T: Timestamp + Lattice + Ord>(
    rows: Rows<'s, T>,
    atom: &PAtom,
    width: usize,
    rel: Rows<'s, T>,
) -> (Rows<'s, T>, usize) {
    let mut key_slots = Vec::new();
    let mut key_positions = Vec::new();
    let mut consts = Vec::new();
    let mut fresh: Vec<usize> = Vec::new();
    let mut repeats = Vec::new();
    for (position, arg) in atom.args.iter().enumerate() {
        match *arg {
            PTerm::Const(value) => consts.push((position, value)),
            PTerm::Var(slot) if (slot as usize) < width => {
                key_slots.push(slot as usize);
                key_positions.push(position);
            }
            PTerm::Var(slot) if (slot as usize) < width + fresh.len() => {
                repeats.push((position, fresh[slot as usize - width]));
            }
            PTerm::Var(_) => fresh.push(position),
        }
    }
    let count = fresh.len();
    let right = rel
        .filter(move |row| {
            consts.iter().all(|(at, value)| row[*at] == *value)
                && repeats.iter().all(|(at, first)| row[*at] == row[*first])
        })
        .map(move |row| {
            let key: Row = key_positions.iter().map(|at| row[*at]).collect();
            let new: Row = fresh.iter().map(|at| row[*at]).collect();
            (key, new)
        });
    let joined = rows
        .map(move |binding| {
            let key: Row = key_slots.iter().map(|slot| binding[*slot]).collect();
            (key, binding)
        })
        .join_map(right, |_, binding, new| {
            let mut out = binding.clone();
            out.extend_from_slice(new);
            out
        });
    (joined, count)
}

/// Every arg is bound, so the rel side keyed on its var positions stays a set.
fn neg<'s, T: Timestamp + Lattice + Ord>(
    rows: Rows<'s, T>,
    atom: &PAtom,
    rel: Rows<'s, T>,
) -> Rows<'s, T> {
    let args = atom.args.clone();
    let consts: Vec<(usize, Value)> = args
        .iter()
        .enumerate()
        .filter_map(|(at, arg)| match arg {
            PTerm::Const(value) => Some((at, *value)),
            PTerm::Var(_) => None,
        })
        .collect();
    let vars: Vec<(usize, u32)> = args
        .iter()
        .enumerate()
        .filter_map(|(at, arg)| match arg {
            PTerm::Var(slot) => Some((at, *slot)),
            PTerm::Const(_) => None,
        })
        .collect();
    let positions = vars.clone();
    let right = rel
        .filter(move |row| consts.iter().all(|(at, value)| row[*at] == *value))
        .map(move |row| positions.iter().map(|(at, _)| row[*at]).collect::<Row>());
    rows.map(move |binding| {
        let key: Row = vars
            .iter()
            .map(|(_, slot)| binding[*slot as usize])
            .collect();
        (key, binding)
    })
    .antijoin(right)
    .map(|(_, binding)| binding)
}

/// `add` with an unbound out slot binds it (wrapping i64 sum); every other call filters.
fn call<'s, T: Timestamp + Lattice + Ord>(
    rows: Rows<'s, T>,
    builtin: Builtin,
    args: &[PTerm],
    width: usize,
) -> (Rows<'s, T>, usize) {
    let args = args.to_vec();
    match builtin {
        Builtin::Add => {
            let (left, right) = (args[0], args[1]);
            let sum = move |binding: &Row| {
                (value(binding, left) as i64).wrapping_add(value(binding, right) as i64) as Value
            };
            match args[2] {
                PTerm::Var(slot) if slot as usize >= width => (
                    rows.map(move |mut binding| {
                        let out = sum(&binding);
                        binding.push(out);
                        binding
                    }),
                    1,
                ),
                out => (
                    rows.filter(move |binding| sum(binding) == value(binding, out)),
                    0,
                ),
            }
        }
        Builtin::Lt => (
            rows.filter(move |binding| {
                (value(binding, args[0]) as i64) < (value(binding, args[1]) as i64)
            }),
            0,
        ),
        Builtin::Neq => (
            rows.filter(move |binding| value(binding, args[0]) != value(binding, args[1])),
            0,
        ),
    }
}
