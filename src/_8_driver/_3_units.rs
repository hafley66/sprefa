//! `compile_units/3` (`2_compiler.pl:545`) through `compile_after_check/5`
//! (`:710`), single-file and project arms.

use super::_4_project::{install_graphs, project_expression_environment, Project};
use super::Stop;
use crate::_2_lower::api::Lowered;
use crate::_2_lower::cx::CallPolicy;
use crate::_2_lower::units::{lower_compiler_units, merge_module_basements, Units};
use crate::_3_check::{check_datalog, Checked};
use crate::_4_comptime::api::derived_bind_slots;
use crate::_4_comptime::sources::{source_unit_module_owners, Live};
use crate::_4_comptime::{evaluate_checked, Compiled, Round};
use crate::_6_eval::one_engine::Attach;
use crate::_6_eval::term::{TermId, Universe};

/// `:545`. Bootstrap lowering defers calls to relations the fixpoint will mint.
pub fn compile_units(
    u: &mut Universe,
    units: &[TermId],
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<Compiled>, Vec<TermId>), Stop> {
    compile_units_attached(u, units, None, fx)
}

/// `compile_units` whose round loops evaluate on sessions attached to `one`.
pub fn compile_units_attached(
    u: &mut Universe,
    units: &[TermId],
    one: Option<Attach>,
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<Compiled>, Vec<TermId>), Stop> {
    let lowered = lower_compiler_units(u, CallPolicy::DeferUnknownCalls, units, None, &[])?;
    if !lowered.diagnostics.is_empty() {
        return Ok((None, lowered.diagnostics));
    }
    compile_after_unit_lower(u, units, None, lowered, one, fx)
}

/// `compile_units_attached` that hands the checked program to `checked` before its round loops
/// evaluate.
pub fn compile_units_attached_checked(
    u: &mut Universe,
    units: &[TermId],
    one: Option<Attach>,
    checked: &mut dyn FnMut(&mut Universe, &Checked),
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<Compiled>, Vec<TermId>), Stop> {
    let lowered = lower_compiler_units(u, CallPolicy::DeferUnknownCalls, units, None, &[])?;
    if !lowered.diagnostics.is_empty() {
        return Ok((None, lowered.diagnostics));
    }
    compile_after_unit_lower_checked(u, units, None, lowered, one, checked, fx)
}

/// `:559`. The TSI environment is visible while the source units lower.
pub fn compile_project_units(
    u: &mut Universe,
    project: Project,
    units: &[TermId],
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<Compiled>, Vec<TermId>), Stop> {
    let owners = source_unit_module_owners(u, units);
    let environment = project_expression_environment(u, &project, &owners);
    let lowered = lower_compiler_units(
        u,
        CallPolicy::DeferUnknownCalls,
        units,
        Some(environment),
        &[],
    )?;
    if !lowered.diagnostics.is_empty() {
        return Ok((None, lowered.diagnostics));
    }
    let (basements, origins, diagnostics) =
        install_graphs(u, &project, &lowered.basements, &lowered.origins).map_err(Stop::Load)?;
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }
    let lowered = Units {
        basements,
        origins,
        diagnostics: Vec::new(),
    };
    compile_after_unit_lower(u, units, Some(project), lowered, None, fx)
}

/// `:686` through `:716`: merge, check, then the comptime fixpoint.
pub fn compile_after_unit_lower(
    u: &mut Universe,
    units: &[TermId],
    project: Option<Project>,
    lowered: Units,
    one: Option<Attach>,
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<Compiled>, Vec<TermId>), Stop> {
    compile_after_unit_lower_checked(u, units, project, lowered, one, &mut |_, _| {}, fx)
}

/// `compile_after_unit_lower` that hands the checked program to `on_checked` before its round
/// loops evaluate.
fn compile_after_unit_lower_checked(
    u: &mut Universe,
    units: &[TermId],
    project: Option<Project>,
    lowered: Units,
    one: Option<Attach>,
    on_checked: &mut dyn FnMut(&mut Universe, &Checked),
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<Compiled>, Vec<TermId>), Stop> {
    let (basement, origins) = merge_module_basements(u, &lowered.basements, &lowered.origins);
    let merged = Lowered {
        program: Some(basement),
        origins,
        diagnostics: Vec::new(),
    };
    let (checked, diagnostics) = check_datalog(u, &merged)?;
    let Some(checked) = checked else {
        return Ok((None, diagnostics));
    };
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }
    on_checked(u, &checked);
    let mut sources = Live {
        derived_bind_slots: derived_bind_slots(u, &checked.rules),
        units: units.to_vec(),
        project,
        imports: Vec::new(),
        one,
    };
    Ok(evaluate_checked(u, &checked, &mut sources, fx)?)
}
