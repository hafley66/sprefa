//! Pass 3: facts and rules against the complete reservation table.
//! Port of `0_lowerer.pl:984-1225`.

use super::cx::{CallPolicy, Cx};
use super::express::{expression_callable, lower_argument};
use super::forms;
use super::host::indexed_goal_origins;
use super::slots;
use crate::_6_eval::term::{Term, TermId};

#[derive(Default)]
pub struct Executables {
    pub seeds: Vec<TermId>,
    pub rules: Vec<TermId>,
    pub origins: Vec<TermId>,
}

pub struct Call {
    pub call: TermId,
    pub goals: Vec<TermId>,
    pub goal_nodes: Vec<TermId>,
}

/// `:985`. A deferrable error advances neither ordinal.
pub fn lower_executables(
    cx: &mut Cx,
    items: &[TermId],
    owner: TermId,
    mut rule_index: i64,
) -> Result<Executables, TermId> {
    let mut out = Executables::default();
    let mut seed_index: i64 = 0;
    for item in items {
        if forms::bind_form(cx.u, *item).is_some() {
            continue;
        }
        if let Some(rule) = forms::rule_form(cx.u, *item) {
            match lower_rule(cx, &rule, owner, rule_index) {
                Ok((term, origins)) => {
                    out.rules.push(term);
                    out.origins.extend(origins);
                    rule_index += 1;
                }
                Err(diagnostic) => {
                    if cx.policy == CallPolicy::DeferUnknownCalls && cx.deferrable(diagnostic) {
                        continue;
                    }
                    return Err(diagnostic);
                }
            }
            continue;
        }
        match lower_seed(cx, *item, owner, seed_index) {
            Ok((seed, origin)) => {
                out.seeds.push(seed);
                out.origins.push(origin);
                seed_index += 1;
            }
            Err(diagnostic) => {
                if cx.policy == CallPolicy::DeferUnknownCalls && cx.deferrable(diagnostic) {
                    continue;
                }
                return Err(diagnostic);
            }
        }
    }
    Ok(out)
}

/// `:1047`.
fn lower_seed(
    cx: &mut Cx,
    node: TermId,
    owner: TermId,
    seed_index: i64,
) -> Result<(TermId, TermId), TermId> {
    let lowered = lower_call(cx, node, owner, false)?;
    let node_id = forms::node_id(cx.u, node).unwrap_or(node);
    if !lowered.goals.is_empty() {
        return Err(cx.plain(node_id, "expression_goals_in_seed"));
    }
    if call_contains_var(cx, lowered.call) {
        return Err(cx.plain(node_id, "variable_in_seed"));
    }
    let index = cx.int(seed_index);
    let key = cx.compound("seed", vec![index]);
    let origin = cx.compound("origin", vec![key, node_id]);
    Ok((lowered.call, origin))
}

/// `:1936`.
fn call_contains_var(cx: &Cx, call: TermId) -> bool {
    fn walk(cx: &Cx, term: TermId) -> bool {
        if cx
            .u
            .functor(term)
            .is_some_and(|(n, a)| n == "var" && a.len() == 1)
        {
            return true;
        }
        match cx.u.get(term) {
            Term::Compound(_, args) => args.iter().any(|a| walk(cx, *a)),
            _ => false,
        }
    }
    cx.u.functor(call)
        .map(|(_, args)| args[1])
        .and_then(|list| cx.u.as_list(list))
        .is_some_and(|arguments| arguments.iter().any(|a| walk(cx, *a)))
}

/// `:1061`. Head goals land after body goals.
fn lower_rule(
    cx: &mut Cx,
    rule: &forms::RuleForm,
    owner: TermId,
    rule_index: i64,
) -> Result<(TermId, Vec<TermId>), TermId> {
    let head = lower_call(cx, rule.head, owner, true)?;
    let mut goals = Vec::new();
    let mut goal_nodes = Vec::new();
    for node in &rule.body {
        let lowered = lower_goal(cx, *node, owner)?;
        goals.extend(lowered.0);
        goal_nodes.extend(lowered.1);
    }
    goals.extend(head.goals);
    goal_nodes.extend(head.goal_nodes);
    let list = cx.u.list(&goals);
    let term = cx.compound("rule", vec![head.call, list]);
    let index = cx.int(rule_index);
    let key = cx.compound("rule", vec![index]);
    let mut origins = vec![cx.compound("origin", vec![key, rule.rule_node])];
    origins.extend(indexed_goal_origins(cx.u, &goal_nodes, rule_index));
    Ok((term, origins))
}

/// `:1092`.
fn lower_goal(
    cx: &mut Cx,
    node: TermId,
    owner: TermId,
) -> Result<(Vec<TermId>, Vec<TermId>), TermId> {
    let parsed = forms::node(cx.u, node);
    if let Some(parsed) = &parsed {
        if let Some(items) = forms::form(cx.u, parsed.payload) {
            let head = forms::form_head_atom(cx.u, &items)
                .and_then(|a| cx.u.functor_or_atom(a).map(|(n, _)| n.to_string()));
            match head.as_deref() {
                Some("not") if items.len() == 2 => {
                    let lowered = lower_call(cx, items[1], owner, false)?;
                    return Ok(pending_goal_result(cx, lowered, "negative", parsed.id));
                }
                Some("not") => return Err(cx.plain(parsed.id, "invalid_negative_goal")),
                Some("count") => return Err(cx.plain(parsed.id, "aggregate_outside_rule_head")),
                _ => {}
            }
        }
    }
    let node_id = parsed.map(|p| p.id).unwrap_or(node);
    let lowered = lower_call(cx, node, owner, false)?;
    Ok(pending_goal_result(cx, lowered, "positive", node_id))
}

/// `:1110`.
fn pending_goal_result(
    cx: &mut Cx,
    lowered: Call,
    polarity: &str,
    node_id: TermId,
) -> (Vec<TermId>, Vec<TermId>) {
    let polarity = cx.atom(polarity);
    let goal = cx.compound("pending_goal", vec![polarity, lowered.call]);
    let mut goals = lowered.goals;
    goals.push(goal);
    let mut nodes = lowered.goal_nodes;
    nodes.push(node_id);
    (goals, nodes)
}

/// `:1122`.
pub fn lower_call(
    cx: &mut Cx,
    node: TermId,
    owner: TermId,
    head_mode: bool,
) -> Result<Call, TermId> {
    let Some(parsed) = forms::node(cx.u, node) else {
        return Err(cx.plain(node, "expected_call"));
    };
    let Some(items) = forms::form(cx.u, parsed.payload) else {
        return Err(cx.plain(parsed.id, "expected_call"));
    };
    let Some(head) = forms::form_head_atom(cx.u, &items) else {
        return Err(cx.plain(parsed.id, "expected_call"));
    };
    let head_name =
        cx.u.functor_or_atom(head)
            .map(|(n, _)| n.to_string())
            .unwrap_or_default();
    if head_name == ":" && items.len() == 5 {
        let arguments = lower_colon_arguments(cx, &items[1..], owner, head_mode)?;
        return finish_call_arguments(cx, head, parsed.id, owner, arguments, head_mode);
    }
    let (callable, arity, _) = match expression_callable(cx, head, owner) {
        Ok(found) => found,
        Err(reason) => return Err(cx.diagnostic(parsed.id, reason)),
    };
    let all = slots::callable_slots(cx, &callable, arity);
    let arguments =
        normalize_call_arguments(cx, head, parsed.id, &all, &items[1..], owner, head_mode)?;
    finish_call_arguments(cx, head, parsed.id, owner, arguments, head_mode)
}

struct Arguments {
    arguments: Vec<TermId>,
    goals: Vec<TermId>,
    goal_nodes: Vec<TermId>,
}

/// `:1169`.
fn lower_colon_arguments(
    cx: &mut Cx,
    nodes: &[TermId],
    owner: TermId,
    head_mode: bool,
) -> Result<Arguments, TermId> {
    let mut out = Arguments {
        arguments: Vec::with_capacity(4),
        goals: Vec::new(),
        goal_nodes: Vec::new(),
    };
    for (position, node) in nodes.iter().enumerate() {
        // :1178. The label position takes an atom or a symbol literal as a constant.
        if position == 1 {
            if let Some(constant) = colon_label_constant(cx, *node) {
                out.arguments.push(constant);
                continue;
            }
        }
        let (value, goals, goal_nodes) = lower_argument(cx, *node, owner, head_mode)?;
        out.arguments.push(value);
        out.goals.extend(goals);
        out.goal_nodes.extend(goal_nodes);
    }
    Ok(out)
}

fn colon_label_constant(cx: &mut Cx, node: TermId) -> Option<TermId> {
    let parsed = forms::node(cx.u, node)?;
    if let Some(name) = forms::atom_name(cx.u, parsed.payload) {
        return Some(cx.compound("const", vec![name]));
    }
    let literal = forms::literal_value(cx.u, parsed.payload)?;
    let symbol = cx.u.unary(literal, "symbol")?;
    Some(cx.compound("const", vec![symbol]))
}

/// `:1226`.
fn normalize_call_arguments(
    cx: &mut Cx,
    name: TermId,
    node_id: TermId,
    all: &[slots::Slot],
    argument_nodes: &[TermId],
    owner: TermId,
    head_mode: bool,
) -> Result<Arguments, TermId> {
    let classified = slots::classify(cx, argument_nodes, all)
        .map_err(|reason| cx.diagnostic(node_id, reason))?;
    let assigned = match slots::assign(cx, &classified, all, &[]) {
        Ok(assigned) => assigned,
        Err(reason) => {
            let too_many = cx.atom("too_many_arguments");
            let reason = if reason == too_many {
                let expected = cx.int(all.len() as i64);
                let observed = cx.int(argument_nodes.len() as i64);
                cx.compound("arity_mismatch", vec![name, expected, observed])
            } else {
                reason
            };
            return Err(cx.diagnostic(node_id, reason));
        }
    };
    let mut bound: Vec<(i64, TermId)> = Vec::with_capacity(assigned.len());
    let mut goals = Vec::new();
    let mut goal_nodes = Vec::new();
    for item in &assigned {
        let (value, own_goals, own_nodes) = lower_argument(cx, item.node, owner, head_mode)?;
        bound.push((item.index, value));
        goals.extend(own_goals);
        goal_nodes.extend(own_nodes);
    }
    Ok(Arguments {
        arguments: slots::fill(cx, all, &bound, node_id),
        goals,
        goal_nodes,
    })
}

/// `:1151`.
fn finish_call_arguments(
    cx: &mut Cx,
    name: TermId,
    node_id: TermId,
    owner: TermId,
    arguments: Arguments,
    head_mode: bool,
) -> Result<Call, TermId> {
    if head_mode {
        let aggregates = arguments
            .arguments
            .iter()
            .filter(|a| {
                cx.u.functor(**a)
                    .is_some_and(|(n, args)| n == "aggregate" && args.len() == 2)
            })
            .count();
        if aggregates > 1 {
            let reason = cx.compound("multiple_count_aggregates", vec![name]);
            return Err(cx.diagnostic(node_id, reason));
        }
    }
    let relation = cx.compound("name", vec![owner, name]);
    let list = cx.u.list(&arguments.arguments);
    let call = cx.compound("call", vec![relation, list]);
    Ok(Call {
        call,
        goals: arguments.goals,
        goal_nodes: arguments.goal_nodes,
    })
}
