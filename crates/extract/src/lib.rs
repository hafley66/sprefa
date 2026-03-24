use serde::Serialize;
use sprefa_schema::RefKind;

/// A raw reference extracted from a source file, before DB insertion.
#[derive(Debug, Clone, Serialize)]
pub struct RawRef {
    pub value: String,
    pub span_start: u32,
    pub span_end: u32,
    pub kind: RefKind,
    pub is_path: bool,
    pub parent_key: Option<String>,
}

/// Trait for language-specific extractors.
/// Each extractor handles a set of file extensions and produces raw refs from source bytes.
pub trait Extractor: Send + Sync {
    fn extensions(&self) -> &[&str];
    fn extract(&self, source: &[u8], path: &str) -> Vec<RawRef>;
}
