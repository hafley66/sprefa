//! Partial application: the Curry rows a half-applied callable emits.
//! Port of `0_lowerer.pl:410-543`.

use super::cx::Cx;
use super::express::decode_bound;
use super::host::{indexed_empty_rule_origins, indexed_goal_origins};
use super::slots::{self, Slot};
use crate::_6_eval::term::{Term, TermId};

/// `:464`. A constant label keeps an empty body; a derived label carries goals.
pub enum LabelSpec {
    Constant(TermId),
    Derived {
        value: TermId,
        goals: Vec<TermId>,
        goal_nodes: Vec<TermId>,
        description: TermId,
    },
}

impl LabelSpec {
    fn description(&self) -> TermId {
        match self {
            LabelSpec::Constant(name) => *name,
            LabelSpec::Derived { description, .. } => *description,
        }
    }
}

pub struct PartialRules {
    pub rules: Vec<TermId>,
    pub origins: Vec<TermId>,
}

/// `:410`. `Err` is the `partial_application_requires_more_arguments`
/// diagnostic every failing conjunct in the v7 condition falls through to.
pub fn partial_bind_rules(
    cx: &mut Cx,
    owner: TermId,
    label: LabelSpec,
    bind_node_id: TermId,
    index: TermId,
    partial: TermId,
    rule_index: i64,
) -> Result<PartialRules, TermId> {
    match build(cx, owner, &label, bind_node_id, index, partial, rule_index) {
        Some(rules) => Ok(rules),
        None => {
            let description = label.description();
            let reason = cx.compound(
                "partial_application_requires_more_arguments",
                vec![description],
            );
            Err(cx.diagnostic(bind_node_id, reason))
        }
    }
}

fn build(
    cx: &mut Cx,
    owner: TermId,
    label: &LabelSpec,
    bind_node_id: TermId,
    index: TermId,
    partial: TermId,
    rule_index: i64,
) -> Option<PartialRules> {
    let (descriptor, bound_list) = {
        let (name, args) = cx.u.functor(partial)?;
        if name != "partial_application" || args.len() != 2 {
            return None;
        }
        (args[0], args[1])
    };
    let (callable_term, arity, return_index) = {
        let (_, args) = cx.u.functor(descriptor)?;
        (args[2], cx.u.as_int(args[3])?, cx.u.as_int(args[5])?)
    };
    // :483. target(Id) identifies as Id; kernel(N) identifies as itself.
    let callable = cx.u.unary(callable_term, "target").unwrap_or(callable_term);
    let curry = product_target(cx, owner, "Curry")?;
    let literal = product_target(cx, owner, "Literal")?;
    let bound = decode_bound(cx, bound_list);
    let bound_rows = partial_bound_rows(cx, &bound)?;
    let decoded = super::express::decode_callable(cx, callable_term);
    let all = slots::callable_slots(cx, &decoded, arity);
    let input = slots::exclude_slot(&all, Some(return_index));
    let call_owner = cx.compound("call", vec![owner, bind_node_id]);
    let call_edge_rules = partial_call_edge_rules(cx, owner, call_owner, literal, &input, &bound)?;

    let arguments = cx.u.list(&[callable, bound_rows]);
    let partial_node = cx.compound("application", vec![curry, arguments]);
    let apply_rule = empty_rule(cx, owner, "Apply", |cx| {
        let call_ref = cx.compound("ref", vec![call_owner]);
        let callable_ref = cx.compound("ref", vec![callable]);
        vec![call_ref, callable_ref]
    });
    let partial_call_rule = empty_rule(cx, owner, "PartialCall", |cx| {
        vec![cx.compound("ref", vec![call_owner])]
    });
    let return_edge_rule = {
        let call_ref = cx.compound("ref", vec![call_owner]);
        let return_atom = cx.atom("return");
        let label_const = cx.compound("const", vec![return_atom]);
        let partial_ref = cx.compound("ref", vec![partial_node]);
        let ordinal = cx.int(return_index);
        let index_const = cx.compound("const", vec![ordinal]);
        colon_rule(
            cx,
            owner,
            call_ref,
            label_const,
            partial_ref,
            index_const,
            &[],
        )
    };
    let (edge_rule, edge_goal_nodes) = partial_edge_rule(cx, label, owner, partial_node, index);

    let mut rules = vec![apply_rule, partial_call_rule];
    rules.extend(call_edge_rules);
    rules.push(return_edge_rule);
    rules.push(edge_rule);
    let count = rules.len();
    let mut origins = indexed_empty_rule_origins(cx.u, count, rule_index, bind_node_id);
    let edge_rule_index = rule_index + count as i64 - 1;
    origins.extend(indexed_goal_origins(
        cx.u,
        &edge_goal_nodes,
        edge_rule_index,
    ));
    Some(PartialRules { rules, origins })
}

/// `:415`. `Curry` and `Literal` must resolve to products in this scope.
fn product_target(cx: &mut Cx, owner: TermId, name: &str) -> Option<TermId> {
    let atom = cx.atom(name);
    let found = cx.reservations.scoped(owner, atom)?;
    let (target, kind) = (found.target, found.kind);
    let product = cx.atom("product");
    (kind == product).then_some(())?;
    cx.u.unary(target, "target")
}

/// `:486`. A bound value that is neither a reference nor a constant fails.
fn partial_bound_rows(cx: &mut Cx, bound: &[(i64, TermId)]) -> Option<TermId> {
    let mut rows = Vec::with_capacity(bound.len());
    for (index, value) in bound {
        let kind = match cx.u.functor(*value) {
            Some(("ref", args)) if args.len() == 1 => "reference",
            Some(("const", args)) if args.len() == 1 => "constant",
            _ => return None,
        };
        let index = cx.int(*index);
        let kind = cx.u.string(kind);
        rows.push(cx.compound("bound", vec![index, kind, *value]));
    }
    Some(cx.u.list(&rows))
}

/// `:495`. Value rules come before the edge rule they feed.
fn partial_call_edge_rules(
    cx: &mut Cx,
    owner: TermId,
    call_owner: TermId,
    literal: TermId,
    input: &[Slot],
    bound: &[(i64, TermId)],
) -> Option<Vec<TermId>> {
    let mut rules = Vec::new();
    for (index, value) in bound {
        let slot = input.iter().find(|s| s.index == *index)?;
        let label = slot.label?;
        let (edge_value, mut value_rules) = partial_call_value_rules(cx, owner, literal, *value)?;
        rules.append(&mut value_rules);
        let call_ref = cx.compound("ref", vec![call_owner]);
        let label_const = cx.compound("const", vec![label]);
        let ordinal = cx.int(*index);
        let index_const = cx.compound("const", vec![ordinal]);
        rules.push(colon_rule(
            cx,
            owner,
            call_ref,
            label_const,
            edge_value,
            index_const,
            &[],
        ));
    }
    Some(rules)
}

/// `:512`. A constant is reified as a `Literal` node.
fn partial_call_value_rules(
    cx: &mut Cx,
    owner: TermId,
    literal: TermId,
    value: TermId,
) -> Option<(TermId, Vec<TermId>)> {
    if cx
        .u
        .functor(value)
        .is_some_and(|(n, a)| n == "ref" && a.len() == 1)
    {
        return Some((value, vec![]));
    }
    let inner = cx.u.unary(value, "const")?;
    let primitive = compiler_literal_type(cx, inner);
    let arguments = cx.u.list(&[primitive, inner]);
    let literal_node = cx.compound("application", vec![literal, arguments]);
    let node_rule = empty_rule(cx, owner, "node", |cx| {
        vec![cx.compound("ref", vec![literal_node])]
    });
    let literal_rule = empty_rule(cx, owner, "Literal", |cx| {
        let node_ref = cx.compound("ref", vec![literal_node]);
        let primitive_ref = cx.compound("ref", vec![primitive]);
        let constant = cx.compound("const", vec![inner]);
        vec![node_ref, primitive_ref, constant]
    });
    let edge_value = cx.compound("ref", vec![literal_node]);
    Some((edge_value, vec![node_rule, literal_rule]))
}

/// `:528`.
fn compiler_literal_type(cx: &mut Cx, value: TermId) -> TermId {
    let name = match cx.u.get(value) {
        Term::Int(_) => "int",
        Term::Str(_) => "text",
        _ => "any",
    };
    let atom = cx.atom(name);
    cx.compound("primitive", vec![atom])
}

/// `:464`.
fn partial_edge_rule(
    cx: &mut Cx,
    label: &LabelSpec,
    owner: TermId,
    partial_node: TermId,
    index: TermId,
) -> (TermId, Vec<TermId>) {
    let owner_ref = cx.compound("ref", vec![owner]);
    let partial_ref = cx.compound("ref", vec![partial_node]);
    let index_const = cx.compound("const", vec![index]);
    match label {
        LabelSpec::Constant(name) => {
            let label_const = cx.compound("const", vec![*name]);
            (
                colon_rule(
                    cx,
                    owner,
                    owner_ref,
                    label_const,
                    partial_ref,
                    index_const,
                    &[],
                ),
                vec![],
            )
        }
        LabelSpec::Derived {
            value,
            goals,
            goal_nodes,
            ..
        } => (
            colon_rule(
                cx,
                owner,
                owner_ref,
                *value,
                partial_ref,
                index_const,
                goals,
            ),
            goal_nodes.clone(),
        ),
    }
}

fn colon_rule(
    cx: &mut Cx,
    owner: TermId,
    subject: TermId,
    label: TermId,
    target: TermId,
    index: TermId,
    goals: &[TermId],
) -> TermId {
    let colon = cx.atom(":");
    let relation = cx.compound("name", vec![owner, colon]);
    let arguments = cx.u.list(&[subject, label, target, index]);
    let call = cx.compound("call", vec![relation, arguments]);
    let body = cx.u.list(goals);
    cx.compound("rule", vec![call, body])
}

fn empty_rule(
    cx: &mut Cx,
    owner: TermId,
    name: &str,
    arguments: impl FnOnce(&mut Cx) -> Vec<TermId>,
) -> TermId {
    let atom = cx.atom(name);
    let relation = cx.compound("name", vec![owner, atom]);
    let arguments = arguments(cx);
    let list = cx.u.list(&arguments);
    let call = cx.compound("call", vec![relation, list]);
    let body = cx.u.empty_list();
    cx.compound("rule", vec![call, body])
}
