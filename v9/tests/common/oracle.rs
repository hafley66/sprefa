//! Naive bottom-up evaluation of a `Plan`: input rows as multiset counts, every rel a set, strata
//! in order, each stratum re-run from its rules until no rule adds a row.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use v9::_2_ir::{Builtin, RelKind};
use v9::_3_plan::{PAtom, PGoal, PRule, PTerm, Plan, RelId, Row, Value};
use v9::_5_ends::Change;

/// Rounds per stratum; a program needing more is a test defect.
const MAX_ROUNDS: usize = 100_000;

#[derive(Default)]
pub struct Oracle {
    pub counts: BTreeMap<(RelId, Row), isize>,
}

impl Oracle {
    pub fn apply(&mut self, changes: &[Change]) {
        for change in changes {
            *self
                .counts
                .entry((change.rel, change.row.clone()))
                .or_default() += change.diff;
        }
    }

    pub fn eval(&self, plan: &Plan) -> Vec<BTreeSet<Row>> {
        let mut inputs = vec![BTreeSet::new(); plan.rels.len()];
        for ((rel, row), count) in &self.counts {
            if *count > 0 {
                inputs[*rel as usize].insert(row.clone());
            }
        }
        let mut facts = vec![BTreeSet::new(); plan.rels.len()];
        for (id, rel) in plan.rels.iter().enumerate() {
            if rel.kind == RelKind::Base {
                facts[id] = inputs[id].clone();
            }
        }
        for stratum in &plan.strata {
            let mut settled = false;
            for _ in 0..MAX_ROUNDS {
                let mut added = false;
                for &id in &stratum.rels {
                    if let Some(request) = plan.rels[id as usize].request {
                        let width = plan.rels[request as usize].cols.len();
                        let visible: BTreeSet<Row> = inputs[id as usize]
                            .iter()
                            .filter(|row| facts[request as usize].contains(&row[..width]))
                            .cloned()
                            .collect();
                        added |= visible != facts[id as usize];
                        facts[id as usize] = visible;
                    }
                }
                for &rule in &stratum.rules {
                    let rule = &plan.rules[rule];
                    for row in eval_rule(rule, &facts) {
                        added |= facts[rule.head.rel as usize].insert(row);
                    }
                }
                if !added {
                    settled = true;
                    break;
                }
            }
            assert!(
                settled,
                "oracle stratum {:?} past {MAX_ROUNDS} rounds",
                stratum.rels
            );
        }
        facts
    }
}

fn value(binding: &[Value], term: PTerm) -> Value {
    match term {
        PTerm::Var(slot) => binding[slot as usize],
        PTerm::Const(value) => value,
    }
}

/// Extends `binding` with `row` against `atom`; None when a const or bound slot disagrees.
fn unify(atom: &PAtom, binding: &Row, row: &Row) -> Option<Row> {
    let mut out = binding.clone();
    for (at, arg) in atom.args.iter().enumerate() {
        match *arg {
            PTerm::Const(value) if row[at] != value => return None,
            PTerm::Const(_) => {}
            PTerm::Var(slot) if (slot as usize) < out.len() => {
                if out[slot as usize] != row[at] {
                    return None;
                }
            }
            PTerm::Var(_) => out.push(row[at]),
        }
    }
    Some(out)
}

fn eval_rule(rule: &PRule, facts: &[BTreeSet<Row>]) -> Vec<Row> {
    let mut bindings: Vec<Row> = vec![Row::new()];
    for goal in &rule.body {
        let Some(width) = bindings.first().map(Vec::len) else {
            return Vec::new();
        };
        bindings = match goal {
            PGoal::Pos(atom) => {
                let bound: Vec<(usize, u32)> = atom
                    .args
                    .iter()
                    .enumerate()
                    .filter_map(|(at, arg)| match arg {
                        PTerm::Var(slot) if (*slot as usize) < width => Some((at, *slot)),
                        _ => None,
                    })
                    .collect();
                let mut index: HashMap<Row, Vec<&Row>> = HashMap::new();
                for row in &facts[atom.rel as usize] {
                    let key = bound.iter().map(|(at, _)| row[*at]).collect();
                    index.entry(key).or_default().push(row);
                }
                bindings
                    .iter()
                    .flat_map(|binding| {
                        let key: Row = bound
                            .iter()
                            .map(|(_, slot)| binding[*slot as usize])
                            .collect();
                        index
                            .get(&key)
                            .into_iter()
                            .flatten()
                            .filter_map(|row| unify(atom, binding, row))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            }
            PGoal::Neg(atom) => bindings
                .into_iter()
                .filter(|binding| {
                    let row: Row = atom.args.iter().map(|arg| value(binding, *arg)).collect();
                    !facts[atom.rel as usize].contains(&row)
                })
                .collect(),
            PGoal::Call(builtin, args) => bindings
                .into_iter()
                .filter_map(|mut binding| {
                    let int = |term: PTerm, binding: &Row| value(binding, term) as i64;
                    match builtin {
                        Builtin::Add => {
                            let sum = int(args[0], &binding).wrapping_add(int(args[1], &binding))
                                as Value;
                            match args[2] {
                                PTerm::Var(slot) if slot as usize >= binding.len() => {
                                    binding.push(sum);
                                    Some(binding)
                                }
                                out => (value(&binding, out) == sum).then_some(binding),
                            }
                        }
                        Builtin::Lt => {
                            (int(args[0], &binding) < int(args[1], &binding)).then_some(binding)
                        }
                        Builtin::Neq => (value(&binding, args[0]) != value(&binding, args[1]))
                            .then_some(binding),
                    }
                })
                .collect(),
        };
    }
    bindings
        .iter()
        .map(|binding| {
            rule.head
                .args
                .iter()
                .map(|arg| value(binding, *arg))
                .collect()
        })
        .collect()
}
