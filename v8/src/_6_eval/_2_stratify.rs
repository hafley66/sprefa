//! Port of `stratify_rules/3`. Positive reads have gap zero, negative reads
//! gap one, and every read of an aggregate rule has gap one. Levels relax to
//! the least fixpoint. A gap-one edge on a dependency cycle is a diagnostic.

use super::program::{Diagnostic, Polarity, Program, Rule};
use super::term::{TermId, Universe};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Dependency {
    pub head: TermId,
    pub body: TermId,
    pub gap: u32,
    pub aggregate: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Strata {
    /// level per derived relation (a relation that heads at least one rule)
    pub levels: HashMap<TermId, u32>,
    pub max_level: u32,
}

impl Strata {
    /// Relations with no rule sit at level zero, as in `relation_level/3`.
    pub fn level(&self, rel: TermId) -> u32 {
        self.levels.get(&rel).copied().unwrap_or(0)
    }
}

pub fn dependencies(rules: &[Rule]) -> Vec<Dependency> {
    let mut out = Vec::new();
    for rule in rules {
        let aggregate = rule.is_aggregate();
        for goal in &rule.body {
            let gap = if aggregate {
                1
            } else {
                match goal.polarity {
                    Polarity::Positive => 0,
                    Polarity::Negative => 1,
                }
            };
            out.push(Dependency {
                head: rule.rel,
                body: goal.rel,
                gap,
                aggregate,
            });
        }
    }
    out
}

fn sorted_rels(u: &Universe, set: &HashSet<TermId>) -> Vec<TermId> {
    let mut v: Vec<TermId> = set.iter().copied().collect();
    v.sort_by(|a, b| u.cmp(*a, *b));
    v
}

pub fn stratify(u: &mut Universe, program: &Program) -> (Strata, Vec<Diagnostic>) {
    let deps = dependencies(&program.rules);
    let mut relations: HashSet<TermId> = HashSet::new();
    for rule in &program.rules {
        relations.insert(rule.rel);
        for goal in &rule.body {
            relations.insert(goal.rel);
        }
    }
    let relations = sorted_rels(u, &relations);

    // transitive closure over the dependency graph
    let mut reach: HashMap<TermId, HashSet<TermId>> = HashMap::new();
    for r in &relations {
        reach.insert(*r, HashSet::new());
    }
    for d in &deps {
        reach.get_mut(&d.head).unwrap().insert(d.body);
    }
    loop {
        let mut changed = false;
        for r in &relations {
            let current: Vec<TermId> = reach[r].iter().copied().collect();
            let mut add = Vec::new();
            for n in &current {
                for m in &reach[n] {
                    if !reach[r].contains(m) {
                        add.push(*m);
                    }
                }
            }
            if !add.is_empty() {
                changed = true;
                reach.get_mut(r).unwrap().extend(add);
            }
        }
        if !changed {
            break;
        }
    }

    let mut strict: Vec<&Dependency> = deps
        .iter()
        .filter(|d| d.gap == 1 && reach[&d.body].contains(&d.head))
        .collect();
    strict.sort_by(|a, b| u.cmp(a.head, b.head).then_with(|| u.cmp(a.body, b.body)));
    strict.dedup_by(|a, b| a.head == b.head && a.body == b.body && a.aggregate == b.aggregate);

    if !strict.is_empty() {
        let mut cycle: HashSet<TermId> = HashSet::new();
        for d in &strict {
            for r in &relations {
                let mutual =
                    *r == d.head || (reach[&d.head].contains(r) && reach[r].contains(&d.head));
                if mutual {
                    cycle.insert(*r);
                }
            }
        }
        let cycle = sorted_rels(u, &cycle);
        let name = if strict.iter().any(|d| d.aggregate) {
            "aggregate_dependency_cycle"
        } else {
            "strict_dependency_cycle"
        };
        let list = u.list(&cycle);
        let payload = u.compound(name, vec![list]);
        return (
            Strata::default(),
            vec![Diagnostic {
                phase: "stratify",
                payload,
            }],
        );
    }

    let mut levels: HashMap<TermId, u32> = relations.iter().map(|r| (*r, 0)).collect();
    loop {
        let mut changed = false;
        for r in &relations {
            let mut next = levels[r];
            for d in deps.iter().filter(|d| d.head == *r) {
                next = next.max(levels[&d.body] + d.gap);
            }
            if next != levels[r] {
                levels.insert(*r, next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let derived: HashSet<TermId> = program.rules.iter().map(|r| r.rel).collect();
    let mut out = Strata::default();
    for r in derived {
        let l = levels[&r];
        out.levels.insert(r, l);
        out.max_level = out.max_level.max(l);
    }
    (out, Vec::new())
}
