//! Linked host executors: no child process. Each answers `Vec<HostRow>`, keyed
//! by the host's declared output column names.

pub mod checkout;
pub mod clock;
pub mod cost;
pub mod env;
pub mod http;
pub mod toml;
pub mod watch;

use std::collections::BTreeMap;

use crate::hosts::HostError;
use crate::types::HostRow;

pub use checkout::SoopyCheckoutExecutor;
pub use cost::TickCostExecutor;
pub use env::EnvExecutor;
pub use http::{HttpGetExecutor, HttpPostExecutor};
pub use toml::TomlJsonExecutor;

/// Two registered host names ask one question under different column spellings
/// (`fetch.ep`, `gh_rest_cond.endpoint_path`); one executor answers both.
pub(crate) fn first_input<'a>(
    env: &'a BTreeMap<String, String>,
    names: &[&str],
) -> Option<&'a str> {
    names
        .iter()
        .find_map(|name| env.get(*name))
        .map(String::as_str)
}

pub(crate) fn required_input<'a>(
    host: &str,
    env: &'a BTreeMap<String, String>,
    names: &[&str],
) -> Result<&'a str, HostError> {
    first_input(env, names).ok_or_else(|| HostError {
        host: host.to_string(),
        message: format!("wants one of the input columns {names:?}"),
    })
}

/// One answer row. `select_columns` keeps the keys a plan declares and drops
/// a row missing any of them, so an executor answers every key it knows.
pub(crate) fn row<const N: usize>(columns: [(&str, serde_json::Value); N]) -> HostRow {
    columns
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect()
}

pub(crate) fn host_error(host: &str, message: impl Into<String>) -> HostError {
    HostError {
        host: host.to_string(),
        message: message.into(),
    }
}

/// `~/` is resolved by the executor that takes the path, never by the language.
pub(crate) fn resolve_home(path: &str) -> std::path::PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => std::path::PathBuf::from(home).join(rest),
            None => std::path::PathBuf::from(path),
        },
        None => std::path::PathBuf::from(path),
    }
}

// @comment-ok: module contract for the whole executor family, the one doc site.
// The linked executors behind the cross-repository host families. Every one
// answers through soopy or an in-process module of this crate: no shell, and no
// `Command::new("git")`, because soopy owns every Git process.
//
// @comment-ok: states the removed-cache invariant a reviewer cannot see from the code.
// EACH CALL ANSWERS FROM SOOPY DIRECTLY, no per-process memo: `hosts.rs`'s own
// per-tick demand dedup is the one "once per tick" this system needs.
//
// ONE ANSWER SERVES SEVERAL NAMES because `select_columns` keeps a row only
// when it carries every column the declaring host names (`hosts.rs`,
// `carries_every_column`). The families are disjoint by column name, so one
// merged row set projects into each sibling's own rows.

pub mod git_history;
pub mod git_refs;
pub mod repo_at;

/// A named stop carrying the host that could not answer.
pub(crate) fn stop(host: &str, message: String) -> HostError {
    HostError {
        host: host.to_string(),
        message,
    }
}

/// The repository root a demand named, absolute: a relative spelling resolves
/// against the harness's cwd, not the checkout the demand is about.
pub(crate) fn checkout_root(host: &str, spelling: &str) -> Result<std::path::PathBuf, HostError> {
    std::fs::canonicalize(std::path::PathBuf::from(spelling)).map_err(|failure| {
        stop(
            host,
            format!("repository root `{spelling}` is not a readable directory: {failure}"),
        )
    })
}

/// `WORK` is the worktree, matching soopy's CLI and `change_facts::parse_revision`.
pub(crate) fn revision_of(spelling: &str) -> soopy::Revision {
    crate::change_facts::parse_revision(spelling)
}
