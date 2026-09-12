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

pub fn cli(input: &Path, trace: bool) -> ExitCode {
    let text = match std::fs::read_to_string(input) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(phase = "macrotime", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    let value: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(phase = "macrotime", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    let case = match value.get("input").unwrap_or(&value).as_object() {
        Some(m) => m.clone(),
        None => {
            tracing::error!(phase = "macrotime", error = "no input object", path = %input.display());
            return ExitCode::from(2);
        }
    };
    let mut u = Universe::new();
    let forms = match terms_from_json(&mut u, case.get("forms")) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(phase = "macrotime", error = %e, field = "forms");
            return ExitCode::from(2);
        }
    };
    let source_rows = match terms_from_json(&mut u, case.get("source_rows")) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(phase = "macrotime", error = %e, field = "source_rows");
            return ExitCode::from(2);
        }
    };
    let empty = json!({});
    let macro_program =
        match macro_program_from_json(&mut u, case.get("macro_program").unwrap_or(&empty)) {
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
