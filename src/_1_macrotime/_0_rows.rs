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
    let spelling = match u.get(text) {
        Term::Str(s) => u.sym_str(*s).to_string(),
        Term::Int(n) => n.to_string(),
        _ => return text,
    };
    u.atom(&spelling)
}

/// `atom_string(Name, Text)` read left to right.
pub fn text_of_atom(u: &mut Universe, name: TermId) -> TermId {
    let Term::Atom(symbol) = u.get(name) else {
        return name;
    };
    let text = u.sym_str(*symbol).to_string();
    u.string(&text)
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
    /// `rows` as a set
    pub set: HashSet<TermId>,
    /// order-independent hash of `rows`: the wrapping sum of `row_hash`
    pub hash: u64,
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
            if g.set.insert(row) {
                g.hash = g.hash.wrapping_add(row_hash(row));
            }
            g.index(u, row, Edit::Append);
        }
        g
    }

    /// Remove `removed` and add `added`, keeping every index in the order a
    /// sorted row set would give it. `rows` is left unsorted.
    pub fn apply(&mut self, u: &mut Universe, removed: &[TermId], added: &[TermId]) {
        let gone: HashSet<TermId> = removed
            .iter()
            .copied()
            .filter(|row| self.set.remove(row))
            .collect();
        if !gone.is_empty() {
            self.rows.retain(|row| !gone.contains(row));
        }
        for &row in &gone {
            self.hash = self.hash.wrapping_sub(row_hash(row));
            self.index(u, row, Edit::Remove);
        }
        for &row in added {
            if self.set.insert(row) {
                self.rows.push(row);
                self.hash = self.hash.wrapping_add(row_hash(row));
                self.index(u, row, Edit::Insert);
            }
        }
    }

    fn index(&mut self, u: &mut Universe, row: TermId, edit: Edit) {
        let (name, args) = match u.functor(row) {
            Some((n, a)) => (n.to_string(), a.to_vec()),
            None => return,
        };
        let payload = match (name.as_str(), args.len()) {
            ("node", 1) => {
                if let Edit::Remove = edit {
                    self.nodes.remove(&args[0]);
                } else {
                    self.nodes.insert(args[0]);
                }
                return;
            }
            ("syntax_form", 1) => {
                if let Edit::Remove = edit {
                    self.forms.remove(&args[0]);
                } else {
                    self.forms.insert(args[0]);
                }
                u.atom("form")
            }
            ("syntax_atom", 2) => u.compound("atom", vec![args[1]]),
            ("syntax_literal", 2) => u.compound("literal", vec![args[1]]),
            ("syntax_variable", 3) => u.compound("variable", vec![args[1], args[2]]),
            ("syntax_frontier", 2) => {
                edit_ordered(&mut self.frontier, (args[0], args[1]), edit, |a, b| {
                    u.cmp(a.0, b.0).then_with(|| u.cmp(a.1, b.1))
                });
                return;
            }
            ("source", 8) => {
                let sources = self.sources.entry(args[0]).or_default();
                edit_ordered(sources, row, edit, |a, b| u.cmp(*a, *b));
                if sources.is_empty() {
                    self.sources.remove(&args[0]);
                }
                return;
            }
            (":", 4) if is_atom_named(u, args[1], "item") => {
                if let Some(target) = u.unary(args[2], "ref") {
                    // Row order of one owner's item rows: target, then ordinal.
                    let items = self.items.entry(args[0]).or_default();
                    edit_ordered(items, (args[3], target), edit, |a, b| {
                        u.cmp(a.1, b.1).then_with(|| u.cmp(a.0, b.0))
                    });
                    if items.is_empty() {
                        self.items.remove(&args[0]);
                    }
                }
                return;
            }
            _ => return,
        };
        let node = args[0];
        // Payload order is row order: `form` before `atom/1` before
        // `literal/1` before `variable/2`, as their rows sort.
        let payloads = self.payloads.entry(node).or_default();
        edit_ordered(payloads, payload, edit, |a, b| u.cmp(*a, *b));
        if payloads.is_empty() {
            self.payloads.remove(&node);
        }
        if name != "syntax_atom" {
            return;
        }
        let first = self.payloads.get(&node).and_then(|payloads| {
            payloads
                .iter()
                .find_map(|payload| u.unary(*payload, "atom"))
        });
        match (edit, first) {
            (Edit::Append, _) => {
                self.names.entry(node).or_insert(args[1]);
            }
            (_, Some(name)) => {
                self.names.insert(node, name);
            }
            (_, None) => {
                self.names.remove(&node);
            }
        }
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

#[derive(Clone, Copy)]
enum Edit {
    /// `build`: rows arrive in sorted order
    Append,
    Insert,
    Remove,
}

fn edit_ordered<T: PartialEq>(
    items: &mut Vec<T>,
    value: T,
    edit: Edit,
    cmp: impl Fn(&T, &T) -> std::cmp::Ordering,
) {
    match edit {
        Edit::Append => items.push(value),
        Edit::Insert => {
            let at = items
                .binary_search_by(|probe| cmp(probe, &value))
                .unwrap_or_else(|at| at);
            items.insert(at, value);
        }
        Edit::Remove => {
            if let Some(at) = items.iter().position(|item| *item == value) {
                items.remove(at);
            }
        }
    }
}

/// One row's share of `Graph::hash`; term ids are hash-consed, so equal rows
/// share an id. splitmix64 finalizer.
pub fn row_hash(row: TermId) -> u64 {
    let mut z = u64::from(row.0).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
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
