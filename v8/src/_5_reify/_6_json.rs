//! The `dl8 reify` transport. Terms use the `_6_eval::json` encoding, which is
//! byte-for-byte what `v8/oracle/eval/json_terms.pl` writes.
//!
//! Seven case shapes, tagged by `entry`, one per wrapped v7 predicate.

use super::api::{CompiledUnit, Stop};
use super::calls::{logical_program_calls, logical_program_rows_calls};
use super::emit::{compiler_view, emit_compiled, Emitter};
use super::graph::logical_program_graph_calls;
use super::rows::logical_program_rows_term;
use crate::_6_eval::json::{term_from_json, term_to_json};
use crate::_6_eval::term::{Term, TermId, Universe};
use serde_json::{json, Value};

fn stop(Stop::Fail(what): Stop) -> String {
    what.to_string()
}

fn term(u: &mut Universe, input: &Value, field: &str) -> Result<TermId, String> {
    let value = input.get(field).ok_or(format!("case without {field}"))?;
    term_from_json(u, value)
}

fn list(u: &mut Universe, input: &Value, field: &str) -> Result<Vec<TermId>, String> {
    let Some(Value::Array(items)) = input.get(field) else {
        return Err(format!("{field} is not a JSON array"));
    };
    items.iter().map(|i| term_from_json(u, i)).collect()
}

/// v7's atom `all` (`0_logical_program_reifier.pl:86`) is `None`.
fn relations(u: &mut Universe, input: &Value) -> Result<Option<Vec<TermId>>, String> {
    let value = input.get("relations").ok_or("case without relations")?;
    let term = term_from_json(u, value)?;
    if matches!(u.get(term), Term::Atom(s) if u.sym_str(*s) == "all") {
        return Ok(None);
    }
    u.as_list(term)
        .ok_or_else(|| "relations is neither all nor a list".into())
        .map(Some)
}

fn unit(u: &mut Universe, input: &Value) -> Result<CompiledUnit, String> {
    Ok(CompiledUnit {
        type_graph_facts: list(u, input, "type_graph_facts")?,
        runtime_program: term(u, input, "runtime_program")?,
        compiler_facts: list(u, input, "compiler_facts")?,
    })
}

fn terms(u: &Universe, rows: &[TermId]) -> Value {
    Value::Array(rows.iter().map(|r| term_to_json(u, *r)).collect())
}

pub fn run(case: &Value) -> Result<(Value, i32), String> {
    let input = case.get("input").ok_or("case without input")?;
    let entry = case
        .get("entry")
        .and_then(|e| e.as_str())
        .ok_or("case without entry")?;
    let mut u = Universe::new();
    match entry {
        "logical_program_rows" => {
            let checked = term(&mut u, input, "checked")?;
            let rows = logical_program_rows_term(&mut u, checked).map_err(stop)?;
            Ok((json!({ "rows": terms(&u, &rows) }), 0))
        }
        "logical_program_calls_4" | "logical_program_calls_5" => {
            let facts = list(&mut u, input, "compiler_facts")?;
            let checked = term(&mut u, input, "checked")?;
            let wanted = relations(&mut u, input)?;
            let calls =
                logical_program_calls(&mut u, &facts, checked, wanted.as_deref()).map_err(stop)?;
            let code = i32::from(!calls.diagnostics.is_empty());
            Ok((
                json!({
                    "calls": terms(&u, &calls.calls),
                    "diagnostics": terms(&u, &calls.diagnostics),
                }),
                code,
            ))
        }
        "logical_program_rows_calls" => {
            let facts = list(&mut u, input, "compiler_facts")?;
            let rows = list(&mut u, input, "rows")?;
            let wanted = relations(&mut u, input)?;
            let calls = logical_program_rows_calls(&mut u, &facts, &rows, wanted.as_deref())
                .map_err(stop)?;
            let code = i32::from(!calls.diagnostics.is_empty());
            Ok((
                json!({
                    "calls": terms(&u, &calls.calls),
                    "diagnostics": terms(&u, &calls.diagnostics),
                }),
                code,
            ))
        }
        "logical_program_graph_calls" => {
            let checked = term(&mut u, input, "checked")?;
            let wanted = relations(&mut u, input)?;
            let calls =
                logical_program_graph_calls(&mut u, checked, wanted.as_deref()).map_err(stop)?;
            Ok((json!({ "calls": terms(&u, &calls) }), 0))
        }
        "compiler_view" => {
            let unit = unit(&mut u, input)?;
            let view = compiler_view(&mut u, &unit).map_err(stop)?;
            Ok((
                json!({
                    "type_graph_facts": terms(&u, &view.type_graph_facts),
                    "compiler_facts": terms(&u, &view.compiler_facts),
                    "logical_program_rows": terms(&u, &view.logical_program_rows),
                    "runtime_program": term_to_json(&u, view.runtime_program),
                }),
                0,
            ))
        }
        "emit_compiled" => {
            let emitter = term(&mut u, input, "emitter")?;
            let unit = unit(&mut u, input)?;
            let emitter = Emitter::of(&u, emitter);
            let emitted = emit_compiled(&mut u, &emitter, &unit).map_err(stop)?;
            let code = i32::from(!emitted.diagnostics.is_empty());
            Ok((
                json!({
                    "artifact": term_to_json(&u, emitted.artifact),
                    "diagnostics": terms(&u, &emitted.diagnostics),
                }),
                code,
            ))
        }
        other => Err(format!("unknown entry {other}")),
    }
}
