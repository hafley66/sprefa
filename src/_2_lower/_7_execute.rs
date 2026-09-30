//! Pass 3: facts and rules against the complete reservation table.
//! Port of `0_lowerer.pl:984-1225`.

use super::cx::{CallPolicy, Cx};
use super::express::{self, expression_callable, lower_argument};
use super::forms;
use super::origins::indexed_goal_origins;
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
    pub post_goals: Vec<TermId>,
    pub post_nodes: Vec<TermId>,
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
                Ok((term, origins, position_rules)) => {
                    out.rules.push(term);
                    out.origins.extend(origins);
                    rule_index += 1;
                    for (position_rule, prefix_nodes) in position_rules {
                        out.rules.push(position_rule);
                        let index = cx.int(rule_index);
                        let key = cx.compound("rule", vec![index]);
                        out.origins
                            .push(cx.compound("origin", vec![key, rule.rule_node]));
                        out.origins
                            .extend(indexed_goal_origins(cx.u, &prefix_nodes, rule_index));
                        rule_index += 1;
                    }
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
    cx.ground_values = true;
    let lowered = lower_call(cx, node, owner, false).and_then(|first| {
        // A variant in a fact is a value; readback patterns are for goals.
        if first.post_goals.is_empty() {
            Ok(first)
        } else {
            lower_call(cx, node, owner, true)
        }
    });
    cx.ground_values = false;
    let lowered = lowered?;
    let node_id = forms::node_id(cx.u, node).unwrap_or(node);
    if !lowered.goals.is_empty() || !lowered.post_goals.is_empty() {
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
) -> Result<(TermId, Vec<TermId>, Vec<(TermId, Vec<TermId>)>), TermId> {
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
    let (goals, goal_nodes) = expand_generic_goals(cx, owner, &goals, &goal_nodes)?;
    let list = cx.u.list(&goals);
    let term = cx.compound("rule", vec![head.call, list]);
    let index = cx.int(rule_index);
    let key = cx.compound("rule", vec![index]);
    let mut origins = vec![cx.compound("origin", vec![key, rule.rule_node])];
    origins.extend(indexed_goal_origins(cx.u, &goal_nodes, rule_index));
    let mut position_rules = tuple_position_rules(cx, owner, &goals, &goal_nodes);
    position_rules.extend(generic_edge_rules(cx, owner, &goals, &goal_nodes));
    Ok((term, origins, position_rules))
}

/// Nesting of `(* ...)` and `(+ ...)` inside one `return` expression.
const GENERIC_TEMPLATE_DEPTH: usize = 16;

/// An instance a rule builds with `intern` is output of the rule: one sibling
/// rule per edge of the callee's `return` expression writes
/// `(: Instance Label Value Index)` from the goals up to that `intern`, the
/// rows a bind writes for the same instance. A nested expression is its own
/// node, `application(Expression, Args)`, built by one more `intern`.
fn generic_edge_rules(
    cx: &mut Cx,
    owner: TermId,
    goals: &[TermId],
    goal_nodes: &[TermId],
) -> Vec<(TermId, Vec<TermId>)> {
    let mut out = Vec::new();
    let intern = cx.atom("intern");
    let return_atom = cx.atom("return");
    for (at, goal) in goals.iter().enumerate() {
        let Some(("pending_goal", parts)) = cx.u.functor(*goal) else {
            continue;
        };
        let Some(("call", call)) = cx.u.functor(parts[1]) else {
            continue;
        };
        let is_intern = cx
            .u
            .args::<2>(call[0], "name")
            .is_some_and(|[_, label]| label == intern);
        let Some(arguments) = cx.u.as_list(call[1]).filter(|_| is_intern) else {
            continue;
        };
        let [constructor, tuple, instance] = arguments[..] else {
            continue;
        };
        let Some(callable) = cx.u.unary(constructor, "ref") else {
            continue;
        };
        if !super::generic::is_generic_callable(cx, callable) {
            continue;
        }
        let Some(tuple_goal) = goals[..at].iter().rev().find_map(|g| tuple_elements(cx, *g, tuple))
        else {
            continue;
        };
        let columns: Vec<(TermId, TermId)> = {
            let mut labelled: Vec<(i64, TermId)> = cx
                .edges
                .owned(callable)
                .into_iter()
                .filter(|edge| edge.name != return_atom)
                .map(|edge| (edge.index, edge.name))
                .collect();
            labelled.sort_by_key(|(index, _)| *index);
            labelled.dedup_by_key(|(index, _)| *index);
            labelled
                .into_iter()
                .map(|(_, label)| label)
                .zip(tuple_goal)
                .collect()
        };
        let Some(template) = cx
            .edges
            .owned(callable)
            .into_iter()
            .find(|edge| edge.name == return_atom)
            .and_then(|edge| cx.u.unary(edge.target, "target"))
        else {
            continue;
        };
        let body = goals[..=at].to_vec();
        let nodes = goal_nodes[..=at.min(goal_nodes.len() - 1)].to_vec();
        let node = nodes.last().copied().unwrap_or(*goal);
        let mut level = vec![(template, instance, body, nodes)];
        for _ in 0..GENERIC_TEMPLATE_DEPTH {
            let mut next = Vec::new();
            for (template, instance, body, nodes) in level {
                let mut seen = Vec::new();
                let edges: Vec<(TermId, TermId, i64)> = cx
                    .edges
                    .owned(template)
                    .into_iter()
                    .map(|edge| (edge.name, edge.target, edge.index))
                    .collect();
                for (label, target, index) in edges {
                    if seen.contains(&index) {
                        continue;
                    }
                    seen.push(index);
                    let mut edge_body = body.clone();
                    let mut edge_nodes = nodes.clone();
                    let value = if let Some(inner) = cx.u.unary(target, "target") {
                        let key = cx.compound("generic_instance", vec![instance, inner]);
                        let child = cx.compound("var", vec![key]);
                        let inner_ref = cx.compound("ref", vec![inner]);
                        let relation = cx.compound("name", vec![owner, intern]);
                        let list = cx.u.list(&[inner_ref, tuple, child]);
                        let call = cx.compound("call", vec![relation, list]);
                        let positive = cx.atom("positive");
                        edge_body.push(cx.compound("pending_goal", vec![positive, call]));
                        edge_nodes.push(node);
                        next.push((inner, child, edge_body.clone(), edge_nodes.clone()));
                        child
                    } else if let Some(value) = cx
                        .u
                        .args::<2>(target, "name")
                        .and_then(|[_, name]| columns.iter().find(|(column, _)| *column == name))
                        .map(|(_, value)| *value)
                    {
                        value
                    } else {
                        target
                    };
                    let label = cx.compound("const", vec![label]);
                    let index = cx.int(index);
                    let index = cx.compound("const", vec![index]);
                    let colon = cx.atom(":");
                    let relation = cx.compound("name", vec![owner, colon]);
                    let list = cx.u.list(&[instance, label, value, index]);
                    let head = cx.compound("call", vec![relation, list]);
                    let list = cx.u.list(&edge_body);
                    out.push((cx.compound("rule", vec![head, list]), edge_nodes));
                }
            }
            level = next;
        }
    }
    out
}

/// The elements of the `tuple` goal whose last argument is `whole`.
fn tuple_elements(cx: &Cx, goal: TermId, whole: TermId) -> Option<Vec<TermId>> {
    let ("pending_goal", parts) = cx.u.functor(goal)? else {
        return None;
    };
    let ("call", call) = cx.u.functor(parts[1])? else {
        return None;
    };
    let [_, label] = cx.u.args::<2>(call[0], "name")?;
    if cx.u.functor_or_atom(label).map(|(n, _)| n) != Some("tuple") {
        return None;
    }
    let arguments = cx.u.as_list(call[1])?;
    let (last, elements) = arguments.split_last()?;
    (*last == whole).then(|| elements.to_vec())
}

/// A tuple built in a rule body and handed to a relation call is output of the
/// rule: one sibling rule per position writes `(: Tuple k Element k)` from the
/// goals up to the tuple goal, so a position reads the same for a written
/// tuple and a built one. A tuple whose elements are not all bound by earlier
/// goals is the callee's own construction and writes no positions.
fn tuple_position_rules(
    cx: &mut Cx,
    owner: TermId,
    goals: &[TermId],
    goal_nodes: &[TermId],
) -> Vec<(TermId, Vec<TermId>)> {
    let mut out = Vec::new();
    for (at, goal) in goals.iter().enumerate() {
        let Some(("pending_goal", parts)) = cx.u.functor(*goal) else {
            continue;
        };
        let Some(("call", call)) = cx.u.functor(parts[1]) else {
            continue;
        };
        let Some(("name", name)) = cx.u.functor(call[0]) else {
            continue;
        };
        if cx.u.functor_or_atom(name[1]).map(|(n, _)| n) != Some("tuple") {
            continue;
        }
        let Some(arguments) = cx.u.as_list(call[1]) else {
            continue;
        };
        let Some((whole, elements)) = arguments.split_last() else {
            continue;
        };
        let earlier: Vec<TermId> = goals[..at].to_vec();
        let bound = |cx: &Cx, term: TermId| {
            !term_has_var(cx, term) || earlier.iter().any(|g| term_mentions(cx, *g, term))
        };
        if !elements.iter().all(|e| bound(cx, *e)) {
            continue;
        }
        let handed_on = goals[at + 1..].iter().any(|later| {
            let Some(("pending_goal", later_parts)) = cx.u.functor(*later) else {
                return false;
            };
            let Some(("call", later_call)) = cx.u.functor(later_parts[1]) else {
                return false;
            };
            let Some(("name", later_name)) = cx.u.functor(later_call[0]) else {
                return false;
            };
            let label = cx
                .u
                .functor_or_atom(later_name[1])
                .map(|(n, _)| n.to_string())
                .unwrap_or_default();
            super::kernel::kernel_relation(None, &label).is_none()
                && !super::kernel::kernel_variadic(None, &label)
                && term_mentions(cx, later_parts[1], *whole)
        });
        if !handed_on {
            continue;
        }
        for (position, element) in elements.iter().enumerate() {
            let index = cx.int(position as i64);
            let index = cx.compound("const", vec![index]);
            let colon = cx.atom(":");
            let relation = cx.compound("name", vec![owner, colon]);
            let list = cx.u.list(&[*whole, index, *element, index]);
            let head = cx.compound("call", vec![relation, list]);
            let body = cx.u.list(&goals[..=at]);
            let rule = cx.compound("rule", vec![head, body]);
            out.push((rule, goal_nodes[..=at.min(goal_nodes.len() - 1)].to_vec()));
        }
    }
    out
}

/// Nesting depth searched inside one lowered goal term.
const POSITION_TERM_DEPTH: usize = 32;

fn term_has_var(cx: &Cx, term: TermId) -> bool {
    let mut level = vec![term];
    for _ in 0..POSITION_TERM_DEPTH {
        let mut next = Vec::new();
        for t in level {
            if cx.u.functor(t).is_some_and(|(n, a)| n == "var" && a.len() == 1) {
                return true;
            }
            if let Term::Compound(_, args) = cx.u.get(t) {
                next.extend(args.iter().copied());
            }
        }
        level = next;
    }
    false
}

fn term_mentions(cx: &Cx, haystack: TermId, needle: TermId) -> bool {
    let mut level = vec![haystack];
    for _ in 0..POSITION_TERM_DEPTH {
        let mut next = Vec::new();
        for t in level {
            if t == needle {
                return true;
            }
            if let Term::Compound(_, args) = cx.u.get(t) {
                next.extend(args.iter().copied());
            }
        }
        level = next;
    }
    false
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
                Some("count") | Some("fold") => {
                    return Err(cx.plain(parsed.id, "aggregate_outside_rule_head"))
                }
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
    goals.extend(lowered.post_goals);
    let mut nodes = lowered.goal_nodes;
    nodes.push(node_id);
    nodes.extend(lowered.post_nodes);
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
    let head = forms::form_head_atom(cx.u, &items);
    if head.is_none() {
        let typed = items
            .first()
            .and_then(|first| express::primitive_path_head(cx, *first, owner));
        if let Some((call_owner, label)) = typed {
            return lower_named_call(cx, parsed.id, label, &items, call_owner, owner, head_mode);
        }
        if let Some(call) = lower_path_call(cx, parsed.id, &items, owner, head_mode)? {
            return Ok(call);
        }
    }
    let Some(head) = head else {
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
    lower_named_call(cx, parsed.id, head, &items, owner, owner, head_mode)
}

/// A call whose head names a relation under `call_owner`; its arguments lower
/// in the scope `owner`.
fn lower_named_call(
    cx: &mut Cx,
    node_id: TermId,
    head: TermId,
    items: &[TermId],
    call_owner: TermId,
    owner: TermId,
    head_mode: bool,
) -> Result<Call, TermId> {
    let head_text =
        cx.u.functor_or_atom(head)
            .map(|(n, _)| n.to_string())
            .unwrap_or_default();
    let variadic = super::kernel::kernel_variadic(None, &head_text)
        && cx.reservations.scoped(call_owner, head).is_none();
    let resolved = if variadic {
        Ok(express::Applied::Relation(
            slots::Callable::Kernel {
                owner: None,
                label: head_text,
            },
            items.len() as i64 - 1,
            cx.u.list(&[]),
        ))
    } else {
        expression_callable(cx, head, call_owner)
    };
    let (callable, arity) = match resolved {
        Ok(express::Applied::Relation(callable, arity, _)) => (callable, arity),
        Ok(express::Applied::Generic(callable)) => {
            return generic_goal(cx, head, node_id, callable, &items[1..], owner, head_mode);
        }
        // A goal position takes a relation only; construction is spelled with
        // `intern` here, so the head names no relation.
        Ok(express::Applied::Construction(_)) => {
            let reason = express::not_a_relation(cx, head, call_owner);
            return Err(cx.diagnostic(node_id, reason));
        }
        Err(reason) => return Err(cx.diagnostic(node_id, reason)),
    };
    let all = slots::callable_slots(cx, &callable, arity);
    let arguments =
        normalize_call_arguments(cx, head, node_id, &all, &items[1..], owner, head_mode)?;
    finish_call_arguments(cx, head, node_id, call_owner, arguments, head_mode)
}

/// `(Callee Column.. Return)` as a goal: a `generic_goal(Callee, Columns,
/// Return)` placeholder that `expand_generic_goals` spells once the goals
/// before it are known. A head or a fact holds no such goal.
#[allow(clippy::too_many_arguments)]
fn generic_goal(
    cx: &mut Cx,
    head: TermId,
    node_id: TermId,
    callable: TermId,
    argument_nodes: &[TermId],
    owner: TermId,
    head_mode: bool,
) -> Result<Call, TermId> {
    if head_mode || cx.ground_values {
        return Err(not_built_yet(cx, node_id));
    }
    let arity = cx.edges.arity(callable);
    let all = slots::callable_slots(cx, &slots::Callable::Target(callable), arity);
    let arguments =
        normalize_call_arguments(cx, head, node_id, &all, argument_nodes, owner, head_mode)?;
    let return_atom = cx.atom("return");
    let return_indices = cx.edges.return_indices(callable, return_atom);
    let mut columns = Vec::with_capacity(all.len());
    let mut result = None;
    for (slot, value) in all.iter().zip(&arguments.arguments) {
        if return_indices.contains(&slot.index) {
            result = Some(*value);
        } else {
            columns.push(*value);
        }
    }
    let Some(result) = result else {
        return Err(not_built_yet(cx, node_id));
    };
    let constructor = cx.compound("ref", vec![callable]);
    let columns = cx.u.list(&columns);
    let call = cx.compound("generic_goal", vec![constructor, columns, result]);
    Ok(Call {
        call,
        goals: arguments.goals,
        goal_nodes: arguments.goal_nodes,
        post_goals: arguments.post_goals,
        post_nodes: arguments.post_nodes,
    })
}

fn not_built_yet(cx: &mut Cx, node_id: TermId) -> TermId {
    let shape = cx.atom("generic_application");
    let reason = cx.compound("not_built_yet", vec![shape]);
    cx.diagnostic(node_id, reason)
}

/// Every `generic_goal` placeholder in rule order. Columns bound by earlier
/// goals and a free return build the instance; any other binding reads it
/// back: `intern_snapshot(Callee, Arguments, Return)` then
/// `tuple(Column.., Arguments)`.
fn expand_generic_goals(
    cx: &mut Cx,
    owner: TermId,
    goals: &[TermId],
    goal_nodes: &[TermId],
) -> Result<(Vec<TermId>, Vec<TermId>), TermId> {
    let mut out = Vec::with_capacity(goals.len());
    let mut nodes = Vec::with_capacity(goal_nodes.len());
    for (at, goal) in goals.iter().enumerate() {
        let node = goal_nodes.get(at).copied().unwrap_or(*goal);
        let placeholder = match cx.u.functor(*goal) {
            Some(("pending_goal", parts)) => match cx.u.functor(parts[1]) {
                Some(("generic_goal", args)) => Some((parts[0], args[0], args[1], args[2])),
                _ => None,
            },
            _ => None,
        };
        let Some((polarity, constructor, columns, result)) = placeholder else {
            out.push(*goal);
            nodes.push(node);
            continue;
        };
        if cx.u.functor_or_atom(polarity).map(|(n, _)| n) != Some("positive") {
            return Err(not_built_yet(cx, node));
        }
        let columns = cx.u.as_list(columns).unwrap_or_default();
        let bound = |cx: &Cx, term: TermId| {
            !term_has_var(cx, term) || out.iter().any(|g| positive_mentions(cx, *g, term))
        };
        let builds = columns.iter().all(|c| bound(cx, *c)) && !bound(cx, result);
        let node_id = forms::node_id(cx.u, node).unwrap_or(node);
        let key = cx.compound("generic_arguments", vec![node_id]);
        let arguments = cx.compound("var", vec![key]);
        let mut elements = columns;
        elements.push(arguments);
        if builds {
            let tuple = generic_kernel_goal(cx, owner, "tuple", &elements);
            let intern =
                generic_kernel_goal(cx, owner, "intern", &[constructor, arguments, result]);
            out.extend([tuple, intern]);
            nodes.extend([node, node]);
            continue;
        }
        let snapshot = generic_kernel_goal(
            cx,
            owner,
            "intern_snapshot",
            &[constructor, arguments, result],
        );
        let tuple = generic_kernel_goal(cx, owner, "tuple", &elements);
        out.extend([snapshot, tuple]);
        nodes.extend([node, node]);
    }
    Ok((out, nodes))
}

fn generic_kernel_goal(cx: &mut Cx, owner: TermId, label: &str, arguments: &[TermId]) -> TermId {
    let label = cx.atom(label);
    let relation = cx.compound("name", vec![owner, label]);
    let list = cx.u.list(arguments);
    let call = cx.compound("call", vec![relation, list]);
    let positive = cx.atom("positive");
    cx.compound("pending_goal", vec![positive, call])
}

fn positive_mentions(cx: &Cx, goal: TermId, term: TermId) -> bool {
    let positive = cx.u.functor(goal).is_some_and(|(n, parts)| {
        n == "pending_goal" && cx.u.functor_or_atom(parts[0]).map(|(p, _)| p) == Some("positive")
    });
    positive && term_mentions(cx, goal, term)
}

struct Arguments {
    arguments: Vec<TermId>,
    goals: Vec<TermId>,
    goal_nodes: Vec<TermId>,
    post_goals: Vec<TermId>,
    post_nodes: Vec<TermId>,
}

/// A sum variant in a goal argument reads back after the outer goal binds
/// its value. Construction and readback use the same `intern` identity.
fn sum_pattern(
    cx: &mut Cx,
    node: TermId,
    owner: TermId,
    head_mode: bool,
) -> Result<Option<(TermId, Vec<TermId>)>, TermId> {
    if head_mode {
        return Ok(None);
    }
    let Some(parsed) = forms::node(cx.u, node) else {
        return Ok(None);
    };
    let Some(items) = forms::form(cx.u, parsed.payload) else {
        return Ok(None);
    };
    let constructor = if let Some(name) = forms::form_head_atom(cx.u, &items) {
        let Some(found) = cx.reservations.scoped(owner, name) else {
            return Ok(None);
        };
        if cx.u.functor_or_atom(found.kind).map(|(kind, _)| kind) != Some("sum") {
            return Ok(None);
        }
        let Some(inner) = cx.u.unary(found.target, "target") else {
            return Ok(None);
        };
        cx.compound("ref", vec![inner])
    } else {
        let Some(head) = items.first().copied() else {
            return Ok(None);
        };
        let Some(constructor) = express::sum_path_constructor(cx, head, owner) else {
            return Ok(None);
        };
        constructor
    };
    let value_key = cx.compound("sum_pattern", vec![parsed.id]);
    let value = cx.compound("var", vec![value_key]);
    let arguments_key = cx.compound("sum_arguments", vec![parsed.id]);
    let list = cx.compound("var", vec![arguments_key]);
    let mut goals = Vec::new();
    let mut push = |cx: &mut Cx, label: &str, args: Vec<TermId>, goals: &mut Vec<TermId>| {
        let name = cx.atom(label);
        let relation = cx.compound("name", vec![owner, name]);
        let args = cx.u.list(&args);
        let call = cx.compound("call", vec![relation, args]);
        let positive = cx.atom("positive");
        goals.push(cx.compound("pending_goal", vec![positive, call]));
    };
    push(
        cx,
        "intern_snapshot",
        vec![constructor, list, value],
        &mut goals,
    );
    let mut elements = Vec::with_capacity(items.len());
    for item in &items[1..] {
        let (element, own_goals, _) = lower_argument(cx, *item, owner, false)?;
        goals.extend(own_goals);
        elements.push(element);
    }
    elements.push(list);
    push(cx, "tuple", elements, &mut goals);
    Ok(Some((value, goals)))
}

/// `((. a b) x y)`: the callable is a dotted path. Every segment but the last
/// names an owner and the last names the relation, so the callable is a `name/2`
/// chain the checker walks. Slots are unknown here, so the arguments keep their
/// source order. `None` when the head is not a path form.
fn lower_path_call(
    cx: &mut Cx,
    node_id: TermId,
    items: &[TermId],
    owner: TermId,
    head_mode: bool,
) -> Result<Option<Call>, TermId> {
    let Some(first) = items.first().copied() else {
        return Ok(None);
    };
    let Some(segments) = express::path_segments(cx.u, first) else {
        return Ok(None);
    };
    if segments.len() < 2 {
        return Err(cx.plain(node_id, "invalid_path"));
    }
    let mut callable_owner = owner;
    for segment in &segments[..segments.len() - 1] {
        let Some(name) = express::path_segment_atom(cx.u, *segment) else {
            return Err(cx.plain(node_id, "invalid_path"));
        };
        callable_owner = cx.compound("name", vec![callable_owner, name]);
    }
    let Some(label) = express::path_segment_atom(cx.u, segments[segments.len() - 1]) else {
        return Err(cx.plain(node_id, "invalid_path"));
    };
    let mut arguments = Vec::with_capacity(items.len() - 1);
    let mut goals = Vec::new();
    let mut goal_nodes = Vec::new();
    let mut post_goals = Vec::new();
    let mut post_nodes = Vec::new();
    for node in &items[1..] {
        if let Some((value, deferred)) = sum_pattern(cx, *node, owner, head_mode)? {
            arguments.push(value);
            post_nodes.extend(std::iter::repeat_n(node_id, deferred.len()));
            post_goals.extend(deferred);
            continue;
        }
        let (value, own_goals, own_nodes) = lower_argument(cx, *node, owner, head_mode)?;
        arguments.push(value);
        goals.extend(own_goals);
        goal_nodes.extend(own_nodes);
    }
    let arguments = Arguments {
        arguments,
        goals,
        goal_nodes,
        post_goals,
        post_nodes,
    };
    let call = finish_call_arguments(cx, label, node_id, callable_owner, arguments, head_mode)?;
    Ok(Some(call))
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
        post_goals: Vec::new(),
        post_nodes: Vec::new(),
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
    let mut post_goals = Vec::new();
    let mut post_nodes = Vec::new();
    for item in &assigned {
        if let Some((value, deferred)) = sum_pattern(cx, item.node, owner, head_mode)? {
            bound.push((item.index, value));
            post_nodes.extend(std::iter::repeat_n(node_id, deferred.len()));
            post_goals.extend(deferred);
            continue;
        }
        let (value, own_goals, own_nodes) = lower_argument(cx, item.node, owner, head_mode)?;
        bound.push((item.index, value));
        goals.extend(own_goals);
        goal_nodes.extend(own_nodes);
    }
    Ok(Arguments {
        arguments: slots::fill(cx, all, &bound, node_id),
        goals,
        goal_nodes,
        post_goals,
        post_nodes,
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
                cx.u.functor(**a).is_some_and(|(n, args)| {
                    (n == "aggregate" && args.len() == 2) || (n == "fold" && args.len() == 3)
                })
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
        post_goals: arguments.post_goals,
        post_nodes: arguments.post_nodes,
    })
}
