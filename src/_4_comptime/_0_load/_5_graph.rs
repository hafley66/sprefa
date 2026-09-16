//! `install_tsi_graph/6` and `tsi_expression_environment/3`. Port of
//! `v7/src/2_comptime/0c_extract_loader.pl:309-336`, `:358-394` and
//! `:578-733`.
//!
//! Unlike the filesystem grapher a diagnostic does not void the install: the
//! row it names is skipped and the rest still becomes a basement (`:358-359`).

use super::api::{diagnostic, owner_index, parts, sorted, string_text, wire_id, Installed};
use super::identity::{identity_map, Identities};
use super::wire::{
    accepted_rows, facts_of, relation_names, stream_owner, tsi_relation_arity, Fact, Owner,
};
use crate::_6_eval::term::{TermId, Universe};

/// `:363`.
pub fn install_tsi_graph(
    u: &mut Universe,
    rows: &[TermId],
    basements: &[TermId],
    origins: &[TermId],
) -> Installed {
    let accepted_terms = accepted_rows(u, rows);
    let accepted = facts_of(u, &accepted_terms);
    let owner = match stream_owner(u, rows) {
        Owner::None => {
            return Installed {
                basements: basements.to_vec(),
                origins: origins.to_vec(),
                diagnostics: vec![],
            }
        }
        Owner::MissingRun => {
            // :373.
            let payload = u.atom("tsi_stream_without_run");
            let none = u.atom("none");
            let row = diagnostic(u, "extract", none, payload);
            return Installed {
                basements: basements.to_vec(),
                origins: origins.to_vec(),
                diagnostics: vec![row],
            };
        }
        Owner::Owner(owner) => owner,
    };

    let (identities, identity_diagnostics) = identity_map(u, owner, &accepted, basements);
    let type_nodes = graph_nodes(u, owner, &accepted, &identities);
    let (type_edges, edge_diagnostics) = graph_edges(u, &accepted, &identities);
    let (names, relations, seeds, relation_diagnostics) =
        comptime_relations(u, owner, &accepted, &identities);
    let mut edges = relation_edges(u, owner, &names);
    edges.extend(type_edges);
    let node_origins = node_origins(u, &accepted, &identities);

    let identity_row = u.compound("node", vec![owner]);
    let module_row = u.compound("module", vec![owner]);
    let product_row = u.compound("product", vec![owner]);
    let mut nodes = vec![identity_row, module_row, product_row];
    nodes.extend(type_nodes);

    let basement = basement_program(u, &nodes, &edges, &relations, &seeds);
    let module_basement = u.compound("module_basement", vec![owner, basement]);
    let origin_list = u.list(&node_origins);
    let module_origins = u.compound("module_origins", vec![owner, origin_list]);

    let mut diagnostics = identity_diagnostics;
    diagnostics.extend(edge_diagnostics);
    diagnostics.extend(relation_diagnostics);
    let diagnostics = sorted(u, diagnostics);

    let mut out_basements = vec![module_basement];
    out_basements.extend_from_slice(basements);
    let mut out_origins = vec![module_origins];
    out_origins.extend_from_slice(origins);
    Installed {
        basements: out_basements,
        origins: out_origins,
        diagnostics,
    }
}

pub fn basement_program(
    u: &mut Universe,
    nodes: &[TermId],
    edges: &[TermId],
    relations: &[TermId],
    seeds: &[TermId],
) -> TermId {
    let node_list = u.list(nodes);
    let edge_list = u.list(edges);
    let graph = u.compound("root_graph", vec![node_list, edge_list]);
    let relation_list = u.list(relations);
    let seed_list = u.list(seeds);
    let empty = u.empty_list();
    let datalog = u.compound("datalog_program", vec![relation_list, seed_list, empty]);
    u.compound("basement_program", vec![graph, datalog])
}

/// `:313-336`. Reservations nest the importers outside the names and neither
/// list is sorted, so the order is the one v7's `findall/3` produced.
pub fn tsi_expression_environment(
    u: &mut Universe,
    rows: &[TermId],
    importers: &[TermId],
) -> TermId {
    let accepted_terms = accepted_rows(u, rows);
    let accepted = facts_of(u, &accepted_terms);
    let (reservations, relations) = match stream_owner(u, rows) {
        Owner::Owner(owner) => {
            let (names, _) = relation_names(u, &accepted);
            let relations = relation_rows(u, owner, &names);
            let product = u.atom("product");
            let mut reservations = Vec::new();
            for importer in importers {
                for name in &names {
                    let name_atom = u.atom(name);
                    let callable = u.compound("tsi_relation", vec![owner, name_atom]);
                    let target = u.compound("target", vec![callable]);
                    reservations.push(
                        u.compound("reservation", vec![*importer, name_atom, target, product]),
                    );
                }
            }
            (reservations, relations)
        }
        _ => (vec![], vec![]),
    };
    let reservation_list = u.list(&reservations);
    let relation_list = u.list(&relations);
    let empty = u.empty_list();
    u.compound(
        "expression_environment",
        vec![reservation_list, relation_list, empty],
    )
}

/// `:581-603`. A primitive class contributes no node: its identity is the
/// prelude product, which `loader_identity/2` does not admit.
fn graph_nodes(
    u: &mut Universe,
    owner: TermId,
    accepted: &[Fact],
    identities: &Identities,
) -> Vec<TermId> {
    let mut nodes = Vec::new();
    for relation in ["tsi.type", "tsi.product", "tsi.sum"] {
        for fact in accepted {
            if fact.relation != relation || fact.arguments.len() != 1 {
                continue;
            }
            let Some(id) = wire_id(u, fact.arguments[0]) else {
                continue;
            };
            let Some(identity) = identities.of(id) else {
                continue;
            };
            if !loader_identity(u, owner, identity) {
                continue;
            }
            let functor = match relation {
                "tsi.type" => "node",
                "tsi.product" => "product",
                _ => "sum",
            };
            nodes.push(u.compound(functor, vec![identity]));
        }
    }
    sorted(u, nodes)
}

fn loader_identity(u: &Universe, owner: TermId, identity: TermId) -> bool {
    let Some((name, args)) = u.functor(identity) else {
        return false;
    };
    match (name, args.len()) {
        ("tsi_node" | "tsi_symbol" | "tsi_edge" | "tsi_arguments", 2) => args[0] == owner,
        ("application", 2) => true,
        _ => false,
    }
}

/// `:609-669`. The wire position orders the claims and is then discarded; the
/// dense per-owner run the checker wants is assigned from that order.
fn graph_edges(
    u: &mut Universe,
    accepted: &[Fact],
    identities: &Identities,
) -> (Vec<TermId>, Vec<TermId>) {
    let mut claims: Vec<TermId> = Vec::new();
    for fact in accepted {
        if fact.relation != "tsi.edge" || fact.arguments.len() != 5 {
            continue;
        }
        let (Some(owner_id), Some(label), Some(target_id), Some(position)) = (
            wire_id(u, fact.arguments[1]),
            u.unary(fact.arguments[2], "text")
                .and_then(|t| string_text(u, t))
                .map(|t| t.to_string()),
            wire_id(u, fact.arguments[3]),
            u.unary(fact.arguments[4], "int").and_then(|p| u.as_int(p)),
        ) else {
            continue;
        };
        let (Some(owner), Some(target)) = (identities.of(owner_id), identities.of(target_id))
        else {
            continue;
        };
        let label = u.atom(&label);
        let position = u.int(position);
        claims.push(u.compound("claim", vec![owner, position, label, target]));
    }
    let claims = sorted(u, claims);

    let mut unique: Vec<Vec<TermId>> = Vec::new();
    let mut seen: Vec<(TermId, TermId)> = Vec::new();
    let mut diagnostics = Vec::new();
    for claim in &claims {
        let row = parts(u, *claim, "claim", 4).expect("claim shape");
        let key = (row[0], row[2]);
        if seen.contains(&key) {
            let payload = u.compound("tsi_duplicate_edge_label", vec![row[0], row[2]]);
            let none = u.atom("none");
            diagnostics.push(diagnostic(u, "extract", none, payload));
        } else {
            unique.push(row.clone());
        }
        seen.push(key);
    }

    let mut edges = Vec::with_capacity(unique.len());
    let mut previous: Option<TermId> = None;
    let mut running = 0;
    for row in &unique {
        let index = owner_index(row[0], &mut previous, &mut running);
        let index_term = u.int(index);
        let target = u.compound("target", vec![row[3]]);
        edges.push(u.compound("pending_edge", vec![row[0], row[2], target, index_term]));
    }

    diagnostics.extend(unplaced_edge_diagnostics(u, accepted, identities));
    let diagnostics = sorted(u, diagnostics);
    (edges, diagnostics)
}

/// `:657-669`.
fn unplaced_edge_diagnostics(
    u: &mut Universe,
    accepted: &[Fact],
    identities: &Identities,
) -> Vec<TermId> {
    let mut rows = Vec::new();
    for fact in accepted {
        if fact.relation != "tsi.edge" || fact.arguments.len() != 5 {
            continue;
        }
        let (Some(edge_id), Some(owner_id), Some(target_id)) = (
            wire_id(u, fact.arguments[0]),
            wire_id(u, fact.arguments[1]),
            wire_id(u, fact.arguments[3]),
        ) else {
            continue;
        };
        if identities.of(owner_id).is_some() && identities.of(target_id).is_some() {
            continue;
        }
        let edge_term = u.int(edge_id);
        let payload = u.compound("tsi_edge_unplaced", vec![edge_term]);
        let none = u.atom("none");
        rows.push(diagnostic(u, "extract", none, payload));
    }
    sorted(u, rows)
}

/// `:675-689`. The relation name is the wire name with its dot; the arity is
/// the registry's.
fn comptime_relations(
    u: &mut Universe,
    owner: TermId,
    accepted: &[Fact],
    identities: &Identities,
) -> (Vec<String>, Vec<TermId>, Vec<TermId>, Vec<TermId>) {
    let (names, diagnostics) = relation_names(u, accepted);
    let relations = relation_rows(u, owner, &names);
    let mut seeds = Vec::new();
    for fact in accepted {
        if !names.contains(&fact.relation) {
            continue;
        }
        let mut arguments = Vec::with_capacity(fact.arguments.len());
        let mut complete = true;
        for argument in &fact.arguments {
            match seed_argument(u, identities, *argument) {
                Some(term) => arguments.push(term),
                None => {
                    complete = false;
                    break;
                }
            }
        }
        if !complete {
            continue;
        }
        let relation = u.atom(&fact.relation);
        let head = u.compound("name", vec![owner, relation]);
        let argument_list = u.list(&arguments);
        seeds.push(u.compound("call", vec![head, argument_list]));
    }
    let seeds = sorted(u, seeds);
    (names, relations, seeds, diagnostics)
}

fn relation_rows(u: &mut Universe, owner: TermId, names: &[String]) -> Vec<TermId> {
    let mut rows = Vec::new();
    for name in names {
        let Some(arity) = tsi_relation_arity(name) else {
            continue;
        };
        let name_atom = u.atom(name);
        let callable = u.compound("tsi_relation", vec![owner, name_atom]);
        let arity = u.int(arity);
        let empty = u.empty_list();
        rows.push(u.compound("relation", vec![callable, arity, empty]));
    }
    rows
}

/// `:706-713`.
fn seed_argument(u: &mut Universe, identities: &Identities, argument: TermId) -> Option<TermId> {
    if let Some(id) = wire_id(u, argument) {
        let identity = identities.of(id)?;
        return Some(u.compound("ref", vec![identity]));
    }
    let (name, args) = u.functor(argument)?;
    let args = args.to_vec();
    match (name, args.len()) {
        ("span", 3) => {
            let span = u.compound("span", args);
            Some(u.compound("const", vec![span]))
        }
        ("text" | "int" | "atom", 1) => Some(u.compound("const", vec![args[0]])),
        _ => None,
    }
}

/// `:715-721`.
fn relation_edges(u: &mut Universe, owner: TermId, names: &[String]) -> Vec<TermId> {
    let mut edges = Vec::with_capacity(names.len());
    for (index, name) in names.iter().enumerate() {
        let name_atom = u.atom(name);
        let callable = u.compound("tsi_relation", vec![owner, name_atom]);
        let target = u.compound("target", vec![callable]);
        let index = u.int(index as i64);
        edges.push(u.compound("pending_edge", vec![owner, name_atom, target, index]));
    }
    edges
}

/// `:723-733`.
fn node_origins(u: &mut Universe, accepted: &[Fact], identities: &Identities) -> Vec<TermId> {
    let mut rows = Vec::new();
    for fact in accepted {
        if fact.relation != "tsi.origin" || fact.arguments.len() != 3 {
            continue;
        }
        let (Some(id), Some(language), Some(span)) = (
            wire_id(u, fact.arguments[0]),
            u.unary(fact.arguments[1], "atom"),
            parts(u, fact.arguments[2], "span", 3),
        ) else {
            continue;
        };
        let Some(identity) = identities.of(id) else {
            continue;
        };
        let node = u.compound("node", vec![identity]);
        let extract = u.compound("extract", vec![language, span[0], span[1], span[2]]);
        rows.push(u.compound("origin", vec![node, extract]));
    }
    sorted(u, rows)
}
