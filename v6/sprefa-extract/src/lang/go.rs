//! The Go extractor arm: tree-sitter-go front-end for type/call/df, ast-grep for
//! cst. Mirrors RustSource/TsSource (same shape, different front-end): cst via
//! ast-grep's go grammar + one tree-sitter-go parse feeding the type/call/df
//! projections.
//!
//! Span bridge: NONE needed (unlike rust.rs's syn line/col -> byte table).
//! tree-sitter nodes give raw byte offsets directly (`start_byte`/`end_byte`), so
//! `Span { start: node.start_byte(), len: node.end_byte() - node.start_byte() }`
//! is the whole story. This is simpler than the rust port.
//!
//! Commit A (skeleton): GoSource wires cst via ast-grep + a tree-sitter-go parse;
//! type/call/df projections are stubbed empty. Commit B ports `walk_go_entities`
//! (TypeF nodes + arrow-type sigs); commit C ports `go_walk_call_defs` +
//! `go_walk_call_sites` (CallF); commit D ports `go_dataflow_from` (DfF nodes +
//! Direct edges).
//!
//! Deferred to `Resolve<TypeF>` (commit 4): type EDGES (field/impl/generic from
//! `go_edges_from`). Deferred follow-ups: the docs facet (`walk_go_docs`); the df
//! enrichment aux (args/fields/lits/param_pos/loops/nests). The const facet is
//! NOT ported: v5 go emits no const entities and no const_value rows
//! (`walk_go_entities` skips `const_declaration`; `extract` leaves `consts`
//! empty), so v6 matches by emitting none either.

use crate::family::{CallF, CstF, DfF, TypeF};
use crate::rows::FamilyBundle;
use crate::seams::{Parser, Project};
use crate::shape::Strings;
use crate::source::{ExtractOutput, FamilyMask, Source};
use super::astgrep::{AstGrepParser, CstProjector};

// ── the tree-sitter-go parse (one parse feeds type/call/df) ──────────────────

/// Parse Go source via tree-sitter-go. Port of v5 `go_parse`
/// (src/graph/typegraph/go.rs:41). tree-sitter 0.25's `Language::new` wraps the
/// `LanguageFn` tree-sitter-go 0.23 exports as `LANGUAGE`; the versions unify
/// with what ast-grep-language already transitively pulls.
fn go_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_go::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// UTF-8 text of a tree-sitter node. Port of v5 `go_text`.
fn go_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

// ════════════════════════════════════════════════════════════════════════════
// TypeF: entity nodes + arrow-type sigs. Commit B.
//
// Ports v5 `walk_go_entities` (the entity half) + `go_fn_type` (the arrow-type
// payload). The name-resolved type EDGES (field/impl/generic from `go_edges_from`)
// land with `Resolve<TypeF>` (commit 4); phase 1 stays pure-content span nodes.
// No const facet (v5 go emits none).
// ════════════════════════════════════════════════════════════════════════════

/// Project the TypeF family: one entity node per type/function/method
/// declaration + an arrow-type sig per callable param/return type reference.
/// Port of v5 `walk_go_entities` + `go_fn_type`. Commit B fills this in.
fn project_types(
    _root: tree_sitter::Node,
    _src: &[u8],
    _strings: &mut Strings,
    _sink: &mut FamilyBundle<TypeF>,
) {
    // Commit B: walk_go_entities + go_fn_type.
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux). Commit C.
//
// Ports v5 `go_walk_call_defs` (defs, incl. func_literal lambdas) +
// `go_walk_call_sites` (sites). Commit C fills this in.
// ════════════════════════════════════════════════════════════════════════════

/// Project the CallF family: one def node per callable (Free / Method / Lambda)
/// + one site per call expression. Port of v5 `go_walk_call_defs` +
/// `go_walk_call_sites`. Commit C fills this in.
fn project_call(
    _root: tree_sitter::Node,
    _src: &[u8],
    _strings: &mut Strings,
    _sink: &mut FamilyBundle<CallF>,
) {
    // Commit C: go_walk_call_defs + go_walk_call_sites.
}

// ════════════════════════════════════════════════════════════════════════════
// DfF: intra-procedural value flow (nodes + Direct edges). Commit D.
//
// Ports v5 `go_dataflow_from` (src/graph/typegraph/go.rs:576). Every value-bearing
// position in a callable's body becomes a NODE; local value flow becomes a
// Direct EDGE. Commit D fills this in.
// ════════════════════════════════════════════════════════════════════════════

/// Project the DfF family: each callable's body lifted to its value-flow graph.
/// Port of v5 `go_dataflow_from`. Commit D fills this in.
fn project_df(
    _root: tree_sitter::Node,
    _src: &[u8],
    _strings: &mut Strings,
    _sink: &mut FamilyBundle<DfF>,
) {
    // Commit D: go_dataflow_from / flow_go.
}

// ════════════════════════════════════════════════════════════════════════════
// GoSource: the Go Source (cst via ast-grep + type/call/df via tree-sitter-go).
//
// The two-parser, masked shape (mirrors RustSource/TsSource). cst runs through
// ast-grep (one dep = the CST floor for every lang); type/call/df run through
// ONE tree-sitter-go parse (three masked projections over the same tree). ONE
// shared `Strings` across all four families.
// ════════════════════════════════════════════════════════════════════════════

/// The Go `Source`. `matches` = the path ends in `.go`. cst via ast-grep's go
/// grammar; type/call/df via one tree-sitter-go parse.
#[derive(Default)]
pub struct GoSource;

impl Source for GoSource {
    fn name(&self) -> &'static str {
        "go"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".go")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). ast-grep's SupportLang has a go grammar, so
        // a .go parses losslessly. Owns its () arena; dropped at block end. A
        // failed ast-grep parse leaves cst None (no panic).
        let cst = if mask.cst {
            let arena = AstGrepParser.make_arena();
            AstGrepParser.parse(&arena, path, content).ok().map(|parsed| {
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                bundle
            })
        } else {
            None
        };

        // type/call/df via ONE tree-sitter-go parse (masked). Byte spans come
        // straight off the tree-sitter nodes (no line/col bridge, unlike syn). A
        // failed parse leaves all three None (partial output: cst above may be
        // Some).
        let mut types = None;
        let mut call = None;
        let mut df = None;
        if mask.types || mask.call || mask.df {
            if let Ok(src) = std::str::from_utf8(content) {
                if let Some(tree) = go_parse(src) {
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
                    if mask.df {
                        let mut bundle = FamilyBundle::<DfF>::default();
                        project_df(root, src_bytes, &mut strings, &mut bundle);
                        df = Some(bundle);
                    }
                }
            }
        }

        ExtractOutput { strings, cst, types, call, df }
    }
}
