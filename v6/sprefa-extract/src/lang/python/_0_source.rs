//! The Python extractor arm: tree-sitter-python front-end for type/call, ast-grep
//! for cst. Mirrors GoSource (same shape, different front-end): cst via ast-grep's
//! python grammar + one tree-sitter-python parse feeding the type/call
//! projections.
//!
//! Span bridge: NONE needed (like go.rs, unlike rust.rs's syn line/col -> byte
//! table). tree-sitter nodes give raw byte offsets directly (`start_byte`/
//! `end_byte`), so `Span { start: node.start_byte(), len: end - start }` is the
//! whole story.
//!
//! Commit A (skeleton): PythonSource wires cst via ast-grep + a
//! tree-sitter-python parse; type/call projections are stubbed empty. Commit B
//! ports `walk_py_entities` (TypeF nodes + arrow-type sigs); commit C ports
//! `py_walk_call_defs` + `py_walk_call_sites` (CallF).
//!
//! Deferred follow-ups: DfF (`py_dataflow_from`), the docs facet
//! (`py_docs_from`), type-edge candidates (`py_edges_from`), both `Resolve`
//! arms, the module plane (src/graph/modgraph/python.rs), and the roster wiring
//! (roster entry + RESOLVE_ARMS row + ROSTER_FIXTURES entry).
//!
//! @comment-ok: the commit-split + deferral ledger mirrors lang/go.rs:1-24.

use crate::family::{CallF, CstF, TypeF};
use crate::lang::{AstGrepParser, CstProjector};
use crate::rows::FamilyBundle;
use crate::seams::{Parser, Project};
use crate::shape::Strings;
use crate::source::{ExtractOutput, FamilyMask, Source};

// ── the tree-sitter-python parse (one parse feeds type/call) ─────────────────

/// Parse Python via tree-sitter-python (v5 `py_parse`). tree-sitter 0.25's
/// `Language::new` wraps tree-sitter-python 0.23's `LANGUAGE`.
fn py_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_python::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// UTF-8 text of a tree-sitter node. Port of v5 `py_text`.
fn py_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
fn node_span(node: tree_sitter::Node) -> crate::shape::Span {
    crate::shape::Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

// ── TypeF: entity nodes + arrow-type sigs (commit B) ───────────────────────

fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let _ = (root, src, strings, sink);
}

// ── CallF: callable definitions (nodes) + call sites (aux, commit C) ───────

fn project_call(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let _ = (root, src, strings, sink);
}

// ── PythonSource: cst via ast-grep + type/call via tree-sitter-python ──────

/// `matches` = `.py`/`.pyi` (SupportLang maps both to Python). cst via ast-grep;
/// type/call via one tree-sitter-python parse.
#[derive(Default)]
pub struct PythonSource;

impl Source for PythonSource {
    fn name(&self) -> &'static str {
        "python"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".py") || path.ends_with(".pyi")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). A failed ast-grep parse leaves cst None.
        let cst = if mask.cst {
            let arena = AstGrepParser.make_arena();
            AstGrepParser
                .parse(&arena, path, content)
                .ok()
                .map(|parsed| {
                    let mut bundle = FamilyBundle::<CstF>::default();
                    CstProjector.project(&parsed, &mut strings, &mut bundle);
                    bundle
                })
        } else {
            None
        };

        // type/call via ONE tree-sitter-python parse (masked). Byte spans come
        // straight off the tree-sitter nodes. A failed parse leaves both None.
        let mut types = None;
        let mut call = None;
        if mask.types || mask.call {
            if let Ok(src) = std::str::from_utf8(content) {
                if let Some(tree) = py_parse(src) {
                    let root = tree.root_node();
                    let src_bytes = src.as_bytes();
                    if mask.types {
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(root, src_bytes, &mut strings, &mut bundle);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let mut bundle = FamilyBundle::<CallF>::default();
                        project_call(root, src_bytes, &mut strings, &mut bundle);
                        call = Some(bundle);
                    }
                }
            }
        }

        ExtractOutput {
            strings,
            cst,
            types,
            call,
            df: None,
        }
    }
}
