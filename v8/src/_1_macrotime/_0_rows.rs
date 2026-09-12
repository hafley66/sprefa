//! A syntax graph row is an ordinary Prolog term. Inside this operator a row
//! is the whole term, so `sort/2` is `u.cmp` plus dedup; `Row` is only the
//! shape the public API hands back, with the functor name as the relation.

use crate::_6_eval::{Row, Term, TermId, Universe};
use std::collections::{HashMap, HashSet};

pub fn args_of<'a>(u: &'a Universe, id: TermId, name: &str, arity: usize) -> Option<&'a [TermId]> {
    match u.functor(id) {
        Some((n, args)) if n == name && args.len() == arity => Some(args),
        _ => None,
    }
}

pub fn is_atom_named(u: &Universe, id: TermId, name: &str) -> bool {
    matches!(u.get(id), Term::Atom(s) if u.sym_str(*s) == name)
}

/// SWI-Prolog 7 keeps `[]` outside the atom type, so `atom([])` is false.
pub fn is_prolog_atom(u: &Universe, id: TermId) -> bool {
    matches!(u.get(id), Term::Atom(s) if *s != u.nil)
}

/// `atom_string(Name, Text)` read right to left.
pub fn atom_of_text(u: &mut Universe, text: TermId) -> TermId {
    match u.get(text).clone() {
        Term::Str(s) => {
            let name = u.sym_str(s).to_string();
            u.atom(&name)
        }
        Term::Int(n) => u.atom(&n.to_string()),
        _ => text,
    }
}

/// `atom_string(Name, Text)` read left to right.
pub fn text_of_atom(u: &mut Universe, name: TermId) -> TermId {
    match u.get(name).clone() {
        Term::Atom(s) => {
            let text = u.sym_str(s).to_string();
            u.string(&text)
        }
        _ => name,
    }
}

pub fn sort_terms(u: &Universe, rows: &mut Vec<TermId>) {
    rows.sort_by(|a, b| u.cmp(*a, *b));
    rows.dedup();
}

pub fn term_of_row(u: &mut Universe, row: &Row) -> TermId {
    if row.args.is_empty() {
        return row.rel;
    }
    let name = match u.get(row.rel) {
        Term::Atom(s) => *s,
        _ => return row.rel,
    };
    u.intern(Term::Compound(name, row.args.clone()))
}

pub fn row_of_term(u: &mut Universe, id: TermId) -> Row {
    match u.get(id).clone() {
        Term::Compound(name, args) => {
            let rel = u.intern(Term::Atom(name));
            Row { rel, args }
        }
        _ => Row {
            rel: id,
            args: Vec::new(),
        },
    }
}

pub fn terms_of_rows(u: &mut Universe, rows: &[Row]) -> Vec<TermId> {
    rows.iter().map(|r| term_of_row(u, r)).collect()
}

pub fn rows_of_terms(u: &mut Universe, terms: &[TermId]) -> Vec<Row> {
    terms.iter().map(|t| row_of_term(u, *t)).collect()
}

pub fn diagnostic(u: &mut Universe, phase: &str, subject: TermId, payload: TermId) -> TermId {
    let phase = u.atom(phase);
    u.compound("diagnostic", vec![phase, subject, payload])
}

pub fn syntax_graph_diagnostic(u: &mut Universe, subject: TermId, payload: TermId) -> TermId {
    diagnostic(u, "syntax_graph", subject, payload)
}

pub fn macrotime_diagnostic(u: &mut Universe, subject: TermId, payload: TermId) -> TermId {
    diagnostic(u, "macrotime", subject, payload)
}

/// Which nodes a row is attached to; `row_reachable/2` keeps a row only when
/// every one of them is still active, and drops rows with no clause at all.
pub enum Anchor {
    Frontier,
    One(TermId),
    Item(TermId, TermId),
    Never,
}

pub fn anchor(u: &Universe, row: TermId) -> Anchor {
    let (name, args) = match u.functor(row) {
        Some(f) => f,
        None => return Anchor::Never,
    };
    match (name, args.len()) {
        ("syntax_frontier", 2) => Anchor::Frontier,
        ("node", 1) | ("syntax_form", 1) => Anchor::One(args[0]),
        ("syntax_atom", 2) | ("syntax_literal", 2) => Anchor::One(args[0]),
        ("syntax_variable", 3) | ("source", 8) => Anchor::One(args[0]),
        (":", 4) if is_atom_named(u, args[1], "item") => match u.unary(args[2], "ref") {
            Some(target) => Anchor::Item(args[0], target),
            None => Anchor::Never,
        },
        _ => Anchor::Never,
    }
}

/// One keyed pass over a row set. Every read the expander and the materializer
/// make is a lookup here; v7 rescans the row list for each of them.
#[derive(Default)]
pub struct Graph {
    pub rows: Vec<TermId>,
    /// `syntax_frontier(Ordinal, Node)` in row order
    pub frontier: Vec<(TermId, TermId)>,
    /// owner to `(ordinal, target)` for label `item`, in row order
    pub items: HashMap<TermId, Vec<(TermId, TermId)>>,
    pub forms: HashSet<TermId>,
    pub nodes: HashSet<TermId>,
    /// node to `form` / `atom(Name)` / `literal(Value)` / `variable(Id, Name)`
    pub payloads: HashMap<TermId, Vec<TermId>>,
    /// node to the first `syntax_atom` name in row order
    pub names: HashMap<TermId, TermId>,
    pub sources: HashMap<TermId, Vec<TermId>>,
}

impl Graph {
    pub fn build(u: &mut Universe, rows: Vec<TermId>) -> Graph {
        let mut g = Graph {
            rows,
            ..Graph::default()
        };
        for i in 0..g.rows.len() {
            let row = g.rows[i];
            let (name, args) = match u.functor(row) {
                Some((n, a)) => (n.to_string(), a.to_vec()),
                None => continue,
            };
            match (name.as_str(), args.len()) {
                ("node", 1) => {
                    g.nodes.insert(args[0]);
                }
                ("syntax_form", 1) => {
                    g.forms.insert(args[0]);
                    let payload = u.atom("form");
                    g.payloads.entry(args[0]).or_default().push(payload);
                }
                ("syntax_atom", 2) => {
                    let payload = u.compound("atom", vec![args[1]]);
                    g.payloads.entry(args[0]).or_default().push(payload);
                    g.names.entry(args[0]).or_insert(args[1]);
                }
                ("syntax_literal", 2) => {
                    let payload = u.compound("literal", vec![args[1]]);
                    g.payloads.entry(args[0]).or_default().push(payload);
                }
                ("syntax_variable", 3) => {
                    let payload = u.compound("variable", vec![args[1], args[2]]);
                    g.payloads.entry(args[0]).or_default().push(payload);
                }
                ("syntax_frontier", 2) => g.frontier.push((args[0], args[1])),
                ("source", 8) => g.sources.entry(args[0]).or_default().push(row),
                (":", 4) if is_atom_named(u, args[1], "item") => {
                    if let Some(target) = u.unary(args[2], "ref") {
                        g.items.entry(args[0]).or_default().push((args[3], target));
                    }
                }
                _ => {}
            }
        }
        g
    }

    /// `keysort/2` is stable, so ties keep the row order the sorted row set
    /// already put them in.
    pub fn ordered_frontier(&self, u: &Universe) -> Vec<(TermId, TermId)> {
        let mut pairs = self.frontier.clone();
        pairs.sort_by(|a, b| u.cmp(a.0, b.0));
        pairs
    }

    pub fn ordered_children(&self, u: &Universe, owner: TermId) -> Vec<(TermId, TermId)> {
        let mut pairs = self.items.get(&owner).cloned().unwrap_or_default();
        pairs.sort_by(|a, b| u.cmp(a.0, b.0));
        pairs
    }

    /// `syntax_row_for_node/2`: the node carries a payload alternative.
    pub fn has_payload(&self, node: TermId) -> bool {
        self.payloads.contains_key(&node)
    }

    pub fn active_nodes(&self) -> HashSet<TermId> {
        let mut queue: Vec<TermId> = self.frontier.iter().map(|(_, n)| *n).collect();
        queue.sort_by_key(|t| t.0);
        queue.dedup();
        let mut seen: HashSet<TermId> = HashSet::new();
        while let Some(node) = queue.pop() {
            if !seen.insert(node) {
                continue;
            }
            if let Some(children) = self.items.get(&node) {
                for (_, child) in children {
                    if !seen.contains(child) {
                        queue.push(*child);
                    }
                }
            }
        }
        seen
    }
}

/// `numlist(0, Count - 1, Expected)` compared against the ordinals a keysort
/// produced.
pub fn dense_ordinals(u: &Universe, pairs: &[(TermId, TermId)]) -> bool {
    pairs
        .iter()
        .enumerate()
        .all(|(i, (ordinal, _))| u.as_int(*ordinal) == Some(i as i64))
}

pub fn ordinal_list(u: &mut Universe, pairs: &[(TermId, TermId)]) -> TermId {
    let ordinals: Vec<TermId> = pairs.iter().map(|(o, _)| *o).collect();
    u.list(&ordinals)
}
