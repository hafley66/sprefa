pub mod _0_types;
pub mod _1_json;
pub mod _2_yaml;
pub mod _3_toml;

pub use _0_types::{AnyDataNode, DataKind, DataNode, ParseError, parse_by_ext};

// Convenience re-exports for callers that need the concrete types.
pub use _1_json::JsonNode;
pub use _2_yaml::YamlNode;
pub use _3_toml::TomlNode;
