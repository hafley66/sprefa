//! `dl8 read <file>`: one JSON object, encoded exactly like `json_terms.pl`.

use super::reader::read;
use crate::_6_eval::json::term_to_json;
use crate::_6_eval::{TermId, Universe};
use serde_json::{Map, Value};
use std::path::Path;
use std::process::ExitCode;

pub fn cli(input: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => {
            tracing::error!(phase = "read", error = %e, path = %input.display());
            return ExitCode::from(2);
        }
    };
    let path = std::env::var("DL8_READ_PATH").unwrap_or_else(|_| input.display().to_string());
    let mut u = Universe::new();
    let result = read(&mut u, &path, &text);
    let encode = |ids: &[TermId]| Value::Array(ids.iter().map(|t| term_to_json(&u, *t)).collect());
    let mut out = Map::new();
    out.insert("forms".into(), encode(&result.forms));
    out.insert("source_rows".into(), encode(&result.source_rows));
    out.insert("diagnostics".into(), encode(&result.diagnostics));
    println!(
        "{}",
        serde_json::to_string_pretty(&Value::Object(out)).unwrap()
    );
    if result.diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
