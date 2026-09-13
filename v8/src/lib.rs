//! dl8: the DL7 language, compiled in Rust. One `_<n>_name` folder per
//! operator, `_6_eval` the shared fixpoint kernel, `compile` the chain.

pub mod _0_read;
pub mod _1_macrotime;
pub mod _2_lower;
pub mod _3_check;
pub mod _4_comptime;
pub mod _5_reify;
pub mod _6_eval;
pub mod _7_effect;
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
use _7_effect::Slice;
use _8_driver::_0_read::{prelude_text, program_text};
use _8_driver::_1_unit::{file_unit, prelude_unit, Unit};
use _8_driver::_2_macro::{expand_units_with_macros, standard_macro_program};
use _8_driver::_4_project::{install_graphs, load_dl7_project, load_tsi_streams, Loaded, Project};
use _8_driver::{Event, Stop};
use std::marker::PhantomData;
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

/// `:112`. The whole pipe as a reducer over the term universe.
pub struct Drive<'a>(PhantomData<&'a ()>);

impl<'a> Slice for Drive<'a> {
    type State = Universe;
    type Event = &'a Path;
    type Output = Result<Compile, Stop>;
    type Effect = Event;

    fn reduce(
        u: &mut Universe,
        path: &'a Path,
        fx: &mut dyn FnMut(Event),
    ) -> Result<Compile, Stop> {
        compile(u, path, fx)
    }
}

/// One phase inside its own span, then its stderr line. A phase that stops the
/// pipe reports nothing; `finish_compile` never reaches its own event either.
fn phase<T>(
    name: &'static str,
    run: impl FnOnce() -> Result<T, Stop>,
    counts: impl FnOnce(&T) -> (usize, usize),
) -> Result<T, Stop> {
    let start = Instant::now();
    let span = tracing::info_span!("phase", name);
    let out = {
        let _entered = span.enter();
        run()
    }?;
    let (rows, diagnostics) = counts(&out);
    phase_event(name, start, rows, diagnostics);
    Ok(out)
}

/// The macrotime phase of both doors. Its event fires even when the expansion
/// stops the pipe, so the phase line always names the units it saw.
fn macro_phase(
    u: &mut Universe,
    units: &[TermId],
    fx: &mut dyn FnMut(Event),
) -> Result<Result<Vec<TermId>, Vec<TermId>>, Stop> {
    let start = Instant::now();
    let span = tracing::info_span!("phase", name = "macrotime");
    let result = {
        let _entered = span.enter();
        expanded_units(u, units, fx)
    };
    let (rows, diagnostics) = match &result {
        Ok(Ok(expanded)) => (expanded.len(), 0),
        Ok(Err(diagnostics)) => (0, diagnostics.len()),
        Err(_) => (0, 0),
    };
    phase_event("macrotime", start, rows, diagnostics);
    result
}

/// `:112`. Prelude, macro library, program, then the units chain.
pub fn compile(u: &mut Universe, path: &Path, fx: &mut dyn FnMut(Event)) -> Result<Compile, Stop> {
    let (prelude, program) = phase(
        "read",
        || {
            let (canonical, text) = program_text(path).map_err(|e| Stop::Io(format!("{e}")))?;
            let prelude = prelude_unit(u, &prelude_text())?;
            let program = file_unit(u, &canonical, &text)?;
            Ok((prelude, program))
        },
        |(prelude, program): &(Unit, Unit)| {
            (2, prelude.diagnostics.len() + program.diagnostics.len())
        },
    )?;
    let mut reader_diagnostics = prelude.diagnostics;
    reader_diagnostics.extend(program.diagnostics);
    if !reader_diagnostics.is_empty() {
        return Ok(stopped(u, reader_diagnostics));
    }
    let expanded = match macro_phase(u, &[program.unit], fx)? {
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
    let (prelude, loaded, tsi_rows, tsi_diagnostics) = phase(
        "read",
        || {
            let prelude = prelude_unit(u, &prelude_text())?;
            let loaded = load_dl7_project(u, root, paths)?.map_err(Stop::Io)?;
            let (tsi_rows, tsi_diagnostics) = load_tsi_streams(u, streams).map_err(Stop::Io)?;
            Ok((prelude, loaded, tsi_rows, tsi_diagnostics))
        },
        |(prelude, loaded, _, tsi_diagnostics): &(Unit, Loaded, Vec<TermId>, Vec<TermId>)| {
            let count =
                prelude.diagnostics.len() + loaded.diagnostics.len() + tsi_diagnostics.len();
            (2, count)
        },
    )?;
    let mut reader_diagnostics = prelude.diagnostics;
    reader_diagnostics.extend(loaded.diagnostics);
    reader_diagnostics.extend(tsi_diagnostics);
    if !reader_diagnostics.is_empty() {
        return Ok(stopped(u, reader_diagnostics));
    }
    let expanded = match macro_phase(u, &loaded.units, fx)? {
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
    let lowered = phase(
        "lower",
        || lower_units(u, units, project.as_ref()),
        |lowered: &Units| (lowered.basements.len(), lowered.diagnostics.len()),
    )?;
    if !lowered.diagnostics.is_empty() {
        return Ok(stopped(u, lowered.diagnostics));
    }

    let (basement, origins) = merge_module_basements(u, &lowered.basements, &lowered.origins);
    let merged = Lowered {
        program: Some(basement),
        origins,
        diagnostics: Vec::new(),
    };
    let (checked, diagnostics) = phase(
        "check",
        || check_datalog(u, &merged).map_err(Stop::from),
        |(checked, diagnostics)| {
            let rows = checked.as_ref().map(|c| c.rules.len()).unwrap_or(0);
            (rows, diagnostics.len())
        },
    )?;
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
    let (compiled, diagnostics) = phase(
        "comptime",
        || {
            evaluate_checked(u, &checked, &mut sources, &mut |r| fx(Event::Round(r)))
                .map_err(Stop::from)
        },
        |(compiled, diagnostics)| {
            let rows = compiled
                .as_ref()
                .map(|c: &Compiled| c.compiler_facts.len())
                .unwrap_or(0);
            (rows, diagnostics.len())
        },
    )?;
    Ok(outputs(u, compiled, diagnostics))
}

/// `:290`. The project door installs its filesystem graphs over the lowered
/// basements; a diagnostic from either step empties the result.
fn lower_units(
    u: &mut Universe,
    units: &[TermId],
    project: Option<&Project>,
) -> Result<Units, Stop> {
    let environment = project.map(|project| {
        let owners = source_unit_module_owners(u, units);
        tsi_expression_environment(u, &project.tsi_rows, &owners)
    });
    let lowered = lower_compiler_units(u, CallPolicy::DeferUnknownCalls, units, environment)?;
    let Some(project) = project else {
        return Ok(lowered);
    };
    if !lowered.diagnostics.is_empty() {
        return Ok(lowered);
    }
    let (basements, origins, diagnostics) =
        install_graphs(u, project, &lowered.basements, &lowered.origins).map_err(Stop::Load)?;
    Ok(if diagnostics.is_empty() {
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
    })
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
