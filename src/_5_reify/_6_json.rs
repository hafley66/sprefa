//! The `dl8 reify` transport. Terms use the `_6_eval::json` encoding, which is
//! byte-for-byte what `v8/oracle/eval/json_terms.pl` writes.
//!
//! Seven case shapes, tagged by `entry`, one per wrapped v7 predicate.

use super::api::Stop;
use super::calls::{logical_program_calls, logical_program_rows_calls};
use super::emit::{compiler_view, emit_compiled, Emitter};
use super::graph::logical_program_graph_calls;
use super::rows::logical_program_rows_term;
use crate::_3_check::Checked;
use crate::_4_comptime::Compiled;
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

fn unit(u: &mut Universe, input: &Value) -> Result<Compiled, String> {
    let runtime = term(u, input, "runtime_program")?;
    Ok(Compiled {
        type_graph_facts: list(u, input, "type_graph_facts")?,
        runtime: checked_from_term(u, runtime)?,
        compiler_facts: list(u, input, "compiler_facts")?,
    })
}

/// The inverse of `Checked::to_term` (`_3_check/_0_api.rs:36`), so the oracle
/// reader builds the struct v7's `compiled_unit/3` second slot holds.
fn checked_from_term(u: &Universe, checked: TermId) -> Result<Checked, String> {
    let Some(("checked_datalog", args)) = u.functor(checked) else {
        return Err("runtime_program is not checked_datalog/4".into());
    };
    let Some(("root_graph", graph)) = u.functor(args[0]) else {
        return Err("root_graph/2 expected".into());
    };
    let Some(("datalog_program", program)) = u.functor(args[1]) else {
        return Err("datalog_program/3 expected".into());
    };
    let lists = [
        graph[0], graph[1], program[0], program[1], program[2], args[2], args[3],
    ]
    .map(|slot| u.as_list(slot));
    let [nodes, edges, relations, seeds, rules, depends, strata] = lists;
    Ok(Checked {
        nodes: nodes.ok_or("nodes")?,
        edges: edges.ok_or("edges")?,
        relations: relations.ok_or("relations")?,
        seeds: seeds.ok_or("seeds")?,
        rules: rules.ok_or("rules")?,
        depends: depends.ok_or("depends")?,
        strata: strata.ok_or("strata")?,
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
