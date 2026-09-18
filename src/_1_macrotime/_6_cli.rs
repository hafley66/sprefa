//! `dl8 expand <case.json>`: read `input` from the case file, run reify,
//! expand and materialize, print every result as JSON terms. Exit 1 when any
//! diagnostic list is non-empty, 2 when the case file cannot be read.

use super::_0_rows::{rows_of_terms, terms_of_rows};
use super::_1_reify::reify_terms;
use super::_2_protocol::MacroProgram;
use super::_4_expand::{expand_terms, Wave};
use super::_5_materialize::materialize_terms;
use crate::_6_eval::json::{program_from_json, term_from_json, term_to_json};
use crate::_6_eval::{Row, TermId, Universe};
use crate::_8_driver::_1_unit::text_unit;
use crate::_8_driver::_2_macro::standard_macro_program;
use serde_json::{json, Map, Value};
use std::path::Path;
use std::process::ExitCode;

pub struct Expansion {
    pub syntax_rows: Vec<Row>,
    pub expanded_rows: Vec<Row>,
    pub origin_rows: Vec<Row>,
    pub diagnostics: Vec<TermId>,
    pub forms: Vec<TermId>,
    pub source_rows: Vec<TermId>,
    pub materialize_diagnostics: Vec<TermId>,
}

/// The order `v7/src/2_comptime/2_compiler.pl` runs the three operators in: a
/// failing stage stops the ones after it.
pub fn run(
    u: &mut Universe,
    forms: &[TermId],
    source_rows: &[TermId],
    macro_program: &MacroProgram,
    fx: &mut dyn FnMut(Wave),
) -> Expansion {
    let (syntax, reify_diagnostics) = reify_terms(u, forms, source_rows);
    if !reify_diagnostics.is_empty() {
        return Expansion {
            syntax_rows: rows_of_terms(u, &syntax),
            expanded_rows: Vec::new(),
            origin_rows: Vec::new(),
            diagnostics: reify_diagnostics,
            forms: Vec::new(),
            source_rows: Vec::new(),
            materialize_diagnostics: Vec::new(),
        };
    }
    let (expanded, origin, diagnostics) = expand_terms(u, syntax.clone(), macro_program, fx);
    if !diagnostics.is_empty() {
        return Expansion {
            syntax_rows: rows_of_terms(u, &syntax),
            expanded_rows: rows_of_terms(u, &expanded),
            origin_rows: rows_of_terms(u, &origin),
            diagnostics,
            forms: Vec::new(),
            source_rows: Vec::new(),
            materialize_diagnostics: Vec::new(),
        };
    }
    let (reader_forms, reader_sources, materialize_diagnostics) =
        materialize_terms(u, expanded.clone());
    Expansion {
        syntax_rows: rows_of_terms(u, &syntax),
        expanded_rows: rows_of_terms(u, &expanded),
        origin_rows: rows_of_terms(u, &origin),
        diagnostics: Vec::new(),
        forms: reader_forms,
        source_rows: reader_sources,
        materialize_diagnostics,
    }
}

fn terms_from_json(u: &mut Universe, value: Option<&Value>) -> Result<Vec<TermId>, String> {
    let items = value
        .and_then(|v| v.as_array())
        .ok_or_else(|| "expected a JSON array of terms".to_string())?;
    items.iter().map(|v| term_from_json(u, v)).collect()
}

pub fn macro_program_from_json(u: &mut Universe, value: &Value) -> Result<MacroProgram, String> {
    let m = value
        .as_object()
        .ok_or_else(|| "macro_program is not an object".to_string())?;
    let edges = terms_from_json(u, m.get("edges"))?;
    let relations = terms_from_json(u, m.get("relations"))?;
    let program = program_from_json(
        u,
        &json!({
            "rules": m.get("rules").cloned().unwrap_or(Value::Array(Vec::new())),
            "seeds": m.get("seeds").cloned().unwrap_or(Value::Array(Vec::new())),
        }),
    )?;
    Ok(MacroProgram {
        edges,
        relations,
        program,
    })
}

fn rows_json(u: &mut Universe, rows: &[Row]) -> Value {
    let terms = terms_of_rows(u, rows);
    terms_json(u, &terms)
}

fn terms_json(u: &Universe, terms: &[TermId]) -> Value {
    Value::Array(terms.iter().map(|t| term_to_json(u, *t)).collect())
}

pub fn expansion_json(u: &mut Universe, e: &Expansion) -> Value {
    let mut m = Map::new();
    m.insert("syntax_rows".into(), rows_json(u, &e.syntax_rows));
    m.insert("expanded_rows".into(), rows_json(u, &e.expanded_rows));
    m.insert("origin_rows".into(), rows_json(u, &e.origin_rows));
    m.insert("diagnostics".into(), terms_json(u, &e.diagnostics));
    m.insert("forms".into(), terms_json(u, &e.forms));
    m.insert("source_rows".into(), terms_json(u, &e.source_rows));
    m.insert(
        "materialize_diagnostics".into(),
        terms_json(u, &e.materialize_diagnostics),
    );
    Value::Object(m)
}

/// A case's forms and source rows: frozen JSON terms, or `source` text read
/// and reader-expanded under `path`, the way `dl8 compile` reads a unit.
fn case_syntax(
    u: &mut Universe,
    case: &Map<String, Value>,
) -> Result<(Vec<TermId>, Vec<TermId>), String> {
    let Some(text) = case.get("source").and_then(|s| s.as_str()) else {
        let forms = terms_from_json(u, case.get("forms"))?;
        let source_rows = terms_from_json(u, case.get("source_rows"))?;
        return Ok((forms, source_rows));
    };
    let path = case
        .get("path")
        .and_then(|p| p.as_str())
        .unwrap_or("source");
    let origin = u.atom(path);
    let unit = text_unit(u, origin, path, text).map_err(|e| format!("{e:?}"))?;
    if !unit.diagnostics.is_empty() {
        let d: Vec<Value> = unit
            .diagnostics
            .iter()
            .map(|d| term_to_json(u, *d))
            .collect();
        return Err(format!("reader diagnostics {}", Value::Array(d)));
    }
    let args = u.args::<5>(unit.unit, "dl7_unit").ok_or("no dl7_unit")?;
    let forms = u.as_list(args[2]).unwrap_or_default();
    let source_rows = u.as_list(args[3]).unwrap_or_default();
    Ok((forms, source_rows))
}

/// The case's frozen macro program, or the bundled `macrotime/*.dl7` library
/// when the case names none.
fn case_macro_program(u: &mut Universe, case: &Map<String, Value>) -> Result<MacroProgram, String> {
    if let Some(frozen) = case.get("macro_program") {
        return macro_program_from_json(u, frozen);
    }
    match standard_macro_program(u, &mut |_| {}) {
        Ok((Some(program), diagnostics)) if diagnostics.is_empty() => Ok(program),
        Ok((_, diagnostics)) => {
            let d: Vec<Value> = diagnostics.iter().map(|d| term_to_json(u, *d)).collect();
            Err(format!("macro library diagnostics {}", Value::Array(d)))
        }
        Err(e) => Err(format!("{e:?}")),
    }
}

/// `input` is a JSON case, or a `.dl7` file expanded through the bundled
/// library under its own path.
pub fn cli(input: &Path, trace: bool) -> ExitCode {
    let text = match std::fs::read_to_string(input) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(phase = "macrotime", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    let case = if input.extension().is_some_and(|x| x == "dl7") {
        let mut m = Map::new();
        m.insert("path".into(), Value::String(input.display().to_string()));
        m.insert("source".into(), Value::String(text));
        m
    } else {
        let value: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(phase = "macrotime", error = %e, path = %input.display());
                return ExitCode::from(2);
            }
        };
        match value.get("input").unwrap_or(&value).as_object() {
            Some(m) => m.clone(),
            None => {
                tracing::error!(phase = "macrotime", error = "no input object", path = %input.display());
                return ExitCode::from(2);
            }
        }
    };
    let mut u = Universe::new();
    let (forms, source_rows) = match case_syntax(&mut u, &case) {
        Ok(syntax) => syntax,
        Err(e) => {
            tracing::error!(phase = "macrotime", error = %e, field = "forms");
            return ExitCode::from(2);
        }
    };
    let macro_program = match case_macro_program(&mut u, &case) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(phase = "macrotime", error = %e, field = "macro_program");
            return ExitCode::from(2);
        }
    };
    let mut sink = |w: Wave| {
        if trace {
            tracing::debug!(target: "dl8::trace", wave = ?w);
        }
    };
    let expansion = run(&mut u, &forms, &source_rows, &macro_program, &mut sink);
    let out = expansion_json(&mut u, &expansion);
    println!("{}", serde_json::to_string(&out).unwrap());
    if expansion.diagnostics.is_empty() && expansion.materialize_diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
