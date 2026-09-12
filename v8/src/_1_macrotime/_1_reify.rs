//! Port of `v7/src/0_reader/1a_syntax_grapher.pl` `reify_syntax/4`. Reader
//! forms become graph rows with every identity unchanged; the first bad node
//! ends the walk and is the only diagnostic.

use super::_0_rows::{args_of, is_prolog_atom, rows_of_terms, sort_terms, syntax_graph_diagnostic};
use crate::_6_eval::{Row, TermId, Universe};
use std::collections::HashMap;

pub struct Reifier<'a> {
    pub u: &'a mut Universe,
    pub sources: HashMap<TermId, Vec<TermId>>,
    pub rows: Vec<TermId>,
    pub item: TermId,
    pub none: TermId,
}

pub fn reify_terms(
    u: &mut Universe,
    forms: &[TermId],
    source_rows: &[TermId],
) -> (Vec<TermId>, Vec<TermId>) {
    let mut sources: HashMap<TermId, Vec<TermId>> = HashMap::new();
    for &row in source_rows {
        if let Some(args) = args_of(u, row, "source", 8) {
            let node = args[0];
            sources.entry(node).or_default().push(row);
        }
    }
    let item = u.atom("item");
    let none = u.atom("none");
    let mut rows;
    {
        let mut r = Reifier {
            u: &mut *u,
            sources,
            rows: Vec::new(),
            item,
            none,
        };
        for (index, &node) in forms.iter().enumerate() {
            if let Some(d) = r.node(node) {
                return (Vec::new(), vec![d]);
            }
            let id = match args_of(r.u, node, "node", 2) {
                Some(args) => args[0],
                None => continue,
            };
            let ordinal = r.u.int(index as i64);
            let row = r.u.compound("syntax_frontier", vec![ordinal, id]);
            r.rows.push(row);
        }
        rows = r.rows;
    }
    sort_terms(u, &mut rows);
    (rows, Vec::new())
}

pub fn reify(
    u: &mut Universe,
    forms: &[TermId],
    source_rows: &[TermId],
) -> (Vec<Row>, Vec<TermId>) {
    let (rows, diagnostics) = reify_terms(u, forms, source_rows);
    (rows_of_terms(u, &rows), diagnostics)
}

impl Reifier<'_> {
    /// `Some(diagnostic)` ends the whole reification, as v7's `error/1`.
    pub fn node(&mut self, node: TermId) -> Option<TermId> {
        let (id, payload) = match args_of(self.u, node, "node", 2) {
            Some(args) => (args[0], args[1]),
            None => {
                let payload = self.u.compound("invalid_reader_node", vec![node]);
                let none = self.none;
                return Some(syntax_graph_diagnostic(self.u, none, payload));
            }
        };
        let found: Vec<TermId> = self.sources.get(&id).cloned().unwrap_or_default();
        let source = match found.len() {
            1 => found[0],
            0 => {
                let payload = self.u.atom("missing_source");
                return Some(syntax_graph_diagnostic(self.u, id, payload));
            }
            _ => {
                let list = self.u.list(&found);
                let payload = self.u.compound("duplicate_source", vec![list]);
                return Some(syntax_graph_diagnostic(self.u, id, payload));
            }
        };
        let node_row = self.u.compound("node", vec![id]);
        let (name, args) = match self.u.functor(payload) {
            Some((n, a)) => (n.to_string(), a.to_vec()),
            None => return Some(self.invalid_payload(id, payload)),
        };
        let payload_row = match (name.as_str(), args.len()) {
            ("atom", 1) => {
                if !is_prolog_atom(self.u, args[0]) {
                    let payload = self.u.compound("invalid_atom_payload", vec![args[0]]);
                    return Some(syntax_graph_diagnostic(self.u, id, payload));
                }
                self.u.compound("syntax_atom", vec![id, args[0]])
            }
            ("literal", 1) => self.u.compound("syntax_literal", vec![id, args[0]]),
            ("variable", 2) => {
                if !is_prolog_atom(self.u, args[1]) {
                    let payload = self
                        .u
                        .compound("invalid_variable_payload", vec![args[0], args[1]]);
                    return Some(syntax_graph_diagnostic(self.u, id, payload));
                }
                self.u
                    .compound("syntax_variable", vec![id, args[0], args[1]])
            }
            ("form", 1) => {
                let children = match self.u.as_list(args[0]) {
                    Some(children) => children,
                    None => {
                        let payload = self.u.compound("invalid_form_children", vec![args[0]]);
                        return Some(syntax_graph_diagnostic(self.u, id, payload));
                    }
                };
                let form_row = self.u.compound("syntax_form", vec![id]);
                self.rows.push(node_row);
                self.rows.push(form_row);
                self.rows.push(source);
                for (index, &child) in children.iter().enumerate() {
                    if let Some(d) = self.node(child) {
                        return Some(d);
                    }
                    let child_id = match args_of(self.u, child, "node", 2) {
                        Some(a) => a[0],
                        None => continue,
                    };
                    let target = self.u.compound("ref", vec![child_id]);
                    let ordinal = self.u.int(index as i64);
                    let item = self.item;
                    let edge = self.u.compound(":", vec![id, item, target, ordinal]);
                    self.rows.push(edge);
                }
                return None;
            }
            _ => return Some(self.invalid_payload(id, payload)),
        };
        self.rows.push(node_row);
        self.rows.push(payload_row);
        self.rows.push(source);
        None
    }

    fn invalid_payload(&mut self, id: TermId, payload: TermId) -> TermId {
        let payload = self.u.compound("invalid_reader_payload", vec![payload]);
        syntax_graph_diagnostic(self.u, id, payload)
    }
}
