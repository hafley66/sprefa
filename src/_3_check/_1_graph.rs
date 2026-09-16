//! The checker's two lookup stores. `CheckerGraph` replaces
//! `open_checker_graph_store/2` (`0_graph_lookup.pl:25`) and `OriginArena`
//! replaces `open_checker_origin_arena/1` (`1_checker.pl:46`). Both are built
//! once per call and dropped with the frame, so the `close_*` halves have no
//! Rust analogue.

use crate::_2_lower::index::{edge_parts, Edge};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::{HashMap, HashSet};

pub struct CheckerGraph {
    /// Source-list order. `assertz/1` keeps duplicates in this order, so the
    /// first matching row reproduces `memberchk/2`.
    pub edges: Vec<Edge>,
    pub forward: HashMap<(TermId, TermId), usize>,
    pub parent: HashMap<TermId, TermId>,
    pub modules: HashSet<TermId>,
    /// `owner_edge_count_index/2` at `1_checker.pl:546`.
    pub owner_edge_count: HashMap<TermId, i64>,
}

impl CheckerGraph {
    /// `install_pending_edges/4` and `install_module_nodes/4`.
    /// `None` when a `pending_edge/4` row is malformed, which is where v7
    /// throws out of the arithmetic in `dense_index_diagnostics/4`.
    pub fn build(u: &Universe, edge_rows: &[TermId], nodes: &[TermId]) -> Option<CheckerGraph> {
        let mut graph = CheckerGraph {
            edges: Vec::with_capacity(edge_rows.len()),
            forward: HashMap::new(),
            parent: HashMap::new(),
            modules: HashSet::new(),
            owner_edge_count: HashMap::new(),
        };
        for row in edge_rows {
            let parts = edge_parts(u, *row)?;
            let position = graph.edges.len();
            graph
                .forward
                .entry((parts.owner, parts.name))
                .or_insert(position);
            if let Some(target) = u.unary(parts.target, "target") {
                graph.parent.entry(target).or_insert(parts.owner);
            }
            *graph.owner_edge_count.entry(parts.owner).or_insert(0) += 1;
            graph.edges.push(parts);
        }
        for node in nodes {
            if let Some(owner) = u.unary(*node, "module") {
                graph.modules.insert(owner);
            }
        }
        Some(graph)
    }

    /// `0_graph_lookup.pl:91`.
    pub fn forward(&self, owner: TermId, name: TermId) -> Option<TermId> {
        self.forward
            .get(&(owner, name))
            .map(|i| self.edges[*i].target)
    }

    /// `0_graph_lookup.pl:100`.
    pub fn parent(&self, owner: TermId) -> Option<TermId> {
        self.parent.get(&owner).copied()
    }

    /// `0_graph_lookup.pl:128`.
    pub fn module_member(&self, owner: TermId) -> bool {
        self.modules.contains(&owner)
    }

    pub fn edge_count(&self, owner: TermId) -> i64 {
        self.owner_edge_count.get(&owner).copied().unwrap_or(0)
    }
}

/// `arena_edge_origin/6`, `arena_seed_origin/4`, `arena_rule_origin/4` and
/// `arena_goal_origin/5`. The arena and the `memberchk/2` fallback at
/// `1_checker.pl:1104-1134` both take the first row for a key, so one
/// first-wins map serves both.
pub struct OriginArena {
    pub edge: HashMap<(TermId, TermId, i64), TermId>,
    pub seed: HashMap<i64, TermId>,
    pub rule: HashMap<i64, TermId>,
    pub goal: HashMap<(i64, i64), TermId>,
    pub none: TermId,
}

impl OriginArena {
    /// `install_checker_origin_fact/4` at `:76`. `origin(node(_), _)` and
    /// `origin(relation(_), _)` are skipped (`:100`, `:102`).
    pub fn build(u: &mut Universe, origins: &[TermId]) -> OriginArena {
        let mut arena = OriginArena {
            edge: HashMap::new(),
            seed: HashMap::new(),
            rule: HashMap::new(),
            goal: HashMap::new(),
            none: u.atom("none"),
        };
        for origin in origins {
            let Some(("origin", args)) = u.functor(*origin) else {
                continue;
            };
            if args.len() != 2 {
                continue;
            }
            let (key, node) = (args[0], args[1]);
            let Some((name, key_args)) = u.functor(key) else {
                continue;
            };
            match (name, key_args.len()) {
                ("edge", 3) => {
                    let (owner, label) = (key_args[0], key_args[1]);
                    if let Some(index) = u.as_int(key_args[2]) {
                        arena.edge.entry((owner, label, index)).or_insert(node);
                    }
                }
                ("seed", 1) => {
                    if let Some(index) = u.as_int(key_args[0]) {
                        arena.seed.entry(index).or_insert(node);
                    }
                }
                ("rule", 1) => {
                    if let Some(index) = u.as_int(key_args[0]) {
                        arena.rule.entry(index).or_insert(node);
                    }
                }
                ("goal", 2) => {
                    if let (Some(rule), Some(goal)) = (u.as_int(key_args[0]), u.as_int(key_args[1]))
                    {
                        arena.goal.entry((rule, goal)).or_insert(node);
                    }
                }
                _ => {}
            }
        }
        arena
    }

    /// `:1104`.
    pub fn edge_origin(&self, owner: TermId, name: TermId, index: i64) -> TermId {
        self.edge
            .get(&(owner, name, index))
            .copied()
            .unwrap_or(self.none)
    }

    /// `:1112`.
    pub fn seed_origin(&self, index: i64) -> TermId {
        self.seed.get(&index).copied().unwrap_or(self.none)
    }

    /// `:1120`.
    pub fn rule_origin(&self, index: i64) -> TermId {
        self.rule.get(&index).copied().unwrap_or(self.none)
    }

    /// `:1128`.
    pub fn goal_origin(&self, rule: i64, goal: i64) -> TermId {
        self.goal.get(&(rule, goal)).copied().unwrap_or(self.none)
    }
}
