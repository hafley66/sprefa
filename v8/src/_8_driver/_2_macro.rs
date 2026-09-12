//! `standard_macro_program/3` (`2_compiler.pl:416`) and
//! `expand_units_with_macros/4` (`:456`).

use super::_0_read::macrotime_text;
use super::_1_unit::macrotime_unit;
use super::_3_units::compile_units;
use super::Stop;
use crate::_1_macrotime::_1_reify::reify_terms;
use crate::_1_macrotime::_4_expand::expand_terms;
use crate::_1_macrotime::_5_materialize::materialize_terms;
use crate::_1_macrotime::_7_slice::slice_macro_program;
use crate::_1_macrotime::{MacroProgram, Wave};
use crate::_4_comptime::Round;
use crate::_6_eval::term::{TermId, Universe};

/// `:416`. The macro library compiles through the ordinary compiler, then is
/// trimmed to the claim and output rule cone.
pub fn standard_macro_program(
    u: &mut Universe,
    fx: &mut dyn FnMut(Round),
) -> Result<(Option<MacroProgram>, Vec<TermId>), Stop> {
    let unit = macrotime_unit(u, &macrotime_text())?;
    if !unit.diagnostics.is_empty() {
        return Ok((None, unit.diagnostics));
    }
    let (compiled, diagnostics) = compile_units(u, &[unit.unit], fx)?;
    let Some(compiled) = compiled else {
        return Ok((None, diagnostics));
    };
    if !diagnostics.is_empty() {
        return Ok((None, diagnostics));
    }
    Ok(slice_macro_program(u, &compiled.runtime))
}

/// `:456`. One unit at a time, diagnostics concatenated.
pub fn expand_units_with_macros(
    u: &mut Universe,
    units: &[TermId],
    macro_program: &MacroProgram,
    fx: &mut dyn FnMut(Wave),
) -> (Vec<TermId>, Vec<TermId>) {
    let mut expanded = Vec::with_capacity(units.len());
    let mut diagnostics = Vec::new();
    for unit in units {
        let (one, unit_diagnostics) = expand_unit_with_macros(u, *unit, macro_program, fx);
        match one {
            Some(one) => expanded.push(one),
            None => return (Vec::new(), unit_diagnostics),
        }
        diagnostics.extend(unit_diagnostics);
    }
    (expanded, diagnostics)
}

/// `:491`. An expansion that rewrote nothing returns the unit untouched.
pub fn expand_unit_with_macros(
    u: &mut Universe,
    unit: TermId,
    macro_program: &MacroProgram,
    fx: &mut dyn FnMut(Wave),
) -> (Option<TermId>, Vec<TermId>) {
    let Some(("dl7_unit", args)) = u.functor(unit).map(|(n, a)| (n, a.to_vec())) else {
        let macrotime = u.atom("macrotime");
        let none = u.atom("none");
        let reason = u.compound("invalid_dl7_unit", vec![unit]);
        let diagnostic = u.compound("diagnostic", vec![macrotime, none, reason]);
        return (None, vec![diagnostic]);
    };
    let (origin, digest) = (args[0], args[1]);
    let forms = u.as_list(args[2]).unwrap_or_default();
    let source_rows = u.as_list(args[3]).unwrap_or_default();
    let expansion_rows = u.as_list(args[4]).unwrap_or_default();

    let (syntax, reify_diagnostics) = reify_terms(u, &forms, &source_rows);
    if !reify_diagnostics.is_empty() {
        return (None, reify_diagnostics);
    }
    let (rows, macro_origin, macro_diagnostics) = expand_terms(u, syntax, macro_program, fx);
    // :515. No diagnostic and no rewrite: the unit is its own expansion.
    if macro_diagnostics.is_empty() && macro_origin.is_empty() {
        return (Some(unit), Vec::new());
    }
    if !macro_diagnostics.is_empty() {
        return (None, macro_diagnostics);
    }
    let (forms, source_rows, materialize_diagnostics) = materialize_terms(u, rows);
    let mut all = expansion_rows;
    all.extend(macro_origin);
    let forms = u.list(&forms);
    let source_rows = u.list(&source_rows);
    let all = u.list(&all);
    (
        Some(u.compound("dl7_unit", vec![origin, digest, forms, source_rows, all])),
        materialize_diagnostics,
    )
}
