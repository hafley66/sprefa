//! Format-agnostic data tree (JSON for v0; YAML/TOML deferred).
//!
//! Ported from v2/src/data/. v3 only ships JSON in this slice — the
//! `DataNode` trait + `AnyDataNode` enum is kept so YAML/TOML slot in
//! later without churning the walker. See bd card sprefa-lpk.

pub mod json;
pub mod types;

pub use json::JsonNode;
pub use types::{parse_by_ext, AnyDataNode, DataKind, DataNode, ParseError};
