//! `Injected`: one sub-tree parsed from a body region of its parent tree.
//! Carries `host_path` (where in the parent's path it lives) and `host_off`
//! (byte offset of its source within the parent's source).

use std::sync::Arc;

use crate::cst::diag::Diag;
use crate::cst::path::Path;

pub struct Injected {
    pub dsl_id:    Arc<str>,
    pub host_path: Path,
    pub host_off:  usize,
    pub tree:      tree_sitter::Tree,
    pub source:    Arc<str>,
    pub injected:  Vec<Injected>,
}

/// Parse a body slice with `grammar` into an `Injected` carrying `host_path`
/// and `host_off`. Fatal failures (bad language, parser returned None) come
/// back as `Err(Diag)`. Tree-level error nodes (recoverable parse errors) do
/// NOT count as fatal — call `collect_errors_shifted` separately if you want
/// them surfaced.
pub fn parse_body(
    dsl_id:    Arc<str>,
    grammar:   &tree_sitter::Language,
    body:      &str,
    host_path: Path,
    host_off:  usize,
) -> Result<Injected, Diag> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(grammar).map_err(|e| {
        Diag::error(
            "cst.parse.language",
            format!("set_language failed: {e}"),
            0..0,
        )
    })?;
    let tree = parser.parse(body, None).ok_or_else(|| {
        Diag::error("cst.parse.failed", "parser returned None", 0..0)
    })?;
    Ok(Injected {
        dsl_id,
        host_path,
        host_off,
        tree,
        source: Arc::from(body),
        injected: Vec::new(),
    })
}
