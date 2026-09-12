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

use _4_comptime::Compiled;
use _6_eval::term::{TermId, Universe};
use _8_driver::_0_read::{prelude_text, program_text};
use _8_driver::_1_unit::{file_unit, prelude_unit};
use _8_driver::_2_macro::{expand_units_with_macros, standard_macro_program};
use _8_driver::_3_units::{compile_project_units, compile_units};
use _8_driver::_4_project::{load_dl7_project, load_tsi_streams, Project};
use _8_driver::{Event, Stop};
use std::path::Path;

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

/// `:112`. Prelude, macro library, program, then the units chain.
pub fn compile(u: &mut Universe, path: &Path, fx: &mut dyn FnMut(Event)) -> Result<Compile, Stop> {
    let (canonical, text) = program_text(path).map_err(|e| Stop::Io(format!("{e}")))?;
    let prelude = prelude_unit(u, &prelude_text())?;
    let program = file_unit(u, &canonical, &text)?;
    let mut reader_diagnostics = prelude.diagnostics;
    reader_diagnostics.extend(program.diagnostics);
    if !reader_diagnostics.is_empty() {
        return Ok(stopped(u, reader_diagnostics));
    }
    let expanded = match expanded_units(u, &[program.unit], fx)? {
        Ok(expanded) => expanded,
        Err(diagnostics) => return Ok(stopped(u, diagnostics)),
    };
    let mut units = vec![prelude.unit];
    units.extend(expanded);
    let (compiled, diagnostics) = compile_units(u, &units, &mut |r| fx(Event::Round(r)))?;
    Ok(outputs(u, compiled, diagnostics))
}

/// `:195` and `:227`. The project term carries the EXPANDED units.
pub fn compile_project(
    u: &mut Universe,
    root: &Path,
    paths: &[&Path],
    streams: &[&Path],
    fx: &mut dyn FnMut(Event),
) -> Result<Compile, Stop> {
    let prelude = prelude_unit(u, &prelude_text())?;
    let loaded = load_dl7_project(u, root, paths)?.map_err(Stop::Io)?;
    let (tsi_rows, tsi_diagnostics) = load_tsi_streams(u, streams).map_err(Stop::Io)?;
    let mut reader_diagnostics = prelude.diagnostics;
    reader_diagnostics.extend(loaded.diagnostics);
    reader_diagnostics.extend(tsi_diagnostics);
    if !reader_diagnostics.is_empty() {
        return Ok(stopped(u, reader_diagnostics));
    }
    let expanded = match expanded_units(u, &loaded.units, fx)? {
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
    let (compiled, diagnostics) =
        compile_project_units(u, project, &units, &mut |r| fx(Event::Round(r)))?;
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
