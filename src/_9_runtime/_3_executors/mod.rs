//! The executor roster: every served name maps to one Rust executor.

#[path = "extract.rs"]
pub mod extract;
#[path = "fetch_json.rs"]
pub mod fetch_json;
#[path = "repo_at.rs"]
pub mod repo_at;
#[path = "soopy_history.rs"]
pub mod soopy_history;
#[path = "soopy_refs.rs"]
pub mod soopy_refs;
#[path = "timer.rs"]
pub mod timer;

use super::reconcile::{application_values, IExecutor};
use crate::_6_eval::{Diagnostic, Term, TermId, Universe};
use std::collections::HashMap;

pub use extract::Extract;
pub use fetch_json::FetchJson;
pub use repo_at::RepoAt;
pub use soopy_history::SoopyHistory;
pub use soopy_refs::SoopyRefs;
pub use timer::Timer;

/// The bound text value at `index` of an application, with its term.
pub(crate) fn text_at(u: &Universe, application: TermId, index: usize) -> Option<(TermId, String)> {
    let value = application_values(u, application)?
        .get(index)
        .copied()
        .flatten()?;
    match u.get(value) {
        Term::Str(sym) => Some((value, u.sym_str(*sym).to_string())),
        _ => None,
    }
}

/// Every soopy error flattened with its causes.
pub(crate) fn message(error: impl std::fmt::Display) -> String {
    format!("{error:#}")
}

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
                match (
                    names.get(fetch_json::RELATION),
                    names.get(fetch_json::ERROR),
                ) {
                    (Some(rel), Some(error)) => {
                        executors.push(Box::new(FetchJson::new(*rel, *error)))
                    }
                    (None, _) => missing(u, "served_relation_unknown", name),
                    (_, None) => missing(u, "executor_relation_unknown", fetch_json::ERROR),
                }
            }
            soopy_refs::RELATION
            | soopy_history::RELATION
            | repo_at::RELATION
            | extract::RELATION => {
                let error = match name.as_str() {
                    soopy_refs::RELATION => soopy_refs::ERROR,
                    soopy_history::RELATION => soopy_history::ERROR,
                    repo_at::RELATION => repo_at::ERROR,
                    _ => extract::ERROR,
                };
                match (names.get(name.as_str()), names.get(error)) {
                    (Some(rel), Some(error)) => executors.push(match name.as_str() {
                        soopy_refs::RELATION => Box::new(SoopyRefs::new(*rel, *error)),
                        soopy_history::RELATION => Box::new(SoopyHistory::new(*rel, *error)),
                        repo_at::RELATION => Box::new(RepoAt::new(*rel, *error)),
                        _ => Box::new(Extract::new(*rel, *error)),
                    }),
                    (None, _) => missing(u, "served_relation_unknown", name),
                    (_, None) => missing(u, "executor_relation_unknown", error),
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
