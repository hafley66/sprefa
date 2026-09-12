//! Deferred alias promotion. Port of `0_lowerer.pl:150-278`.
//!
//! A `reference` reservation whose name resolves, through a chain of names, to
//! a deferred expression becomes an `expression` reservation, and the chain
//! step becomes one `:`/4 rule.

use super::cx::Cx;
use super::index::{edge_parts, reservation_parts, ReservationIndex};
use crate::_6_eval::term::{TermId, Universe};

#[derive(Clone, Debug)]
pub struct Promotion {
    pub owner: TermId,
    pub name: TermId,
    pub reference_owner: TermId,
    pub reference_name: TermId,
    pub index: TermId,
    pub bind_node_id: TermId,
    pub immediate_owner: TermId,
    pub immediate_index: TermId,
}

#[derive(Default)]
pub struct Promoted {
    pub derived_reservations: Vec<TermId>,
    pub reservations: Vec<TermId>,
    pub edges: Vec<TermId>,
    pub promotions: Vec<Promotion>,
}

/// `:150`.
pub fn promote_deferred_aliases(
    u: &mut Universe,
    reservations: &[TermId],
    index: &ReservationIndex,
    edges: &[TermId],
    visible_edges: &[TermId],
    declaration_origins: &[TermId],
) -> Promoted {
    let promotions: Vec<Promotion> = reservations
        .iter()
        .filter_map(|row| {
            deferred_alias_promotion(u, *row, index, visible_edges, declaration_origins)
        })
        .collect();
    let mut out = Promoted {
        promotions,
        ..Default::default()
    };
    promote_alias_reservations(u, reservations, &mut out);
    out.edges = promote_alias_edges(u, edges, &out.promotions);
    out
}

/// `:165`.
fn deferred_alias_promotion(
    u: &mut Universe,
    row: TermId,
    index: &ReservationIndex,
    visible_edges: &[TermId],
    declaration_origins: &[TermId],
) -> Option<Promotion> {
    let parts = reservation_parts(u, row)?;
    if !u
        .functor_or_atom(parts.kind)
        .is_some_and(|(n, a)| n == "reference" && a.is_empty())
    {
        return None;
    }
    let (name_functor, name_args) = u.functor(parts.target)?;
    if name_functor != "name" || name_args.len() != 2 {
        return None;
    }
    let (reference_owner, reference_name) = (name_args[0], name_args[1]);
    let edge_index = visible_edges.iter().find_map(|edge| {
        let e = edge_parts(u, *edge)?;
        (e.owner == parts.owner && e.name == parts.name && e.target == parts.target)
            .then_some(e.index)
    })?;
    let index_term = u.int(edge_index);
    let bind_node_id = declaration_origins.iter().find_map(|origin| {
        let (n, args) = u.functor(*origin)?;
        if n != "origin" || args.len() != 2 {
            return None;
        }
        let (k, key) = u.functor(args[0])?;
        (k == "edge"
            && key.len() == 3
            && key[0] == parts.owner
            && key[1] == parts.name
            && key[2] == index_term)
            .then_some(args[1])
    })?;
    let (immediate_owner, immediate_index) =
        scoped_alias_target(u, reference_owner, reference_name, index, visible_edges)?;
    Some(Promotion {
        owner: parts.owner,
        name: parts.name,
        reference_owner,
        reference_name,
        index: index_term,
        bind_node_id,
        immediate_owner,
        immediate_index,
    })
}

/// `:183`.
fn scoped_alias_target(
    u: &mut Universe,
    owner: TermId,
    name: TermId,
    index: &ReservationIndex,
    edges: &[TermId],
) -> Option<(TermId, TermId)> {
    let found = index.scoped(owner, name)?;
    let (immediate_owner, target, kind) = (found.owner, found.target, found.kind);
    let immediate_index = reservation_edge_index(u, immediate_owner, name, target, edges)?;
    let mut visited = vec![(immediate_owner, name)];
    alias_terminal_deferred(u, target, kind, name, index, &mut visited)?;
    Some((immediate_owner, immediate_index))
}

/// `:207`.
fn reservation_edge_index(
    u: &mut Universe,
    owner: TermId,
    name: TermId,
    target: TermId,
    edges: &[TermId],
) -> Option<TermId> {
    // :212. A deferred expression reservation names a one-argument edge target.
    let edge_target = match u.functor(target) {
        Some((n, args)) if n == "deferred_expression" && args.len() == 3 => {
            let inner = args[0];
            u.compound("deferred_expression", vec![inner])
        }
        _ => target,
    };
    edges.iter().find_map(|edge| {
        let e = edge_parts(u, *edge)?;
        (e.owner == owner && e.name == name && e.target == edge_target).then(|| u.int(e.index))
    })
}

/// `:193`. Walks the reference chain to its deferred expression.
fn alias_terminal_deferred(
    u: &mut Universe,
    target: TermId,
    kind: TermId,
    _name: TermId,
    index: &ReservationIndex,
    visited: &mut Vec<(TermId, TermId)>,
) -> Option<TermId> {
    let expression = u.atom("expression");
    if kind == expression {
        if let Some((n, args)) = u.functor(target) {
            if n == "deferred_expression" && args.len() == 3 {
                return Some(args[0]);
            }
        }
    }
    let reference = u.atom("reference");
    if kind != reference {
        return None;
    }
    let (n, args) = u.functor(target)?;
    if n != "name" || args.len() != 2 {
        return None;
    }
    let (next_owner, next_name) = (args[0], args[1]);
    if visited.contains(&(next_owner, next_name)) {
        return None;
    }
    let found = index.scoped(next_owner, next_name)?;
    let (row_owner, row_target, row_kind) = (found.owner, found.target, found.kind);
    visited.push((row_owner, next_name));
    alias_terminal_deferred(u, row_target, row_kind, next_name, index, visited)
}

/// `:217`. A promoted reservation leaves the derived list.
fn promote_alias_reservations(u: &mut Universe, reservations: &[TermId], out: &mut Promoted) {
    for row in reservations {
        let parts = reservation_parts(u, *row);
        let promotion = parts.as_ref().and_then(|p| {
            let is_reference = u
                .functor_or_atom(p.kind)
                .is_some_and(|(n, a)| n == "reference" && a.is_empty());
            is_reference
                .then(|| {
                    out.promotions
                        .iter()
                        .find(|m| m.owner == p.owner && m.name == p.name)
                        .cloned()
                })
                .flatten()
        });
        match promotion {
            Some(m) => {
                let atom = u.compound("atom", vec![m.reference_name]);
                let node = u.compound("node", vec![m.bind_node_id, atom]);
                let target = u.compound("deferred_expression", vec![node, m.bind_node_id, m.index]);
                let kind = u.atom("expression");
                out.reservations
                    .push(u.compound("reservation", vec![m.owner, m.name, target, kind]));
            }
            None => {
                out.reservations.push(*row);
                out.derived_reservations.push(*row);
            }
        }
    }
}

/// `:239`.
fn promote_alias_edges(
    u: &mut Universe,
    edges: &[TermId],
    promotions: &[Promotion],
) -> Vec<TermId> {
    edges
        .iter()
        .map(|edge| {
            let Some(e) = edge_parts(u, *edge) else {
                return *edge;
            };
            let names_a_name = u
                .functor(e.target)
                .is_some_and(|(n, a)| n == "name" && a.len() == 2);
            if !names_a_name {
                return *edge;
            }
            let index_term = u.int(e.index);
            let Some(m) = promotions
                .iter()
                .find(|m| m.owner == e.owner && m.name == e.name && m.index == index_term)
            else {
                return *edge;
            };
            let atom = u.compound("atom", vec![m.reference_name]);
            let node = u.compound("node", vec![m.bind_node_id, atom]);
            let target = u.compound("deferred_expression", vec![node]);
            u.compound("pending_edge", vec![e.owner, e.name, target, index_term])
        })
        .collect()
}

/// `:256`. One rule and two origins per promotion.
pub fn promoted_alias_rules(
    cx: &mut Cx,
    promotions: &[Promotion],
    rule_index: i64,
) -> (Vec<TermId>, Vec<TermId>) {
    let mut rules = Vec::with_capacity(promotions.len());
    let mut origins = Vec::with_capacity(promotions.len() * 2);
    for (offset, m) in promotions.iter().enumerate() {
        let index = rule_index + offset as i64;
        let bind = cx.compound("derived_bind", vec![m.bind_node_id]);
        let value = cx.compound("var", vec![bind]);
        let colon = cx.atom(":");
        let head_relation = cx.compound("name", vec![m.owner, colon]);
        let owner_ref = cx.compound("ref", vec![m.owner]);
        let name_const = cx.compound("const", vec![m.name]);
        let index_const = cx.compound("const", vec![m.index]);
        let head_arguments = cx.u.list(&[owner_ref, name_const, value, index_const]);
        let head = cx.compound("call", vec![head_relation, head_arguments]);
        let goal_relation = cx.compound("name", vec![m.reference_owner, colon]);
        let immediate_ref = cx.compound("ref", vec![m.immediate_owner]);
        let reference_const = cx.compound("const", vec![m.reference_name]);
        let immediate_const = cx.compound("const", vec![m.immediate_index]);
        let goal_arguments =
            cx.u.list(&[immediate_ref, reference_const, value, immediate_const]);
        let goal_call = cx.compound("call", vec![goal_relation, goal_arguments]);
        let positive = cx.atom("positive");
        let goal = cx.compound("pending_goal", vec![positive, goal_call]);
        let body = cx.u.list(&[goal]);
        rules.push(cx.compound("rule", vec![head, body]));
        let index_term = cx.int(index);
        let rule_key = cx.compound("rule", vec![index_term]);
        origins.push(cx.compound("origin", vec![rule_key, m.bind_node_id]));
        let zero = cx.int(0);
        let goal_key = cx.compound("goal", vec![index_term, zero]);
        origins.push(cx.compound("origin", vec![goal_key, m.bind_node_id]));
    }
    (rules, origins)
}
