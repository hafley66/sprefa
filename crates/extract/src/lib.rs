use serde::Serialize;

// ── built-in kind constants ──────────────────────────────────────────────────
// These are the canonical kind strings for built-in extractors (JS, RS, rules).
// User-defined rules can use arbitrary kind strings beyond these.

pub mod kind {
    pub const IMPORT_PATH: &str = "import_path";
    pub const IMPORT_NAME: &str = "import_name";
    pub const EXPORT_NAME: &str = "export_name";
    pub const IMPORT_ALIAS: &str = "import_alias";
    pub const EXPORT_LOCAL_BINDING: &str = "export_local_binding";
    pub const DEP_NAME: &str = "dep_name";
    pub const DEP_VERSION: &str = "dep_version";
    pub const RS_USE: &str = "rs_use";
    pub const RS_DECLARE: &str = "rs_declare";
    pub const RS_MOD: &str = "rs_mod";
}

/// Git context passed to extractors. Rules use this to filter by repo/branch/tag.
/// Built-in extractors (js, rs) ignore it.
#[derive(Debug, Clone, Default)]
pub struct ExtractContext<'a> {
    pub repo: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub tags: &'a [&'a str],
}

/// A raw reference extracted from a source file, before DB insertion.
#[derive(Debug, Clone, Serialize)]
pub struct RawRef {
    pub value: String,
    pub span_start: u32,
    pub span_end: u32,
    /// Semantic kind string (e.g. "import_path", "rs_declare", "dep_name").
    /// Free-text; not tied to an enum.
    pub kind: String,
    /// Which extractor/rule produced this ref (e.g. "js", "rs", "cargo-deps").
    pub rule_name: String,
    pub is_path: bool,
    pub parent_key: Option<String>,
    /// "/"-joined structural path through the parsed tree to this leaf.
    /// e.g. "dependencies/express/version" or "paths//v1/widgets/post/operationId".
    /// Used by anti-unification to reconstruct selectors from pairs of refs.
    pub node_path: Option<String>,
    /// When set, this ref drives demand scanning.
    /// "repo" = value is a repository name, "rev" = value is a tag/branch to scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan: Option<String>,
}

/// Trait for language-specific extractors.
/// Each extractor handles a set of file extensions and produces raw refs from source bytes.
pub trait Extractor: Send + Sync {
    fn extensions(&self) -> &[&str];
    fn extract(&self, source: &[u8], path: &str, ctx: &ExtractContext) -> Vec<RawRef>;
}
