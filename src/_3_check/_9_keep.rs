//! The one-engine keep of checked programs (A1). One process lower+checks a
//! prelude more than once (`checked_alone`, the comptime units, the runtime
//! finish). A program is keyed by its raw basement five-lists plus origin
//! rows; when the kept program is the exporter prefix of the request, only
//! the delta resolves and re-emits over the union. Rules whose consult notes
//! still hold are reused; a changed rule resolves alone at its own index.

use super::api::{
    basement_parts, check_datalog, emit_checked, product_relation_rows, prolog_sort,
    relation_arities, relations_refs, resolve_fresh, Basement, Checked, Resolved,
};
use super::bind::bind_diagnostics;
use super::graph::{CheckerGraph, OriginArena};
use super::resolve::{resolve_edges, resolve_rules, resolve_seeds, Cx, Note};
use super::Stop;
use crate::_2_lower::index::{edge_parts, Edge};
use crate::_2_lower::Lowered;
use crate::_6_eval::term::{TermId, Universe};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The checked program of one input key: the wholesale result and the
/// resolution middle a later delta resolves against.
pub struct UnitChecked {
    checked: Checked,
    resolved: Resolved,
    origin_rows: Vec<TermId>,
}

/// Raw-list equality. `TermId`s are interned, so list equality is structural.
#[derive(PartialEq)]
struct Key {
    nodes: Vec<TermId>,
    edges: Vec<TermId>,
    relations: Vec<TermId>,
    seeds: Vec<TermId>,
    rules: Vec<TermId>,
    origins: Vec<TermId>,
}

impl Key {
    fn of(basement: &Basement, origins: &[TermId]) -> Key {
        Key {
            nodes: basement.nodes.clone(),
            edges: basement.edges.clone(),
            relations: basement.relations.clone(),
            seeds: basement.seeds.clone(),
            rules: basement.rules.clone(),
            origins: origins.to_vec(),
        }
    }
}

#[derive(Default)]
pub struct CheckKeep {
    entries: RefCell<Vec<(Key, Arc<UnitChecked>)>>,
    /// Resolved rules by raw rule row: every entry holding the row, with its
    /// resolution slot. The first candidate whose consult notes still hold wins.
    by_rule: RefCell<HashMap<TermId, Vec<(Arc<UnitChecked>, usize)>>>,
}

impl CheckKeep {
    fn hit(&self, key: &Key) -> Option<Arc<UnitChecked>> {
        self.entries
            .borrow()
            .iter()
            .find(|(k, _)| *k == *key)
            .map(|(_, unit)| unit.clone())
    }

    fn insert(&self, key: Key, unit: UnitChecked) {
        let mut entries = self.entries.borrow_mut();
        if entries.iter().any(|(k, _)| *k == key) {
            return;
        }
        let unit = Arc::new(unit);
        let mut by_rule = self.by_rule.borrow_mut();
        for (slot, row) in unit.resolved.rule_rows.iter().enumerate() {
            by_rule.entry(*row).or_default().push((unit.clone(), slot));
        }
        entries.push((key, unit));
    }

    /// A kept resolution of `row` whose consult notes hold against `graph`
    /// and `arities`: every owner it read is unchanged between its source
    /// graph and `graph`, and every arity it matched still first-wins.
    fn rule_hit(
        &self,
        row: TermId,
        graph: &CheckerGraph,
        arities: &HashMap<TermId, i64>,
    ) -> Option<(Arc<UnitChecked>, usize)> {
        let candidates = self.by_rule.borrow().get(&row).cloned().unwrap_or_default();
        candidates.into_iter().find(|(source, slot)| {
            source.resolved.rule_reads[*slot].iter().all(|note| match note {
                Note::Owner(owner) => owner_unchanged(&source.resolved.graph, graph, *owner),
                Note::Arity(target, arity) => arities.get(target) == Some(arity),
            })
        })
    }
}

/// One owner's graph rows are the same in both graphs: its edge rows, parent,
/// module mark and the counts derived from its edges.
fn owner_unchanged(old: &CheckerGraph, new: &CheckerGraph, owner: TermId) -> bool {
    let rows = |graph: &CheckerGraph| {
        graph
            .by_owner
            .get(&owner)
            .map(|slots| {
                slots
                    .iter()
                    .map(|&slot| graph.edges[slot].row)
                    .collect::<Vec<TermId>>()
            })
            .unwrap_or_default()
    };
    rows(old) == rows(new)
        && old.parent.get(&owner) == new.parent.get(&owner)
        && old.modules.contains(&owner) == new.modules.contains(&owner)
        && old.owner_edge_count.get(&owner) == new.owner_edge_count.get(&owner)
        && old.owner_return_indices.get(&owner) == new.owner_return_indices.get(&owner)
        && old.owner_slots.get(&owner) == new.owner_slots.get(&owner)
}

/// Check `lowered` once per input key per process. `exporter` is the
/// `module_basement`/`module_origins` slot the run merged ahead of `lowered`;
/// when a kept unit carries exactly that content its union graph is the
/// prefix and only the delta resolves.
pub fn check_units(
    u: &mut Universe,
    keep: Option<&CheckKeep>,
    lowered: &Lowered,
    exporter: Option<(TermId, TermId)>,
) -> Result<(Option<Checked>, Vec<TermId>), Stop> {
    let Some(keep) = keep else {
        return check_datalog(u, lowered);
    };
    let Some(program) = lowered.program else {
        return check_datalog(u, lowered);
    };
    let Some(basement) = basement_parts(u, program) else {
        return check_datalog(u, lowered);
    };
    let key = Key::of(&basement, &lowered.origins);
    if let Some(unit) = keep.hit(&key) {
        tracing::info!(target: "dl8::a1", "check_hit");
        return Ok((Some(unit.checked.clone()), Vec::new()));
    }

    let attempt = exporter.and_then(|(module_basement, module_origins)| {
        let base_program = u.args::<2>(module_basement, "module_basement")?[1];
        let base_basement = basement_parts(u, base_program)?;
        let base_origins = u.as_list(u.args::<2>(module_origins, "module_origins")?[1])?;
        let base = keep.hit(&Key::of(&base_basement, &base_origins))?;
        Some(base)
    });
    if let Some(base) = attempt {
        match delta(u, keep, &base, &basement, &lowered.origins) {
            Ok((checked, diagnostics, resolved)) => {
                if let (Some(checked), Some(resolved)) = (&checked, resolved) {
                    keep.insert(
                        key,
                        UnitChecked {
                            checked: checked.clone(),
                            resolved,
                            origin_rows: lowered.origins.clone(),
                        },
                    );
                }
                return Ok((checked, diagnostics));
            }
            Err(Fall::Stop(stop)) => return Err(stop),
            Err(Fall::Back(reason)) => {
                tracing::info!(target: "dl8::a1", reason, "check_fallback");
            }
        }
    }

    let Some(graph) = CheckerGraph::build(u, &basement.edges, &basement.nodes) else {
        return check_datalog(u, lowered);
    };
    let resolved = resolve_fresh(u, graph, basement, &lowered.origins)?;
    tracing::info!(target: "dl8::a1", "check_full");
    let outcome = emit_checked(u, &resolved)?;
    if outcome.1.is_empty() {
        if let Some(checked) = &outcome.0 {
            keep.insert(
                key,
                UnitChecked {
                    checked: checked.clone(),
                    resolved,
                    origin_rows: lowered.origins.clone(),
                },
            );
        }
    }
    Ok(outcome)
}

/// Why a delta fell back to a full check.
enum Fall {
    Stop(Stop),
    Back(&'static str),
}

impl From<Stop> for Fall {
    fn from(stop: Stop) -> Fall {
        Fall::Stop(stop)
    }
}

/// Resolve and emit only the delta against the kept unit, replaying its
/// output. Rules reuse a kept resolution keyed on their consult notes;
/// everything else resolves alone. `Result::Err` is a fallback reason or a
/// structural stop.
fn delta(
    u: &mut Universe,
    keep: &CheckKeep,
    base: &UnitChecked,
    basement: &Basement,
    origin_rows: &[TermId],
) -> Result<(Option<Checked>, Vec<TermId>, Option<Resolved>), Fall> {
    let kept = &base.resolved;
    for (name, merged, kept_rows) in [
        ("nodes", &basement.nodes, &kept.nodes),
        ("edges", &basement.edges, &kept.edge_rows),
        ("relations", &basement.relations, &kept.relation_rows),
        ("seeds", &basement.seeds, &kept.seed_rows),
        ("origins", &origin_rows.to_vec(), &base.origin_rows),
    ] {
        if merged.len() < kept_rows.len() || merged[..kept_rows.len()] != kept_rows[..] {
            return Err(Fall::Back(name));
        }
    }

    let delta_edge_rows = &basement.edges[kept.edge_rows.len()..];
    let mut delta_edges: Vec<Edge> = Vec::with_capacity(delta_edge_rows.len());
    for row in delta_edge_rows {
        delta_edges
            .push(edge_parts(u, *row).ok_or(Fall::Stop(Stop::Fail("pending_edge/4 expected")))?);
    }
    // A delta edge under a kept owner changed the owner's arity in the union
    // graph; its kept relation row is stale and every rule that read it
    // re-resolves against the re-derived row.
    let touches_kept_owner = delta_edges
        .iter()
        .any(|part| kept.graph.by_owner.contains_key(&part.owner));

    let mut graph = kept.graph.clone();
    graph.ingest(u, delta_edges.iter().cloned());
    graph.add_modules(u, &basement.nodes[kept.nodes.len()..]);
    let origins = OriginArena::build(u, origin_rows);

    let mut declared = product_relation_rows(u, &graph, &origin_rows[base.origin_rows.len()..]);
    let mut kept_relations = kept.relations.clone();
    if touches_kept_owner {
        declared = product_relation_rows(u, &graph, origin_rows);
        let derived: HashSet<TermId> = declared
            .iter()
            .filter_map(|row| match u.functor(*row) {
                Some(("relation", args)) if args.len() == 3 => Some(args[0]),
                _ => None,
            })
            .collect();
        kept_relations.retain(|row| match u.functor(*row) {
            Some(("relation", args)) if args.len() == 3 => !derived.contains(&args[0]),
            _ => true,
        });
    }
    let Some(supplied) = relations_refs(u, &basement.relations[kept.relation_rows.len()..], &[])
    else {
        return Err(Fall::Stop(Stop::Fail("relation/3 expected")));
    };
    let mut all = kept_relations;
    all.extend(declared);
    all.extend(supplied);
    let relations = prolog_sort(u, all);
    let arities = relation_arities(u, &relations);
    let notes = RefCell::new(Vec::new());
    let cx = Cx {
        graph: &graph,
        relations: &arities,
        reads: Some(&notes),
    };

    let mut diagnostics = bind_diagnostics(u, &graph, &origins, &delta_edges);
    let mut built = kept.built.clone();
    let (colon_delta, edge_mints_delta, edge_diagnostics) =
        resolve_edges(u, &cx, &origins, &delta_edges, &mut built);
    let (seeds_delta, seed_mints_delta, seed_diagnostics) = resolve_seeds(
        u,
        &cx,
        &basement.seeds[kept.seed_rows.len()..],
        &origins,
        kept.seed_rows.len() as i64,
    )?;
    // Rules position by position: a kept or cached resolution whose consult
    // notes still hold is reused, everything else resolves alone at its own
    // index. Stratification reads the union in emit_checked.
    let mut resolved_rules = Vec::with_capacity(basement.rules.len());
    let mut rule_reads = Vec::with_capacity(basement.rules.len());
    for (index, row) in basement.rules.iter().enumerate() {
        if let Some((source, slot)) = keep.rule_hit(*row, &graph, &arities) {
            resolved_rules.push(source.resolved.resolved_rules[slot]);
            rule_reads.push(source.resolved.rule_reads[slot].clone());
            continue;
        }
        let one =
            resolve_rules(u, &cx, &basement.rules[index..index + 1], &origins, index as i64)?;
        resolved_rules.extend(one.rules);
        rule_reads.extend(one.reads);
        diagnostics.extend(one.diagnostics);
    }
    let mut resolved_seeds = kept.resolved_seeds.clone();
    resolved_seeds.extend(seeds_delta);
    let mut seed_mints = kept.seed_mints.clone();
    seed_mints.extend(seed_mints_delta);
    let mut colon_edges = kept.colon_edges.clone();
    colon_edges.extend(colon_delta);
    let mut edge_mints = kept.edge_mints.clone();
    edge_mints.extend(edge_mints_delta);
    let resolved = Resolved {
        graph,
        origins,
        relations,
        resolved_rules,
        resolved_seeds,
        seed_mints,
        colon_edges,
        edge_mints,
        built,
        diagnostics,
        nodes: basement.nodes.clone(),
        edge_rows: basement.edges.clone(),
        relation_rows: basement.relations.clone(),
        seed_rows: basement.seeds.clone(),
        rule_rows: basement.rules.clone(),
        rule_reads,
    };
    tracing::info!(target: "dl8::a1", "check_delta");
    let outcome = emit_checked(u, &resolved)?;
    let resolved = if outcome.1.is_empty() && outcome.0.is_some() {
        Some(resolved)
    } else {
        None
    };
    Ok((outcome.0, outcome.1, resolved))
}
