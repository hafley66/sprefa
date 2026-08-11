use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    pub default_model_preset: Option<String>,
    pub model_presets: BTreeMap<String, String>,
    /// Model-name prefix -> harness id, overriding lane.rs's compiled table.
    pub model_harness: BTreeMap<String, String>,
    /// Model-family prefix -> owning harness for the flat-rate-plan ban.
    pub opencode_banned: BTreeMap<String, String>,
}

static CONFIG: OnceLock<Result<Config, anyhow::Error>> = OnceLock::new();

/// The process-wide Config, loaded exactly once from the default path. A
/// missing file falls back to the default (nothing configured); a file that
/// fails to read or parse is a loud error, never a silent default.
pub fn loaded() -> Result<&'static Config> {
    match CONFIG.get_or_init(load_once) {
        Ok(config) => Ok(config),
        Err(error) => Err(anyhow::anyhow!("load the boop config: {error:#}")),
    }
}

fn load_once() -> Result<Config, anyhow::Error> {
    let path = default_path()?;
    load(&path)
}

pub fn default_path() -> Result<PathBuf> {
    let root = dirs::config_dir().context("resolve the user config directory")?;
    Ok(root.join("boop").join("config.json"))
}

pub fn load(path: &Path) -> Result<Config> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

pub fn resolve_model(preset: &str, path: &Path) -> Result<String> {
    let config = load(path)?;
    config
        .model_presets
        .get(preset)
        .cloned()
        .with_context(|| format!("model preset `{preset}` is absent from {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_provider_model_presets() {
        let config: Config = serde_json::from_str(
            r#"
{
  "default-model-preset": "flash4",
  "model-presets": {
    "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
    "luna": "gpt-5.6-luna@medium"
  },
  "model-harness": {
    "glm": "opencode"
  },
  "opencode-banned": {
    "gemini": "gemini"
  }
}
"#,
        )
        .unwrap();
        assert_eq!(
            config,
            Config {
                default_model_preset: Some("flash4".into()),
                model_presets: BTreeMap::from([
                    (
                        "flash4".into(),
                        "openrouter/deepseek/deepseek-v4-flash-0731".into()
                    ),
                    ("luna".into(), "gpt-5.6-luna@medium".into()),
                ]),
                model_harness: BTreeMap::from([("glm".into(), "opencode".into())]),
                opencode_banned: BTreeMap::from([("gemini".into(), "gemini".into())]),
            }
        );
    }
}
