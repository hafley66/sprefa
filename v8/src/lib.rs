//! dl8: the DL7 language, compiled in Rust. One `_<n>_name` folder per
//! operator, `_6_eval` the shared fixpoint kernel, `compile` the chain.

pub mod _0_read;
pub mod _1_macrotime;
pub mod _2_lower;
pub mod _3_check;
pub mod _4_comptime;
pub mod _5_reify;
pub mod _6_eval;
pub mod _8_driver;

use _2_lower::api::Lowered;
use _2_lower::cx::CallPolicy;
use _2_lower::units::{lower_compiler_units, merge_module_basements, Units};
use _3_check::check_datalog;
use _4_comptime::api::derived_bind_slots;
use _4_comptime::evaluate_checked;
use _4_comptime::load::tsi_expression_environment;
use _4_comptime::sources::{source_unit_module_owners, Live};
use _4_comptime::Compiled;
use _6_eval::term::{TermId, Universe};
use _8_driver::_0_read::{prelude_text, program_text};
use _8_driver::_1_unit::{file_unit, prelude_unit};
use _8_driver::_2_macro::{expand_units_with_macros, standard_macro_program};
use _8_driver::_4_project::{install_graphs, load_dl7_project, load_tsi_streams, Project};
use _8_driver::{Event, Stop};
use std::path::Path;
use std::time::Instant;

/// `compile_dl7/4` at `2_compiler.pl:75`, one field per output argument.
pub struct Compile {
    pub compiler_rows: Vec<TermId>,
    pub runtime_program: TermId,
    pub diagnostics: Vec<TermId>,
}

/// `compiled_outputs/3` at `:466`: no compiled unit is an empty row list and an
/// empty runtime program, never a missing one.
fn outputs(u: &mut Universe, compiled: Option<Compiled>, diagnostics: Vec<TermId>) -> Compile {
    match compiled {
        Some(compiled) => Compile {
            compiler_rows: compiled.compiler_facts,
            runtime_program: compiled.runtime.to_term(u),
            diagnostics,
        },
        None => Compile {
            compiler_rows: Vec::new(),
            runtime_program: u.empty_list(),
            diagnostics,
        },
    }
}

fn stopped(u: &mut Universe, diagnostics: Vec<TermId>) -> Compile {
    outputs(u, None, diagnostics)
}

/// One phase line on stderr: the folder name, the measured milliseconds, the
/// row count that left the phase and its diagnostics.
fn phase_event(name: &'static str, start: Instant, rows: usize, diagnostics: usize) {
    tracing::info!(
        target: "dl8::phase",
        name,
        ms = start.elapsed().as_millis() as u64,
        rows,
        diagnostics,
    );
}

/// `:112`. Prelude, macro library, program, then the units chain.
pub fn compile(u: &mut Universe, path: &Path, fx: &mut dyn FnMut(Event)) -> Result<Compile, Stop> {
    let read_start = Instant::now();
    let read_span = tracing::info_span!("phase", name = "read");
    let (prelude, program) = {
        let _entered = read_span.enter();
        let (canonical, text) = program_text(path).map_err(|e| Stop::Io(format!("{e}")))?;
        let prelude = prelude_unit(u, &prelude_text())?;
        let program = file_unit(u, &canonical, &text)?;
        (prelude, program)
    };
    let mut reader_diagnostics = prelude.diagnostics;
    reader_diagnostics.extend(program.diagnostics);
    phase_event("read", read_start, 2, reader_diagnostics.len());
    if !reader_diagnostics.is_empty() {
        return Ok(stopped(u, reader_diagnostics));
    }
    let macro_start = Instant::now();
    let macro_span = tracing::info_span!("phase", name = "macrotime");
    let macro_result = {
        let _entered = macro_span.enter();
        expanded_units(u, &[program.unit], fx)
    };
    let macro_rows = match &macro_result {
        Ok(Ok(expanded)) => expanded.len(),
        _ => 0,
    };
    let macro_diagnostics = match &macro_result {
        Ok(Err(diagnostics)) => diagnostics.len(),
        _ => 0,
    };
    phase_event("macrotime", macro_start, macro_rows, macro_diagnostics);
    let expanded = match macro_result? {
        Ok(expanded) => expanded,
        Err(diagnostics) => return Ok(stopped(u, diagnostics)),
    };
    let mut units = vec![prelude.unit];
    units.extend(expanded);
    finish_compile(u, &units, None, fx)
}

/// `:195` and `:227`. The project term carries the EXPANDED units.
pub fn compile_project(
    u: &mut Universe,
    root: &Path,
    paths: &[&Path],
    streams: &[&Path],
    fx: &mut dyn FnMut(Event),
) -> Result<Compile, Stop> {
    let read_start = Instant::now();
    let read_span = tracing::info_span!("phase", name = "read");
    let (prelude, loaded, tsi) = {
        let _entered = read_span.enter();
        let prelude = prelude_unit(u, &prelude_text())?;
        let loaded = load_dl7_project(u, root, paths)?.map_err(Stop::Io)?;
        let tsi = load_tsi_streams(u, streams).map_err(Stop::Io)?;
        (prelude, loaded, tsi)
    };
    let (tsi_rows, tsi_diagnostics) = tsi;
    let mut reader_diagnostics = prelude.diagnostics;
    reader_diagnostics.extend(loaded.diagnostics);
    reader_diagnostics.extend(tsi_diagnostics);
    phase_event("read", read_start, 2, reader_diagnostics.len());
    if !reader_diagnostics.is_empty() {
        return Ok(stopped(u, reader_diagnostics));
    }
    let macro_start = Instant::now();
    let macro_span = tracing::info_span!("phase", name = "macrotime");
    let macro_result = {
        let _entered = macro_span.enter();
        expanded_units(u, &loaded.units, fx)
    };
    let macro_rows = match &macro_result {
        Ok(Ok(expanded)) => expanded.len(),
        _ => 0,
    };
    let macro_diagnostics = match &macro_result {
        Ok(Err(diagnostics)) => diagnostics.len(),
        _ => 0,
    };
    phase_event("macrotime", macro_start, macro_rows, macro_diagnostics);
    let expanded = match macro_result? {
        Ok(expanded) => expanded,
        Err(diagnostics) => return Ok(stopped(u, diagnostics)),
    };
    let Some(("dl7_project", args)) = u.functor(loaded.project).map(|(n, a)| (n, a.to_vec()))
    else {
        return Err(Stop::Io("project is not dl7_project/2".into()));
    };
    let unit_list = u.list(&expanded);
    let project = Project {
        project: u.compound("dl7_project", vec![args[0], unit_list]),
        tsi_rows,
        cwd: std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    };
    let mut units = vec![prelude.unit];
    units.extend(expanded);
    finish_compile(u, &units, Some(project), fx)
}

/// The lower, check and comptime phases with their own span and event each.
fn finish_compile(
    u: &mut Universe,
    units: &[TermId],
    project: Option<Project>,
    fx: &mut dyn FnMut(Event),
) -> Result<Compile, Stop> {
    let lower_start = Instant::now();
    let lower_span = tracing::info_span!("phase", name = "lower");
    let lowered = {
        let _entered = lower_span.enter();
        let environment = project.as_ref().map(|project| {
            let owners = source_unit_module_owners(u, units);
            tsi_expression_environment(u, &project.tsi_rows, &owners)
        });
        let lowered = lower_compiler_units(u, CallPolicy::DeferUnknownCalls, units, environment)?;
        if !lowered.diagnostics.is_empty() {
            lowered
        } else {
            match project.as_ref() {
                None => lowered,
                Some(project) => {
                    let (basements, origins, diagnostics) =
                        install_graphs(u, project, &lowered.basements, &lowered.origins)
                            .map_err(Stop::Load)?;
                    if diagnostics.is_empty() {
                        Units {
                            basements,
                            origins,
                            diagnostics: Vec::new(),
                        }
                    } else {
                        Units {
                            basements: Vec::new(),
                            origins: Vec::new(),
                            diagnostics,
                        }
                    }
                }
            }
        }
    };
    phase_event(
        "lower",
        lower_start,
        lowered.basements.len(),
        lowered.diagnostics.len(),
    );
    if !lowered.diagnostics.is_empty() {
        return Ok(stopped(u, lowered.diagnostics));
    }

    let (basement, origins) = merge_module_basements(u, &lowered.basements, &lowered.origins);
    let merged = Lowered {
        program: Some(basement),
        origins,
        diagnostics: Vec::new(),
    };
    let check_start = Instant::now();
    let check_span = tracing::info_span!("phase", name = "check");
    let (checked, diagnostics) = {
        let _entered = check_span.enter();
        check_datalog(u, &merged)?
    };
    let check_rows = checked
        .as_ref()
        .map(|checked| checked.rules.len())
        .unwrap_or(0);
    phase_event("check", check_start, check_rows, diagnostics.len());
    let Some(checked) = checked else {
        return Ok(stopped(u, diagnostics));
    };
    if !diagnostics.is_empty() {
        return Ok(stopped(u, diagnostics));
    }

    let mut sources = Live {
        derived_bind_slots: derived_bind_slots(u, &checked.rules),
        units: units.to_vec(),
        project,
    };
    let comptime_start = Instant::now();
    let comptime_span = tracing::info_span!("phase", name = "comptime");
    let (compiled, diagnostics) = {
        let _entered = comptime_span.enter();
        evaluate_checked(u, &checked, &mut sources, &mut |r| fx(Event::Round(r)))?
    };
    let comptime_rows = compiled
        .as_ref()
        .map(|compiled| compiled.compiler_facts.len())
        .unwrap_or(0);
    phase_event("comptime", comptime_start, comptime_rows, diagnostics.len());
    Ok(outputs(u, compiled, diagnostics))
}

/// `:387`. The macro library compiles once, then rewrites every source unit.
fn expanded_units(
    u: &mut Universe,
    units: &[TermId],
    fx: &mut dyn FnMut(Event),
) -> Result<Result<Vec<TermId>, Vec<TermId>>, Stop> {
    let (macro_program, diagnostics) = standard_macro_program(u, &mut |r| fx(Event::Round(r)))?;
    let Some(macro_program) = macro_program else {
        return Ok(Err(diagnostics));
    };
    if !diagnostics.is_empty() {
        return Ok(Err(diagnostics));
    }
    let (expanded, diagnostics) =
        expand_units_with_macros(u, units, &macro_program, &mut |w| fx(Event::Wave(w)));
    if !diagnostics.is_empty() {
        return Ok(Err(diagnostics));
    }
    Ok(Ok(expanded))
}
