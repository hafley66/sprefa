//! The executor roster: every served name maps to one Rust executor.

#[path = "extract.rs"]
pub mod extract;
#[path = "fetch_json.rs"]
pub mod fetch_json;
#[path = "fs_at.rs"]
pub mod fs_at;
#[path = "fs_json.rs"]
pub mod fs_json;
#[path = "git_history.rs"]
pub mod git_history;
#[path = "git_refs.rs"]
pub mod git_refs;
#[path = "timer.rs"]
pub mod timer;

use super::reconcile::{application_values, IExecutor};
use crate::_6_eval::kernel::kernel_ref;
use crate::_6_eval::{Diagnostic, Term, TermId, Universe};
use std::collections::HashMap;

pub use extract::Extract;
pub use fetch_json::FetchJson;
pub use fs_at::FsAt;
pub use fs_json::FsJson;
pub use git_history::GitHistory;
pub use git_refs::GitRefs;
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
            fs_json::RELATION => {
                match (
                    names.get(fs_json::RELATION),
                    names.get(fs_json::ERROR),
                    names.get(fs_json::NONE),
                ) {
                    (Some(rel), Some(error), Some(none)) => {
                        let (rel, error, none) = (*rel, *error, *none);
                        let colon = kernel_ref(u, ":");
                        executors.push(Box::new(FsJson::new(rel, error, colon, none)))
                    }
                    (None, _, _) => missing(u, "served_relation_unknown", name),
                    (_, None, _) => missing(u, "executor_relation_unknown", fs_json::ERROR),
                    (_, _, None) => missing(u, "executor_relation_unknown", fs_json::NONE),
                }
            }
            git_refs::RELATION | git_history::RELATION | fs_at::RELATION | extract::RELATION => {
                let error = match name.as_str() {
                    git_refs::RELATION => git_refs::ERROR,
                    git_history::RELATION => git_history::ERROR,
                    fs_at::RELATION => fs_at::ERROR,
                    _ => extract::ERROR,
                };
                match (names.get(name.as_str()), names.get(error)) {
                    (Some(rel), Some(error)) => executors.push(match name.as_str() {
                        git_refs::RELATION => Box::new(GitRefs::new(*rel, *error)),
                        git_history::RELATION => Box::new(GitHistory::new(*rel, *error)),
                        fs_at::RELATION => Box::new(FsAt::new(*rel, *error)),
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

/// Every relation name `executors_for` builds an executor for.
pub const ROSTER: [&str; 7] = [
    timer::RELATION,
    fetch_json::RELATION,
    fs_json::RELATION,
    git_refs::RELATION,
    git_history::RELATION,
    fs_at::RELATION,
    extract::RELATION,
];

/// The Once executors the program's names can build. A name whose companion
/// relation is undeclared builds nothing here; the runtime still reports it.
pub fn once_executors(
    u: &mut Universe,
    names: &HashMap<String, TermId>,
) -> Vec<Box<dyn IExecutor>> {
    ROSTER
        .iter()
        .filter(|name| names.contains_key(**name))
        .filter_map(|name| executors_for(u, names, &[name.to_string()]).ok())
        .flatten()
        .filter(|executor| executor.cadence() == super::reconcile::Cadence::Once)
        .collect()
}
