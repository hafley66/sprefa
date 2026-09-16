//! Reader-node shapes. Port of `bind_form/4` (`0_lowerer.pl:1940`),
//! `edge_bind_form/4` (`:1948`), `rule_form/4` (`:1952`), `node_id/2` (`:1956`)
//! and `expression_bind_target/1` (`:713`).

use crate::_6_eval::term::{TermId, Universe};

pub struct Node {
    pub id: TermId,
    pub payload: TermId,
}

/// `node(NodeId, Payload)`.
pub fn node(u: &Universe, term: TermId) -> Option<Node> {
    let (name, args) = u.functor(term)?;
    if name != "node" || args.len() != 2 {
        return None;
    }
    Some(Node {
        id: args[0],
        payload: args[1],
    })
}

pub fn node_id(u: &Universe, term: TermId) -> Option<TermId> {
    node(u, term).map(|n| n.id)
}

/// `form(Nodes)` as a proper list.
pub fn form(u: &Universe, payload: TermId) -> Option<Vec<TermId>> {
    u.unary(payload, "form").and_then(|items| u.as_list(items))
}

pub fn atom_name(u: &Universe, payload: TermId) -> Option<TermId> {
    u.unary(payload, "atom")
}

pub fn literal_value(u: &Universe, payload: TermId) -> Option<TermId> {
    u.unary(payload, "literal")
}

/// `variable(Identity, Name)`.
pub fn variable(u: &Universe, payload: TermId) -> Option<(TermId, TermId)> {
    let (name, args) = u.functor(payload)?;
    if name != "variable" || args.len() != 2 {
        return None;
    }
    Some((args[0], args[1]))
}

/// The head of a form, when it is an atom node.
pub fn form_head_atom(u: &Universe, items: &[TermId]) -> Option<TermId> {
    let head = node(u, *items.first()?)?;
    atom_name(u, head.payload)
}

fn head_is(u: &Universe, items: &[TermId], want: &str) -> bool {
    form_head_atom(u, items).is_some_and(|a| u.functor_or_atom(a).is_some_and(|(n, _)| n == want))
}

pub struct Bind {
    pub bind_node: TermId,
    pub name: TermId,
    pub target: TermId,
}

/// `:1940`. Two clauses: an atom label, and the empty form naming `'()'`.
pub fn bind_form(u: &mut Universe, term: TermId) -> Option<Bind> {
    let n = node(u, term)?;
    let items = form(u, n.payload)?;
    if items.len() != 3 || !head_is(u, &items, ":") {
        return None;
    }
    let label = node(u, items[1])?;
    if let Some(name) = atom_name(u, label.payload) {
        return Some(Bind {
            bind_node: n.id,
            name,
            target: items[2],
        });
    }
    if form(u, label.payload).is_some_and(|inner| inner.is_empty()) {
        return Some(Bind {
            bind_node: n.id,
            name: u.atom("()"),
            target: items[2],
        });
    }
    None
}

pub struct EdgeBind {
    pub bind_node: TermId,
    pub label: TermId,
    pub target: TermId,
}

/// `:1948`. Any label node.
pub fn edge_bind_form(u: &Universe, term: TermId) -> Option<EdgeBind> {
    let n = node(u, term)?;
    let items = form(u, n.payload)?;
    if items.len() != 3 || !head_is(u, &items, ":") {
        return None;
    }
    Some(EdgeBind {
        bind_node: n.id,
        label: items[1],
        target: items[2],
    })
}

pub struct RuleForm {
    pub rule_node: TermId,
    pub head: TermId,
    pub body: Vec<TermId>,
}

/// `:1952`.
pub fn rule_form(u: &Universe, term: TermId) -> Option<RuleForm> {
    let n = node(u, term)?;
    let items = form(u, n.payload)?;
    if items.len() < 2 || !head_is(u, &items, "<-") {
        return None;
    }
    Some(RuleForm {
        rule_node: n.id,
        head: items[1],
        body: items[2..].to_vec(),
    })
}

/// `:713`. A form target is an expression unless it is empty or headed by
/// `*`, `+` or `Host`.
pub fn expression_bind_target(u: &Universe, target: TermId) -> bool {
    let Some(n) = node(u, target) else {
        return false;
    };
    let Some(items) = form(u, n.payload) else {
        return false;
    };
    if items.is_empty() {
        return false;
    }
    !(head_is(u, &items, "*") || head_is(u, &items, "+") || head_is(u, &items, "Host"))
}
