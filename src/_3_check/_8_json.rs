//! The `dl8 check` transport. Terms use the `_6_eval::json` encoding, which is
//! byte-for-byte what `v8/oracle/eval/json_terms.pl` writes.
//!
//! Two case shapes, tagged by `entry`: `check_datalog` carries a
//! `basement_program` and its origins, `check_resolved_rules` carries a
//! relation list and a rule list.

use super::api::{check_datalog, Stop};
use super::resolved::check_resolved_rules;
use crate::_2_lower::Lowered;
use crate::_6_eval::json::{term_from_json, term_to_json};
use crate::_6_eval::term::{TermId, Universe};
use serde_json::{json, Value};

fn list_from_json(u: &mut Universe, value: Option<&Value>) -> Result<Vec<TermId>, String> {
    let Some(Value::Array(items)) = value else {
        return Err("expected a JSON array".into());
    };
    items.iter().map(|i| term_from_json(u, i)).collect()
}

pub fn run(case: &Value) -> Result<(Value, i32), String> {
    let input = case.get("input").ok_or("case without input")?;
    let entry = case
        .get("entry")
        .and_then(|e| e.as_str())
        .unwrap_or("check_datalog");
    let mut u = Universe::new();
    match entry {
        "check_datalog" => {
            let program = term_from_json(&mut u, input.get("program").ok_or("no program")?)?;
            let origins = list_from_json(&mut u, input.get("origins"))?;
            let lowered = Lowered {
                program: Some(program),
                origins,
                diagnostics: vec![],
            };
            let (checked, diagnostics) =
                check_datalog(&mut u, &lowered).map_err(|Stop::Fail(what)| what.to_string())?;
            let checked_json = match &checked {
                Some(checked) => {
                    let term = checked.to_term(&mut u);
                    term_to_json(&u, term)
                }
                None => Value::Array(vec![]),
            };
            let code = i32::from(!diagnostics.is_empty());
            Ok((
                json!({
                    "checked": checked_json,
                    "checked_bound": true,
                    "diagnostics": terms(&u, &diagnostics),
                }),
                code,
            ))
        }
        "check_resolved_rules" => {
            let relations = list_from_json(&mut u, input.get("relations"))?;
            let rules = list_from_json(&mut u, input.get("rules"))?;
            let resolved = check_resolved_rules(&mut u, &relations, &rules)
                .map_err(|Stop::Fail(what)| what.to_string())?;
            let code = i32::from(!resolved.diagnostics.is_empty());
            Ok((
                json!({
                    "depends": terms(&u, &resolved.depends),
                    "depends_bound": true,
                    "strata": terms(&u, &resolved.strata),
                    "strata_bound": true,
                    "diagnostics": terms(&u, &resolved.diagnostics),
                }),
                code,
            ))
        }
        other => Err(format!("unknown entry {other}")),
    }
}

fn terms(u: &Universe, rows: &[TermId]) -> Value {
    Value::Array(rows.iter().map(|r| term_to_json(u, *r)).collect())
}
