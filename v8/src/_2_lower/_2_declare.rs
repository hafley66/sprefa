//! Passes 1 and 2: mint every nested owner, reserve every bind in that owner.
//! Port of `0_lowerer.pl:672-982`.

use super::forms;
use super::host;
use crate::_6_eval::term::{TermId, Universe};

#[derive(Default, Debug)]
pub struct Declared {
    pub nodes: Vec<TermId>,
    pub edges: Vec<TermId>,
    pub relations: Vec<TermId>,
    pub origins: Vec<TermId>,
    pub reservations: Vec<TermId>,
}

impl Declared {
    pub fn extend(&mut self, other: Declared) {
        self.nodes.extend(other.nodes);
        self.edges.extend(other.edges);
        self.relations.extend(other.relations);
        self.origins.extend(other.origins);
        self.reservations.extend(other.reservations);
    }
}

/// The seven-tuple `lower_target/4` returns (`:748`).
pub struct Target {
    pub term: TermId,
    pub kind: &'static str,
    pub declared: Declared,
}

pub type Fallible<T> = Result<T, TermId>;

pub fn diagnostic(u: &mut Universe, node: TermId, reason: TermId) -> TermId {
    let lower = u.atom("lower");
    u.compound("diagnostic", vec![lower, node, reason])
}

pub fn reason(u: &mut Universe, name: &str) -> TermId {
    u.atom(name)
}

/// `:673`. Only bind forms advance the ordinal.
pub fn lower_declarations(
    u: &mut Universe,
    items: &[TermId],
    owner: TermId,
    module_identity: TermId,
) -> Fallible<Declared> {
    let mut out = Declared::default();
    let mut index: i64 = 0;
    for item in items {
        if forms::bind_form(u, *item).is_some() {
            out.extend(lower_bind(u, *item, owner, module_identity, index)?);
            index += 1;
        }
    }
    Ok(out)
}

/// `:701`.
pub fn lower_bind(
    u: &mut Universe,
    bind_node: TermId,
    owner: TermId,
    module_identity: TermId,
    index: i64,
) -> Fallible<Declared> {
    let Some(bind) = forms::bind_form(u, bind_node) else {
        let node = forms::node_id(u, bind_node).unwrap_or(bind_node);
        let r = reason(u, "expected_bind");
        return Err(diagnostic(u, node, r));
    };
    if forms::expression_bind_target(u, bind.target) {
        return Ok(finish_derived_bind(
            u,
            bind.bind_node,
            owner,
            bind.name,
            bind.target,
            index,
        ));
    }
    let target = lower_target(u, bind.target, owner, module_identity)?;
    Ok(finish_bind(
        u,
        target,
        bind.bind_node,
        owner,
        bind.name,
        index,
    ))
}

/// `:719`.
fn finish_derived_bind(
    u: &mut Universe,
    bind_node_id: TermId,
    owner: TermId,
    name: TermId,
    target_node: TermId,
    index: i64,
) -> Declared {
    let index_term = u.int(index);
    let edge_target = u.compound("deferred_expression", vec![target_node]);
    let edge = u.compound("pending_edge", vec![owner, name, edge_target, index_term]);
    let reservation_target = u.compound(
        "deferred_expression",
        vec![target_node, bind_node_id, index_term],
    );
    let kind = u.atom("expression");
    let reservation = u.compound("reservation", vec![owner, name, reservation_target, kind]);
    let origin = edge_origin(u, owner, name, index, bind_node_id);
    Declared {
        nodes: vec![],
        edges: vec![edge],
        relations: vec![],
        origins: vec![origin],
        reservations: vec![reservation],
    }
}

pub fn edge_origin(
    u: &mut Universe,
    owner: TermId,
    name: TermId,
    index: i64,
    node: TermId,
) -> TermId {
    let index_term = u.int(index);
    let edge = u.compound("edge", vec![owner, name, index_term]);
    u.compound("origin", vec![edge, node])
}

/// `:728`.
fn finish_bind(
    u: &mut Universe,
    target: Target,
    bind_node_id: TermId,
    owner: TermId,
    name: TermId,
    index: i64,
) -> Declared {
    let index_term = u.int(index);
    let edge = u.compound("pending_edge", vec![owner, name, target.term, index_term]);
    let kind = u.atom(target.kind);
    let reservation = u.compound("reservation", vec![owner, name, target.term, kind]);
    let mut origins = vec![edge_origin(u, owner, name, index, bind_node_id)];
    // :740. A product bind also records the relation it declares.
    if target.kind == "product" {
        if let Some(inner) = u.unary(target.term, "target") {
            let relation = u.compound("relation", vec![inner]);
            origins.push(u.compound("origin", vec![relation, bind_node_id]));
        }
    }
    let mut declared = target.declared;
    let mut edges = vec![edge];
    edges.append(&mut declared.edges);
    origins.extend(declared.origins);
    let mut reservations = vec![reservation];
    reservations.append(&mut declared.reservations);
    Declared {
        nodes: declared.nodes,
        edges,
        relations: declared.relations,
        origins,
        reservations,
    }
}

/// `:748`. Eight clauses in source order.
pub fn lower_target(
    u: &mut Universe,
    target_node: TermId,
    parent_owner: TermId,
    module_identity: TermId,
) -> Fallible<Target> {
    let Some(node) = forms::node(u, target_node) else {
        let r = reason(u, "unsupported_bind_target");
        return Err(diagnostic(u, target_node, r));
    };
    if let Some(items) = forms::form(u, node.payload) {
        if let Some(head) = forms::form_head_atom(u, &items) {
            let head_name = u
                .functor_or_atom(head)
                .map(|(n, _)| n.to_string())
                .unwrap_or_default();
            match head_name.as_str() {
                "*" | "+" => {
                    let kind = if head_name == "*" { "product" } else { "sum" };
                    let owner = u.compound("owner", vec![module_identity, node.id]);
                    let declared = lower_bind_list(u, &items[1..], owner, module_identity)?;
                    return Ok(finish_constructor_target(u, declared, node.id, owner, kind));
                }
                "Host" if items.len() == 4 => {
                    let owner = u.compound("owner", vec![module_identity, node.id]);
                    return host::lower_host_target(u, &items, node.id, owner, module_identity);
                }
                _ => {}
            }
        }
        if items.is_empty() {
            // :774. The empty form is the reference `'()'`.
            let empty = u.atom("()");
            let term = u.compound("name", vec![parent_owner, empty]);
            return Ok(Target {
                term,
                kind: "reference",
                declared: Declared::default(),
            });
        }
        let r = reason(u, "unsupported_bind_target");
        return Err(diagnostic(u, node.id, r));
    }
    if let Some(name) = forms::atom_name(u, node.payload) {
        let term = u.compound("name", vec![parent_owner, name]);
        return Ok(Target {
            term,
            kind: "reference",
            declared: Declared::default(),
        });
    }
    if let Some(value) = forms::literal_value(u, node.payload) {
        let term = u.compound("const", vec![value]);
        return Ok(Target {
            term,
            kind: "literal",
            declared: Declared::default(),
        });
    }
    if forms::variable(u, node.payload).is_some() {
        let r = reason(u, "variable_bind_target");
        return Err(diagnostic(u, node.id, r));
    }
    let r = reason(u, "unsupported_bind_target");
    Err(diagnostic(u, node.id, r))
}

/// `:868`.
pub fn finish_constructor_target(
    u: &mut Universe,
    mut declared: Declared,
    node_id: TermId,
    owner: TermId,
    kind: &'static str,
) -> Target {
    let classifier = u.compound(kind, vec![owner]);
    let identity = u.compound("node", vec![owner]);
    let mut nodes = vec![identity, classifier];
    nodes.append(&mut declared.nodes);
    let mut relations = constructor_relations(u, kind, owner, &declared.edges);
    relations.append(&mut declared.relations);
    let node_origin = u.compound("node", vec![owner]);
    let origin = u.compound("origin", vec![node_origin, node_id]);
    let mut origins = vec![origin];
    origins.append(&mut declared.origins);
    let term = u.compound("target", vec![owner]);
    Target {
        term,
        kind,
        declared: Declared {
            nodes,
            edges: declared.edges,
            relations,
            origins,
            reservations: declared.reservations,
        },
    }
}

/// `:884`. A sum declares no relation.
fn constructor_relations(
    u: &mut Universe,
    kind: &str,
    owner: TermId,
    edges: &[TermId],
) -> Vec<TermId> {
    if kind != "product" {
        return vec![];
    }
    let return_atom = u.atom("return");
    let mut arity: i64 = 0;
    let mut return_indices: Vec<i64> = Vec::new();
    for edge in edges {
        let Some(parts) = super::index::edge_parts(u, *edge) else {
            continue;
        };
        if parts.owner != owner {
            continue;
        }
        arity += 1;
        if parts.name == return_atom {
            return_indices.push(parts.index);
        }
    }
    // :893. One return edge gives one key set of every other position.
    let key_sets = if return_indices.len() == 1 {
        let except = return_indices[0];
        let positions: Vec<TermId> = (0..arity)
            .filter(|i| *i != except)
            .map(|i| u.int(i))
            .collect();
        let inner = u.list(&positions);
        u.list(&[inner])
    } else {
        u.empty_list()
    };
    let arity_term = u.int(arity);
    vec![u.compound("relation", vec![owner, arity_term, key_sets])]
}

/// `:917`.
pub fn lower_bind_list(
    u: &mut Universe,
    bindings: &[TermId],
    owner: TermId,
    module_identity: TermId,
) -> Fallible<Declared> {
    let mut out = Declared::default();
    for (index, bind) in bindings.iter().enumerate() {
        out.extend(lower_edge_bind(
            u,
            *bind,
            owner,
            module_identity,
            index as i64,
        )?);
    }
    Ok(out)
}

/// `:938`.
fn lower_edge_bind(
    u: &mut Universe,
    bind_node: TermId,
    owner: TermId,
    module_identity: TermId,
    index: i64,
) -> Fallible<Declared> {
    let Some(bind) = forms::edge_bind_form(u, bind_node) else {
        let node = forms::node_id(u, bind_node).unwrap_or(bind_node);
        let r = reason(u, "expected_bind");
        return Err(diagnostic(u, node, r));
    };
    let Some(label) = forms::node(u, bind.label) else {
        let r = reason(u, "edge_label_must_be_atom_or_expression");
        return Err(diagnostic(u, bind.label, r));
    };
    if forms::atom_name(u, label.payload).is_some() {
        return lower_bind(u, bind_node, owner, module_identity, index);
    }
    if forms::form(u, label.payload).is_some() {
        // :958. The compound-label path accepts the same target forms.
        let target = if forms::expression_bind_target(u, bind.target) {
            Target {
                term: u.compound("deferred_expression", vec![bind.target]),
                kind: "reference",
                declared: Declared::default(),
            }
        } else {
            lower_target(u, bind.target, owner, module_identity)?
        };
        return Ok(finish_compound_edge_bind(
            u,
            target,
            bind.bind_node,
            owner,
            bind.label,
            index,
        ));
    }
    let r = reason(u, "edge_label_must_be_atom_or_expression");
    Err(diagnostic(u, label.id, r))
}

/// `:965`.
fn finish_compound_edge_bind(
    u: &mut Universe,
    target: Target,
    bind_node_id: TermId,
    owner: TermId,
    label_node: TermId,
    index: i64,
) -> Declared {
    let marker = u.compound("compound_label", vec![bind_node_id]);
    let index_term = u.int(index);
    let edge_target = u.compound("deferred_compound_edge", vec![label_node, target.term]);
    let edge = u.compound("pending_edge", vec![owner, marker, edge_target, index_term]);
    let reservation_target = u.compound(
        "derived_compound_edge",
        vec![label_node, target.term, bind_node_id, index_term],
    );
    let kind = u.atom("compound_edge");
    let reservation = u.compound("reservation", vec![owner, marker, reservation_target, kind]);
    let origin = edge_origin(u, owner, marker, index, bind_node_id);
    let mut declared = target.declared;
    let mut edges = vec![edge];
    edges.append(&mut declared.edges);
    let mut origins = vec![origin];
    origins.extend(declared.origins);
    let mut reservations = vec![reservation];
    reservations.append(&mut declared.reservations);
    Declared {
        nodes: declared.nodes,
        edges,
        relations: declared.relations,
        origins,
        reservations,
    }
}
