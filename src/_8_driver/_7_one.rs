//! The process engine of `DL8_ONE_ENGINE=1`: one install holds the macro library's comptime,
//! the macrotime cone and the stdlib comptime rules; every program compiled with it reuses them.

use super::_0_read::{macrotime_text, prelude_text};
use super::_1_unit::{macrotime_unit, prelude_unit};
use super::_3_units::compile_units_attached;
use super::Stop;
use crate::_1_macrotime::_4_expand::macro_cone;
use crate::_1_macrotime::{slice_macro_program, MacroProgram, MacroSession};
use crate::_2_lower::api::Lowered as LoweredUnits;
use crate::_2_lower::cx::CallPolicy;
use crate::_2_lower::units::{lower_compiler_units, merge_module_basements};
use crate::_3_check::api::prolog_sort;
use crate::_3_check::Checked;
use crate::_4_comptime::api::{colon_rows, graph_seeds, intern_rows, source_application_edges};
use crate::_4_comptime::rounds::{compiler_round_seeds, round_program};
use crate::_4_comptime::Round;
use crate::_6_eval::ir_eval::{IrSession, Pending, Pool, SharedPool};
use crate::_6_eval::ir_lower::Declared;
use crate::_6_eval::one_engine::{Attach, Stdlib};
use crate::_6_eval::term::{TermId, Universe};
use std::cell::RefCell;
use std::rc::Rc;

/// Holds `TermId`s of one `Universe`: one per universe.
#[derive(Default)]
pub struct OneEngine {
    pub pool: SharedPool,
    pub macro_program: Option<MacroProgram>,
    pub macro_session: Option<MacroSession>,
    pub stdlib: Option<Rc<RefCell<Stdlib>>>,
    /// The A1 lower and check keeps, one per process engine.
    pub lower: crate::_2_lower::keep::LowerKeep,
    pub check: crate::_3_check::keep::CheckKeep,
}

impl OneEngine {
    pub fn new() -> Self {
        Self {
            pool: Rc::new(RefCell::new(Pool::default())),
            ..Self::default()
        }
    }

    /// The comptime round loops of a program compiled with this engine attach here.
    pub fn attach(&self) -> Option<Attach> {
        self.stdlib.clone().map(Attach::Stdlib)
    }
}

/// `unit` lowered and checked alone, or its diagnostics.
fn checked_alone(
    u: &mut Universe,
    keep: crate::_4_comptime::sources::Keep,
    unit: TermId,
) -> Result<Result<Checked, Vec<TermId>>, Stop> {
    let lowered = lower_compiler_units(
        u,
        CallPolicy::DeferUnknownCalls,
        &[unit],
        None,
        &[],
        Some(keep.lower),
    )?;
    if !lowered.diagnostics.is_empty() {
        return Ok(Err(lowered.diagnostics));
    }
    let (basement, origins) = merge_module_basements(u, &lowered.basements, &lowered.origins);
    let merged = LoweredUnits {
        program: Some(basement),
        origins,
        diagnostics: Vec::new(),
    };
    let (checked, diagnostics) =
        crate::_3_check::keep::check_units(u, Some(keep.check), &merged, None)?;
    match checked {
        Some(checked) if diagnostics.is_empty() => Ok(Ok(checked)),
        _ => Ok(Err(diagnostics)),
    }
}

/// The macro program and its rules in a canonical order, for set equality.
fn rule_set(program: &MacroProgram) -> (Vec<String>, Vec<String>, Vec<TermId>, Vec<TermId>) {
    let mut rules: Vec<String> = program.program.rules.iter().map(|rule| format!("{rule:?}")).collect();
    let mut seeds: Vec<String> = program.program.seeds.iter().map(|row| format!("{row:?}")).collect();
    rules.sort();
    seeds.sort();
    let (mut edges, mut relations) = (program.edges.clone(), program.relations.clone());
    edges.sort_by_key(|id| id.0);
    relations.sort_by_key(|id| id.0);
    (rules, seeds, edges, relations)
}

/// The stdlib program: the prelude's checked rules with its round-1 seeds.
fn stdlib_program(
    u: &mut Universe,
    keep: crate::_4_comptime::sources::Keep,
) -> Result<Result<crate::_6_eval::Program, Vec<TermId>>, Stop> {
    let prelude = prelude_unit(u, &prelude_text())?;
    if !prelude.diagnostics.is_empty() {
        return Ok(Err(prelude.diagnostics));
    }
    let checked = match checked_alone(u, keep, prelude.unit)? {
        Ok(checked) => checked,
        Err(diagnostics) => return Ok(Err(diagnostics)),
    };
    let mut base = graph_seeds(u, &checked.nodes, &checked.edges);
    base.extend_from_slice(&checked.seeds);
    let base = prolog_sort(u, base);
    let mut edges = colon_rows(u, &base);
    edges.extend(source_application_edges(u, &checked.rules));
    let edges = prolog_sort(u, edges);
    let requests = intern_rows(u, &base);
    let seeds = compiler_round_seeds(u, &base, &edges, &requests, &[]);
    let program = round_program(u, &checked.rules, &seeds)?;
    Ok(Ok(Stdlib::program(u, program.rules, &program.seeds)))
}

/// The macro program of `one`, building the process engine on first use: the macrotime cone and
/// the stdlib program lowered, then installed with the macro library's first comptime round.
pub fn one_macro_program(
    u: &mut Universe,
    one: &mut OneEngine,
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<MacroProgram>, Vec<TermId>), Stop> {
    if let Some(program) = &one.macro_program {
        return Ok((Some(program.clone()), Vec::new()));
    }
    let unit = macrotime_unit(u, &macrotime_text())?;
    if !unit.diagnostics.is_empty() {
        return Ok((None, unit.diagnostics));
    }
    let keep = crate::_4_comptime::sources::Keep {
        lower: &one.lower,
        check: &one.check,
    };
    let checked = match checked_alone(u, keep, unit.unit)? {
        Ok(checked) => checked,
        Err(diagnostics) => return Ok((None, diagnostics)),
    };
    let (Some(expected), diagnostics) = slice_macro_program(u, &checked) else {
        return Ok((None, slice_macro_program(u, &checked).1));
    };
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }
    let stdlib = match stdlib_program(u, keep)? {
        Ok(program) => program,
        Err(diagnostics) => return Ok((None, diagnostics)),
    };
    let lowered_stdlib = Pending::lower(u, "p3.", &stdlib, &Declared::default()).map_err(|d| Stop::Load(format!("one engine stdlib lowering {}", u.display(d.payload))))?;
    let mut pending = Vec::new();
    if let Some((cone, declared)) = macro_cone(u, &expected) {
        pending.push(Pending::lower(u, "p2.", &cone, &declared).map_err(|d| Stop::Load(format!("one engine macro cone lowering {}", u.display(d.payload))))?);
    }
    pending.push(lowered_stdlib);
    one.pool.borrow_mut().pending = pending;
    let (compiled, diagnostics) = compile_units_attached(
        u,
        &[unit.unit],
        Some(Attach::Pool(one.pool.clone())),
        Some(keep),
        fx,
    )?;
    let Some(compiled) = compiled else {
        return Ok((None, diagnostics));
    };
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }
    let (Some(sliced), diagnostics) = slice_macro_program(u, &compiled.runtime) else {
        return Ok((None, slice_macro_program(u, &compiled.runtime).1));
    };
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }
    let program = if rule_set(&sliced) == rule_set(&expected) {
        expected
    } else {
        tracing::info!(target: "dl8::one", "one engine macro mismatch");
        sliced
    };
    if !one.pool.borrow().pending.is_empty() {
        tracing::info!(target: "dl8::one", "one engine pool never installed");
        one.pool.borrow_mut().pending.clear();
    }
    let session = IrSession::configured().with_pool(one.pool.clone());
    one.macro_session = Some(MacroSession::with_ir(session));
    let stdlib = Stdlib::adopt(u, one.pool.clone(), &stdlib).map_err(|d| Stop::Load(format!("one engine stdlib part {}", u.display(d.payload))))?;
    one.stdlib = Some(Rc::new(RefCell::new(stdlib)));
    one.macro_program = Some(program.clone());
    Ok((Some(program), Vec::new()))
}
