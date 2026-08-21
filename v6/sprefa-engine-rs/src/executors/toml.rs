//! `toml_json(config_path, bucket) -> doc: json`.
//! `basic-toml` is already in this lock through sprefa-extract's manifest arm.

use std::collections::BTreeMap;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::{host_error, required_input, resolve_home, row};

/// A missing file answers zero rows, matching the shell body's `|| true`.
pub struct TomlJsonExecutor;

impl IHostExecutor for TomlJsonExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let spelled = required_input(host, env, &["config_path", "path"])?;
        let path = resolve_home(spelled);
        let span = tracing::info_span!("toml_json", host, path = %path.display());
        let _entered = span.enter();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(Vec::new());
        };
        let doc: serde_json::Value = basic_toml::from_str(&text)
            .map_err(|failure| host_error(host, format!("parse {}: {failure}", path.display())))?;
        Ok(vec![row([
            ("config_path", serde_json::json!(spelled)),
            ("doc", doc),
        ])])
    }
}
