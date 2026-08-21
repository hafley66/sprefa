//! Linked host executors: no child process. Each answers `Vec<HostRow>`, keyed
//! by the host's declared output column names.

pub mod checkout;
pub mod env;
pub mod fetch;
pub mod repos;
pub mod toml;

use std::collections::BTreeMap;

use crate::hosts::HostError;
use crate::types::HostRow;

pub use checkout::SoopyCheckoutExecutor;
pub use env::EnvExecutor;
pub use fetch::HttpFetchExecutor;
pub use repos::GhReposExecutor;
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
