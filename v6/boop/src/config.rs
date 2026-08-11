use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
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
    config.model_presets.get(preset).cloned().with_context(|| {
        let available = config.model_presets.keys().cloned().collect::<Vec<_>>();
        let available = if available.is_empty() {
            "none".to_owned()
        } else {
            available.join(", ")
        };
        format!(
            "model preset `{preset}` is absent from {} (available: {available})",
            path.display()
        )
    })
}

/// Pick a lane's spawn model: explicit --model, then --preset, then
/// default-model-preset, applied to every harness. The explicit slot carries
/// an already resolved model string; `preset` and `default_preset` are names
/// resolved to model strings on demand.
pub fn resolve_spawn_model(
    explicit: Option<&str>,
    preset: Option<&str>,
    default_preset: Option<&str>,
    path: &Path,
) -> Result<Option<String>> {
    if let Some(model) = explicit {
        return Ok(Some(model.to_owned()));
    }
    if let Some(preset) = preset {
        return Ok(Some(resolve_model(preset, path)?));
    }
    if let Some(preset) = default_preset {
        return Ok(Some(resolve_model(preset, path)?));
    }
    Ok(None)
}

/// The loaded config as pretty JSON, including the defaults a missing file
/// yields. `boop config show` prints this.
pub fn show(path: &Path) -> Result<String> {
    let config = load(path)?;
    serde_json::to_string_pretty(&config)
        .map_err(|error| anyhow::anyhow!("serialize the boop config: {error}"))
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

    fn write_config(text: &str, name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("boop-config-{}-{name}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn default_preset_resolves_for_any_harness() {
        let path = write_config(
            r#"{ "default-model-preset": "flash4", "model-presets": {
                "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "any-harness",
        );
        assert_eq!(
            resolve_spawn_model(None, None, Some("flash4"), &path).unwrap(),
            Some("openrouter/deepseek/deepseek-v4-flash-0731".into())
        );
    }

    #[test]
    fn explicit_model_beats_preset_beats_default() {
        let path = write_config(
            r#"{ "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "precedence",
        );
        assert_eq!(
            resolve_spawn_model(Some("my-model"), Some("flash4"), Some("luna"), &path).unwrap(),
            Some("my-model".into())
        );
        assert_eq!(
            resolve_spawn_model(None, Some("flash4"), Some("luna"), &path).unwrap(),
            Some("openrouter/deepseek/deepseek-v4-flash-0731".into())
        );
        assert_eq!(
            resolve_spawn_model(None, None, Some("luna"), &path).unwrap(),
            Some("gpt-5.6-luna@medium".into())
        );
        assert_eq!(resolve_spawn_model(None, None, None, &path).unwrap(), None);
    }

    #[test]
    fn collapsed_resolution_matches_all_precedence_cases() {
        let path = write_config(
            r#"{ "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "collapsed",
        );
        let explicit = Some("my-model");
        let preset = Some("flash4");
        let default = Some("luna");
        let cases = [
            (explicit, preset, default, Some("my-model".to_owned())),
            (
                None,
                preset,
                default,
                Some("openrouter/deepseek/deepseek-v4-flash-0731".to_owned()),
            ),
            (None, None, default, Some("gpt-5.6-luna@medium".to_owned())),
            (None, None, None, None),
        ];
        for (explicit, preset, default, expected) in cases {
            assert_eq!(
                resolve_spawn_model(explicit, preset, default, &path).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn missing_preset_lists_available_names() {
        let path = write_config(
            r#"{ "model-presets": { "flash4": "openrouter/deepseek/deepseek-v4-flash-0731",
                "luna": "gpt-5.6-luna@medium" } }"#,
            "missing-preset",
        );
        let error = resolve_model("nope", &path).unwrap_err().to_string();
        assert!(
            error.contains("model preset `nope` is absent from"),
            "{error}"
        );
        assert!(error.contains("available: flash4, luna"), "{error}");
    }

    #[test]
    fn show_on_missing_file_prints_the_default_config() {
        let dir = std::env::temp_dir().join(format!("boop-config-show-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let rendered = show(&path).unwrap();
        let parsed: Config = serde_json::from_str(&rendered).unwrap();
        assert_eq!(parsed, Config::default());
        assert!(rendered.contains("\"model-presets\": {}"), "{rendered}");
    }
}
