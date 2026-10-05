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

/// Tarjan's strongly connected components, numbered in completion order: every
/// component a relation reads completes before the relation's own.
pub(crate) fn components(edges: &[Vec<(usize, u32)>]) -> Vec<usize> {
    const UNVISITED: usize = usize::MAX;
    let n = edges.len();
    let mut index = vec![UNVISITED; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut component = vec![UNVISITED; n];
    let mut stack = Vec::new();
    let mut next_index = 0;
    let mut next_component = 0;
    // `(node, next edge to visit)`, one frame per node on the current path.
    let mut path: Vec<(usize, usize)> = Vec::new();
    for root in 0..n {
        if index[root] != UNVISITED {
            continue;
        }
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;
        path.push((root, 0));
        while let Some((node, edge)) = path.pop() {
            if let Some(&(body, _)) = edges[node].get(edge) {
                path.push((node, edge + 1));
                if index[body] == UNVISITED {
                    index[body] = next_index;
                    low[body] = next_index;
                    next_index += 1;
                    stack.push(body);
                    on_stack[body] = true;
                    path.push((body, 0));
                } else if on_stack[body] {
                    low[node] = low[node].min(index[body]);
                }
                continue;
            }
            if let Some(&(parent, _)) = path.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == index[node] {
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component[member] = next_component;
                    if member == node {
                        break;
                    }
                }
                next_component += 1;
            }
        }
    }
    component
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
    let position: HashMap<TermId, usize> = relations
        .iter()
        .enumerate()
        .map(|(index, rel)| (*rel, index))
        .collect();
    let mut edges: Vec<Vec<(usize, u32)>> = vec![Vec::new(); relations.len()];
    for d in &deps {
        edges[position[&d.head]].push((position[&d.body], d.gap));
    }
    let component = components(&edges);

    // A gap-one read inside one component lies on a cycle.
    let strict: Vec<&Dependency> = deps
        .iter()
        .filter(|d| d.gap == 1 && component[position[&d.head]] == component[position[&d.body]])
        .collect();
    if !strict.is_empty() {
        let cyclic: HashSet<usize> = strict
            .iter()
            .map(|d| component[position[&d.head]])
            .collect();
        let cycle: HashSet<TermId> = relations
            .iter()
            .enumerate()
            .filter(|(index, _)| cyclic.contains(&component[*index]))
            .map(|(_, rel)| *rel)
            .collect();
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

    // Edges inside a component have gap zero: one pass in completion order is
    // the least fixpoint of `level(head) >= level(body) + gap`.
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); relations.len()];
    for (index, c) in component.iter().enumerate() {
        members[*c].push(index);
    }
    let mut levels = vec![0u32; relations.len()];
    for (c, group) in members.iter().enumerate() {
        let level = group
            .iter()
            .flat_map(|head| edges[*head].iter())
            .filter(|(body, _)| component[*body] != c)
            .map(|(body, gap)| levels[*body] + gap)
            .max()
            .unwrap_or(0);
        for index in group {
            levels[*index] = level;
        }
    }
    let derived: HashSet<TermId> = program.rules.iter().map(|r| r.rel).collect();
    let mut out = Strata::default();
    for r in derived {
        let l = levels[position[&r]];
        out.levels.insert(r, l);
        out.max_level = out.max_level.max(l);
    }
    (out, Vec::new())
}
