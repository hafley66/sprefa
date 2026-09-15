//! The `dl8 lower` transport. Terms use the `_6_eval::json` encoding, which is
//! byte-for-byte what `v8/oracle/eval/json_terms.pl` writes.

use super::api::{lower_datalog, Lowered};
use super::cx::CallPolicy;
use super::derived::Stop;
use crate::_6_eval::json::{term_from_json, term_to_json};
use crate::_6_eval::term::Universe;
use serde_json::{json, Value};

pub fn run(case: &Value) -> Result<Value, String> {
    let input = case.get("input").ok_or("case without input")?;
    let policy = match input.get("policy").and_then(|p| p.as_str()) {
        Some("strict") | None => CallPolicy::Strict,
        Some("defer_unknown_calls") => CallPolicy::DeferUnknownCalls,
        Some(other) => return Err(format!("unknown policy {other}")),
    };
    let mut u = Universe::new();
    let unit = term_from_json(&mut u, input.get("unit").ok_or("case without unit")?)?;
    let environment = match input.get("environment") {
        Some(value) => term_from_json(&mut u, value)?,
        None => {
            let empty = u.empty_list();
            u.compound("expression_environment", vec![empty, empty, empty])
        }
    };
    match lower_datalog(&mut u, policy, unit, environment) {
        Ok(lowered) => Ok(encode(&u, &lowered)),
        Err(Stop::Diagnostic(diagnostic)) => {
            let lowered = Lowered {
                program: Some(u.empty_list()),
                origins: vec![],
                diagnostics: vec![diagnostic],
            };
            Ok(encode(&u, &lowered))
        }
        Err(Stop::Unported(what)) => Err(format!("not built yet: {what}")),
    }
}

fn encode(u: &Universe, lowered: &Lowered) -> Value {
    let program = match lowered.program {
        Some(term) => term_to_json(u, term),
        None => Value::Null,
    };
    json!({
        "program": program,
        "program_bound": lowered.program.is_some(),
        "origins": lowered.origins.iter().map(|o| term_to_json(u, *o)).collect::<Vec<_>>(),
        "diagnostics": lowered.diagnostics.iter().map(|d| term_to_json(u, *d)).collect::<Vec<_>>(),
    })
}
