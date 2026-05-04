//! Format-agnostic data tree types. Ported from v2/src/data/_0_types.rs.
//!
//! v3 v0 ships JSON only. `AnyDataNode` is a single-variant enum so the
//! walker takes `&dyn DataNode` polymorphism uniformly; YAML/TOML add
//! variants without walker changes.

use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message:     Arc<str>,
    pub byte_offset: Option<u32>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.byte_offset {
            Some(off) => write!(f, "{} (byte {})", self.message, off),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    pub fn new(msg: impl Into<Arc<str>>) -> Self {
        Self { message: msg.into(), byte_offset: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Object,
    Array,
    Scalar,
    Null,
}

/// Format-agnostic node in a parsed data file.
pub trait DataNode: Clone + Send + Sync + Sized {
    fn kind(&self) -> DataKind;
    fn byte_range(&self) -> (u32, u32);
    fn as_scalar_text(&self) -> Option<Cow<'_, str>>;
    fn entries(&self) -> Box<dyn Iterator<Item = (Self, Self)> + '_>;
    fn items(&self) -> Box<dyn Iterator<Item = Self> + '_>;
    fn source(&self) -> &[u8];
}

#[derive(Clone)]
pub enum AnyDataNode {
    Json(super::json::JsonNode),
    Yaml(super::yaml::YamlNode),
    Toml(super::toml::TomlNode),
}

impl DataNode for AnyDataNode {
    fn kind(&self) -> DataKind {
        match self {
            AnyDataNode::Json(n) => n.kind(),
            AnyDataNode::Yaml(n) => n.kind(),
            AnyDataNode::Toml(n) => n.kind(),
        }
    }

    fn byte_range(&self) -> (u32, u32) {
        match self {
            AnyDataNode::Json(n) => n.byte_range(),
            AnyDataNode::Yaml(n) => n.byte_range(),
            AnyDataNode::Toml(n) => n.byte_range(),
        }
    }

    fn as_scalar_text(&self) -> Option<Cow<'_, str>> {
        match self {
            AnyDataNode::Json(n) => n.as_scalar_text(),
            AnyDataNode::Yaml(n) => n.as_scalar_text(),
            AnyDataNode::Toml(n) => n.as_scalar_text(),
        }
    }

    fn entries(&self) -> Box<dyn Iterator<Item = (Self, Self)> + '_> {
        match self {
            AnyDataNode::Json(n) => Box::new(
                n.entries().map(|(k, v)| (AnyDataNode::Json(k), AnyDataNode::Json(v))),
            ),
            AnyDataNode::Yaml(n) => Box::new(
                n.entries().map(|(k, v)| (AnyDataNode::Yaml(k), AnyDataNode::Yaml(v))),
            ),
            AnyDataNode::Toml(n) => Box::new(
                n.entries().map(|(k, v)| (AnyDataNode::Toml(k), AnyDataNode::Toml(v))),
            ),
        }
    }

    fn items(&self) -> Box<dyn Iterator<Item = Self> + '_> {
        match self {
            AnyDataNode::Json(n) => Box::new(n.items().map(AnyDataNode::Json)),
            AnyDataNode::Yaml(n) => Box::new(n.items().map(AnyDataNode::Yaml)),
            AnyDataNode::Toml(n) => Box::new(n.items().map(AnyDataNode::Toml)),
        }
    }

    fn source(&self) -> &[u8] {
        match self {
            AnyDataNode::Json(n) => n.source(),
            AnyDataNode::Yaml(n) => n.source(),
            AnyDataNode::Toml(n) => n.source(),
        }
    }
}

pub fn parse_by_ext(ext: &str, src: Arc<[u8]>) -> Result<AnyDataNode, ParseError> {
    match ext {
        "json"          => super::json::JsonNode::parse(src).map(AnyDataNode::Json),
        "yaml" | "yml"  => super::yaml::YamlNode::parse(src).map(AnyDataNode::Yaml),
        "toml"          => super::toml::TomlNode::parse(src).map(AnyDataNode::Toml),
        other => Err(ParseError::new(Arc::from(format!("unsupported extension: {other}")))),
    }
}
