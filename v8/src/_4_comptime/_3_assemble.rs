//! `assemble_generated_program/5`. Port of
//! `1a_generated_program_assembler.pl:17-397`.

use super::api::{diagnostic, is_kernel_ref};
use crate::_3_check::api::prolog_sort;
use crate::_6_eval::term::{Term, TermId, Universe};
use std::collections::{HashMap, HashSet};

/// `body/4` read as `(GoalIndex, Some((Polarity, Application)))`; `None` when
/// the row's polarity or application is not the shape `:198` matches.
pub type BodyHeader = (TermId, Option<(TermId, TermId)>);

pub struct Assembled {
    pub relations: Vec<TermId>,
    pub rules: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// `application_context/4` at `:352`, indexed by the identity each row keys on.
#[derive(Default)]
pub struct Cx {
    pub colon_by_owner: HashMap<TermId, Vec<[TermId; 3]>>,
    pub apply: HashMap<TermId, Vec<TermId>>,
    pub variables: HashMap<TermId, Vec<TermId>>,
    pub literals: HashMap<TermId, Vec<TermId>>,
    pub def_ids: Vec<TermId>,
    pub def_arities: HashMap<TermId, Vec<TermId>>,
    pub head_ids: Vec<TermId>,
    pub head_apps: HashMap<TermId, Vec<TermId>>,
    pub body_by_rule: HashMap<TermId, Vec<BodyHeader>>,
    pub fragment_ids: Vec<TermId>,
}

fn ref_of(u: &Universe, term: TermId) -> Option<TermId> {
    u.unary(term, "ref")
}

fn const_of(u: &Universe, term: TermId) -> Option<TermId> {
    u.unary(term, "const")
}

/// `call(Rel, Args)` split into the relation term and the argument list.
fn call_parts(u: &Universe, row: TermId) -> Option<(TermId, Vec<TermId>)> {
    let ("call", args) = u.functor(row)? else {
        return None;
    };
    if args.len() != 2 {
        return None;
    }
    let (rel, list) = (args[0], args[1]);
    Some((rel, u.as_list(list)?))
}

/// `:17`. Diagnostics discard every generated relation and rule, not only the
/// offending one; fork F5.
pub fn assemble_generated_program(
    u: &mut Universe,
    rows: &[TermId],
    base_relations: &[TermId],
) -> Assembled {
    let cx = index(u, rows);
    let (relations0, definition_diagnostics) = assemble_definitions(u, &cx);
    let collision_diagnostics = collision_diagnostics(u, &relations0, base_relations);

    let mut all = base_relations.to_vec();
    all.extend_from_slice(&relations0);
    let all = prolog_sort(u, all);
    let arities = relation_arities(u, &all);

    let (rules0, rule_diagnostics) = assemble_rules(u, &cx, &arities);
    let orphan = orphan_diagnostics(u, &cx);

    let mut diagnostics = definition_diagnostics;
    diagnostics.extend(collision_diagnostics);
    diagnostics.extend(rule_diagnostics);
    diagnostics.extend(orphan);
    let diagnostics = prolog_sort(u, diagnostics);
    if diagnostics.is_empty() {
        Assembled {
            relations: prolog_sort(u, relations0),
            rules: prolog_sort(u, rules0),
            diagnostics,
        }
    } else {
        Assembled {
            relations: vec![],
            rules: vec![],
            diagnostics,
        }
    }
}

/// `memberchk(relation(ref(R), Arity, _), Relations)` over the sorted list.
fn relation_arities(u: &Universe, relations: &[TermId]) -> HashMap<TermId, TermId> {
    let mut out = HashMap::new();
    for row in relations {
        let Some(("relation", args)) = u.functor(*row) else {
            continue;
        };
        if args.len() != 3 {
            continue;
        }
        let Some(id) = ref_of(u, args[0]) else {
            continue;
        };
        out.entry(id).or_insert(args[1]);
    }
    out
}

pub fn index(u: &mut Universe, rows: &[TermId]) -> Cx {
    let mut cx = Cx::default();
    let mut named: HashMap<String, Vec<TermId>> = HashMap::new();
    let mut by_rel: HashMap<TermId, Vec<Vec<TermId>>> = HashMap::new();
    let mut def_ids = Vec::new();
    let mut head_ids = Vec::new();
    let mut fragment_ids = Vec::new();

    for row in rows {
        let Some((rel, args)) = call_parts(u, *row) else {
            continue;
        };
        if is_kernel_ref(u, rel, ":") && args.len() == 4 {
            if let Some(owner) = ref_of(u, args[0]) {
                cx.colon_by_owner
                    .entry(owner)
                    .or_default()
                    .push([args[1], args[2], args[3]]);
            }
            if let (Some(name), Some(target), Some(_)) = (
                const_of(u, args[1]).and_then(|n| atom_text(u, n)),
                ref_of(u, args[2]),
                const_of(u, args[3]),
            ) {
                if ref_of(u, args[0]).is_some() {
                    named.entry(name).or_default().push(target);
                }
            }
            continue;
        }
        if is_kernel_ref(u, rel, "def") && args.len() == 2 {
            if let Some(id) = ref_of(u, args[0]) {
                def_ids.push(id);
                if let Some(arity) = const_of(u, args[1]) {
                    cx.def_arities.entry(id).or_default().push(arity);
                }
            }
            continue;
        }
        if is_kernel_ref(u, rel, "head") && args.len() == 2 {
            if let Some(id) = ref_of(u, args[0]) {
                head_ids.push(id);
                if let Some(application) = ref_of(u, args[1]) {
                    cx.head_apps.entry(id).or_default().push(application);
                }
            }
            continue;
        }
        if is_kernel_ref(u, rel, "body") {
            if let Some(id) = args.first().and_then(|a| ref_of(u, *a)) {
                fragment_ids.push(id);
                if args.len() == 4 {
                    if let Some(goal) = const_of(u, args[1]) {
                        let header = match (const_of(u, args[2]), ref_of(u, args[3])) {
                            (Some(polarity), Some(application)) => Some((polarity, application)),
                            _ => None,
                        };
                        cx.body_by_rule.entry(id).or_default().push((goal, header));
                    }
                }
            }
            continue;
        }
        if let Some(id) = ref_of(u, rel) {
            by_rel.entry(id).or_default().push(args);
        }
    }

    for apply in named.get("Apply").into_iter().flatten() {
        for args in by_rel.get(apply).into_iter().flatten() {
            if args.len() != 2 {
                continue;
            }
            if let (Some(application), Some(callable)) = (ref_of(u, args[0]), ref_of(u, args[1])) {
                cx.apply.entry(application).or_default().push(callable);
            }
        }
    }
    cx.variables = node_family(u, &named, "Variable", &by_rel, true);
    cx.literals = node_family(u, &named, "Literal", &by_rel, false);
    cx.def_ids = sorted_unique(u, def_ids);
    cx.head_ids = sorted_unique(u, head_ids);
    cx.fragment_ids = sorted_unique(u, fragment_ids);
    cx
}

/// `variable_node_rows/3` at `:379` and `literal_node_rows/3` at `:388`. The
/// variable row needs `const(Name)`; the literal row takes the value as read.
fn node_family(
    u: &mut Universe,
    named: &HashMap<String, Vec<TermId>>,
    family: &str,
    by_rel: &HashMap<TermId, Vec<Vec<TermId>>>,
    variable: bool,
) -> HashMap<TermId, Vec<TermId>> {
    let mut out: HashMap<TermId, Vec<TermId>> = HashMap::new();
    let empty = Vec::new();
    let relations = named.get(family).unwrap_or(&empty).clone();
    for rel in relations {
        let rows = by_rel.get(&rel).cloned().unwrap_or_default();
        for args in rows {
            if args.len() != 3 {
                continue;
            }
            let Some(node) = ref_of(u, args[0]) else {
                continue;
            };
            let payload = if variable {
                let (Some(scope), Some(name)) = (ref_of(u, args[1]), const_of(u, args[2])) else {
                    continue;
                };
                u.compound("variable", vec![scope, name])
            } else {
                let Some(primitive) = ref_of(u, args[1]) else {
                    continue;
                };
                u.compound("literal", vec![primitive, args[2]])
            };
            out.entry(node).or_default().push(payload);
        }
    }
    out
}

fn atom_text(u: &Universe, id: TermId) -> Option<String> {
    match u.get(id) {
        Term::Atom(s) => Some(u.sym_str(*s).to_string()),
        _ => None,
    }
}

fn sorted_unique(u: &Universe, mut ids: Vec<TermId>) -> Vec<TermId> {
    ids.sort_by(|a, b| u.cmp(*a, *b));
    ids.dedup();
    ids
}

/// `:36`.
fn assemble_definitions(u: &mut Universe, cx: &Cx) -> (Vec<TermId>, Vec<TermId>) {
    let mut relations = Vec::new();
    let mut diagnostics = Vec::new();
    for id in &cx.def_ids {
        let arities = sorted_unique(u, cx.def_arities.get(id).cloned().unwrap_or_default());
        match arities.as_slice() {
            [only] => {
                let arity = *only;
                if u.as_int(arity).is_some_and(|n| n >= 0) {
                    let reference = u.compound("ref", vec![*id]);
                    let keys = u.empty_list();
                    relations.push(u.compound("relation", vec![reference, arity, keys]));
                } else {
                    let reason = u.compound("invalid_generated_arity", vec![*id, arity]);
                    diagnostics.push(diagnostic(u, "assemble", reason));
                }
            }
            _ => {
                let list = u.list(&arities);
                let reason = u.compound("conflicting_generated_definitions", vec![*id, list]);
                diagnostics.push(diagnostic(u, "assemble", reason));
            }
        }
    }
    (relations, diagnostics)
}

/// `:77`.
fn collision_diagnostics(
    u: &mut Universe,
    relations: &[TermId],
    base_relations: &[TermId],
) -> Vec<TermId> {
    let mut declared = Vec::new();
    for row in base_relations {
        if let Some(("relation", args)) = u.functor(*row) {
            if args.len() == 3 {
                declared.push(args[0]);
            }
        }
    }
    let mut out = Vec::new();
    for row in relations {
        let Some(("relation", args)) = u.functor(*row) else {
            continue;
        };
        let reference = args[0];
        if declared.contains(&reference) {
            let reason = u.compound("generated_relation_already_declared", vec![reference]);
            out.push(diagnostic(u, "assemble", reason));
        }
    }
    out
}

/// `:90`.
fn assemble_rules(
    u: &mut Universe,
    cx: &Cx,
    arities: &HashMap<TermId, TermId>,
) -> (Vec<TermId>, Vec<TermId>) {
    let mut rules = Vec::new();
    let mut diagnostics = Vec::new();
    for rule_id in &cx.head_ids {
        let applications = sorted_unique(u, cx.head_apps.get(rule_id).cloned().unwrap_or_default());
        let [application] = applications.as_slice() else {
            let list = u.list(&applications);
            let reason = u.compound("conflicting_generated_heads", vec![*rule_id, list]);
            diagnostics.push(diagnostic(u, "assemble", reason));
            continue;
        };
        let none = u.atom("none");
        let (head, mut own) =
            assemble_application(u, cx, arities, "head", *rule_id, none, *application);
        let (body, body_diagnostics) = assemble_body(u, cx, arities, *rule_id);
        own.extend(body_diagnostics);
        if own.is_empty() {
            if let Some(head) = head {
                let goals = u.list(&body);
                rules.push(u.compound("rule", vec![head, goals]));
            }
        }
        diagnostics.extend(own);
    }
    (rules, diagnostics)
}

/// `:129`. Every `body/4` fragment whose rule has no `head/2` row.
fn orphan_diagnostics(u: &mut Universe, cx: &Cx) -> Vec<TermId> {
    let heads: HashSet<TermId> = cx.head_ids.iter().copied().collect();
    let mut out = Vec::new();
    for id in &cx.fragment_ids {
        if !heads.contains(id) {
            let reason = u.compound("orphan_generated_rule_fragment", vec![*id]);
            out.push(diagnostic(u, "assemble", reason));
        }
    }
    out
}

fn expected_positions(u: &mut Universe, count: usize) -> Vec<TermId> {
    (0..count as i64).map(|n| u.int(n)).collect()
}

/// `:159`.
fn assemble_body(
    u: &mut Universe,
    cx: &Cx,
    arities: &HashMap<TermId, TermId>,
    rule_id: TermId,
) -> (Vec<TermId>, Vec<TermId>) {
    let headers = cx.body_by_rule.get(&rule_id).cloned().unwrap_or_default();
    let indices = sorted_unique(u, headers.iter().map(|h| h.0).collect());
    let expected = expected_positions(u, indices.len());
    if indices != expected {
        let list = u.list(&indices);
        let reason = u.compound("non_dense_generated_goals", vec![rule_id, list]);
        return (vec![], vec![diagnostic(u, "assemble", reason)]);
    }
    let mut goals = Vec::new();
    let mut diagnostics = Vec::new();
    for index in &indices {
        let mut matched: Vec<TermId> = Vec::new();
        for (goal, header) in &headers {
            if goal == index {
                if let Some((polarity, application)) = *header {
                    matched.push(u.compound("goal", vec![polarity, application]));
                }
            }
        }
        let matched = sorted_unique(u, matched);
        let [only] = matched.as_slice() else {
            let list = u.list(&matched);
            let reason = u.compound(
                "conflicting_generated_body_goal",
                vec![rule_id, *index, list],
            );
            diagnostics.push(diagnostic(u, "assemble", reason));
            continue;
        };
        let Some(("goal", parts)) = u.functor(*only).map(|(n, a)| (n, a.to_vec())) else {
            continue;
        };
        let (polarity_term, application) = (parts[0], parts[1]);
        let Some(polarity) = generated_polarity(u, polarity_term) else {
            let reason = u.compound(
                "invalid_generated_polarity",
                vec![rule_id, *index, polarity_term],
            );
            diagnostics.push(diagnostic(u, "assemble", reason));
            continue;
        };
        let (call, own) =
            assemble_application(u, cx, arities, "body", rule_id, *index, application);
        if own.is_empty() {
            if let Some(call) = call {
                let polarity = u.atom(polarity);
                goals.push(u.compound("checked_goal", vec![polarity, call]));
            }
        }
        diagnostics.extend(own);
    }
    (goals, diagnostics)
}

/// `:392`. The atom and the string spell the same polarity.
fn generated_polarity(u: &Universe, term: TermId) -> Option<&'static str> {
    let text = match u.get(term) {
        Term::Atom(s) | Term::Str(s) => u.sym_str(*s),
        _ => return None,
    };
    match text {
        "positive" => Some("positive"),
        "negative" => Some("negative"),
        _ => None,
    }
}

/// `:232`.
#[allow(clippy::too_many_arguments)]
fn assemble_application(
    u: &mut Universe,
    cx: &Cx,
    arities: &HashMap<TermId, TermId>,
    part: &str,
    rule_id: TermId,
    goal_index: TermId,
    application: TermId,
) -> (Option<TermId>, Vec<TermId>) {
    let callables = sorted_unique(u, cx.apply.get(&application).cloned().unwrap_or_default());
    let [relation] = callables.as_slice() else {
        let list = u.list(&callables);
        let part_atom = u.atom(part);
        let reason = u.compound(
            "generated_application_callable",
            vec![part_atom, rule_id, goal_index, application, list],
        );
        return (None, vec![diagnostic(u, "assemble", reason)]);
    };
    let relation = *relation;
    let Some(arity) = arities.get(&relation).copied() else {
        let reason = if part == "head" {
            u.compound(
                "generated_head_undeclared_relation",
                vec![rule_id, relation],
            )
        } else {
            u.compound(
                "generated_body_undeclared_relation",
                vec![rule_id, goal_index, relation],
            )
        };
        return (None, vec![diagnostic(u, "assemble", reason)]);
    };
    let arity = u.as_int(arity).unwrap_or(-1).max(0) as usize;
    let (arguments, diagnostics) = assemble_arguments(u, cx, rule_id, arity, application);
    if !diagnostics.is_empty() {
        return (None, diagnostics);
    }
    let reference = u.compound("ref", vec![relation]);
    let list = u.list(&arguments);
    (Some(u.compound("call", vec![reference, list])), vec![])
}

/// `:264`.
fn assemble_arguments(
    u: &mut Universe,
    cx: &Cx,
    rule_id: TermId,
    arity: usize,
    application: TermId,
) -> (Vec<TermId>, Vec<TermId>) {
    let rows = cx
        .colon_by_owner
        .get(&application)
        .cloned()
        .unwrap_or_default();
    let mut argument_rows: Vec<(TermId, TermId)> = Vec::new();
    for row in &rows {
        if let Some(position) = const_of(u, row[2]) {
            argument_rows.push((position, row[1]));
        }
    }
    let positions = sorted_unique(u, argument_rows.iter().map(|(p, _)| *p).collect());
    let expected = expected_positions(u, arity);
    if positions != expected {
        let expected_list = u.list(&expected);
        let observed_list = u.list(&positions);
        let expected_term = u.compound("expected", vec![expected_list]);
        let observed_term = u.compound("observed", vec![observed_list]);
        let application_atom = u.atom("application");
        let reason = u.compound(
            "generated_argument_positions",
            vec![
                application_atom,
                rule_id,
                application,
                expected_term,
                observed_term,
            ],
        );
        return (vec![], vec![diagnostic(u, "assemble", reason)]);
    }
    let mut arguments = Vec::new();
    let mut diagnostics = Vec::new();
    for position in &expected {
        let values = sorted_unique(
            u,
            argument_rows
                .iter()
                .filter(|(p, _)| p == position)
                .map(|(_, v)| *v)
                .collect(),
        );
        let node = match values.as_slice() {
            [only] => ref_of(u, *only),
            _ => None,
        };
        let Some(node) = node else {
            let list = u.list(&values);
            let reason = u.compound("invalid_generated_argument", vec![rule_id, *position, list]);
            diagnostics.push(diagnostic(u, "assemble", reason));
            continue;
        };
        match node_result(u, cx, rule_id, *position, node) {
            Ok(argument) => arguments.push(argument),
            Err(d) => diagnostics.push(d),
        }
    }
    (arguments, diagnostics)
}

/// `:320`.
fn node_result(
    u: &mut Universe,
    cx: &Cx,
    rule_id: TermId,
    position: TermId,
    node: TermId,
) -> Result<TermId, TermId> {
    let variables = sorted_unique(u, cx.variables.get(&node).cloned().unwrap_or_default());
    let literals = sorted_unique(u, cx.literals.get(&node).cloned().unwrap_or_default());
    if literals.is_empty() {
        if let [only] = variables.as_slice() {
            if let Some(("variable", parts)) = u.functor(*only).map(|(n, a)| (n, a.to_vec())) {
                let named = matches!(u.get(parts[1]), Term::Atom(_) | Term::Str(_));
                if parts[0] == rule_id && named {
                    let generated = u.compound("generated", vec![rule_id, parts[1]]);
                    return Ok(u.compound("var", vec![generated]));
                }
            }
        }
    }
    if variables.is_empty() {
        if let [only] = literals.as_slice() {
            if let Some(("literal", parts)) = u.functor(*only).map(|(n, a)| (n, a.to_vec())) {
                if u.unary(parts[1], "const").is_some() {
                    return Ok(parts[1]);
                }
            }
        }
        if literals.is_empty() {
            return Ok(u.compound("ref", vec![node]));
        }
    }
    let variable_list = u.list(&variables);
    let literal_list = u.list(&literals);
    let reason = u.compound(
        "invalid_generated_value_node",
        vec![rule_id, position, node, variable_list, literal_list],
    );
    Err(diagnostic(u, "assemble", reason))
}
