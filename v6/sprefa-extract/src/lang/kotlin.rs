//! The Kotlin extractor arm: tree-sitter-kotlin front-end for type/call/df,
//! ast-grep for cst. Mirrors GoSource (the "floor as the only tier" shape -
//! kotlin has no syn/oxc analog either): cst via ast-grep's kotlin grammar +
//! one tree-sitter-kotlin parse feeding the type/call/df projections.
//!
//! The grammar crate is `tree-sitter-kotlin-sg` (the ast-grep fork), NOT
//! `tree-sitter-kotlin`: it is the exact crate v5's kotlin front-end carries
//! (root Cargo.toml: `tree-sitter-kotlin-sg = "0.4"`, so the v6 parse is
//! byte-identical to the oracle's), it is already in this workspace's lock as
//! an ast-grep-language transitive (0.4.1, one copy), and it exports
//! `LANGUAGE: LanguageFn` the way tree-sitter-go 0.23 does, which tree-sitter
//! 0.25's `Language::new` wraps. Zero new dup risk (it deps only
//! `tree-sitter-language` + `cc`, no tree-sitter core).
//!
//! Span bridge: NONE needed (same as go.rs; unlike rust.rs's syn line/col ->
//! byte table). tree-sitter nodes give raw byte offsets directly
//! (`start_byte`/`end_byte`), so `Span { start: node.start_byte(), len:
//! node.end_byte() - node.start_byte() }` is the whole story.
//!
//! Commit A (skeleton): KotlinSource wires cst via ast-grep + a
//! tree-sitter-kotlin parse; type/call/df projections are stubbed empty.
//! Commit B ports `walk_kotlin_entities` + `kotlin_fn_type` (TypeF nodes +
//! arrow-type sigs); commit C ports `kt_walk_call_defs` + `kt_walk_call_sites`
//! (CallF); commit D ports `kotlin_dataflow_from` (DfF nodes + Direct edges,
//! incl. the `lam_sym` closure naming).
//!
//! Deferred follow-ups (the same set the other langs parked): the docs facet
//! (`walk_kotlin_docs`); the df enrichment aux (args/fields/lits/param_pos/
//! loops/nests); the type_edge candidates (`kotlin_decl_edges`) +
//! `Resolve<TypeF>`/`Resolve<CallF>` arms - v5 kotlin DOES emit type_edge, and
//! both resolve arms land with the traits/codegen arc, not this port. The
//! const facet is NOT ported: v5 kotlin emits no const entities and no
//! const_value rows (`extract` leaves `consts` at Default), so v6 matches by
//! emitting none either.

use crate::family::{CstF, CallF, DfF, TypeF};
use crate::rows::FamilyBundle;
use crate::seams::{Parser, Project};
use crate::shape::Strings;
use crate::source::{ExtractOutput, FamilyMask, Source};
use super::astgrep::{AstGrepParser, CstProjector};

// ── the tree-sitter-kotlin parse (one parse feeds type/call/df) ─────────────

/// Parse Kotlin source via tree-sitter-kotlin-sg. Port of v5's inline parse in
/// `KotlinTypes::extract` (src/graph/typegraph/kotlin.rs:13). tree-sitter
/// 0.25's `Language::new` wraps the `LanguageFn` tree-sitter-kotlin-sg 0.4
/// exports as `LANGUAGE`; the versions unify with what ast-grep-language
/// already transitively pulls (one copy, 0.4.1).
fn kt_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// Project the TypeF family. Commit B ports `walk_kotlin_entities` +
/// `kotlin_fn_type`.
fn project_types(
    _root: tree_sitter::Node,
    _src: &[u8],
    _strings: &mut Strings,
    _sink: &mut FamilyBundle<TypeF>,
) {
}

/// Project the CallF family. Commit C ports `kt_walk_call_defs` +
/// `kt_walk_call_sites`.
fn project_call(
    _root: tree_sitter::Node,
    _src: &[u8],
    _strings: &mut Strings,
    _sink: &mut FamilyBundle<CallF>,
) {
}

/// Project the DfF family. Commit D ports `kotlin_dataflow_from`.
fn project_df(
    _root: tree_sitter::Node,
    _src: &[u8],
    _file: &str,
    _strings: &mut Strings,
    _sink: &mut FamilyBundle<DfF>,
) {
}

// ════════════════════════════════════════════════════════════════════════════
// KotlinSource: the Kotlin Source (cst via ast-grep + type/call/df via
// tree-sitter-kotlin).
//
// The two-parser, masked shape (mirrors GoSource/RustSource/TsSource). cst runs
// through ast-grep (one dep = the CST floor for every lang); type/call/df run
// through ONE tree-sitter-kotlin parse (three masked projections over the same
// tree). ONE shared `Strings` across all four families.
// ════════════════════════════════════════════════════════════════════════════

/// The Kotlin `Source`. `matches` = the path ends in `.kt` or `.kts` (v5
/// `KotlinTypes::matches`). cst via ast-grep's kotlin grammar; type/call/df via
/// one tree-sitter-kotlin parse.
#[derive(Default)]
pub struct KotlinSource;

impl Source for KotlinSource {
    fn name(&self) -> &'static str {
        "kotlin"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".kt") || path.ends_with(".kts")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). ast-grep's SupportLang has a kotlin
        // grammar (the same tree-sitter-kotlin-sg crate), so a .kt parses
        // losslessly. Owns its () arena; dropped at block end. A failed
        // ast-grep parse leaves cst None (no panic).
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

        // type/call/df via ONE tree-sitter-kotlin parse (masked). Byte spans
        // come straight off the tree-sitter nodes (no line/col bridge, unlike
        // syn). A failed parse leaves all three None (partial output: cst
        // above may be Some).
        let mut types = None;
        let mut call = None;
        let mut df = None;
        if mask.types || mask.call || mask.df {
            if let Ok(src) = std::str::from_utf8(content) {
                if let Some(tree) = kt_parse(src) {
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
                        project_df(root, src_bytes, path, &mut strings, &mut bundle);
                        df = Some(bundle);
                    }
                }
            }
        }

        ExtractOutput { strings, cst, types, call, df }
    }
}
