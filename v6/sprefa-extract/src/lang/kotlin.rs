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

use std::collections::BTreeSet;

use crate::family::{CallF, CstF, DfF, SigSlot, TypeEntityKind, TypeF, TypeSig};
use crate::rows::{FamilyBundle, Node};
use crate::seams::{Parser, Project};
use crate::shape::{Span, Strings};
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

/// UTF-8 text of a tree-sitter node. Port of v5's inline `utf8_text` calls.
fn kt_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
fn node_span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

/// The first direct child of `node` with `kind`. Port of v5 `kt_first_child`.
fn kt_first_child<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let kids: Vec<tree_sitter::Node<'a>> = node.children(&mut cursor).collect();
    kids.into_iter().find(|c| c.kind() == kind)
}

// ════════════════════════════════════════════════════════════════════════════
// TypeF: entity nodes + arrow-type sigs. Commit B.
//
// Ports v5 `walk_kotlin_entities` (src/graph/typegraph/kotlin.rs:741) +
// `kotlin_fn_type` (kotlin.rs:847, the arrow-type payload). Entities:
// class_declaration / object_declaration / companion_object with a direct
// type_identifier child -> Class/Interface/Enum (keyword scan of the decl's own
// children: an `interface` keyword -> Interface, an `enum` keyword -> Enum,
// else Class); EVERY function_declaration (top-level, member, or local - v5
// never mints a Method entity for kotlin) -> Function + arrow sigs.
//
// v6 drops v5's `sym`/`parent`/`file`/`line`: a node is span+kind+name; the
// entity span is anchored at the decl node's start byte so
// `line_of(span.start)` equals v5's reported `entity.line` (the decl start
// row, 1-based). The type-edge candidates (`kotlin_decl_edges`: field/impl/
// variant/generic rows v5 kotlin DOES emit) are DEFERRED to the traits/codegen
// arc with `Resolve<TypeF>` - this port ships phase 1 only. No const facet
// (v5 kotlin leaves `consts` at Default; v6 matches by emitting none).
// ════════════════════════════════════════════════════════════════════════════

/// Project the TypeF family: one entity node per class/object/fun declaration
/// + an arrow-type sig per callable param/return type reference. Port of v5
/// `walk_kotlin_entities` + `kotlin_fn_type`.
fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    walk_kotlin_entities(root, src, strings, sink);
}

/// Walk every declaration, minting one entity node per class/object/fun decl.
/// Port of v5 `walk_kotlin_entities`. Recurses everywhere: member funs, local
/// funs, nested classes, and companion objects all mint (v5's walk has no
/// depth gate). `companion_object` is a distinct grammar node from
/// `object_declaration`; both mint a `class` entity the same way.
fn walk_kotlin_entities(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_declaration" | "object_declaration" | "companion_object" => {
                let mut c = child.walk();
                let kids: Vec<tree_sitter::Node> = child.children(&mut c).collect();
                if let Some(id) = kids.iter().find(|n| n.kind() == "type_identifier") {
                    let name = kt_text(*id, src).to_string();
                    let kind = if kids.iter().any(|n| n.kind() == "interface") {
                        TypeEntityKind::Interface
                    } else if kids.iter().any(|n| n.kind() == "enum") {
                        TypeEntityKind::Enum
                    } else {
                        TypeEntityKind::Class
                    };
                    push_entity(sink, strings, node_span(child), &name, kind);
                }
            }
            "function_declaration" => {
                if let Some(id) = kt_first_child(child, "simple_identifier") {
                    let name = kt_text(id, src).to_string();
                    let span = node_span(child);
                    push_entity(sink, strings, span, &name, TypeEntityKind::Function);
                    fn_sigs(sink, strings, span, child, src);
                }
            }
            _ => {}
        }
        walk_kotlin_entities(child, src, strings, sink);
    }
}

fn push_entity(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    span: Span,
    name: &str,
    kind: TypeEntityKind,
) {
    sink.nodes.push(Node::new(span, kind).with_name(strings.intern(name)));
}

// ── arrow-type signatures (port of v5 `kotlin_fn_type`) ─────────────────────

/// The arrow-type sigs of one `fun`: param type-refs (one slot per `parameter`
/// child of `function_value_parameters`, positional) + return type-refs (all
/// unioned into slot pos=0). Declared type-parameter names and Kotlin builtins
/// are excluded from refs. Port of v5 `kotlin_fn_type`, incl. its overwrite
/// semantics: the LAST type-node child fills `ret` (an extension receiver's
/// user_type reads as `ret` until the real return type overwrites it - a
/// builtin receiver like `String` noise-filters to empty; V5-IS-CORRECT ports
/// the quirk). `owner` is the callable entity's span (the sig join key).
fn fn_sigs(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    node: tree_sitter::Node,
    src: &[u8],
) {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();

    // Declared type-parameter names: excluded from refs, like the decl pass.
    let mut tparams: BTreeSet<String> = BTreeSet::new();
    for n in &children {
        if n.kind() != "type_parameters" {
            continue;
        }
        let mut c = n.walk();
        for tp in n.children(&mut c).filter(|n| n.kind() == "type_parameter") {
            if let Some(name) = kt_first_child(tp, "type_identifier") {
                tparams.insert(kt_text(name, src).to_string());
            }
        }
    }

    let mut pos: u32 = 0;
    let mut ret_refs: Vec<String> = Vec::new();
    for n in &children {
        match n.kind() {
            "function_value_parameters" => {
                let mut c = n.walk();
                for p in n.children(&mut c).filter(|n| n.kind() == "parameter") {
                    // The parameter's name is a simple_identifier (not collected:
                    // collect_kotlin_refs only reads user_type); its type recurses.
                    for name in kotlin_type_refs(p, src, &tparams) {
                        push_sig(sink, strings, owner, SigSlot::Param, pos, &name);
                    }
                    pos += 1;
                }
            }
            // The return type is a type-node sibling after the parameter list.
            k if is_kotlin_type_node(k) => ret_refs = kotlin_type_refs(*n, src, &tparams),
            _ => {}
        }
    }
    for name in ret_refs {
        push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
    }
}

fn push_sig(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    slot: SigSlot,
    pos: u32,
    name: &str,
) {
    sink.aux.sigs.push(TypeSig { owner, slot, pos, ty: strings.intern(name) });
}

fn is_kotlin_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "user_type" | "nullable_type" | "function_type" | "parenthesized_type"
    )
}

/// Collect the type names referenced anywhere under `node`, de-duplicated and
/// sorted: each `user_type`'s own dotted path is one ref, its `type_arguments`
/// recurse into more refs. Declared type-parameter names and Kotlin builtins
/// are not refs. Port of v5 `kotlin_type_refs`/`collect_kotlin_refs`.
fn kotlin_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_kotlin_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_kotlin_refs(
    node: tree_sitter::Node,
    src: &[u8],
    params: &BTreeSet<String>,
    out: &mut Vec<String>,
) {
    if node.kind() == "user_type" {
        let mut cursor = node.walk();
        let segs: Vec<&str> = node
            .children(&mut cursor)
            .filter(|n| n.kind() == "type_identifier")
            .map(|n| kt_text(n, src))
            .collect();
        let name = segs.join(".");
        if !name.is_empty() && !params.contains(&name) && !is_noise_kotlin(&name) {
            out.push(name);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor).filter(|n| n.kind() != "type_identifier") {
            collect_kotlin_refs(child, src, params, out);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_kotlin_refs(child, src, params, out);
    }
}

/// Kotlin builtin type filter: a reference to `Int`/`String`/`Unit`/... carries
/// no resolvable declaration. Port of v5 `is_noise_kotlin`.
fn is_noise_kotlin(name: &str) -> bool {
    matches!(
        name,
        "Int" | "Long" | "Short" | "Byte" | "Float" | "Double" | "Boolean" | "Char"
            | "String" | "Unit" | "Any" | "Nothing"
    )
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
