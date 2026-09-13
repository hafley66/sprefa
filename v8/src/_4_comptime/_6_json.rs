//! Case transport. The term encoding is `oracle/eval/json_terms.pl`; the case
//! shape is written by `oracle/comptime/dump_comptime.pl`.

use super::api::{evaluate_checked, Compiled, Refreeze, Sources};
use super::assemble::assemble_generated_program;
use super::finish::{generated_expression_environment, validate_functional_rows};
use super::host::{erase_host_planning_rows, validate_hosted_relations};
use super::rounds::{Outcome, Round};
use crate::_3_check::{Checked, Stop};
use crate::_6_eval::json::{term_from_json, term_to_json};
use crate::_6_eval::term::{TermId, Universe};
use serde_json::{json, Map, Value};

/// v7's recorded answers for `final_checked_program/5` and
/// `deferred_checked_program/5`, replayed in call order.
pub struct Replay {
    pub entries: Vec<ReplayEntry>,
    pub next: usize,
}

pub struct ReplayEntry {
    pub deferred: bool,
    pub facts_len: usize,
    pub generated_len: usize,
    pub checked: Option<Checked>,
    pub diagnostics: Vec<TermId>,
}

impl Sources for Replay {
    fn refreeze(
        &mut self,
        _u: &mut Universe,
        mode: Refreeze,
        compiler_facts: &[TermId],
        generated_relations: &[TermId],
    ) -> Result<(Option<Checked>, Vec<TermId>), Stop> {
        let Some(entry) = self.entries.get(self.next) else {
            return Err(Stop::Fail("replay exhausted"));
        };
        self.next += 1;
        if entry.deferred != (mode == Refreeze::Deferred) {
            return Err(Stop::Fail("replay mode differs"));
        }
        if entry.facts_len != compiler_facts.len() {
            return Err(Stop::Fail("replay compiler facts length differs"));
        }
        if entry.generated_len != generated_relations.len() {
            return Err(Stop::Fail("replay generated relations length differs"));
        }
        let checked = entry.checked.as_ref().map(clone_checked);
        Ok((checked, entry.diagnostics.clone()))
    }
}

fn clone_checked(c: &Checked) -> Checked {
    Checked {
        nodes: c.nodes.clone(),
        edges: c.edges.clone(),
        relations: c.relations.clone(),
        seeds: c.seeds.clone(),
        rules: c.rules.clone(),
        depends: c.depends.clone(),
        strata: c.strata.clone(),
    }
}

fn terms(u: &mut Universe, value: Option<&Value>) -> Result<Vec<TermId>, String> {
    let Some(Value::Array(items)) = value else {
        return Ok(vec![]);
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(term_from_json(u, item)?);
    }
    Ok(out)
}

fn list(u: &Universe, ids: &[TermId]) -> Value {
    Value::Array(ids.iter().map(|id| term_to_json(u, *id)).collect())
}

/// `checked_datalog(root_graph(Nodes, Edges), datalog_program(R, S, U), D, St)`
/// as the flat object the dump writes.
pub fn checked_from_json(u: &mut Universe, value: &Value) -> Result<Checked, String> {
    let m = value.as_object().ok_or("checked is not an object")?;
    Ok(Checked {
        nodes: terms(u, m.get("nodes"))?,
        edges: terms(u, m.get("edges"))?,
        relations: terms(u, m.get("relations"))?,
        seeds: terms(u, m.get("seeds"))?,
        rules: terms(u, m.get("rules"))?,
        depends: terms(u, m.get("depends"))?,
        strata: terms(u, m.get("strata"))?,
    })
}

pub fn checked_to_json(u: &Universe, c: &Checked) -> Value {
    json!({
        "nodes": list(u, &c.nodes), "edges": list(u, &c.edges),
        "relations": list(u, &c.relations), "seeds": list(u, &c.seeds),
        "rules": list(u, &c.rules), "depends": list(u, &c.depends),
        "strata": list(u, &c.strata)
    })
}

pub fn replay_from_json(u: &mut Universe, value: Option<&Value>) -> Result<Replay, String> {
    let mut entries = Vec::new();
    if let Some(Value::Array(items)) = value {
        for item in items {
            let m = item.as_object().ok_or("replay entry is not an object")?;
            let checked = match m.get("checked") {
                Some(Value::Object(_)) => Some(checked_from_json(u, &m["checked"])?),
                _ => None,
            };
            entries.push(ReplayEntry {
                deferred: m.get("deferred").and_then(|d| d.as_bool()).unwrap_or(false),
                facts_len: m.get("facts_len").and_then(|n| n.as_u64()).unwrap_or(0) as usize,
                generated_len: m.get("generated_len").and_then(|n| n.as_u64()).unwrap_or(0)
                    as usize,
                checked,
                diagnostics: terms(u, m.get("diagnostics"))?,
            });
        }
    }
    Ok(Replay { entries, next: 0 })
}

fn compiled_to_json(u: &Universe, compiled: &Compiled) -> Value {
    json!({
        "type_graph_facts": list(u, &compiled.type_graph_facts),
        "runtime": checked_to_json(u, &compiled.runtime),
        "compiler_facts": list(u, &compiled.compiler_facts)
    })
}

/// One row per `Round` event, so the dl8 round count meets v7's.
fn rounds_to_json(events: &[Round]) -> Value {
    Value::Array(
        events
            .iter()
            .map(|event| match event {
                Round::Evaluate {
                    outer,
                    round,
                    rules,
                    seeds,
                    closure,
                } => json!({"event": "evaluate", "outer": outer, "round": round,
                            "rules": rules, "seeds": seeds, "closure": closure}),
                Round::Assemble {
                    outer,
                    round,
                    relations,
                    rules,
                } => json!({"event": "assemble", "outer": outer, "round": round,
                            "relations": relations, "rules": rules}),
                Round::Decision {
                    outer,
                    round,
                    outcome,
                } => json!({"event": "decision", "outer": outer, "round": round,
                "outcome": match outcome {
                    Outcome::Stable => "stable",
                    Outcome::Continue => "continue",
                    Outcome::LimitExhausted => "limit_exhausted",
                }}),
                Round::Refreeze { outer, deferred } => {
                    json!({"event": "refreeze", "outer": outer, "deferred": deferred})
                }
            })
            .collect(),
    )
}

/// `entry` selects which v7 predicate the case froze.
pub fn run(case: &Value) -> Result<(Value, u8), String> {
    let mut u = Universe::new();
    let entry = case.get("entry").and_then(|e| e.as_str()).unwrap_or("");
    let input = case.get("input").ok_or("case without input")?;
    let mut out = Map::new();
    match entry {
        "evaluate_checked" => {
            let checked = checked_from_json(&mut u, input.get("checked").ok_or("no checked")?)?;
            let mut replay = replay_from_json(&mut u, input.get("replay"))?;
            let mut events = Vec::new();
            let (compiled, diagnostics) = {
                let mut fx = |event: Round| events.push(event);
                match evaluate_checked(&mut u, &checked, &mut replay, &mut fx) {
                    Ok(result) => result,
                    Err(Stop::Fail(reason)) => return Err(reason.to_string()),
                }
            };
            out.insert(
                "compiled".into(),
                match &compiled {
                    Some(compiled) => compiled_to_json(&u, compiled),
                    None => Value::Array(vec![]),
                },
            );
            out.insert("diagnostics".into(), list(&u, &diagnostics));
            out.insert("rounds".into(), rounds_to_json(&events));
            let code = u8::from(!diagnostics.is_empty());
            Ok((Value::Object(out), code))
        }
        "assemble_generated_program" => {
            let rows = terms(&mut u, input.get("compiler_rows"))?;
            let base = terms(&mut u, input.get("base_relations"))?;
            let assembled = assemble_generated_program(&mut u, &rows, &base);
            out.insert("generated_relations".into(), list(&u, &assembled.relations));
            out.insert("generated_rules".into(), list(&u, &assembled.rules));
            out.insert("diagnostics".into(), list(&u, &assembled.diagnostics));
            let code = u8::from(!assembled.diagnostics.is_empty());
            Ok((Value::Object(out), code))
        }
        "validate_hosted_relations" => {
            let nodes = terms(&mut u, input.get("nodes"))?;
            let edges = terms(&mut u, input.get("edges"))?;
            let relations = terms(&mut u, input.get("relations"))?;
            let facts = terms(&mut u, input.get("compiler_facts"))?;
            let diagnostics = validate_hosted_relations(&mut u, &nodes, &edges, &relations, &facts);
            out.insert("diagnostics".into(), list(&u, &diagnostics));
            let code = u8::from(!diagnostics.is_empty());
            Ok((Value::Object(out), code))
        }
        "erase_host_planning_rows" => {
            let nodes = terms(&mut u, input.get("nodes"))?;
            let edges = terms(&mut u, input.get("edges"))?;
            let relations = terms(&mut u, input.get("relations"))?;
            let seeds = terms(&mut u, input.get("seeds"))?;
            let rules = terms(&mut u, input.get("rules"))?;
            let erased = erase_host_planning_rows(&u, &nodes, &edges, &relations, &seeds, &rules);
            out.insert("relations".into(), list(&u, &erased.relations));
            out.insert("seeds".into(), list(&u, &erased.seeds));
            out.insert("rules".into(), list(&u, &erased.rules));
            Ok((Value::Object(out), 0))
        }
        "validate_functional_rows" => {
            let relations = terms(&mut u, input.get("relations"))?;
            let rows = terms(&mut u, input.get("rows"))?;
            let diagnostics = validate_functional_rows(&mut u, &relations, &rows);
            out.insert("diagnostics".into(), list(&u, &diagnostics));
            let code = u8::from(!diagnostics.is_empty());
            Ok((Value::Object(out), code))
        }
        "generated_expression_environment" => {
            let facts = terms(&mut u, input.get("compiler_facts"))?;
            let relations = terms(&mut u, input.get("generated_relations"))?;
            let slots = terms(&mut u, input.get("derived_bind_slots"))?;
            let (reservations, relations, edges) =
                generated_expression_environment(&mut u, &facts, &relations, &slots);
            out.insert("reservations".into(), list(&u, &reservations));
            out.insert("relations".into(), list(&u, &relations));
            out.insert("edges".into(), list(&u, &edges));
            Ok((Value::Object(out), 0))
        }
        _ => Err(format!("unknown comptime entry {entry:?}")),
    }
}
