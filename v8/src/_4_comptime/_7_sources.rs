//! The live `Sources`: `final_checked_program/5` (`2_compiler.pl:929`) and
//! `deferred_checked_program/5` (`:949`). `Replay` stays for the oracle.

use super::api::{Refreeze, Sources};
use super::finish::{generated_expression_environment, merge_expression_environments};
use crate::_2_lower::api::Lowered;
use crate::_2_lower::cx::CallPolicy;
use crate::_2_lower::units::{lower_compiler_units, merge_module_basements};
use crate::_3_check::api::prolog_sort;
use crate::_3_check::{check_datalog, Checked, Stop};
use crate::_4_comptime::load::tsi_expression_environment;
use crate::_6_eval::term::{TermId, Universe};
use crate::_8_driver::_4_project::{install_graphs, Project};

/// `compile_context(Units, ProjectContext, DerivedBindSlots)` at `:931`.
pub struct Live {
    pub units: Vec<TermId>,
    pub project: Option<Project>,
    pub derived_bind_slots: Vec<TermId>,
}

impl Sources for Live {
    fn refreeze(
        &mut self,
        u: &mut Universe,
        mode: Refreeze,
        compiler_facts: &[TermId],
        generated_relations: &[TermId],
    ) -> Result<(Option<Checked>, Vec<TermId>), Stop> {
        let environment = final_expression_environment(
            u,
            compiler_facts,
            generated_relations,
            &self.derived_bind_slots,
            &self.units,
            self.project.as_ref(),
        );
        let policy = match mode {
            Refreeze::Final => CallPolicy::Strict,
            Refreeze::Deferred => CallPolicy::DeferUnknownCalls,
        };
        let reservations = u
            .functor(environment)
            .map(|(_, args)| args[0])
            .and_then(|list| u.as_list(list))
            .unwrap_or_default();
        let lowered = lower_compiler_units(u, policy, &self.units, Some(environment))
            .map_err(|_| Stop::Fail("lowering reached an unported construct"))?;
        if !lowered.diagnostics.is_empty() {
            return Ok((None, lowered.diagnostics));
        }
        let basements = freeze_module_basements(u, &reservations, &lowered.basements);
        let (basements, origins, diagnostics) =
            install_final_project_graph(u, self.project.as_ref(), &basements, &lowered.origins)?;
        if !diagnostics.is_empty() {
            return Ok((None, diagnostics));
        }
        let (basement, origins) = merge_module_basements(u, &basements, &origins);
        let basement = add_generated_relations(u, basement, generated_relations);
        let lowered = Lowered {
            program: Some(basement),
            origins,
            diagnostics: Vec::new(),
        };
        check_datalog(u, &lowered)
    }
}

/// The three lists `expression_environment/3` carries.
pub type Environment = (Vec<TermId>, Vec<TermId>, Vec<TermId>);

/// `:1090`.
pub fn final_expression_environment(
    u: &mut Universe,
    compiler_facts: &[TermId],
    generated_relations: &[TermId],
    derived_bind_slots: &[TermId],
    units: &[TermId],
    project: Option<&Project>,
) -> TermId {
    let generated = generated_expression_environment(
        u,
        compiler_facts,
        generated_relations,
        derived_bind_slots,
    );
    let tsi = match project {
        Some(project) => {
            let owners = source_unit_module_owners(u, units);
            let term = tsi_expression_environment(u, &project.tsi_rows, &owners);
            environment_lists(u, term)
        }
        None => (Vec::new(), Vec::new(), Vec::new()),
    };
    let merged = merge_expression_environments(u, generated, tsi);
    let reservations = u.list(&merged.0);
    let relations = u.list(&merged.1);
    let edges = u.list(&merged.2);
    u.compound(
        "expression_environment",
        vec![reservations, relations, edges],
    )
}

fn environment_lists(u: &Universe, term: TermId) -> Environment {
    let Some(("expression_environment", args)) = u.functor(term) else {
        return (Vec::new(), Vec::new(), Vec::new());
    };
    (
        u.as_list(args[0]).unwrap_or_default(),
        u.as_list(args[1]).unwrap_or_default(),
        u.as_list(args[2]).unwrap_or_default(),
    )
}

/// `2_compiler.pl:571`. The prelude unit owns no source module.
pub fn source_unit_module_owners(u: &mut Universe, units: &[TermId]) -> Vec<TermId> {
    let prelude = u.atom("prelude");
    let origins: Vec<TermId> = units
        .iter()
        .filter_map(|unit| match u.functor(*unit) {
            Some(("dl7_unit", args)) if args[0] != prelude => Some(args[0]),
            _ => None,
        })
        .collect();
    origins
        .into_iter()
        .map(|origin| u.compound("module", vec![origin]))
        .collect()
}

/// `:1006`. A deferred edge whose name the environment now reserves becomes a
/// real target.
pub fn freeze_module_basements(
    u: &mut Universe,
    reservations: &[TermId],
    basements: &[TermId],
) -> Vec<TermId> {
    let mut out = Vec::with_capacity(basements.len());
    for row in basements {
        let Some(("module_basement", args)) = u.functor(*row).map(|(n, a)| (n, a.to_vec())) else {
            out.push(*row);
            continue;
        };
        let (owner, basement) = (args[0], args[1]);
        let Some(("basement_program", parts)) = u.functor(basement).map(|(n, a)| (n, a.to_vec()))
        else {
            out.push(*row);
            continue;
        };
        let Some(("root_graph", graph)) = u.functor(parts[0]).map(|(n, a)| (n, a.to_vec())) else {
            out.push(*row);
            continue;
        };
        let Some(edges) = u.as_list(graph[1]) else {
            out.push(*row);
            continue;
        };
        let frozen: Vec<TermId> = edges
            .iter()
            .map(|edge| freeze_pending_edge(u, reservations, *edge))
            .collect();
        let edge_list = u.list(&frozen);
        let root = u.compound("root_graph", vec![graph[0], edge_list]);
        let program = u.compound("basement_program", vec![root, parts[1]]);
        out.push(u.compound("module_basement", vec![owner, program]));
    }
    out
}

/// `:1016`.
fn freeze_pending_edge(u: &mut Universe, reservations: &[TermId], edge: TermId) -> TermId {
    let Some(("pending_edge", args)) = u.functor(edge).map(|(n, a)| (n, a.to_vec())) else {
        return edge;
    };
    if u.unary(args[2], "deferred_expression").is_none() {
        return edge;
    }
    let product = u.atom("product");
    for row in reservations {
        let Some(("reservation", parts)) = u.functor(*row) else {
            continue;
        };
        if parts.len() != 4 || parts[0] != args[0] || parts[1] != args[1] || parts[3] != product {
            continue;
        }
        if u.unary(parts[2], "target").is_none() {
            continue;
        }
        let target = parts[2];
        return u.compound("pending_edge", vec![args[0], args[1], target, args[3]]);
    }
    edge
}

/// `:1025`.
fn install_final_project_graph(
    u: &mut Universe,
    project: Option<&Project>,
    basements: &[TermId],
    origins: &[TermId],
) -> Result<crate::_8_driver::_4_project::Installed, Stop> {
    match project {
        None => Ok((basements.to_vec(), origins.to_vec(), Vec::new())),
        Some(project) => install_graphs(u, project, basements, origins)
            .map_err(|_| Stop::Fail("project graph install failed")),
    }
}

/// `:1046`. Generated declarations lose their `ref` wrapper and join the
/// authored ones under one `sort/2`.
pub fn add_generated_relations(
    u: &mut Universe,
    basement: TermId,
    generated_relations: &[TermId],
) -> TermId {
    let Some(("basement_program", parts)) = u.functor(basement).map(|(n, a)| (n, a.to_vec()))
    else {
        return basement;
    };
    let Some(("datalog_program", program)) = u.functor(parts[1]).map(|(n, a)| (n, a.to_vec()))
    else {
        return basement;
    };
    let mut relations = u.as_list(program[0]).unwrap_or_default();
    for row in generated_relations {
        let Some(("relation", args)) = u.functor(*row).map(|(n, a)| (n, a.to_vec())) else {
            continue;
        };
        let Some(inner) = u.unary(args[0], "ref") else {
            continue;
        };
        relations.push(u.compound("relation", vec![inner, args[1], args[2]]));
    }
    let relations = prolog_sort(u, relations);
    let relation_list = u.list(&relations);
    let datalog = u.compound(
        "datalog_program",
        vec![relation_list, program[1], program[2]],
    );
    u.compound("basement_program", vec![parts[0], datalog])
}
