//! `lower_datalog/4`, `/5` and `lower_datalog_deferred/5`.
//! Port of `0_lowerer.pl:21-147` and `:281-292`.

use super::cx::{CallPolicy, Cx};
use super::declare::{lower_declarations, Declared};
use super::derived::{lower_derived_bind_rules, Stop};
use super::execute::lower_executables;
use super::index::{EdgeIndex, ReservationIndex};
use super::promote::{promote_deferred_aliases, promoted_alias_rules};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::HashMap;

pub struct Unit {
    pub origin: TermId,
    pub forms: Vec<TermId>,
}

/// `dl7_unit(Origin, Digest, Forms, SourceRows, ExpansionRows)` at `:56`.
pub fn unit_parts(u: &Universe, term: TermId) -> Option<Unit> {
    let (name, args) = u.functor(term)?;
    if name != "dl7_unit" || args.len() != 5 {
        return None;
    }
    Some(Unit {
        origin: args[0],
        forms: u.as_list(args[2])?,
    })
}

#[derive(Default)]
pub struct Environment {
    pub reservations: Vec<TermId>,
    pub relations: Vec<TermId>,
    pub edges: Vec<TermId>,
}

/// `expression_environment(Reservations, Relations, Edges)` at `:77`.
pub fn environment_parts(u: &Universe, term: TermId) -> Option<Environment> {
    let (name, args) = u.functor(term)?;
    if name != "expression_environment" || args.len() != 3 {
        return None;
    }
    Some(Environment {
        reservations: u.as_list(args[0])?,
        relations: u.as_list(args[1])?,
        edges: u.as_list(args[2])?,
    })
}

pub struct Lowered {
    /// `None` reproduces the unbound `Program` of `0_lowerer.pl:292`.
    pub program: Option<TermId>,
    pub origins: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// `memberchk(relation(Callable, Arity, KeySets), Relations)`: the first row
/// for a callable wins, so later duplicates never overwrite.
fn relation_dictionary(u: &Universe, rows: &[TermId]) -> HashMap<TermId, (i64, TermId)> {
    let mut out = HashMap::new();
    for row in rows {
        let Some((name, args)) = u.functor(*row) else {
            continue;
        };
        if name != "relation" || args.len() != 3 {
            continue;
        }
        let Some(arity) = u.as_int(args[1]) else {
            continue;
        };
        out.entry(args[0]).or_insert((arity, args[2]));
    }
    out
}

/// `:52`.
#[tracing::instrument(skip_all)]
pub fn lower_datalog(
    u: &mut Universe,
    policy: CallPolicy,
    unit_term: TermId,
    environment_term: TermId,
) -> Result<Lowered, Stop> {
    let Some(unit) = unit_parts(u, unit_term) else {
        let reason = u.compound("invalid_dl7_unit", vec![unit_term]);
        let lower = u.atom("lower");
        let unit_atom = u.atom("unit");
        let diagnostic = u.compound("diagnostic", vec![lower, unit_atom, reason]);
        return Ok(Lowered {
            program: Some(empty_program(u)),
            origins: vec![],
            diagnostics: vec![diagnostic],
        });
    };
    let imported = environment_parts(u, environment_term).unwrap_or_default();
    let module_identity = unit.origin;
    let module_owner = u.compound("module", vec![module_identity]);

    let declared = match lower_declarations(u, &unit.forms, module_owner, module_identity) {
        Ok(declared) => declared,
        Err(diagnostic) => return Ok(stopped(u, diagnostic)),
    };

    // :78. Local rows come first in every visible list.
    let visible_reservations = concat(&declared.reservations, &imported.reservations);
    let visible_relations = concat(&declared.relations, &imported.relations);
    let visible_edges = concat(&declared.edges, &imported.edges);

    let mut reservation_index = ReservationIndex::build(u, &visible_reservations);
    let promoted = promote_deferred_aliases(
        u,
        &declared.reservations,
        &reservation_index,
        &declared.edges,
        &visible_edges,
        &declared.origins,
    );
    reservation_index.install_promoted(u, &promoted.reservations);
    let promoted_edges = concat(&promoted.edges, &imported.edges);
    let edge_index = EdgeIndex::build(u, &promoted_edges);
    let relations = relation_dictionary(u, &visible_relations);

    let mut cx = Cx {
        u,
        reservations: &reservation_index,
        edges: &edge_index,
        relations: &relations,
        policy,
    };

    let derived = lower_derived_bind_rules(&mut cx, &promoted.derived_reservations, 0)?;
    let (alias_rules, alias_origins) =
        promoted_alias_rules(&mut cx, &promoted.promotions, derived.rules.len() as i64);
    let rule_index = (derived.rules.len() + alias_rules.len()) as i64;
    let executables = match lower_executables(&mut cx, &unit.forms, module_owner, rule_index) {
        Ok(executables) => executables,
        Err(diagnostic) => {
            // :292. v7 leaves Program unbound here; every sibling binds [].
            return Ok(Lowered {
                program: None,
                origins: vec![],
                diagnostics: vec![diagnostic],
            });
        }
    };

    let mut rules = derived.rules;
    rules.extend(alias_rules);
    rules.extend(executables.rules);
    let mut origins = declared.origins.clone();
    origins.extend(derived.origins);
    origins.extend(alias_origins);
    origins.extend(executables.origins);

    let program = build_program(
        cx.u,
        module_owner,
        &declared,
        &promoted.edges,
        &executables.seeds,
        &rules,
    );
    Ok(Lowered {
        program: Some(program),
        origins,
        diagnostics: vec![],
    })
}

/// `:281`.
fn build_program(
    u: &mut Universe,
    module_owner: TermId,
    declared: &Declared,
    edges: &[TermId],
    seeds: &[TermId],
    rules: &[TermId],
) -> TermId {
    let identity = u.compound("node", vec![module_owner]);
    let module_row = u.compound("module", vec![module_owner]);
    let product_row = u.compound("product", vec![module_owner]);
    let mut nodes = vec![identity, module_row, product_row];
    nodes.extend(declared.nodes.iter().copied());
    let node_list = u.list(&nodes);
    let edge_list = u.list(edges);
    let graph = u.compound("root_graph", vec![node_list, edge_list]);
    let relation_list = u.list(&declared.relations);
    let seed_list = u.list(seeds);
    let rule_list = u.list(rules);
    let datalog = u.compound("datalog_program", vec![relation_list, seed_list, rule_list]);
    u.compound("basement_program", vec![graph, datalog])
}

fn empty_program(u: &mut Universe) -> TermId {
    u.empty_list()
}

/// `:71`, `:147`. Program `[]`, no origins, one diagnostic.
fn stopped(u: &mut Universe, diagnostic: TermId) -> Lowered {
    Lowered {
        program: Some(empty_program(u)),
        origins: vec![],
        diagnostics: vec![diagnostic],
    }
}

fn concat(own: &[TermId], imported: &[TermId]) -> Vec<TermId> {
    let mut out = own.to_vec();
    out.extend_from_slice(imported);
    out
}
