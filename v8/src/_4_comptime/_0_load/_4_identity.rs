//! `identity_map/5` and the passes that name every wire id.
//! Port of `v7/src/2_comptime/0c_extract_loader.pl:412-576`.

use super::api::{atom_text, diagnostic, parts, sorted, wire_id};
use super::wire::{Fact, IDENTITY_PASSES};
use crate::_6_eval::term::{TermId, Universe};
use std::collections::BTreeMap;

/// `identity(Id, Term)` rows, in v7's list order. `identity_of/3` is
/// `memberchk/2`, so the FIRST entry for an id wins and the primitive block
/// sits in front of the base block (`:420`).
pub struct Identities {
    pub rows: Vec<(i64, TermId)>,
}

impl Identities {
    pub fn of(&self, id: i64) -> Option<TermId> {
        self.rows
            .iter()
            .find(|(wire, _)| *wire == id)
            .map(|(_, term)| *term)
    }

    pub fn holds(&self, id: i64, term: TermId) -> bool {
        self.rows
            .iter()
            .any(|(wire, held)| *wire == id && *held == term)
    }

    /// `:536-541`. Every row for the id goes, the replacement goes in front.
    pub fn replace(&mut self, id: i64, term: TermId) {
        self.rows.retain(|(wire, _)| *wire != id);
        self.rows.insert(0, (id, term));
    }
}

/// `:416-429`.
pub fn identity_map(
    u: &mut Universe,
    owner: TermId,
    accepted: &[Fact],
    basements: &[TermId],
) -> (Identities, Vec<TermId>) {
    let (mut rows, primitive_diagnostics) = primitive_identities(u, accepted, basements);
    rows.extend(base_identities(u, owner, accepted));
    let mut identities = Identities { rows };
    application_identities(u, accepted, &mut identities, IDENTITY_PASSES);

    let mut diagnostics = primitive_diagnostics;
    diagnostics.extend(unresolved_diagnostics(u, accepted, &identities));
    diagnostics.extend(value_diagnostics(u, accepted));
    diagnostics.extend(application_diagnostics(u, accepted, &identities));
    let diagnostics = sorted(u, diagnostics);
    (identities, diagnostics)
}

/// `:431-449`. Claims are sorted by class then id, and a class the prelude
/// does not export is named once per class.
fn primitive_identities(
    u: &mut Universe,
    accepted: &[Fact],
    basements: &[TermId],
) -> (Vec<(i64, TermId)>, Vec<TermId>) {
    let mut claims: Vec<(String, i64)> = Vec::new();
    for fact in accepted {
        if fact.relation != "tsi.primitive" || fact.arguments.len() != 2 {
            continue;
        }
        let (Some(id), Some(class)) = (
            wire_id(u, fact.arguments[0]),
            u.unary(fact.arguments[1], "atom")
                .and_then(|a| atom_text(u, a))
                .map(|t| t.to_string()),
        ) else {
            continue;
        };
        claims.push((class, id));
    }
    claims.sort();
    claims.dedup();

    let prelude_edges = prelude_edges(u, basements);
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    for (class, id) in claims {
        match prelude_primitive(u, &prelude_edges, &class) {
            Some(identity) => rows.push((id, identity)),
            None => {
                let class = u.atom(&class);
                let payload = u.compound("tsi_primitive_class_absent", vec![class]);
                let none = u.atom("none");
                diagnostics.push(diagnostic(u, "extract", none, payload));
            }
        }
    }
    (rows, diagnostics)
}

/// `:451-457`. The first prelude basement and, inside it, the first edge with
/// the class label.
fn prelude_edges(u: &mut Universe, basements: &[TermId]) -> Vec<TermId> {
    let prelude_atom = u.atom("prelude");
    let prelude = u.compound("module", vec![prelude_atom]);
    for basement in basements {
        let Some(row) = parts(u, *basement, "module_basement", 2) else {
            continue;
        };
        if row[0] != prelude {
            continue;
        }
        let Some(program) = parts(u, row[1], "basement_program", 2) else {
            continue;
        };
        let Some(graph) = parts(u, program[0], "root_graph", 2) else {
            continue;
        };
        return u.as_list(graph[1]).unwrap_or_default();
    }
    Vec::new()
}

fn prelude_primitive(u: &mut Universe, edges: &[TermId], class: &str) -> Option<TermId> {
    // :459-460. The unit class is spelled `()` in the prelude.
    let label = if class == "unit" { "()" } else { class };
    let label = u.atom(label);
    let prelude_atom = u.atom("prelude");
    let prelude = u.compound("module", vec![prelude_atom]);
    for edge in edges {
        let Some(row) = parts(u, *edge, "pending_edge", 4) else {
            continue;
        };
        if row[0] != prelude || row[1] != label {
            continue;
        }
        if let Some(target) = u.unary(row[2], "target") {
            return Some(target);
        }
    }
    None
}

/// `:464-486`. A symbol, an edge, an argument list, then an ordinary type node.
fn base_identities(u: &mut Universe, owner: TermId, accepted: &[Fact]) -> Vec<(i64, TermId)> {
    let mut claims: Vec<TermId> = Vec::new();
    let push = |u: &mut Universe, claims: &mut Vec<TermId>, id: i64, functor: &str| {
        let id_term = u.int(id);
        let identity = u.compound(functor, vec![owner, id_term]);
        claims.push(u.compound("identity", vec![id_term, identity]));
    };
    for fact in accepted {
        let head = fact.arguments.first().and_then(|a| wire_id(u, *a));
        match (fact.relation.as_str(), fact.arguments.len()) {
            ("tsi.symbol", 1) | ("rust.impl", 3) => {
                if let Some(id) = head {
                    push(u, &mut claims, id, "tsi_symbol");
                }
            }
            ("tsi.edge", 5) => {
                if let Some(id) = head {
                    push(u, &mut claims, id, "tsi_edge");
                }
            }
            ("tsi.called", 3) => {
                if let Some(id) = wire_id(u, fact.arguments[2]) {
                    push(u, &mut claims, id, "tsi_arguments");
                }
            }
            ("tsi.type", 1) => {
                if let Some(id) = head {
                    push(u, &mut claims, id, "tsi_node");
                }
            }
            _ => {}
        }
    }
    let claims = sorted(u, claims);
    claims
        .iter()
        .filter_map(|claim| {
            let row = parts(u, *claim, "identity", 2)?;
            Some((u.as_int(row[0])?, row[1]))
        })
        .collect()
}

/// `:490-498`. Stops as soon as a pass resolves nothing; PLAN fork 2 covers
/// the silent cap.
fn application_identities(
    u: &mut Universe,
    accepted: &[Fact],
    identities: &mut Identities,
    passes: u32,
) {
    for _ in 0..passes {
        let applications = resolved_applications(u, accepted, identities);
        if applications.is_empty() {
            return;
        }
        for (id, term) in applications {
            identities.replace(id, term);
        }
    }
}

/// `:500-511`.
fn resolved_applications(
    u: &mut Universe,
    accepted: &[Fact],
    identities: &Identities,
) -> Vec<(i64, TermId)> {
    let arguments = argument_slots(u, accepted);
    let value_argument_lists = value_argument_lists(u, accepted);
    let mut found: Vec<TermId> = Vec::new();
    for fact in accepted {
        if fact.relation != "tsi.called" || fact.arguments.len() != 3 {
            continue;
        }
        let (Some(result), Some(callee_id), Some(list_id)) = (
            wire_id(u, fact.arguments[0]),
            wire_id(u, fact.arguments[1]),
            wire_id(u, fact.arguments[2]),
        ) else {
            continue;
        };
        let Some(callee) = identities.of(callee_id) else {
            continue;
        };
        let Some(list) =
            argument_identities(u, &arguments, &value_argument_lists, identities, list_id)
        else {
            continue;
        };
        let list = u.list(&list);
        let application = u.compound("application", vec![callee, list]);
        if identities.holds(result, application) {
            continue;
        }
        let result_term = u.int(result);
        found.push(u.compound("identity", vec![result_term, application]));
    }
    let found = sorted(u, found);
    found
        .iter()
        .filter_map(|row| {
            let parts = parts(u, *row, "identity", 2)?;
            Some((u.as_int(parts[0])?, parts[1]))
        })
        .collect()
}

fn argument_slots(u: &Universe, accepted: &[Fact]) -> BTreeMap<i64, Vec<(i64, i64)>> {
    let mut out: BTreeMap<i64, Vec<(i64, i64)>> = BTreeMap::new();
    for fact in accepted {
        if fact.relation != "tsi.argument" || fact.arguments.len() != 3 {
            continue;
        }
        let (Some(list), Some(position), Some(argument)) = (
            wire_id(u, fact.arguments[0]),
            u.unary(fact.arguments[1], "int").and_then(|p| u.as_int(p)),
            wire_id(u, fact.arguments[2]),
        ) else {
            continue;
        };
        out.entry(list).or_default().push((position, argument));
    }
    for slots in out.values_mut() {
        slots.sort();
        slots.dedup();
    }
    out
}

fn value_argument_lists(u: &Universe, accepted: &[Fact]) -> Vec<i64> {
    accepted
        .iter()
        .filter(|fact| fact.relation == "tsi.value_argument" && fact.arguments.len() == 3)
        .filter_map(|fact| wire_id(u, fact.arguments[0]))
        .collect()
}

/// `:513-534`. No value argument on the list, positions dense from 0, and
/// every argument already named.
fn argument_identities(
    u: &Universe,
    arguments: &BTreeMap<i64, Vec<(i64, i64)>>,
    value_argument_lists: &[i64],
    identities: &Identities,
    list: i64,
) -> Option<Vec<TermId>> {
    let _ = u;
    if value_argument_lists.contains(&list) {
        return None;
    }
    let empty = Vec::new();
    let slots = arguments.get(&list).unwrap_or(&empty);
    for (index, (position, _)) in slots.iter().enumerate() {
        if *position != index as i64 {
            return None;
        }
    }
    let mut out = Vec::with_capacity(slots.len());
    for (_, argument) in slots {
        out.push(identities.of(*argument)?);
    }
    Some(out)
}

/// `:549-557`. A `tsi.value` id carries its own diagnostic instead.
fn unresolved_diagnostics(
    u: &mut Universe,
    accepted: &[Fact],
    identities: &Identities,
) -> Vec<TermId> {
    let values: Vec<i64> = accepted
        .iter()
        .filter(|fact| fact.relation == "tsi.value" && fact.arguments.len() == 2)
        .filter_map(|fact| wire_id(u, fact.arguments[0]))
        .collect();
    let mut rows = Vec::new();
    for fact in accepted {
        for argument in &fact.arguments {
            let Some(id) = wire_id(u, *argument) else {
                continue;
            };
            if identities.of(id).is_some() || values.contains(&id) {
                continue;
            }
            let relation = u.atom(&fact.relation);
            let id_term = u.int(id);
            let payload = u.compound("tsi_id_unresolved", vec![relation, id_term]);
            let none = u.atom("none");
            rows.push(diagnostic(u, "extract", none, payload));
        }
    }
    sorted(u, rows)
}

/// `:561-568`.
fn application_diagnostics(
    u: &mut Universe,
    accepted: &[Fact],
    identities: &Identities,
) -> Vec<TermId> {
    let mut rows = Vec::new();
    for fact in accepted {
        if fact.relation != "tsi.called" || fact.arguments.len() != 3 {
            continue;
        }
        let Some(result) = wire_id(u, fact.arguments[0]) else {
            continue;
        };
        let named = identities
            .of(result)
            .and_then(|term| parts(u, term, "application", 2))
            .is_some();
        if named {
            continue;
        }
        let result_term = u.int(result);
        let payload = u.compound("tsi_called_unresolved", vec![result_term]);
        let none = u.atom("none");
        rows.push(diagnostic(u, "extract", none, payload));
    }
    sorted(u, rows)
}

/// `:572-576`. The wire carries a value's type, never its literal.
fn value_diagnostics(u: &mut Universe, accepted: &[Fact]) -> Vec<TermId> {
    let mut rows = Vec::new();
    for fact in accepted {
        if fact.relation != "tsi.value" || fact.arguments.len() != 2 {
            continue;
        }
        let Some(id) = wire_id(u, fact.arguments[0]) else {
            continue;
        };
        let id_term = u.int(id);
        let payload = u.compound("tsi_value_lacks_literal", vec![id_term]);
        let none = u.atom("none");
        rows.push(diagnostic(u, "extract", none, payload));
    }
    sorted(u, rows)
}
