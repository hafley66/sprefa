//! Expression lowering. Port of `0_lowerer.pl:1427-1934`.

use super::cx::Cx;
use super::forms;
use super::kernel;
use super::slots::{self as slots, Callable, Slot};
use crate::_6_eval::term::TermId;
use std::cmp::Ordering;

#[derive(Clone, Debug)]
pub struct Lowering {
    pub value: TermId,
    pub goals: Vec<TermId>,
    pub origins: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

impl Lowering {
    fn value(value: TermId) -> Self {
        Lowering {
            value,
            goals: vec![],
            origins: vec![],
            diagnostics: vec![],
        }
    }
}

fn none(cx: &mut Cx, diagnostics: Vec<TermId>) -> Lowering {
    let value = cx.atom("none");
    Lowering {
        value,
        goals: vec![],
        origins: vec![],
        diagnostics,
    }
}

pub fn is_partial(cx: &Cx, value: TermId) -> bool {
    cx.u.functor(value)
        .is_some_and(|(n, a)| n == "partial_application" && a.len() == 2)
}

/// `:1433`. Eight clauses in source order.
pub fn lower_expression(cx: &mut Cx, node: TermId, owner: TermId) -> Lowering {
    let Some(parsed) = forms::node(cx.u, node) else {
        let reason = cx.plain(node, "unresolved_expression_form");
        return none(cx, vec![reason]);
    };
    if let Some((identity, _)) = forms::variable(cx.u, parsed.payload) {
        let value = cx.compound("var", vec![identity]);
        return Lowering::value(value);
    }
    if let Some(literal) = forms::literal_value(cx.u, parsed.payload) {
        let value = cx.compound("const", vec![literal]);
        return Lowering::value(value);
    }
    if let Some(name) = forms::atom_name(cx.u, parsed.payload) {
        if let Some(found) = cx.reservations.scoped(owner, name) {
            let (row_owner, row_name, target, kind) =
                (found.owner, found.name, found.target, found.kind);
            return lexical_atom_value(cx, row_owner, row_name, target, kind, parsed.id, owner);
        }
        let value = cx.compound("name", vec![owner, name]);
        return Lowering::value(value);
    }
    let Some(items) = forms::form(cx.u, parsed.payload) else {
        let reason = cx.plain(parsed.id, "unresolved_expression_form");
        return none(cx, vec![reason]);
    };
    if items.is_empty() {
        let empty = cx.atom("()");
        let value = cx.compound("name", vec![owner, empty]);
        return Lowering::value(value);
    }
    if let Some(head) = forms::form_head_atom(cx.u, &items) {
        return match expression_callable(cx, head, owner) {
            Ok((callable, arity, key_sets)) => lower_expression_call(
                cx,
                callable,
                arity,
                key_sets,
                head,
                parsed.id,
                &items[1..],
                owner,
            ),
            Err(reason) => {
                let diagnostic = cx.diagnostic(parsed.id, reason);
                none(cx, vec![diagnostic])
            }
        };
    }
    // :1458. An operator position that is itself an expression.
    let operator = lower_expression(cx, items[0], owner);
    apply_expression_operator(cx, operator, parsed.id, &items[1..], owner)
}

/// `:1472`.
fn lexical_atom_value(
    cx: &mut Cx,
    bind_owner: TermId,
    row_name: TermId,
    target: TermId,
    kind: TermId,
    node_id: TermId,
    owner: TermId,
) -> Lowering {
    let expression = cx.atom("expression");
    let deferred =
        cx.u.functor(target)
            .is_some_and(|(n, a)| n == "deferred_expression" && a.len() == 3);
    if deferred && kind == expression {
        let index = cx.u.functor(target).map(|(_, a)| a[2]).unwrap();
        let lookup = cx.compound("derived_lookup", vec![node_id]);
        let value = cx.compound("var", vec![lookup]);
        let goal = colon_goal(cx, owner, bind_owner, row_name, value, index);
        return Lowering {
            value,
            goals: vec![goal],
            origins: vec![node_id],
            diagnostics: vec![],
        };
    }
    if let Some(inner) = cx.u.unary(target, "target") {
        let value = cx.compound("ref", vec![inner]);
        return Lowering::value(value);
    }
    let value = cx.compound("name", vec![owner, row_name]);
    Lowering::value(value)
}

/// `pending_goal(positive, call(name(Owner, ':'), [ref(Bind), const(Name), Value, Index]))`.
pub fn colon_goal(
    cx: &mut Cx,
    owner: TermId,
    bind_owner: TermId,
    name: TermId,
    value: TermId,
    index: TermId,
) -> TermId {
    let colon = cx.atom(":");
    let relation = cx.compound("name", vec![owner, colon]);
    let bind_ref = cx.compound("ref", vec![bind_owner]);
    let name_const = cx.compound("const", vec![name]);
    let index_const = cx.compound("const", vec![index]);
    let arguments = cx.u.list(&[bind_ref, name_const, value, index_const]);
    let call = cx.compound("call", vec![relation, arguments]);
    let positive = cx.atom("positive");
    cx.compound("pending_goal", vec![positive, call])
}

/// `:1488`. `Err` carries the bare reason term.
pub fn expression_callable(
    cx: &mut Cx,
    name: TermId,
    owner: TermId,
) -> Result<(Callable, i64, TermId), TermId> {
    if let Some(found) = cx.reservations.scoped(owner, name) {
        let (target, kind) = (found.target, found.kind);
        let kind_name =
            cx.u.functor_or_atom(kind)
                .map(|(n, _)| n.to_string())
                .unwrap_or_default();
        // :1499. Only a product or a generated callable is callable.
        if kind_name == "product" || kind_name == "derived_callable" {
            if let Some(callable) = cx.u.unary(target, "target") {
                return match cx.relations.get(&callable) {
                    Some((arity, key_sets)) => Ok((Callable::Target(callable), *arity, *key_sets)),
                    None => Err(cx.compound("undeclared_relation", vec![callable])),
                };
            }
        }
        return Err(cx.compound("not_relation", vec![name]));
    }
    let text =
        cx.u.functor_or_atom(name)
            .map(|(n, _)| n.to_string())
            .unwrap_or_default();
    if let Some(arity) = kernel::kernel_relation(&text) {
        let key_sets = key_sets_term(cx, &kernel::kernel_keys(&text));
        return Ok((Callable::Kernel(text), arity as i64, key_sets));
    }
    Err(cx.compound("undeclared_relation", vec![name]))
}

fn key_sets_term(cx: &mut Cx, key_sets: &[Vec<u32>]) -> TermId {
    let rows: Vec<TermId> = key_sets
        .iter()
        .map(|set| {
            let items: Vec<TermId> = set.iter().map(|p| cx.int(*p as i64)).collect();
            cx.u.list(&items)
        })
        .collect();
    cx.u.list(&rows)
}

fn key_sets_of(cx: &Cx, term: TermId) -> Vec<Vec<i64>> {
    cx.u.as_list(term)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    cx.u.as_list(*row)
                        .map(|items| items.iter().filter_map(|i| cx.u.as_int(*i)).collect())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `:1876`.
pub fn expression_return_position(
    cx: &mut Cx,
    callable: &Callable,
    node_id: TermId,
) -> Result<i64, TermId> {
    let indices: Vec<i64> = match callable {
        Callable::Target(owner) => {
            let return_atom = cx.atom("return");
            cx.edges.return_indices(*owner, return_atom)
        }
        Callable::Kernel(name) => kernel::kernel_return_positions(name)
            .into_iter()
            .map(|i| i as i64)
            .collect(),
    };
    let term = callable.term(cx);
    match indices.len() {
        1 => Ok(indices[0]),
        0 => {
            let reason = cx.compound("expression_without_return", vec![term]);
            Err(cx.diagnostic(node_id, reason))
        }
        _ => {
            let items: Vec<TermId> = indices.iter().map(|i| cx.int(*i)).collect();
            let list = cx.u.list(&items);
            let reason = cx.compound("expression_multiple_returns", vec![term, list]);
            Err(cx.diagnostic(node_id, reason))
        }
    }
}

/// `:1629`.
#[allow(clippy::too_many_arguments)]
fn lower_expression_call(
    cx: &mut Cx,
    callable: Callable,
    arity: i64,
    key_sets: TermId,
    name: TermId,
    node_id: TermId,
    argument_nodes: &[TermId],
    owner: TermId,
) -> Lowering {
    let return_index = match expression_return_position(cx, &callable, node_id) {
        Ok(index) => index,
        Err(diagnostic) => return none(cx, vec![diagnostic]),
    };
    let all = slots::callable_slots(cx, &callable, arity);
    let input = slots::exclude_slot(&all, Some(return_index));
    let normalized =
        normalize_expression_arguments(cx, name, node_id, &input, &[], argument_nodes, owner);
    finish_expression_bindings(
        cx,
        &callable,
        arity,
        key_sets,
        return_index,
        name,
        node_id,
        &input,
        normalized,
        owner,
    )
}

struct Normalized {
    bound: Vec<(i64, TermId)>,
    goals: Vec<TermId>,
    origins: Vec<TermId>,
    diagnostics: Vec<TermId>,
}

/// `:1744`.
fn normalize_expression_arguments(
    cx: &mut Cx,
    name: TermId,
    node_id: TermId,
    input: &[Slot],
    existing: &[(i64, TermId)],
    argument_nodes: &[TermId],
    owner: TermId,
) -> Normalized {
    let classified = match slots::classify(cx, argument_nodes, input) {
        Ok(classified) => classified,
        Err(reason) => {
            let diagnostic = cx.diagnostic(node_id, reason);
            return Normalized {
                bound: vec![],
                goals: vec![],
                origins: vec![],
                diagnostics: vec![diagnostic],
            };
        }
    };
    let reserved: Vec<i64> = existing.iter().map(|(i, _)| *i).collect();
    let assigned = match slots::assign(cx, &classified, input, &reserved) {
        Ok(assigned) => assigned,
        Err(reason) => {
            let reason = expression_argument_reason(
                cx,
                reason,
                name,
                input.len(),
                existing.len() + argument_nodes.len(),
            );
            let diagnostic = cx.diagnostic(node_id, reason);
            return Normalized {
                bound: vec![],
                goals: vec![],
                origins: vec![],
                diagnostics: vec![diagnostic],
            };
        }
    };
    let mut bound = existing.to_vec();
    let mut goals = Vec::new();
    let mut origins = Vec::new();
    let mut diagnostics = Vec::new();
    for item in &assigned {
        let lowered = lower_expression(cx, item.node, owner);
        bound.push((item.index, lowered.value));
        goals.extend(lowered.goals);
        origins.extend(lowered.origins);
        diagnostics.extend(lowered.diagnostics);
    }
    sort_bound(cx, &mut bound);
    Normalized {
        bound,
        goals,
        origins,
        diagnostics,
    }
}

/// `sort/2` over `bound(Index, Value)`: standard order, duplicates removed.
fn sort_bound(cx: &Cx, bound: &mut Vec<(i64, TermId)>) {
    bound.sort_by(|a, b| match a.0.cmp(&b.0) {
        Ordering::Equal => cx.u.cmp(a.1, b.1),
        other => other,
    });
    bound.dedup();
}

/// `:1771`.
fn expression_argument_reason(
    cx: &mut Cx,
    reason: TermId,
    name: TermId,
    expected: usize,
    observed: usize,
) -> TermId {
    let too_many = cx.atom("too_many_arguments");
    if reason != too_many {
        return reason;
    }
    let expected = cx.int(expected as i64);
    let observed = cx.int(observed as i64);
    cx.compound("expression_arity_mismatch", vec![name, expected, observed])
}

/// `:1657`.
#[allow(clippy::too_many_arguments)]
fn finish_expression_bindings(
    cx: &mut Cx,
    callable: &Callable,
    arity: i64,
    key_sets: TermId,
    return_index: i64,
    name: TermId,
    node_id: TermId,
    input: &[Slot],
    normalized: Normalized,
    owner: TermId,
) -> Lowering {
    if let Some(first) = normalized.diagnostics.first() {
        return none(cx, vec![*first]);
    }
    if !slots::missing(input, &normalized.bound).is_empty() {
        let value = partial_application_term(
            cx,
            owner,
            name,
            callable,
            arity,
            key_sets,
            return_index,
            &normalized.bound,
        );
        return Lowering {
            value,
            goals: normalized.goals,
            origins: normalized.origins,
            diagnostics: vec![],
        };
    }
    if let Some(diagnostic) =
        expression_mode_diagnostics(cx, name, return_index, arity, key_sets, node_id)
    {
        return none(cx, vec![diagnostic]);
    }
    let arguments = slots::ordered(input, &normalized.bound);
    finish_expression_call_arguments(
        cx,
        name,
        node_id,
        return_index,
        arguments,
        normalized.goals,
        normalized.origins,
        owner,
    )
}

/// `callable(CallOwner, Name, Callable, Arity, KeySets, ReturnIndex)` at `:1651`.
#[allow(clippy::too_many_arguments)]
fn partial_application_term(
    cx: &mut Cx,
    owner: TermId,
    name: TermId,
    callable: &Callable,
    arity: i64,
    key_sets: TermId,
    return_index: i64,
    bound: &[(i64, TermId)],
) -> TermId {
    let callable_term = callable.term(cx);
    let arity = cx.int(arity);
    let return_index = cx.int(return_index);
    let descriptor = cx.compound(
        "callable",
        vec![owner, name, callable_term, arity, key_sets, return_index],
    );
    let rows: Vec<TermId> = bound
        .iter()
        .map(|(index, value)| {
            let index = cx.int(*index);
            cx.compound("bound", vec![index, *value])
        })
        .collect();
    let list = cx.u.list(&rows);
    cx.compound("partial_application", vec![descriptor, list])
}

/// `:1816`.
fn expression_mode_diagnostics(
    cx: &mut Cx,
    name: TermId,
    return_index: i64,
    arity: i64,
    key_sets: TermId,
    node_id: TermId,
) -> Option<TermId> {
    let supplied = slots::positions_except(arity, Some(return_index));
    let sets = key_sets_of(cx, key_sets);
    if sets
        .iter()
        .any(|set| slots::positions_subset(set, &supplied))
    {
        return None;
    }
    let items: Vec<TermId> = supplied.iter().map(|p| cx.int(*p)).collect();
    let list = cx.u.list(&items);
    let supplied_term = cx.compound("supplied", vec![list]);
    let keys_term = cx.compound("keys", vec![key_sets]);
    let return_term_inner = cx.int(return_index);
    let return_term = cx.compound("return", vec![return_term_inner]);
    let reason = cx.compound(
        "ambiguous_expression_projection",
        vec![name, supplied_term, keys_term, return_term],
    );
    Some(cx.diagnostic(node_id, reason))
}

/// `:1848`.
#[allow(clippy::too_many_arguments)]
fn finish_expression_call_arguments(
    cx: &mut Cx,
    name: TermId,
    node_id: TermId,
    return_index: i64,
    mut arguments: Vec<TermId>,
    mut goals: Vec<TermId>,
    mut origins: Vec<TermId>,
    owner: TermId,
) -> Lowering {
    let expression = cx.compound("expression", vec![node_id]);
    let value = cx.compound("var", vec![expression]);
    let at = (return_index.max(0) as usize).min(arguments.len());
    arguments.insert(at, value);
    let relation = cx.compound("name", vec![owner, name]);
    let list = cx.u.list(&arguments);
    let call = cx.compound("call", vec![relation, list]);
    let positive = cx.atom("positive");
    goals.push(cx.compound("pending_goal", vec![positive, call]));
    origins.push(node_id);
    Lowering {
        value,
        goals,
        origins,
        diagnostics: vec![],
    }
}

/// `:1680`.
fn apply_expression_operator(
    cx: &mut Cx,
    operator: Lowering,
    node_id: TermId,
    argument_nodes: &[TermId],
    owner: TermId,
) -> Lowering {
    if let Some(first) = operator.diagnostics.first() {
        return none(cx, vec![*first]);
    }
    if !is_partial(cx, operator.value) {
        let reason = cx.compound("expression_operator_not_callable", vec![operator.value]);
        let diagnostic = cx.diagnostic(node_id, reason);
        return none(cx, vec![diagnostic]);
    }
    let (descriptor, bound_list) = {
        let (_, args) = cx.u.functor(operator.value).unwrap();
        (args[0], args[1])
    };
    let (call_owner, name, callable_term, arity, key_sets, return_index) = {
        let (_, args) = cx.u.functor(descriptor).unwrap();
        (args[0], args[1], args[2], args[3], args[4], args[5])
    };
    let arity = cx.u.as_int(arity).unwrap_or(0);
    let return_index = cx.u.as_int(return_index).unwrap_or(0);
    let callable = decode_callable(cx, callable_term);
    let existing = decode_bound(cx, bound_list);
    let all = slots::callable_slots(cx, &callable, arity);
    let input = slots::exclude_slot(&all, Some(return_index));
    let mut normalized =
        normalize_expression_arguments(cx, name, node_id, &input, &existing, argument_nodes, owner);
    let mut goals = operator.goals;
    goals.append(&mut normalized.goals);
    normalized.goals = goals;
    let mut origins = operator.origins;
    origins.append(&mut normalized.origins);
    normalized.origins = origins;
    finish_expression_bindings(
        cx,
        &callable,
        arity,
        key_sets,
        return_index,
        name,
        node_id,
        &input,
        normalized,
        call_owner,
    )
}

pub fn decode_callable(cx: &Cx, term: TermId) -> Callable {
    if let Some(owner) = cx.u.unary(term, "target") {
        return Callable::Target(owner);
    }
    let name =
        cx.u.unary(term, "kernel")
            .and_then(|n| cx.u.functor_or_atom(n).map(|(n, _)| n.to_string()))
            .unwrap_or_default();
    Callable::Kernel(name)
}

pub fn decode_bound(cx: &Cx, list: TermId) -> Vec<(i64, TermId)> {
    cx.u.as_list(list)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    cx.u.functor(*row).and_then(|(n, a)| {
                        if n == "bound" && a.len() == 2 {
                            cx.u.as_int(a[0]).map(|i| (i, a[1]))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `:1908`.
pub fn lower_argument(
    cx: &mut Cx,
    node: TermId,
    owner: TermId,
    head_mode: bool,
) -> Result<(TermId, Vec<TermId>, Vec<TermId>), TermId> {
    if let Some(parsed) = forms::node(cx.u, node) {
        if let Some(items) = forms::form(cx.u, parsed.payload) {
            let is_count = forms::form_head_atom(cx.u, &items)
                .is_some_and(|a| cx.u.functor_or_atom(a).is_some_and(|(n, _)| n == "count"));
            if is_count {
                if !head_mode {
                    return Err(cx.plain(parsed.id, "aggregate_outside_rule_head"));
                }
                if items.len() != 2 {
                    return Err(cx.plain(parsed.id, "invalid_count_aggregate"));
                }
                let lowered = lower_expression(cx, items[1], owner);
                if let Some(first) = lowered.diagnostics.first() {
                    return Err(*first);
                }
                let count = cx.atom("count");
                let value = cx.compound("aggregate", vec![count, lowered.value]);
                return Ok((value, lowered.goals, lowered.origins));
            }
        }
    }
    let lowered = lower_expression(cx, node, owner);
    match lowered.diagnostics.first() {
        Some(first) => Err(*first),
        None => Ok((lowered.value, lowered.goals, lowered.origins)),
    }
}
