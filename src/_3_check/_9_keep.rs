//! The one-engine keep of checked programs (A1). One process lower+checks a
//! prelude more than once (`checked_alone`, the comptime units, the runtime
//! finish). A program is keyed by its raw basement five-lists plus origin
//! rows; when the kept program is the exporter prefix of the request, only
//! the delta resolves and re-emits over the union.

use super::api::{
    basement_parts, check_datalog, emit_checked, product_relation_rows, prolog_sort,
    relation_arities, relations_refs, resolve_fresh, Basement, Checked, Resolved,
};
use super::bind::bind_diagnostics;
use super::graph::{CheckerGraph, OriginArena};
use super::resolve::{resolve_edges, resolve_rules, resolve_seeds, Cx};
use super::Stop;
use crate::_2_lower::index::{edge_parts, Edge};
use crate::_2_lower::Lowered;
use crate::_6_eval::term::{TermId, Universe};
use std::cell::RefCell;
use std::rc::Rc;

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
    entries: RefCell<Vec<(Key, Rc<UnitChecked>)>>,
}

impl CheckKeep {
    fn hit(&self, key: &Key) -> Option<Rc<UnitChecked>> {
        self.entries
            .borrow()
            .iter()
            .find(|(k, _)| *k == *key)
            .map(|(_, unit)| unit.clone())
    }

    fn insert(&self, key: Key, unit: UnitChecked) {
        let mut entries = self.entries.borrow_mut();
        if !entries.iter().any(|(k, _)| *k == key) {
            entries.push((key, Rc::new(unit)));
        }
    }
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
        match delta(u, &base, &basement, &lowered.origins) {
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
/// output. `Result::Err` is a fallback reason or a structural stop.
fn delta(
    u: &mut Universe,
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
        ("rules", &basement.rules, &kept.rule_rows),
        ("origins", &origin_rows.to_vec(), &base.origin_rows),
    ] {
        if merged.len() < kept_rows.len() || merged[..kept_rows.len()] != kept_rows[..] {
            return Err(Fall::Back(name));
        }
    }

    // The guard the design pins: an edge re-checked into a kept owner's
    // arities, keys or strata re-derives everything downstream of it.
    let delta_edge_rows = &basement.edges[kept.edge_rows.len()..];
    let mut delta_edges: Vec<Edge> = Vec::with_capacity(delta_edge_rows.len());
    for row in delta_edge_rows {
        delta_edges
            .push(edge_parts(u, *row).ok_or(Fall::Stop(Stop::Fail("pending_edge/4 expected")))?);
    }
    if delta_edges
        .iter()
        .any(|part| kept.graph.by_owner.contains_key(&part.owner))
    {
        return Err(Fall::Back("prelude_edge"));
    }

    let mut graph = kept.graph.clone();
    graph.ingest(u, delta_edges.iter().cloned());
    graph.add_modules(u, &basement.nodes[kept.nodes.len()..]);
    let origins = OriginArena::build(u, origin_rows);

    let declared = product_relation_rows(u, &graph, &origin_rows[base.origin_rows.len()..]);
    let Some(supplied) = relations_refs(u, &basement.relations[kept.relation_rows.len()..], &[])
    else {
        return Err(Fall::Stop(Stop::Fail("relation/3 expected")));
    };
    let mut all = kept.relations.clone();
    all.extend(declared);
    all.extend(supplied);
    let relations = prolog_sort(u, all);
    let arities = relation_arities(u, &relations);
    let cx = Cx {
        graph: &graph,
        relations: &arities,
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
    let (rules_delta, rule_diagnostics) = resolve_rules(
        u,
        &cx,
        &basement.rules[kept.rule_rows.len()..],
        &origins,
        kept.rule_rows.len() as i64,
    )?;
    diagnostics.extend(edge_diagnostics);
    diagnostics.extend(seed_diagnostics);
    diagnostics.extend(rule_diagnostics);

    let mut resolved_rules = kept.resolved_rules.clone();
    resolved_rules.extend(rules_delta);
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
