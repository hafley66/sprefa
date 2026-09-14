//! The executor roster: every served name maps to one Rust executor.

#[path = "fetch_json.rs"]
pub mod fetch_json;
#[path = "timer.rs"]
pub mod timer;

use super::reconcile::IExecutor;
use crate::_6_eval::{Diagnostic, TermId, Universe};
use std::collections::HashMap;

pub use fetch_json::FetchJson;
pub use timer::Timer;

/// A companion relation the executor writes (`fetch_json_error`) must be
/// declared, so a missing one is a diagnostic at construction.
pub fn executors_for(
    u: &mut Universe,
    names: &HashMap<String, TermId>,
    served: &[String],
) -> Result<Vec<Box<dyn IExecutor>>, Vec<Diagnostic>> {
    let mut executors: Vec<Box<dyn IExecutor>> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut missing = |u: &mut Universe, reason: &str, name: &str| {
        let atom = u.atom(name);
        let payload = u.compound(reason, vec![atom]);
        diagnostics.push(Diagnostic {
            phase: "eval",
            payload,
        });
    };
    for name in served {
        match name.as_str() {
            timer::RELATION => match names.get(timer::RELATION) {
                Some(rel) => executors.push(Box::new(Timer::new(*rel))),
                None => missing(u, "served_relation_unknown", name),
            },
            fetch_json::RELATION => {
                match (names.get(fetch_json::RELATION), names.get(fetch_json::ERROR)) {
                    (Some(rel), Some(error)) => {
                        executors.push(Box::new(FetchJson::new(*rel, *error)))
                    }
                    (None, _) => missing(u, "served_relation_unknown", name),
                    (_, None) => missing(u, "executor_relation_unknown", fetch_json::ERROR),
                }
            }
            _ => missing(u, "served_relation_no_executor", name),
        }
    }
    match diagnostics.is_empty() {
        true => Ok(executors),
        false => Err(diagnostics),
    }
}
