//! Port of `v7/src/0_reader/1b_syntax_materializer.pl` `materialize_syntax/4`.
//! The active frontier becomes reader forms again with every identity intact.
//! A node that fails any check contributes no form, only diagnostics.

use super::_0_rows::{
    dense_ordinals, ordinal_list, rows_of_terms, sort_terms, syntax_graph_diagnostic,
    terms_of_rows, Graph,
};
use crate::_6_eval::{Row, TermId, Universe};
use std::collections::HashSet;

pub fn materialize(u: &mut Universe, rows: &[Row]) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let terms = terms_of_rows(u, rows);
    materialize_terms(u, terms)
}

pub fn materialize_terms(
    u: &mut Universe,
    rows: Vec<TermId>,
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let g = Graph::build(u, rows);
    let pairs = g.ordered_frontier(u);
    if !dense_ordinals(u, &pairs) {
        let list = ordinal_list(u, &pairs);
        let label = u.atom("syntax_frontier");
        let payload = u.compound("non_dense", vec![label, list]);
        let none = u.atom("none");
        let d = syntax_graph_diagnostic(u, none, payload);
        return (Vec::new(), Vec::new(), vec![d]);
    }
    let roots: Vec<TermId> = pairs.iter().map(|(_, node)| *node).collect();
    let mut pass = Pass {
        u: &mut *u,
        g: &g,
        seen: HashSet::new(),
    };
    let mut forms = Vec::new();
    let mut sources = Vec::new();
    let mut diagnostics = Vec::new();
    pass.nodes(&roots, &mut forms, &mut sources, &mut diagnostics);
    (forms, sources, diagnostics)
}

struct Pass<'a> {
    u: &'a mut Universe,
    g: &'a Graph,
    seen: HashSet<TermId>,
}

impl Pass<'_> {
    fn nodes(
        &mut self,
        nodes: &[TermId],
        forms: &mut Vec<TermId>,
        sources: &mut Vec<TermId>,
        diagnostics: &mut Vec<TermId>,
    ) {
        for &node in nodes {
            self.node(node, forms, sources, diagnostics);
        }
    }

    fn node(
        &mut self,
        node: TermId,
        forms: &mut Vec<TermId>,
        sources: &mut Vec<TermId>,
        diagnostics: &mut Vec<TermId>,
    ) {
        if !self.seen.insert(node) {
            let payload = self.u.atom("reused_syntax_occurrence");
            diagnostics.push(syntax_graph_diagnostic(self.u, node, payload));
            return;
        }
        let mut own: Vec<TermId> = Vec::new();
        if !self.g.nodes.contains(&node) {
            let kind = self.u.atom("syntax_node");
            let empty = self.u.empty_list();
            let payload = self.u.compound("expected_one", vec![kind, empty]);
            own.push(syntax_graph_diagnostic(self.u, node, payload));
        }
        let mut payloads = self.g.payloads.get(&node).cloned().unwrap_or_default();
        sort_terms(self.u, &mut payloads);
        let payload = if payloads.len() == 1 {
            Some(payloads[0])
        } else {
            let list = self.u.list(&payloads);
            let reason = self.u.compound("payload_alternatives", vec![list]);
            own.push(syntax_graph_diagnostic(self.u, node, reason));
            None
        };
        let found = self.g.sources.get(&node).cloned().unwrap_or_default();
        let source = if found.len() == 1 {
            Some(found[0])
        } else {
            let list = self.u.list(&found);
            let reason = self.u.compound("source_rows", vec![list]);
            own.push(syntax_graph_diagnostic(self.u, node, reason));
            None
        };
        let mut child_sources: Vec<TermId> = Vec::new();
        let reader_payload = match payload {
            None => None,
            Some(p) => self.payload(node, p, &mut own, &mut child_sources),
        };
        diagnostics.extend(own.iter().copied());
        if own.is_empty() {
            if let (Some(reader_payload), Some(source)) = (reader_payload, source) {
                let form = self.u.compound("node", vec![node, reader_payload]);
                forms.push(form);
                sources.push(source);
            }
        }
        sources.append(&mut child_sources);
    }

    /// `materialize_payload/8`. A form walks its child edges, everything else
    /// is the payload term the graph already holds.
    fn payload(
        &mut self,
        node: TermId,
        payload: TermId,
        own: &mut Vec<TermId>,
        child_sources: &mut Vec<TermId>,
    ) -> Option<TermId> {
        let name = match self.u.functor_or_atom(payload) {
            Some((n, _)) => n.to_string(),
            None => return Some(payload),
        };
        if name != "form" {
            return Some(payload);
        }
        let pairs = self.g.ordered_children(self.u, node);
        if !dense_ordinals(self.u, &pairs) {
            let list = ordinal_list(self.u, &pairs);
            let label = self.u.atom("item");
            let reason = self.u.compound("non_dense", vec![label, list]);
            own.push(syntax_graph_diagnostic(self.u, node, reason));
        }
        let children: Vec<TermId> = pairs.iter().map(|(_, child)| *child).collect();
        let mut child_forms: Vec<TermId> = Vec::new();
        let mut child_diagnostics: Vec<TermId> = Vec::new();
        self.nodes(
            &children,
            &mut child_forms,
            child_sources,
            &mut child_diagnostics,
        );
        own.append(&mut child_diagnostics);
        let list = self.u.list(&child_forms);
        Some(self.u.compound("form", vec![list]))
    }
}

pub fn materialize_rows(u: &mut Universe, rows: &[TermId]) -> (Vec<Row>, Vec<TermId>, Vec<TermId>) {
    let (forms, sources, diagnostics) = materialize_terms(u, rows.to_vec());
    (rows_of_terms(u, &forms), sources, diagnostics)
}
