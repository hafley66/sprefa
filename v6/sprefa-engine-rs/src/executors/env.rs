//! `env_var(name, bucket) -> value` read straight from the process table.

use std::collections::BTreeMap;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::{required_input, row};

/// An unset variable answers zero rows, and that zero-row shape IS the absent
/// meaning the config rules rely on.
pub struct EnvExecutor;

impl IHostExecutor for EnvExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let name = required_input(host, env, &["name", "var_name"])?;
        let span = tracing::info_span!("env", host, name);
        let _entered = span.enter();
        match std::env::var(name) {
            Ok(value) => Ok(vec![row([
                ("name", serde_json::json!(name)),
                ("value", serde_json::json!(value)),
            ])]),
            Err(_) => Ok(Vec::new()),
        }
    }
}
