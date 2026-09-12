//! The `dl8 load` case transport. One JSON object per frozen v7 call; terms
//! use the `_6_eval::json` encoding, which is what
//! `v8/oracle/eval/json_terms.pl` writes.

use super::api::{Installed, Loaded};
use super::project::install_project_graph;
use super::source::{install_source_fact_graph, load_source_fact_texts};
use super::tsi::{install_tsi_graph, tsi_expression_environment};
use super::wire::{accepted_rows, load_tsi_lines};
use crate::_6_eval::json::{term_from_json, term_to_json};
use crate::_6_eval::term::{TermId, Universe};
use serde_json::{json, Value};

/// Case paths are frozen with the repository root rewritten to this token, so
/// the same bytes mean the same thing on any machine.
pub const CASE_ROOT: &str = "/CASEROOT";

pub fn run(case: &Value) -> Result<Value, String> {
    let input = case.get("input").ok_or("case without input")?;
    let call = input
        .get("call")
        .and_then(|c| c.as_str())
        .ok_or("case without call")?;
    let mut u = Universe::new();
    match call {
        "install_project_graph" => {
            let project = term(&mut u, input, "project")?;
            let basements = terms(&mut u, input, "basements")?;
            let origins = terms(&mut u, input, "origins")?;
            let installed =
                install_project_graph(&mut u, CASE_ROOT, project, &basements, &origins)?;
            Ok(encode_installed(&u, &installed))
        }
        "load_tsi_lines" => {
            let origin = term(&mut u, input, "origin")?;
            let lines = lines(input, "lines")?;
            let loaded = load_tsi_lines(&mut u, origin, &lines);
            Ok(encode_loaded(&u, &loaded))
        }
        "accepted_rows" => {
            let rows = terms(&mut u, input, "rows")?;
            let accepted = accepted_rows(&u, &rows);
            Ok(json!({
                "accepted": encode_list(&u, &accepted),
                "diagnostics": Value::Array(vec![]),
            }))
        }
        "install_tsi_graph" => {
            let rows = terms(&mut u, input, "rows")?;
            let basements = terms(&mut u, input, "basements")?;
            let origins = terms(&mut u, input, "origins")?;
            let installed = install_tsi_graph(&mut u, &rows, &basements, &origins);
            Ok(encode_installed(&u, &installed))
        }
        "tsi_expression_environment" => {
            let rows = terms(&mut u, input, "rows")?;
            let importers = terms(&mut u, input, "importers")?;
            let environment = tsi_expression_environment(&mut u, &rows, &importers);
            Ok(json!({
                "environment": term_to_json(&u, environment),
                "diagnostics": Value::Array(vec![]),
            }))
        }
        "load_source_fact_texts" => {
            let files = source_files(&mut u, input)?;
            let loaded = load_source_fact_texts(&mut u, &files);
            Ok(encode_loaded(&u, &loaded))
        }
        "install_source_fact_graph" => {
            let rows = terms(&mut u, input, "rows")?;
            let basements = terms(&mut u, input, "basements")?;
            let origins = terms(&mut u, input, "origins")?;
            let installed = install_source_fact_graph(&mut u, &rows, &basements, &origins)?;
            Ok(encode_installed(&u, &installed))
        }
        other => Err(format!("unknown call {other}")),
    }
}

fn term(u: &mut Universe, input: &Value, key: &str) -> Result<TermId, String> {
    term_from_json(u, input.get(key).ok_or(format!("case without {key}"))?)
}

fn terms(u: &mut Universe, input: &Value, key: &str) -> Result<Vec<TermId>, String> {
    let items = input
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or(format!("case without {key}"))?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(term_from_json(u, item)?);
    }
    Ok(out)
}

fn lines(input: &Value, key: &str) -> Result<Vec<String>, String> {
    let items = input
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or(format!("case without {key}"))?;
    items
        .iter()
        .map(|item| {
            // The dump writes every value through term_json, so a line is the
            // string term {"s": text}.
            item.get("s")
                .and_then(|s| s.as_str())
                .or_else(|| item.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "line is not a string".to_string())
        })
        .collect()
}

/// `file(Path, Text)` per entry; `unreadable` marks a file the dump could not
/// read, which the pure half never sees.
fn source_files(u: &mut Universe, input: &Value) -> Result<Vec<(TermId, String)>, String> {
    let items = input
        .get("files")
        .and_then(|v| v.as_array())
        .ok_or("case without files")?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let file = term_from_json(u, item)?;
        let parts = super::api::parts(u, file, "file", 2).ok_or("file is not file/2")?;
        let text = super::api::string_text(u, parts[1])
            .or_else(|| super::api::atom_text(u, parts[1]))
            .ok_or("file text is not text")?
            .to_string();
        out.push((parts[0], text));
    }
    Ok(out)
}

fn encode_list(u: &Universe, terms: &[TermId]) -> Value {
    Value::Array(terms.iter().map(|t| term_to_json(u, *t)).collect())
}

fn encode_loaded(u: &Universe, loaded: &Loaded) -> Value {
    json!({
        "rows": encode_list(u, &loaded.rows),
        "diagnostics": encode_list(u, &loaded.diagnostics),
    })
}

fn encode_installed(u: &Universe, installed: &Installed) -> Value {
    json!({
        "basements": encode_list(u, &installed.basements),
        "origins": encode_list(u, &installed.origins),
        "diagnostics": encode_list(u, &installed.diagnostics),
    })
}
