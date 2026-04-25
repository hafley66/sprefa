//! Format-agnostic data tree (JSON / YAML / TOML).
//!
//! Ported from v2/src/data/. The `DataNode` trait + `AnyDataNode` enum
//! lets the walker stay format-agnostic; `parse_by_ext` dispatches on
//! file extension.

pub mod json;
pub mod toml;
pub mod types;
pub mod yaml;

pub use json::JsonNode;
pub use toml::TomlNode;
pub use types::{parse_by_ext, AnyDataNode, DataKind, DataNode, ParseError};
pub use yaml::YamlNode;
