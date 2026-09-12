//! Port of `v7/src/1_libtime/0b_syntax_rewriter.pl`. One wave: walk the active
//! frontier in ordinal order, replace each claimed node by its ordered
//! expansion outputs, rebuild the child edges of every form above them, then
//! keep only what the new frontier still reaches.

use super::_0_rows::{
    anchor, args_of, dense_ordinals, is_atom_named, macrotime_diagnostic, ordinal_list, sort_terms,
    Anchor, Graph,
};
use super::_2_protocol::{Claim, Output};
use crate::_6_eval::{TermId, Universe};
use std::collections::HashMap;

pub struct Rewrite {
    pub rows: Vec<TermId>,
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
    available: &Graph,
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
            available,
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
            rows: Vec::new(),
            origin: Vec::new(),
            diagnostics,
        };
    }
    sort_terms(u, &mut origin);
    let rows = apply_rewrite(u, g, available, &new_roots, &overrides);
    Rewrite {
        rows,
        origin,
        diagnostics: Vec::new(),
    }
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

/// `apply_rewrite/5`: merge in what the closure derived, drop the old frontier
/// and every overridden child edge, write the new ones, then prune to the
/// nodes the new frontier reaches.
fn apply_rewrite(
    u: &mut Universe,
    g: &Graph,
    available: &Graph,
    roots: &[TermId],
    overrides: &[(TermId, Vec<TermId>)],
) -> Vec<TermId> {
    let mut all: Vec<TermId> = g.rows.clone();
    all.extend_from_slice(&available.rows);
    sort_terms(u, &mut all);
    let owners: std::collections::HashSet<TermId> =
        overrides.iter().map(|(owner, _)| *owner).collect();
    let mut candidates: Vec<TermId> = all
        .into_iter()
        .filter(|row| {
            if args_of(u, *row, "syntax_frontier", 2).is_some() {
                return false;
            }
            match args_of(u, *row, ":", 4) {
                Some(args) if is_atom_named(u, args[1], "item") => !owners.contains(&args[0]),
                _ => true,
            }
        })
        .collect();
    for (index, root) in roots.iter().enumerate() {
        let ordinal = u.int(index as i64);
        let row = u.compound("syntax_frontier", vec![ordinal, *root]);
        candidates.push(row);
    }
    let item = u.atom("item");
    for (owner, children) in overrides {
        for (index, child) in children.iter().enumerate() {
            let target = u.compound("ref", vec![*child]);
            let ordinal = u.int(index as i64);
            let row = u.compound(":", vec![*owner, item, target, ordinal]);
            candidates.push(row);
        }
    }
    sort_terms(u, &mut candidates);
    let next = Graph::build(u, candidates);
    let reachable = next.active_nodes();
    let mut rows: Vec<TermId> = next
        .rows
        .iter()
        .copied()
        .filter(|row| match anchor(u, *row) {
            Anchor::Frontier => true,
            Anchor::One(node) => reachable.contains(&node),
            Anchor::Item(owner, target) => {
                reachable.contains(&owner) && reachable.contains(&target)
            }
            Anchor::Never => false,
        })
        .collect();
    sort_terms(u, &mut rows);
    rows
}
