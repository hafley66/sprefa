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
    /// "/"-joined structural path through the parsed tree to this leaf.
    /// e.g. "dependencies/express/version" or "paths//v1/widgets/post/operationId".
    /// Used by anti-unification to reconstruct selectors from pairs of refs.
    pub node_path: Option<String>,
}

/// Trait for language-specific extractors.
/// Each extractor handles a set of file extensions and produces raw refs from source bytes.
pub trait Extractor: Send + Sync {
    fn extensions(&self) -> &[&str];
    fn extract(&self, source: &[u8], path: &str) -> Vec<RawRef>;
}
