//! Port of `v7/src/0_reader/1a_syntax_grapher.pl` `reify_syntax/4`. Reader
//! forms become graph rows with every identity unchanged; the first bad node
//! ends the walk and is the only diagnostic.

use super::_0_rows::{args_of, is_prolog_atom, rows_of_terms, syntax_graph_diagnostic};
use crate::_6_eval::{Row, TermId, Universe};
use std::collections::HashMap;

pub struct Reifier<'a> {
    pub u: &'a mut Universe,
    pub sources: HashMap<TermId, Vec<TermId>>,
    pub rows: Vec<TermId>,
    pub item: TermId,
    pub none: TermId,
    /// `false` checks every node and builds no row: only the diagnostics are wanted.
    pub build: bool,
}

pub fn reify_terms(
    u: &mut Universe,
    forms: &[TermId],
    source_rows: &[TermId],
) -> (Vec<TermId>, Vec<TermId>) {
    walk(u, forms, source_rows, true)
}

/// `reify_terms`' diagnostics alone: the same checks, no row built.
pub fn check_terms(u: &mut Universe, forms: &[TermId], source_rows: &[TermId]) -> Vec<TermId> {
    walk(u, forms, source_rows, false).1
}

fn walk(
    u: &mut Universe,
    forms: &[TermId],
    source_rows: &[TermId],
    build: bool,
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
            build,
        };
        for (index, &node) in forms.iter().enumerate() {
            if let Some(d) = r.node(node) {
                return (Vec::new(), vec![d]);
            }
            if !build {
                continue;
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
    // A set of rows: integer order and dedup, no structural term comparison.
    rows.sort_unstable_by_key(|row| row.0);
    rows.dedup();
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
        let source = match self.source_row(id) {
            Ok(source) => source,
            Err(diagnostic) => return Some(diagnostic),
        };
        let node_row = self.row("node", vec![id]);
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
                self.row("syntax_atom", vec![id, args[0]])
            }
            ("literal", 1) => self.row("syntax_literal", vec![id, args[0]]),
            ("variable", 2) => {
                if !is_prolog_atom(self.u, args[1]) {
                    let payload = self
                        .u
                        .compound("invalid_variable_payload", vec![args[0], args[1]]);
                    return Some(syntax_graph_diagnostic(self.u, id, payload));
                }
                self.row("syntax_variable", vec![id, args[0], args[1]])
            }
            ("form", 1) => return self.form_node(id, args[0], node_row, source),
            _ => return Some(self.invalid_payload(id, payload)),
        };
        if self.build {
            self.rows.extend([node_row, payload_row].into_iter().flatten());
            self.rows.push(source);
        }
        None
    }

    /// The row `name(args)`, built only when the walk builds rows.
    fn row(&mut self, name: &str, args: Vec<TermId>) -> Option<TermId> {
        self.build.then(|| self.u.compound(name, args))
    }

    /// Exactly one `source/8` row per node id; zero or many is a diagnostic.
    fn source_row(&mut self, id: TermId) -> Result<TermId, TermId> {
        let found: Vec<TermId> = self.sources.get(&id).cloned().unwrap_or_default();
        match found.len() {
            1 => Ok(found[0]),
            0 => {
                let payload = self.u.atom("missing_source");
                Err(syntax_graph_diagnostic(self.u, id, payload))
            }
            _ => {
                let list = self.u.list(&found);
                let payload = self.u.compound("duplicate_source", vec![list]);
                Err(syntax_graph_diagnostic(self.u, id, payload))
            }
        }
    }

    /// A form pushes its own three rows, then one `item` edge per child.
    fn form_node(
        &mut self,
        id: TermId,
        children_term: TermId,
        node_row: Option<TermId>,
        source: TermId,
    ) -> Option<TermId> {
        let Some(children) = self.u.as_list(children_term) else {
            let payload = self
                .u
                .compound("invalid_form_children", vec![children_term]);
            return Some(syntax_graph_diagnostic(self.u, id, payload));
        };
        let form_row = self.row("syntax_form", vec![id]);
        if self.build {
            self.rows.extend([node_row, form_row].into_iter().flatten());
            self.rows.push(source);
        }
        for (index, &child) in children.iter().enumerate() {
            if let Some(d) = self.node(child) {
                return Some(d);
            }
            if !self.build {
                continue;
            }
            let Some(child_id) = args_of(self.u, child, "node", 2).map(|a| a[0]) else {
                continue;
            };
            let target = self.u.compound("ref", vec![child_id]);
            let ordinal = self.u.int(index as i64);
            let item = self.item;
            let edge = self.u.compound(":", vec![id, item, target, ordinal]);
            self.rows.push(edge);
        }
        None
    }

    fn invalid_payload(&mut self, id: TermId, payload: TermId) -> TermId {
        let payload = self.u.compound("invalid_reader_payload", vec![payload]);
        syntax_graph_diagnostic(self.u, id, payload)
    }
}
