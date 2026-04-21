use std::borrow::Cow;
use std::sync::Arc;

use bytes::Bytes;

// ---------------------------------------------------------------------------
// ParseError
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: Arc<str>,
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

    pub fn at(msg: impl Into<Arc<str>>, offset: u32) -> Self {
        Self { message: msg.into(), byte_offset: Some(offset) }
    }
}

// ---------------------------------------------------------------------------
// DataKind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Object,
    Array,
    Scalar,
    Null,
}

// ---------------------------------------------------------------------------
// DataNode trait
// ---------------------------------------------------------------------------

/// Format-agnostic node in a parsed data file (JSON / YAML / TOML).
///
/// Implementations are owned, cheaply cloneable wrappers (Arc-based inner).
/// All byte offsets are into the original source slice returned by `source()`.
pub trait DataNode: Clone + Send + Sync + Sized {
    fn kind(&self) -> DataKind;

    /// Byte range `[start, end)` into `source()`.
    fn byte_range(&self) -> (u32, u32);

    /// For scalars: the logical text value (strings unquoted+unescaped,
    /// numbers/bools/datetimes as-is from source). None for Object/Array/Null.
    fn as_scalar_text(&self) -> Option<Cow<'_, str>>;

    /// Key-value pairs for Object nodes. Undefined behavior on other kinds.
    fn entries(&self) -> Box<dyn Iterator<Item = (Self, Self)> + '_>;

    /// Items for Array nodes. Undefined behavior on other kinds.
    fn items(&self) -> Box<dyn Iterator<Item = Self> + '_>;

    /// The full original source bytes this node was parsed from.
    fn source(&self) -> &[u8];
}

// ---------------------------------------------------------------------------
// AnyDataNode — dispatch enum
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub enum AnyDataNode {
    Json(super::json::JsonNode),
    Yaml(super::yaml::YamlNode),
    Toml(super::toml_::TomlNode),
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
                n.entries().map(|(k, v)| (AnyDataNode::Json(k), AnyDataNode::Json(v)))
            ),
            AnyDataNode::Yaml(n) => Box::new(
                n.entries().map(|(k, v)| (AnyDataNode::Yaml(k), AnyDataNode::Yaml(v)))
            ),
            AnyDataNode::Toml(n) => Box::new(
                n.entries().map(|(k, v)| (AnyDataNode::Toml(k), AnyDataNode::Toml(v)))
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

// ---------------------------------------------------------------------------
// parse_by_ext
// ---------------------------------------------------------------------------

pub fn parse_by_ext(ext: &str, src: Arc<Bytes>) -> Result<AnyDataNode, ParseError> {
    match ext {
        "json" => super::json::JsonNode::parse(src).map(AnyDataNode::Json),
        "yaml" | "yml" => super::yaml::YamlNode::parse(src).map(AnyDataNode::Yaml),
        "toml" => super::toml_::TomlNode::parse(src).map(AnyDataNode::Toml),
        other => Err(ParseError::new(Arc::from(format!("unsupported extension: {other}")))),
    }
}
