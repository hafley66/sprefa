//! Port of `v7/src/1_libtime/0b_syntax_rewriter.pl`. One wave: walk the active
//! frontier in ordinal order, replace each claimed node by its ordered
//! expansion outputs, rebuild the child edges of every form above them, then
//! keep only what the new frontier still reaches. The wave is returned as the
//! rows it removes from and adds to the graph.

use super::_0_rows::{
    anchor, args_of, dense_ordinals, macrotime_diagnostic, ordinal_list, sort_terms, Anchor, Graph,
};
use super::_2_protocol::{Available, Claim, Output};
use crate::_6_eval::{TermId, Universe};
use std::collections::{HashMap, HashSet};

pub struct Rewrite {
    pub removed: Vec<TermId>,
    pub added: Vec<TermId>,
    /// the nodes the new frontier reaches
    pub active: HashSet<TermId>,
    pub origin: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

struct Pass<'a> {
    u: &'a mut Universe,
    g: &'a Graph,
    available: &'a Graph,
    claims: HashMap<TermId, Vec<TermId>>,
    outputs: HashMap<TermId, Vec<(TermId, TermId)>>,
    wave: TermId,
    overrides: Vec<(TermId, Vec<TermId>)>,
    origin: Vec<TermId>,
    diagnostics: Vec<TermId>,
}

pub fn rewrite_active_graph(
    u: &mut Universe,
    g: &Graph,
    available: &Available,
    claims: &[Claim],
    outputs: &[Output],
    wave: i64,
) -> Rewrite {
    let mut by_node: HashMap<TermId, Vec<TermId>> = HashMap::new();
    for claim in claims {
        by_node
            .entry(claim.invocation)
            .or_default()
            .push(claim.identity);
    }
    let mut by_invocation: HashMap<TermId, Vec<(TermId, TermId)>> = HashMap::new();
    for output in outputs {
        by_invocation
            .entry(output.invocation)
            .or_default()
            .push((output.ordinal, output.output));
    }
    let wave_term = u.int(wave);
    let roots: Vec<TermId> = g.ordered_frontier(u).iter().map(|(_, n)| *n).collect();
    let mut new_roots: Vec<TermId> = Vec::new();
    let (overrides, mut origin, diagnostics) = {
        let mut pass = Pass {
            u: &mut *u,
            g,
            available: &available.graph,
            claims: by_node,
            outputs: by_invocation,
            wave: wave_term,
            overrides: Vec::new(),
            origin: Vec::new(),
            diagnostics: Vec::new(),
        };
        for root in roots {
            let mut own = pass.node(root);
            new_roots.append(&mut own);
        }
        (pass.overrides, pass.origin, pass.diagnostics)
    };
    if !diagnostics.is_empty() {
        return Rewrite {
            removed: Vec::new(),
            added: Vec::new(),
            active: HashSet::new(),
            origin: Vec::new(),
            diagnostics,
        };
    }
    sort_terms(u, &mut origin);
    let (removed, added, active) = apply_rewrite(u, g, available, &new_roots, &overrides);
    Rewrite {
        removed,
        added,
        active,
        origin,
        diagnostics: Vec::new(),
    }
}

/// A macro's own `syntax_diagnostic(Node, Payload)` rows: each is a
/// `macrotime_diagnostic(Node, Payload)`, and a wave that reports rewrites
/// nothing.
pub fn reported_diagnostics(u: &mut Universe, reported: &[(TermId, TermId)]) -> Vec<TermId> {
    reported
        .iter()
        .map(|(node, payload)| macrotime_diagnostic(u, *node, *payload))
        .collect()
}

impl Pass<'_> {
    /// The nodes this node becomes. A claimed node never has its children
    /// walked; it is replaced whole.
    fn node(&mut self, node: TermId) -> Vec<TermId> {
        if let Some(macros) = self.claims.get(&node).cloned() {
            if !macros.is_empty() {
                return self.claimed(node, &macros);
            }
        }
        if self.g.forms.contains(&node) {
            let children: Vec<TermId> = self
                .g
                .ordered_children(self.u, node)
                .iter()
                .map(|(_, c)| *c)
                .collect();
            let mut rewritten = Vec::new();
            let at = self.overrides.len();
            for child in children {
                let mut own = self.node(child);
                rewritten.append(&mut own);
            }
            self.overrides.insert(at, (node, rewritten));
        }
        vec![node]
    }

    fn claimed(&mut self, node: TermId, macros: &[TermId]) -> Vec<TermId> {
        if macros.len() != 1 {
            let list = self.u.list(macros);
            let payload = self.u.compound("conflicting_macro_claims", vec![list]);
            let d = macrotime_diagnostic(self.u, node, payload);
            self.diagnostics.push(d);
            return Vec::new();
        }
        let macro_identity = macros[0];
        let mut pairs = self.outputs.get(&node).cloned().unwrap_or_default();
        pairs.sort_by(|a, b| self.u.cmp(a.0, b.0));
        let mut diagnostics = Vec::new();
        if !dense_ordinals(self.u, &pairs) {
            let list = ordinal_list(self.u, &pairs);
            let payload = self.u.compound("non_dense_expansion_ordinals", vec![list]);
            diagnostics.push(macrotime_diagnostic(self.u, node, payload));
        }
        for (_, output) in &pairs {
            if !self.available.has_payload(*output) {
                let payload = self.u.compound("unknown_expansion_output", vec![*output]);
                diagnostics.push(macrotime_diagnostic(self.u, node, payload));
            }
        }
        if !diagnostics.is_empty() {
            self.diagnostics.append(&mut diagnostics);
            return Vec::new();
        }
        let wave = self.wave;
        let claim = self
            .u
            .compound("expansion_claim", vec![node, macro_identity, wave]);
        self.origin.push(claim);
        for (ordinal, output) in &pairs {
            let row = self.u.compound(
                "expansion_output",
                vec![node, macro_identity, wave, *output, *ordinal],
            );
            self.origin.push(row);
        }
        pairs.iter().map(|(_, output)| *output).collect()
    }
}

/// `apply_rewrite/5`: the graph plus what the closure derived, without the
/// old frontier and the overridden child edges, with the new ones, pruned to
/// what the new frontier reaches; returned as rows removed and added.
fn apply_rewrite(
    u: &mut Universe,
    g: &Graph,
    available: &Available,
    roots: &[TermId],
    overrides: &[(TermId, Vec<TermId>)],
) -> (Vec<TermId>, Vec<TermId>, HashSet<TermId>) {
    let a = &available.graph;
    let mut owners: HashMap<TermId, &[TermId]> = HashMap::new();
    for (owner, children) in overrides {
        owners.entry(*owner).or_insert(children);
    }
    // A derived `node` or `item` row counts only on nodes with a syntax row.
    let derived_item =
        |owner: TermId, target: TermId| available.identity(owner) && available.identity(target);
    let mut active: HashSet<TermId> = HashSet::new();
    let mut queue: Vec<TermId> = roots.to_vec();
    while let Some(node) = queue.pop() {
        if !active.insert(node) {
            continue;
        }
        if let Some(children) = owners.get(&node) {
            queue.extend(children.iter().filter(|c| !active.contains(c)));
            continue;
        }
        for (_, child) in g.items.get(&node).into_iter().flatten() {
            if !active.contains(child) {
                queue.push(*child);
            }
        }
        for (_, child) in a.items.get(&node).into_iter().flatten() {
            if !active.contains(child) && derived_item(node, *child) {
                queue.push(*child);
            }
        }
    }
    let item = u.atom("item");
    let mut frontier: HashSet<TermId> = HashSet::new();
    for (index, root) in roots.iter().enumerate() {
        let ordinal = u.int(index as i64);
        frontier.insert(u.compound("syntax_frontier", vec![ordinal, *root]));
    }
    let mut edges: HashSet<TermId> = HashSet::new();
    for (owner, children) in &owners {
        if !active.contains(owner) {
            continue;
        }
        for (index, child) in children.iter().enumerate() {
            let target = u.compound("ref", vec![*child]);
            let ordinal = u.int(index as i64);
            edges.insert(u.compound(":", vec![*owner, item, target, ordinal]));
        }
    }
    let removed: Vec<TermId> = g
        .rows
        .iter()
        .copied()
        .filter(|row| {
            let kept = match anchor(u, *row) {
                Anchor::Frontier => frontier.contains(row),
                Anchor::One(node) => active.contains(&node),
                Anchor::Item(owner, _) if owners.contains_key(&owner) => edges.contains(row),
                Anchor::Item(owner, target) => active.contains(&owner) && active.contains(&target),
                Anchor::Never => false,
            };
            !kept
        })
        .collect();
    let mut added: Vec<TermId> = frontier
        .into_iter()
        .chain(edges)
        .filter(|row| !g.set.contains(row))
        .collect();
    for &row in &available.extra {
        let kept = match anchor(u, row) {
            Anchor::One(node) => {
                active.contains(&node)
                    && (args_of(u, row, "node", 1).is_none() || available.identity(node))
            }
            Anchor::Item(owner, target) => {
                !owners.contains_key(&owner)
                    && active.contains(&owner)
                    && active.contains(&target)
                    && derived_item(owner, target)
            }
            Anchor::Frontier | Anchor::Never => false,
        };
        if kept {
            added.push(row);
        }
    }
    (removed, added, active)
}
