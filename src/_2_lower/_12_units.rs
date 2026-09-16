//! Port of `v7/src/2_comptime/0a_module_lowerer.pl`: per-unit lowering under an
//! owner, the exporter environment, basement merge and alias install.

use super::api::{lower_datalog, Lowered};
use super::cx::CallPolicy;
use super::derived::Stop;
use crate::_3_check::api::prolog_sort;
use crate::_6_eval::term::{TermId, Universe};
use std::collections::{HashMap, HashSet};

/// `module_basement(Owner, Basement)` and `module_origins(Owner, Origins)`
/// lists, the shape `install_project_graph/6` also takes and returns.
#[derive(Default)]
pub struct Units {
    pub basements: Vec<TermId>,
    pub origins: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// `:74`.
pub fn unit_module_owner(u: &mut Universe, unit: TermId) -> Option<TermId> {
    let (name, args) = u.functor(unit)?;
    if name != "dl7_unit" || args.len() != 5 {
        return None;
    }
    let origin = args[0];
    Some(u.compound("module", vec![origin]))
}

pub fn empty_environment(u: &mut Universe) -> TermId {
    let empty = u.empty_list();
    u.compound("expression_environment", vec![empty, empty, empty])
}

fn lower_one(
    u: &mut Universe,
    policy: CallPolicy,
    unit: TermId,
    environment: TermId,
    out: &mut Units,
) -> Result<(), Stop> {
    let owner = unit_module_owner(u, unit).unwrap_or(unit);
    // `0_lowerer.pl:271`: a declaration error is Program `[]`, no origins, one
    // diagnostic, never a failure.
    let lowered = match lower_datalog(u, policy, unit, environment) {
        Ok(lowered) => lowered,
        Err(Stop::Diagnostic(diagnostic)) => Lowered {
            program: Some(u.empty_list()),
            origins: Vec::new(),
            diagnostics: vec![diagnostic],
        },
        Err(other) => return Err(other),
    };
    // `0_lowerer.pl:292` leaves Program unbound; every reader of a basement is
    // gated on an empty diagnostic list, so `[]` stands in.
    let program = lowered.program.unwrap_or_else(|| u.empty_list());
    let origins = u.list(&lowered.origins);
    out.basements
        .push(u.compound("module_basement", vec![owner, program]));
    out.origins
        .push(u.compound("module_origins", vec![owner, origins]));
    out.diagnostics.extend(lowered.diagnostics);
    Ok(())
}

/// `:38`, `:57`, `:69`. Every unit under one environment, no exporter split.
pub fn lower_units_flat(
    u: &mut Universe,
    policy: CallPolicy,
    units: &[TermId],
    environment: TermId,
) -> Result<Units, Stop> {
    let mut out = Units::default();
    for unit in units {
        lower_one(u, policy, *unit, environment, &mut out)?;
    }
    Ok(out)
}

/// `:84`, `:97`, `:155`, `:168`. The exporter lowers first and its top-level
/// callable declarations become the importers' environment.
pub fn lower_units_with_exporter(
    u: &mut Universe,
    policy: CallPolicy,
    exporter: TermId,
    importers: &[TermId],
    generated: Option<TermId>,
) -> Result<Units, Stop> {
    let exporter_owner = unit_module_owner(u, exporter).unwrap_or(exporter);
    let exporter_environment = match generated {
        Some(environment) => environment,
        None => empty_environment(u),
    };
    let mut out = Units::default();
    lower_one(u, policy, exporter, exporter_environment, &mut out)?;
    if !out.diagnostics.is_empty() {
        // :127, :199, :222. The exporter's own basement is the whole result.
        return Ok(out);
    }
    let exporter_basement = exported_basement(u, &out.basements, exporter_owner);
    let prelude = module_expression_environment(u, exporter_owner, exporter_basement);

    let mut importer_diagnostics = Vec::new();
    for unit in importers {
        let owner = unit_module_owner(u, *unit).unwrap_or(*unit);
        let reowned = reown_expression_reservations(u, prelude, owner);
        let environment = match generated {
            Some(generated) => merge_expression_environments(u, generated, reowned),
            None => reowned,
        };
        let mut one = Units::default();
        lower_one(u, policy, *unit, environment, &mut one)?;
        out.basements.extend(one.basements);
        out.origins.extend(one.origins);
        importer_diagnostics.extend(one.diagnostics);
    }
    if !importer_diagnostics.is_empty() {
        out.diagnostics = importer_diagnostics;
        return Ok(out);
    }
    let importer_owners: Vec<TermId> = importers
        .iter()
        .map(|unit| unit_module_owner(u, *unit).unwrap_or(*unit))
        .collect();
    let (basements, origins) = install_module_aliases(
        u,
        exporter_owner,
        &importer_owners,
        &out.basements,
        &out.origins,
    );
    Ok(Units {
        basements,
        origins,
        diagnostics: Vec::new(),
    })
}

fn exported_basement(u: &Universe, basements: &[TermId], owner: TermId) -> TermId {
    for row in basements {
        if let Some(("module_basement", args)) = u.functor(*row) {
            if args[0] == owner {
                return args[1];
            }
        }
    }
    owner
}

/// `basement_program(root_graph(Nodes, Edges), datalog_program(R, S, Rules))`.
fn basement_lists(u: &Universe, basement: TermId) -> Option<[Vec<TermId>; 5]> {
    let ("basement_program", args) = u.functor(basement)? else {
        return None;
    };
    if args.len() != 2 {
        return None;
    }
    let (graph, program) = (args[0], args[1]);
    let ("root_graph", graph_args) = u.functor(graph)? else {
        return None;
    };
    let ("datalog_program", program_args) = u.functor(program)? else {
        return None;
    };
    Some([
        u.as_list(graph_args[0])?,
        u.as_list(graph_args[1])?,
        u.as_list(program_args[0])?,
        u.as_list(program_args[1])?,
        u.as_list(program_args[2])?,
    ])
}

/// `:303`. Every top-level exporter edge whose target is a declared relation.
pub fn module_expression_environment(
    u: &mut Universe,
    exporter_owner: TermId,
    basement: TermId,
) -> TermId {
    let Some([_, edges, relations, _, _]) = basement_lists(u, basement) else {
        return empty_environment(u);
    };
    let declared: HashSet<TermId> = relations
        .iter()
        .filter_map(|row| match u.functor(*row) {
            Some(("relation", args)) if args.len() == 3 => Some(args[0]),
            _ => None,
        })
        .collect();
    let mut reservations = Vec::new();
    let imported = u.atom("imported");
    let product = u.atom("product");
    for edge in &edges {
        let Some(("pending_edge", args)) = u.functor(*edge) else {
            continue;
        };
        if args.len() != 4 || args[0] != exporter_owner {
            continue;
        }
        let (name, target) = (args[1], args[2]);
        let Some(callable) = u.unary(target, "target") else {
            continue;
        };
        if !declared.contains(&callable) {
            continue;
        }
        reservations.push(u.compound("reservation", vec![imported, name, target, product]));
    }
    let reservations = u.list(&reservations);
    let relations = u.list(&relations);
    let edges = u.list(&edges);
    u.compound(
        "expression_environment",
        vec![reservations, relations, edges],
    )
}

/// `:333`.
pub fn reown_expression_reservations(
    u: &mut Universe,
    environment: TermId,
    owner: TermId,
) -> TermId {
    let Some(("expression_environment", args)) = u.functor(environment) else {
        return environment;
    };
    let (reservations, relations, edges) = (args[0], args[1], args[2]);
    let Some(rows) = u.as_list(reservations) else {
        return environment;
    };
    let reowned: Vec<TermId> = rows
        .iter()
        .map(|row| match u.functor(*row) {
            Some(("reservation", args)) if args.len() == 4 => {
                let (name, target, kind) = (args[1], args[2], args[3]);
                u.compound("reservation", vec![owner, name, target, kind])
            }
            _ => *row,
        })
        .collect();
    let reservations = u.list(&reowned);
    u.compound(
        "expression_environment",
        vec![reservations, relations, edges],
    )
}

/// `:270`. Append then `sort/2` each of the three lists.
pub fn merge_expression_environments(u: &mut Universe, first: TermId, second: TermId) -> TermId {
    let (Some(("expression_environment", a)), Some(("expression_environment", b))) =
        (u.functor(first), u.functor(second))
    else {
        return first;
    };
    let a = [a[0], a[1], a[2]];
    let b = [b[0], b[1], b[2]];
    let mut merged = Vec::with_capacity(3);
    for slot in 0..3 {
        let mut rows = u.as_list(a[slot]).unwrap_or_default();
        rows.extend(u.as_list(b[slot]).unwrap_or_default());
        let rows = prolog_sort(u, rows);
        merged.push(u.list(&rows));
    }
    u.compound("expression_environment", merged)
}

/// `:360`. Concatenate already-owned rows in source order.
pub fn merge_module_basements(
    u: &mut Universe,
    basements: &[TermId],
    module_origins: &[TermId],
) -> (TermId, Vec<TermId>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut relations = Vec::new();
    let mut seeds = Vec::new();
    let mut rules = Vec::new();
    for row in basements {
        let Some(("module_basement", args)) = u.functor(*row) else {
            continue;
        };
        let Some([n, e, r, s, l]) = basement_lists(u, args[1]) else {
            continue;
        };
        nodes.extend(n);
        edges.extend(e);
        relations.extend(r);
        seeds.extend(s);
        rules.extend(l);
    }
    let mut origins = Vec::new();
    for row in module_origins {
        if let Some(("module_origins", args)) = u.functor(*row) {
            origins.extend(u.as_list(args[1]).unwrap_or_default());
        }
    }
    let node_list = u.list(&nodes);
    let edge_list = u.list(&edges);
    let graph = u.compound("root_graph", vec![node_list, edge_list]);
    let relation_list = u.list(&relations);
    let seed_list = u.list(&seeds);
    let rule_list = u.list(&rules);
    let program = u.compound("datalog_program", vec![relation_list, seed_list, rule_list]);
    (
        u.compound("basement_program", vec![graph, program]),
        origins,
    )
}

/// `:398`. Every concrete top-level exporter edge, exposed through each
/// importer that does not already bind the name.
pub fn install_module_aliases(
    u: &mut Universe,
    exporter: TermId,
    importers: &[TermId],
    basements: &[TermId],
    origins: &[TermId],
) -> (Vec<TermId>, Vec<TermId>) {
    let mut basements = basements.to_vec();
    let mut origins = origins.to_vec();
    for importer in importers {
        let exporter_basement = exported_basement(u, &basements, exporter);
        let export_edges: Vec<TermId> = basement_lists(u, exporter_basement)
            .map(|[_, edges, _, _, _]| edges)
            .unwrap_or_default()
            .into_iter()
            .filter(|edge| match u.functor(*edge) {
                Some(("pending_edge", args)) => args.len() == 4 && args[0] == exporter,
                _ => false,
            })
            .collect();
        install_importer_aliases(
            u,
            *importer,
            exporter,
            &export_edges,
            &mut basements,
            &mut origins,
        );
    }
    (basements, origins)
}

/// `:415`.
fn install_importer_aliases(
    u: &mut Universe,
    importer: TermId,
    exporter: TermId,
    export_edges: &[TermId],
    basements: &mut [TermId],
    origins: &mut [TermId],
) {
    let Some(slot) = basements.iter().position(|row| match u.functor(*row) {
        Some(("module_basement", args)) => args[0] == importer,
        _ => false,
    }) else {
        return;
    };
    let Some(("module_basement", args)) = u.functor(basements[slot]) else {
        return;
    };
    let basement = args[1];
    let Some([nodes, edges, relations, seeds, rules]) = basement_lists(u, basement) else {
        return;
    };
    let mut bound: HashSet<TermId> = HashSet::new();
    let mut index = -1i64;
    for edge in &edges {
        if let Some(("pending_edge", args)) = u.functor(*edge) {
            if args.len() == 4 && args[0] == importer {
                bound.insert(args[1]);
                index = index.max(u.as_int(args[3]).unwrap_or(-1));
            }
        }
    }
    let mut next = index + 1;
    let edge_origins = origin_index(u, origins, exporter);
    let none = u.atom("none");

    let mut alias_edges = Vec::new();
    let mut alias_origins = Vec::new();
    for edge in export_edges {
        let Some(("pending_edge", args)) = u.functor(*edge) else {
            continue;
        };
        let (name, target, export_index) = (args[1], args[2], args[3]);
        if !importable_target(u, target) || bound.contains(&name) {
            continue;
        }
        let node = *edge_origins.get(&(name, export_index)).unwrap_or(&none);
        let alias_index = u.int(next);
        alias_edges.push(u.compound("pending_edge", vec![importer, name, target, alias_index]));
        let edge_key = u.compound("edge", vec![importer, name, alias_index]);
        alias_origins.push(u.compound("origin", vec![edge_key, node]));
        next += 1;
    }
    if alias_edges.is_empty() {
        return;
    }
    let mut all = edges;
    all.extend(alias_edges);
    let node_list = u.list(&nodes);
    let edge_list = u.list(&all);
    let graph = u.compound("root_graph", vec![node_list, edge_list]);
    let relation_list = u.list(&relations);
    let seed_list = u.list(&seeds);
    let rule_list = u.list(&rules);
    let program = u.compound("datalog_program", vec![relation_list, seed_list, rule_list]);
    let replacement = u.compound("basement_program", vec![graph, program]);
    basements[slot] = u.compound("module_basement", vec![importer, replacement]);
    append_module_origins(u, importer, &alias_origins, origins);
}

/// `:462`, keyed on the edge name and the exporting ordinal.
fn origin_index(
    u: &Universe,
    module_origins: &[TermId],
    owner: TermId,
) -> HashMap<(TermId, TermId), TermId> {
    let mut out = HashMap::new();
    for row in module_origins {
        let Some(("module_origins", args)) = u.functor(*row) else {
            continue;
        };
        if args[0] != owner {
            continue;
        }
        for origin in u.as_list(args[1]).unwrap_or_default() {
            let Some(("origin", origin_args)) = u.functor(origin) else {
                continue;
            };
            let Some(("edge", edge)) = u.functor(origin_args[0]) else {
                continue;
            };
            if edge.len() == 3 && edge[0] == owner {
                out.entry((edge[1], edge[2])).or_insert(origin_args[1]);
            }
        }
        break;
    }
    out
}

/// `:458`. A deferred target is never aliased.
fn importable_target(u: &Universe, target: TermId) -> bool {
    match u.functor(target) {
        Some(("deferred_expression", args)) => args.len() != 1,
        Some(("deferred_compound_edge", args)) => args.len() != 2,
        _ => true,
    }
}

/// `:477`.
fn append_module_origins(
    u: &mut Universe,
    owner: TermId,
    added: &[TermId],
    module_origins: &mut [TermId],
) {
    for slot in module_origins.iter_mut() {
        let Some(("module_origins", args)) = u.functor(*slot) else {
            continue;
        };
        if args[0] != owner {
            continue;
        }
        let mut rows = u.as_list(args[1]).unwrap_or_default();
        rows.extend_from_slice(added);
        let list = u.list(&rows);
        *slot = u.compound("module_origins", vec![owner, list]);
        return;
    }
}

/// `2_compiler.pl:655`, `:665`. The prelude unit, if present, is the exporter.
pub fn lower_compiler_units(
    u: &mut Universe,
    policy: CallPolicy,
    units: &[TermId],
    environment: Option<TermId>,
) -> Result<Units, Stop> {
    let prelude = u.atom("prelude");
    let exporter = units.iter().position(|unit| match u.functor(*unit) {
        Some(("dl7_unit", args)) => args[0] == prelude,
        _ => false,
    });
    match exporter {
        Some(slot) => {
            let mut importers = units.to_vec();
            let exporter = importers.remove(slot);
            lower_units_with_exporter(u, policy, exporter, &importers, environment)
        }
        None => {
            let environment = match environment {
                Some(environment) => environment,
                None => empty_environment(u),
            };
            lower_units_flat(u, policy, units, environment)
        }
    }
}
