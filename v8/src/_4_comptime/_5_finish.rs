//! The outer source-refreeze loop and the runtime program. Port of
//! `2_compiler.pl:757-919`, `:1062-1158`, `:1469-1502` and `:1572-1591`.

use super::api::{
    colon_call_parts, colon_rows, compiler_value_target, diagnostic, graph_seeds, intern_rows,
    kernel_call_args, source_application_edges, Compiled, Generated, Refreeze, Sources,
};
use super::host::{erase_host_planning_rows, validate_hosted_relations};
use super::rounds::{rounds, Round, RoundOutcome, COMPILER_ROUND_LIMIT};
use crate::_3_check::api::prolog_sort;
use crate::_3_check::{check_resolved_rules, Checked, Stop};
use crate::_6_eval::term::{Term, TermId, Universe};
use std::collections::HashMap;

type Outcome = Result<(Option<Compiled>, Vec<TermId>), Stop>;

/// `:757`.
pub fn finish_evaluation(
    u: &mut Universe,
    outcome: RoundOutcome,
    sources: &mut dyn Sources,
    fx: &mut dyn FnMut(Round),
) -> Outcome {
    if !outcome.diagnostics.is_empty() {
        return Ok((None, outcome.diagnostics));
    }
    let facts = outcome.closure;
    let generated = outcome.generated;
    fx(Round::Refreeze {
        outer: 1,
        deferred: false,
    });
    let (final_checked, diagnostics) =
        sources.refreeze(u, Refreeze::Final, &facts, &generated.relations)?;
    continue_source_refreeze(
        u,
        diagnostics,
        final_checked,
        facts,
        generated,
        1,
        sources,
        fx,
    )
}

/// `:769`.
#[allow(clippy::too_many_arguments)]
pub fn continue_source_refreeze(
    u: &mut Universe,
    diagnostics: Vec<TermId>,
    checked: Option<Checked>,
    facts: Vec<TermId>,
    generated: Generated,
    outer: i64,
    sources: &mut dyn Sources,
    fx: &mut dyn FnMut(Round),
) -> Outcome {
    if diagnostics.is_empty() {
        let Some(checked) = checked else {
            return Err(Stop::Fail("refreeze returned no checked program"));
        };
        let bind = derived_bind_diagnostics(u, &checked.rules, &facts);
        if bind.is_empty() {
            return finish_final_check(u, &checked, &facts, &generated);
        }
        return expand_source_compiler(u, &checked, facts, generated, outer, sources, fx);
    }
    if !diagnostics.iter().all(|d| deferrable(u, *d)) {
        return Ok((None, diagnostics));
    }
    fx(Round::Refreeze {
        outer,
        deferred: true,
    });
    let (deferred, deferred_diagnostics) =
        sources.refreeze(u, Refreeze::Deferred, &facts, &generated.relations)?;
    if !deferred_diagnostics.is_empty() {
        return Ok((None, deferred_diagnostics));
    }
    let Some(deferred) = deferred else {
        return Err(Stop::Fail("deferred refreeze returned no checked program"));
    };
    expand_source_compiler(u, &deferred, facts, generated, outer, sources, fx)
}

/// `:807`. One reason only, in phase `lower`; fork F2.
pub fn deferrable(u: &Universe, d: TermId) -> bool {
    let Some(("diagnostic", args)) = u.functor(d) else {
        return false;
    };
    if args.len() != 3 || u.functor_or_atom(args[0]).map(|(n, _)| n) != Some("lower") {
        return false;
    }
    matches!(u.functor(args[2]), Some(("not_relation", a)) if a.len() == 1)
}

/// `:820`. Another whole inner fixpoint, seeded from the closed compiler facts.
#[allow(clippy::too_many_arguments)]
pub fn expand_source_compiler(
    u: &mut Universe,
    checked: &Checked,
    facts: Vec<TermId>,
    generated: Generated,
    outer: i64,
    sources: &mut dyn Sources,
    fx: &mut dyn FnMut(Round),
) -> Outcome {
    if outer >= COMPILER_ROUND_LIMIT {
        let limit = u.int(COMPILER_ROUND_LIMIT);
        let reason = u.compound("source_refreeze_limit_exhausted", vec![limit]);
        let d = diagnostic(u, "compile", reason);
        return Ok((None, vec![d]));
    }
    let base_relations: Vec<TermId> = checked
        .relations
        .iter()
        .copied()
        .filter(|r| !generated.relations.contains(r))
        .collect();
    let mut base = graph_seeds(u, &checked.nodes, &checked.edges);
    base.extend_from_slice(&checked.seeds);
    let base_seeds = prolog_sort(u, base);
    let mut frozen = colon_rows(u, &facts);
    frozen.extend(source_application_edges(u, &checked.rules));
    let frozen_edges = prolog_sort(u, frozen);
    let frozen_requests = intern_rows(u, &facts);

    let next = rounds(
        u,
        &checked.rules,
        &base_relations,
        &base_seeds,
        frozen_edges,
        frozen_requests,
        generated.relations.clone(),
        generated.rules.clone(),
        outer,
        fx,
    )?;
    if !next.diagnostics.is_empty() {
        return Ok((None, next.diagnostics));
    }
    fx(Round::Refreeze {
        outer: outer + 1,
        deferred: false,
    });
    let (final_checked, diagnostics) =
        sources.refreeze(u, Refreeze::Final, &next.closure, &next.generated.relations)?;
    continue_source_refreeze(
        u,
        diagnostics,
        final_checked,
        next.closure,
        next.generated,
        outer + 1,
        sources,
        fx,
    )
}

/// `:869` through `:919`, the three terminal steps.
pub fn finish_final_check(
    u: &mut Universe,
    checked: &Checked,
    facts: &[TermId],
    generated: &Generated,
) -> Outcome {
    let mut validation = validate_functional_rows(u, &checked.relations, facts);
    validation.extend(validate_hosted_relations(
        u,
        &checked.nodes,
        &checked.edges,
        &checked.relations,
        facts,
    ));
    let validation = prolog_sort(u, validation);
    if !validation.is_empty() {
        return Ok((None, validation));
    }

    let type_graph_facts = type_graph_facts(u, facts);
    let mut relations = checked.relations.clone();
    relations.extend_from_slice(&generated.relations);
    let relations = prolog_sort(u, relations);
    let mut rules = checked.rules.clone();
    rules.extend_from_slice(&generated.rules);
    let rules = prolog_sort(u, rules);

    let erased = erase_host_planning_rows(
        u,
        &checked.nodes,
        &checked.edges,
        &relations,
        &checked.seeds,
        &rules,
    );
    let resolved = check_resolved_rules(u, &erased.relations, &erased.rules)?;
    if !resolved.diagnostics.is_empty() {
        return Ok((None, resolved.diagnostics));
    }
    Ok((
        Some(Compiled {
            type_graph_facts,
            runtime: Checked {
                nodes: checked.nodes.clone(),
                edges: checked.edges.clone(),
                relations: erased.relations,
                seeds: erased.seeds,
                rules: erased.rules,
                depends: resolved.depends,
                strata: resolved.strata,
            },
            compiler_facts: facts.to_vec(),
        }),
        vec![],
    ))
}

/// `:1469`. The located diagnostic carries the rule's node id, not `none`.
pub fn derived_bind_diagnostics(
    u: &mut Universe,
    rules: &[TermId],
    rows: &[TermId],
) -> Vec<TermId> {
    let mut by_owner_name_index: HashMap<(TermId, TermId, TermId), ()> = HashMap::new();
    let mut by_owner_index: HashMap<(TermId, TermId), ()> = HashMap::new();
    for row in rows {
        let Some([owner, name, _, index]) = colon_call_parts(u, *row) else {
            continue;
        };
        let (Some(owner), Some(index)) = (u.unary(owner, "ref"), u.unary(index, "const")) else {
            continue;
        };
        by_owner_index.insert((owner, index), ());
        if let Some(name) = u.unary(name, "const") {
            by_owner_name_index.insert((owner, name, index), ());
        }
    }
    let mut bind = Vec::new();
    let mut label = Vec::new();
    for rule in rules {
        let Some(("rule", args)) = u.functor(*rule).map(|(n, a)| (n, a.to_vec())) else {
            continue;
        };
        let Some([owner, name, value, index]) = colon_call_parts(u, args[0]) else {
            continue;
        };
        let (Some(owner), Some(index)) = (u.unary(owner, "ref"), u.unary(index, "const")) else {
            continue;
        };
        if let (Some(name), Some(node)) = (
            u.unary(name, "const"),
            u.unary(value, "var")
                .and_then(|v| u.unary(v, "derived_bind")),
        ) {
            if !by_owner_name_index.contains_key(&(owner, name, index)) {
                let reason = u.compound("missing_derived_bind", vec![owner, name, index]);
                bind.push(located(u, "compile", node, reason));
            }
        }
        if let Some(node) = u
            .unary(name, "var")
            .and_then(|v| u.unary(v, "derived_label"))
        {
            if !by_owner_index.contains_key(&(owner, index)) {
                let reason = u.compound("missing_derived_edge_label", vec![owner, index]);
                label.push(located(u, "compile", node, reason));
            }
        }
    }
    bind.extend(label);
    prolog_sort(u, bind)
}

fn located(u: &mut Universe, phase: &str, node: TermId, reason: TermId) -> TermId {
    let phase = u.atom(phase);
    u.compound("diagnostic", vec![phase, node, reason])
}

/// `0_evaluator.pl:557`. Declared zero-based functional keys over a closure.
pub fn validate_functional_rows(
    u: &mut Universe,
    relations: &[TermId],
    rows: &[TermId],
) -> Vec<TermId> {
    let sorted = prolog_sort(u, rows.to_vec());
    let mut by_relation: HashMap<TermId, Vec<(TermId, Vec<TermId>)>> = HashMap::new();
    for row in &sorted {
        let Some(("call", args)) = u.functor(*row) else {
            continue;
        };
        let Some(items) = u.as_list(args[1]) else {
            continue;
        };
        by_relation.entry(args[0]).or_default().push((*row, items));
    }
    let mut out = Vec::new();
    for declaration in relations {
        let Some(("relation", args)) = u.functor(*declaration).map(|(n, a)| (n, a.to_vec())) else {
            continue;
        };
        let (relation, key_sets) = (args[0], args[2]);
        let Some(key_sets) = u.as_list(key_sets) else {
            continue;
        };
        let Some(relation_rows) = by_relation.get(&relation).cloned() else {
            continue;
        };
        for positions_term in key_sets {
            let Some(positions) = u.as_list(positions_term) else {
                continue;
            };
            let mut keyed: Vec<(Vec<TermId>, TermId)> = Vec::new();
            for (row, items) in &relation_rows {
                let mut key = Vec::with_capacity(positions.len());
                for position in &positions {
                    let Some(at) = u.as_int(*position).and_then(|n| items.get(n as usize)) else {
                        key.clear();
                        break;
                    };
                    key.push(*at);
                }
                if key.len() == positions.len() {
                    keyed.push((key, *row));
                }
            }
            keyed.sort_by(|a, b| u.cmp_rows(&a.0, &b.0));
            let mut start = 0;
            while start < keyed.len() {
                let mut end = start + 1;
                while end < keyed.len() && keyed[end].0 == keyed[start].0 {
                    end += 1;
                }
                for left in start..end {
                    for right in (left + 1)..end {
                        let values = u.list(&keyed[start].0);
                        let reason = u.compound(
                            "functional_key_conflict",
                            vec![
                                relation,
                                positions_term,
                                values,
                                keyed[left].1,
                                keyed[right].1,
                            ],
                        );
                        out.push(diagnostic(u, "evaluate", reason));
                    }
                }
                start = end;
            }
        }
    }
    prolog_sort(u, out)
}

/// `:1572`. The graph subset of the closure, relabelled by `semantic_label/2`.
pub fn type_graph_facts(u: &mut Universe, facts: &[TermId]) -> Vec<TermId> {
    let mut out = Vec::new();
    for row in facts {
        for name in ["node", "module", "product", "sum"] {
            if let Some(args) = kernel_call_args(u, *row, name) {
                if args.len() == 1 {
                    if let Some(identity) = u.unary(args[0], "ref") {
                        out.push(u.compound(name, vec![identity]));
                    }
                }
            }
        }
        let Some([owner, label, target, index]) = colon_call_parts(u, *row) else {
            continue;
        };
        let (Some(owner), Some(index)) = (u.unary(owner, "ref"), u.unary(index, "const")) else {
            continue;
        };
        let Some(label) = semantic_label(u, label) else {
            continue;
        };
        out.push(u.compound(":", vec![owner, label, target, index]));
    }
    prolog_sort(u, out)
}

/// `:1590`.
pub fn semantic_label(u: &Universe, label: TermId) -> Option<TermId> {
    u.unary(label, "const").or_else(|| u.unary(label, "ref"))
}

/// `:1062`. Generated declarations as an expression environment for the next
/// source lowering. The driver lane calls this; `Replay` does not.
pub fn generated_expression_environment(
    u: &mut Universe,
    facts: &[TermId],
    generated_relations: &[TermId],
    derived_bind_slots: &[TermId],
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let mut relations = Vec::with_capacity(generated_relations.len());
    for row in generated_relations {
        let Some(("relation", args)) = u.functor(*row).map(|(n, a)| (n, a.to_vec())) else {
            continue;
        };
        let Some(id) = u.unary(args[0], "ref") else {
            continue;
        };
        let key_sets = generated_return_key_sets(u, facts, id, args[1], args[2]);
        relations.push(u.compound("relation", vec![id, args[1], key_sets]));
    }
    let relations = prolog_sort(u, relations);

    let product = u.atom("product");
    let derived_callable = u.atom("derived_callable");
    let generated_ids: Vec<TermId> = generated_relations
        .iter()
        .filter_map(|r| match u.functor(*r) {
            Some(("relation", args)) if args.len() == 3 => u.unary(args[0], "ref"),
            _ => None,
        })
        .collect();

    let mut reservations = Vec::new();
    let mut edges = Vec::new();
    for row in facts {
        let Some([owner, name, value, index]) = colon_call_parts(u, *row) else {
            continue;
        };
        let (Some(owner), Some(index)) = (u.unary(owner, "ref"), u.unary(index, "const")) else {
            continue;
        };
        if let Some(name) = u.unary(name, "const") {
            if let Some(relation) = u.unary(value, "ref") {
                let kind = if generated_ids.contains(&relation) {
                    Some(product)
                } else {
                    let slot = u.compound("derived_bind_slot", vec![owner, name, index]);
                    derived_bind_slots
                        .contains(&slot)
                        .then_some(derived_callable)
                };
                if let Some(kind) = kind {
                    if matches!(u.get(name), Term::Atom(_)) {
                        let target = u.compound("target", vec![relation]);
                        reservations
                            .push(u.compound("reservation", vec![owner, name, target, kind]));
                    }
                }
            }
        }
        if let (Some(name), Some(target)) =
            (u.unary(name, "const"), compiler_value_target(u, value))
        {
            edges.push(u.compound("pending_edge", vec![owner, name, target, index]));
        }
    }
    (
        prolog_sort(u, reservations),
        relations,
        prolog_sort(u, edges),
    )
}

/// `:1132`. A declared key set wins; otherwise one `return` edge names the
/// inputs and every other position becomes the key.
pub fn generated_return_key_sets(
    u: &mut Universe,
    facts: &[TermId],
    relation: TermId,
    arity: TermId,
    key_sets: TermId,
) -> TermId {
    if !u.as_list(key_sets).is_some_and(|k| k.is_empty()) {
        return key_sets;
    }
    let return_atom = u.atom("return");
    let mut indices = Vec::new();
    for row in facts {
        let Some([owner, name, _, index]) = colon_call_parts(u, *row) else {
            continue;
        };
        if u.unary(owner, "ref") != Some(relation) || u.unary(name, "const") != Some(return_atom) {
            continue;
        }
        if let Some(index) = u.unary(index, "const") {
            indices.push(index);
        }
    }
    let indices = prolog_sort(u, indices);
    let [only] = indices.as_slice() else {
        return u.empty_list();
    };
    let except = u.as_int(*only);
    let arity = u.as_int(arity).unwrap_or(0);
    let inputs: Vec<TermId> = (0..arity)
        .filter(|n| Some(*n) != except)
        .map(|n| u.int(n))
        .collect();
    let inputs = u.list(&inputs);
    u.list(&[inputs])
}

/// `:1106`.
pub fn merge_expression_environments(
    u: &mut Universe,
    left: (Vec<TermId>, Vec<TermId>, Vec<TermId>),
    right: (Vec<TermId>, Vec<TermId>, Vec<TermId>),
) -> (Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let mut reservations = left.0;
    reservations.extend(right.0);
    let mut relations = left.1;
    relations.extend(right.1);
    let mut edges = left.2;
    edges.extend(right.2);
    (
        prolog_sort(u, reservations),
        prolog_sort(u, relations),
        prolog_sort(u, edges),
    )
}
