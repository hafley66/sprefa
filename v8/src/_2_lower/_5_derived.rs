//! Derived bind rules: the rules a reservation emits before pass 3 runs.
//! Port of `0_lowerer.pl:295-670`.

use super::cx::{CallPolicy, Cx};
use super::express::{is_partial, lower_expression};
use super::host::{host_metadata_rules, indexed_goal_origins};
use super::index::reservation_parts;
use super::partial::{partial_bind_rules, LabelSpec};
use crate::_6_eval::term::TermId;

/// A lowering either finishes, reports one diagnostic, or reaches a construct
/// whose port is not built yet.
#[derive(Debug)]
pub enum Stop {
    Diagnostic(TermId),
    Unported(String),
}

/// One reservation's rules and their origins.
pub struct Block {
    pub rules: Vec<TermId>,
    pub origins: Vec<TermId>,
}

#[derive(Default)]
pub struct Derived {
    pub rules: Vec<TermId>,
    pub origins: Vec<TermId>,
}

/// `:295`. A deferrable diagnostic advances no ordinal.
pub fn lower_derived_bind_rules(
    cx: &mut Cx,
    reservations: &[TermId],
    mut rule_index: i64,
) -> Result<Derived, Stop> {
    let mut out = Derived::default();
    for row in reservations {
        let Some(parts) = reservation_parts(cx.u, *row) else {
            continue;
        };
        let kind =
            cx.u.functor_or_atom(parts.kind)
                .map(|(n, _)| n.to_string())
                .unwrap_or_default();
        match kind.as_str() {
            "host" => {
                let Some((rules, origins)) =
                    host_metadata_rules(cx.u, parts.owner, parts.target, rule_index)
                else {
                    continue;
                };
                rule_index += rules.len() as i64;
                out.rules.extend(rules);
                out.origins.extend(origins);
            }
            "compound_edge" => {
                match compound_edge_rule(cx, parts.owner, parts.target, rule_index)? {
                    Some(block) => {
                        rule_index += block.rules.len() as i64;
                        out.rules.extend(block.rules);
                        out.origins.extend(block.origins);
                    }
                    None => continue,
                }
            }
            "expression" => {
                match expression_rule(cx, parts.owner, parts.name, parts.target, rule_index)? {
                    Some(block) => {
                        rule_index += block.rules.len() as i64;
                        out.rules.extend(block.rules);
                        out.origins.extend(block.origins);
                    }
                    None => continue,
                }
            }
            _ => continue,
        }
    }
    Ok(out)
}

fn deferrable_skip(cx: &Cx, diagnostics: &[TermId]) -> bool {
    cx.policy == CallPolicy::DeferUnknownCalls
        && !diagnostics.is_empty()
        && diagnostics.iter().all(|d| cx.deferrable(*d))
}

/// `:365`. `deferred_expression(TargetNode, BindNodeId, Index)`.
fn expression_rule(
    cx: &mut Cx,
    owner: TermId,
    name: TermId,
    target: TermId,
    rule_index: i64,
) -> Result<Option<Block>, Stop> {
    let Some((_, args)) = cx.u.functor(target) else {
        return Ok(None);
    };
    let (target_node, bind_node_id, index) = (args[0], args[1], args[2]);
    let lowered = lower_expression(cx, target_node, owner);
    if lowered.diagnostics.is_empty() && is_partial(cx, lowered.value) {
        // :369. A half-applied target emits the Curry block instead of one rule.
        let block = partial_bind_rules(
            cx,
            owner,
            LabelSpec::Constant(name),
            bind_node_id,
            index,
            lowered.value,
            rule_index,
        )
        .map_err(Stop::Diagnostic)?;
        return Ok(Some(Block {
            rules: block.rules,
            origins: block.origins,
        }));
    }
    if let Some(first) = lowered.diagnostics.first() {
        if deferrable_skip(cx, &lowered.diagnostics) {
            return Ok(None);
        }
        return Err(Stop::Diagnostic(*first));
    }
    let bind = cx.compound("derived_bind", vec![bind_node_id]);
    let bind_value = cx.compound("var", vec![bind]);
    let goals = replace_in(cx, lowered.value, bind_value, &lowered.goals);
    let name_const = cx.compound("const", vec![name]);
    let index_const = cx.compound("const", vec![index]);
    let head = colon_head(cx, owner, name_const, bind_value, index_const);
    let body = cx.u.list(&goals);
    let rule = cx.compound("rule", vec![head, body]);
    Ok(Some(Block {
        rules: vec![rule],
        origins: rule_origins(cx, rule_index, bind_node_id, &lowered.origins),
    }))
}

/// `:305`. `derived_compound_edge(LabelNode, TargetTerm, BindNodeId, Index)`.
fn compound_edge_rule(
    cx: &mut Cx,
    owner: TermId,
    target: TermId,
    rule_index: i64,
) -> Result<Option<Block>, Stop> {
    let Some((_, args)) = cx.u.functor(target) else {
        return Ok(None);
    };
    let (label_node, target_term, bind_node_id, index) = (args[0], args[1], args[2], args[3]);
    let label = lower_expression(cx, label_node, owner);
    let outcome = compound_edge_target(cx, target_term, owner);
    let mut diagnostics = label.diagnostics.clone();
    diagnostics.extend(outcome.diagnostics.clone());
    if diagnostics.is_empty() && is_partial(cx, label.value) {
        let reason = cx.atom("partial_edge_label_requires_more_arguments");
        return Err(Stop::Diagnostic(cx.diagnostic(bind_node_id, reason)));
    }
    if diagnostics.is_empty() && outcome.partial {
        // :334. The label's own goals ride into the Curry block's edge rule.
        let derived = cx.compound("derived_label", vec![bind_node_id]);
        let derived_label = cx.compound("var", vec![derived]);
        let goals = replace_in(cx, label.value, derived_label, &label.goals);
        let description = cx.compound("compound_label", vec![bind_node_id]);
        let block = partial_bind_rules(
            cx,
            owner,
            LabelSpec::Derived {
                value: derived_label,
                goals,
                goal_nodes: label.origins.clone(),
                description,
            },
            bind_node_id,
            index,
            outcome.value,
            rule_index,
        )
        .map_err(Stop::Diagnostic)?;
        return Ok(Some(Block {
            rules: block.rules,
            origins: block.origins,
        }));
    }
    if !diagnostics.is_empty() {
        if deferrable_skip(cx, &diagnostics) {
            return Ok(None);
        }
        // :563. A deferrable diagnostic never masks a real one.
        let chosen = diagnostics
            .iter()
            .find(|d| !cx.deferrable(**d))
            .copied()
            .unwrap_or(diagnostics[0]);
        return Err(Stop::Diagnostic(chosen));
    }
    let derived = cx.compound("derived_label", vec![bind_node_id]);
    let derived_label = cx.compound("var", vec![derived]);
    let mut goals = replace_in(cx, label.value, derived_label, &label.goals);
    let mut goal_nodes = label.origins.clone();
    let rule_target = if outcome.structural {
        outcome.value
    } else {
        let key = cx.compound("derived_edge_target", vec![bind_node_id]);
        let target_variable = cx.compound("var", vec![key]);
        let replaced = replace_in(cx, outcome.value, target_variable, &outcome.goals);
        goals.extend(replaced);
        goal_nodes.extend(outcome.origins.clone());
        target_variable
    };
    let index_const = cx.compound("const", vec![index]);
    let head = colon_head(cx, owner, derived_label, rule_target, index_const);
    let body = cx.u.list(&goals);
    let rule = cx.compound("rule", vec![head, body]);
    Ok(Some(Block {
        rules: vec![rule],
        origins: rule_origins(cx, rule_index, bind_node_id, &goal_nodes),
    }))
}

struct Outcome {
    value: TermId,
    goals: Vec<TermId>,
    origins: Vec<TermId>,
    diagnostics: Vec<TermId>,
    structural: bool,
    partial: bool,
}

/// `:570`.
fn compound_edge_target(cx: &mut Cx, target: TermId, owner: TermId) -> Outcome {
    if let Some(node) = cx.u.unary(target, "deferred_expression") {
        let lowered = lower_expression(cx, node, owner);
        let partial = lowered.diagnostics.is_empty() && is_partial(cx, lowered.value);
        return Outcome {
            value: lowered.value,
            goals: lowered.goals,
            origins: lowered.origins,
            diagnostics: lowered.diagnostics,
            structural: false,
            partial,
        };
    }
    // :582. An atom target reads through the nearest deferred binding only.
    if let Some((functor, args)) = cx.u.functor(target) {
        if functor == "name" && args.len() == 2 {
            let (lookup_owner, name) = (args[0], args[1]);
            let found = cx
                .reservations
                .scoped(lookup_owner, name)
                .map(|r| (r.owner, r.target, r.kind));
            if let Some((bind_owner, row_target, kind)) = found {
                let expression = cx.atom("expression");
                let deferred =
                    cx.u.functor(row_target)
                        .is_some_and(|(n, a)| n == "deferred_expression" && a.len() == 3);
                if kind == expression && deferred {
                    let index = cx.u.functor(row_target).map(|(_, a)| a[2]).unwrap();
                    let key = cx.compound("compound_target", vec![name]);
                    let lookup = cx.compound("derived_lookup", vec![key]);
                    let value = cx.compound("var", vec![lookup]);
                    let goal = super::express::colon_goal(
                        cx,
                        lookup_owner,
                        bind_owner,
                        name,
                        value,
                        index,
                    );
                    return Outcome {
                        value,
                        goals: vec![goal],
                        origins: vec![],
                        diagnostics: vec![],
                        structural: false,
                        partial: false,
                    };
                }
            }
        }
    }
    // :558. A structural target keeps its own shape, with target(X) as ref(X).
    let value = match cx.u.unary(target, "target") {
        Some(inner) => cx.compound("ref", vec![inner]),
        None => target,
    };
    Outcome {
        value,
        goals: vec![],
        origins: vec![],
        diagnostics: vec![],
        structural: true,
        partial: false,
    }
}

fn colon_head(cx: &mut Cx, owner: TermId, label: TermId, target: TermId, index: TermId) -> TermId {
    let colon = cx.atom(":");
    let relation = cx.compound("name", vec![owner, colon]);
    let owner_ref = cx.compound("ref", vec![owner]);
    let arguments = cx.u.list(&[owner_ref, label, target, index]);
    cx.compound("call", vec![relation, arguments])
}

fn rule_origins(cx: &mut Cx, rule_index: i64, node: TermId, goal_nodes: &[TermId]) -> Vec<TermId> {
    let index = cx.int(rule_index);
    let key = cx.compound("rule", vec![index]);
    let mut origins = vec![cx.compound("origin", vec![key, node])];
    origins.extend(indexed_goal_origins(cx.u, goal_nodes, rule_index));
    origins
}

fn replace_in(cx: &mut Cx, from: TermId, to: TermId, goals: &[TermId]) -> Vec<TermId> {
    goals.iter().map(|g| cx.replace(from, to, *g)).collect()
}
