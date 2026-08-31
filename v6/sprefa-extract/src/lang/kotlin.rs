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
//! KotlinSource wires cst via ast-grep + a
//! tree-sitter-kotlin parse; type/call/df projections are stubbed empty.
//! `walk_kotlin_entities` + `kotlin_fn_type` cover TypeF (nodes +
//! arrow-type sigs); `kt_walk_call_defs` + `kt_walk_call_sites` cover CallF;
//! `kotlin_dataflow_from` covers DfF (nodes + Direct edges,
//! incl. the `lam_sym` closure naming).
//!
//! Deferred follow-ups (the same set the other langs parked): df literal/loop/
//! nesting aux. Named-argument field names are emitted. The type_edge
//! candidates (`kotlin_decl_edges`) +
//! `Resolve<TypeF>` land here (v5 kotlin DOES emit type_edge); `Resolve<CallF>`
//! lives with the call port. The const facet is NOT ported: v5 kotlin emits no
//! const entities and no
//! const_value rows (`extract` leaves `consts` at Default), so v6 matches by
//! emitting none either.
// @comment-ok: the module header is a crate-level doc block predating the rail

use std::collections::BTreeSet;

use super::astgrep::{AstGrepParser, CstProjector};
use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstF, DfArg, DfEdgeKind, DfF, DfField, DfNodeKind,
    DfParam, DocFact, DocTag, ProjectEdge, SigSlot, Specifier, SpecifierKind, TypeEdgeCandidate,
    TypeEdgeKind, TypeEntityKind, TypeF, TypeSig,
    ResolutionOrigin,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{DefIndex, Parser, Project, Resolve, corpus_defs, covering_def, def_named};
use crate::shape::{ContentId, FamilyTag, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::source::{ExtractOutput, FamilyMask, ProjectCx, Source};
use crate::trace;

// ── the tree-sitter-kotlin parse (one parse feeds type/call/df) ─────────────

/// Parse Kotlin source via tree-sitter-kotlin-sg. Port of v5's inline parse in
/// `KotlinTypes::extract` (src/graph/typegraph/kotlin.rs:13). tree-sitter
/// 0.25's `Language::new` wraps the `LanguageFn` tree-sitter-kotlin-sg 0.4
/// exports as `LANGUAGE`; the versions unify with what ast-grep-language
/// already transitively pulls (one copy, 0.4.1).
pub(crate) fn kt_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// UTF-8 text of a tree-sitter node. Port of v5's inline `utf8_text` calls.
pub(crate) fn kt_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
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
pub(crate) fn kt_first_child<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let kids: Vec<tree_sitter::Node<'a>> = node.children(&mut cursor).collect();
    kids.into_iter().find(|c| c.kind() == kind)
}

// ════════════════════════════════════════════════════════════════════════════
// TypeF: entity nodes + arrow-type sigs.
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
// variant/generic rows v5 kotlin DOES emit) are collected in phase 1 and bound
// by `Resolve<TypeF>`. No const facet
// (v5 kotlin leaves `consts` at Default; v6 matches by emitting none).
// @comment-ok: pre-existing TypeF section header block
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
                    let span = node_span(child);
                    push_entity(sink, strings, span, &name, kind);
                    // v5 walk_kotlin docs + edges walk class/object only, not
                    // companion (kotlin_decl_edges runs on class/object only).
                    if child.kind() != "companion_object" {
                        kt_decl_edges(child, span, src, strings, sink);
                        if let Some(text) = kotlin_leading_kdoc(child, src) {
                            push_kt_doc(sink, strings, span, &text);
                        }
                    }
                }
            }
            "function_declaration" => {
                if let Some(id) = kt_first_child(child, "simple_identifier") {
                    let name = kt_text(id, src).to_string();
                    let span = node_span(child);
                    push_entity(sink, strings, span, &name, TypeEntityKind::Function);
                    if let Some(text) = kotlin_leading_kdoc(child, src) {
                        push_kt_doc(sink, strings, span, &text);
                    }
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
    sink.nodes
        .push(Node::new(span, kind).with_name(strings.intern(name)));
}

// ── type-edge candidates (the Resolve<TypeF> input) ──────────────────────────

/// The type-edge candidates for one class/object decl: field/impl/generic/
/// variant rows, `to` as written (a `Cache<Item>` yields Cache AND Item; a
/// bare `ctor: Wire` primary-ctor arg with no val/var is not part of the
/// shape). Port of v5 `kotlin_decl_edges` (src/graph/typegraph/kotlin.rs:649);
/// `Resolve<TypeF>` binds them. `owner` is the decl node's span (the entity
/// join key).
// @comment-ok: function doc block mirroring the go/rust decl-edge walkers
fn kt_decl_edges(
    decl: tree_sitter::Node,
    owner: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut cursor = decl.walk();
    let children: Vec<tree_sitter::Node> = decl.children(&mut cursor).collect();

    let Some(owner_name) = children
        .iter()
        .find(|n| n.kind() == "type_identifier")
        .map(|n| kt_text(*n, src))
    else {
        return;
    };

    // Keyword-level split: `interface` is an anonymous token under the same
    // class_declaration node kind as `class`; its supertypes are generic.
    let is_interface = children.iter().any(|n| n.kind() == "interface");
    let super_kind = if is_interface {
        TypeEdgeKind::Generic
    } else {
        TypeEdgeKind::Impl
    };

    // Declared type-parameter names: their bounds are generic edges and the
    // names themselves are not type refs.
    let mut params: BTreeSet<String> = BTreeSet::new();
    for n in &children {
        if n.kind() != "type_parameters" {
            continue;
        }
        let mut c = n.walk();
        for tp in n.children(&mut c).filter(|n| n.kind() == "type_parameter") {
            let mut cc = tp.walk();
            let kids: Vec<tree_sitter::Node> = tp.children(&mut cc).collect();
            if let Some(name) = kids.iter().find(|n| n.kind() == "type_identifier") {
                params.insert(kt_text(*name, src).to_string());
            }
            for bound in kids.iter().filter(|n| n.kind() != "type_identifier") {
                for to in kotlin_type_refs(*bound, src, &params) {
                    push_kt_candidate(sink, strings, owner, &to, TypeEdgeKind::Generic);
                }
            }
        }
    }

    for n in &children {
        match n.kind() {
            "delegation_specifier" => {
                // constructor_invocation = superclass call, bare user_type =
                // interface; both are supertypes, kind set by the owner flavor.
                for to in kotlin_type_refs(*n, src, &params) {
                    push_kt_candidate(sink, strings, owner, &to, super_kind);
                }
            }
            "primary_constructor" => {
                let mut c = n.walk();
                for param in n.children(&mut c).filter(|n| n.kind() == "class_parameter") {
                    let mut cc = param.walk();
                    let kids: Vec<tree_sitter::Node> = param.children(&mut cc).collect();
                    // val/var (binding_pattern_kind) makes it a field; a bare
                    // constructor arg is not part of the type's shape.
                    if !kids.iter().any(|n| n.kind() == "binding_pattern_kind") {
                        continue;
                    }
                    for kid in kids.iter().filter(|n| n.kind() != "simple_identifier") {
                        for to in kotlin_type_refs(*kid, src, &params) {
                            push_kt_candidate(sink, strings, owner, &to, TypeEdgeKind::Field);
                        }
                    }
                }
            }
            "class_body" => {
                let mut c = n.walk();
                for prop in n
                    .children(&mut c)
                    .filter(|n| n.kind() == "property_declaration")
                {
                    let mut cc = prop.walk();
                    for vd in prop
                        .children(&mut cc)
                        .filter(|n| n.kind() == "variable_declaration")
                    {
                        for to in kotlin_type_refs(vd, src, &params) {
                            push_kt_candidate(sink, strings, owner, &to, TypeEdgeKind::Field);
                        }
                    }
                }
            }
            "enum_class_body" => {
                let mut c = n.walk();
                for entry in n.children(&mut c).filter(|n| n.kind() == "enum_entry") {
                    let mut cc = entry.walk();
                    let name = entry
                        .children(&mut cc)
                        .find(|n| n.kind() == "simple_identifier");
                    if let Some(name) = name {
                        let variant = format!("{owner_name}::{}", kt_text(name, src));
                        push_kt_candidate(sink, strings, owner, &variant, TypeEdgeKind::Variant);
                    }
                }
            }
            _ => {}
        }
    }
}

fn push_kt_candidate(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    to: &str,
    kind: TypeEdgeKind,
) {
    sink.aux.candidates.push(TypeEdgeCandidate {
        owner,
        to: strings.intern(to),
        kind,
    });
}

// ── doc facet (port of v5 `walk_kotlin_docs`) ────────────────────────────────

/// Push one DocFact for a documented decl. Kotlin docs carry no parent (v5
/// mints method docs with no owner).
fn push_kt_doc(sink: &mut FamilyBundle<TypeF>, strings: &mut Strings, span: Span, text: &str) {
    sink.aux.docs.push(DocFact {
        owner: span,
        parent: None,
        text: strings.intern(text),
        tags: parse_jsdoc_tags(text, strings),
    });
}

/// The cleaned KDoc block directly above `node`, or None: a `*comment*` previous
/// sibling whose text opens with `/**`. Port of v5 `kotlin_leading_kdoc`.
fn kotlin_leading_kdoc(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let prev = node.prev_sibling()?;
    if !prev.kind().contains("comment") {
        return None;
    }
    let raw = prev.utf8_text(src).ok()?;
    if !raw.trim_start().starts_with("/**") {
        return None;
    }
    Some(clean_block_comment(raw))
}

/// Strip a `/** ... */` block down to its prose. Port of v5 `clean_block_comment`.
fn clean_block_comment(raw: &str) -> String {
    let inner = raw.trim();
    let inner = inner
        .strip_prefix("/**")
        .or_else(|| inner.strip_prefix("/*!"))
        .or_else(|| inner.strip_prefix("/*"))
        .unwrap_or(inner);
    let inner = inner.strip_suffix("*/").unwrap_or(inner);
    let mut lines: Vec<String> = inner
        .lines()
        .map(|l| {
            let t = l.trim_start();
            let t = t.strip_prefix('*').unwrap_or(t);
            t.strip_prefix(' ').unwrap_or(t).to_string()
        })
        .collect();
    while lines.first().is_some_and(|s| s.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|s| s.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Split a KDoc block into `@tag` rows; named tags carry a leading name into
/// `arg`, a leading `{type}` annotation is dropped. Port of v5 `parse_jsdoc_tags`.
fn parse_jsdoc_tags(text: &str, strings: &mut Strings) -> Vec<DocTag> {
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim_start();
        let Some(rest) = l.strip_prefix('@') else {
            continue;
        };
        let mut it = rest.splitn(2, char::is_whitespace);
        let tag = it.next().unwrap_or("").to_string();
        let mut body = it.next().unwrap_or("").trim_start();
        if body.starts_with('{') {
            if let Some(end) = body.find('}') {
                body = body[end + 1..].trim_start();
            }
        }
        let named = matches!(
            tag.as_str(),
            "param"
                | "arg"
                | "argument"
                | "property"
                | "prop"
                | "throws"
                | "exception"
                | "typeparam"
                | "tparam"
        );
        let (arg, desc) = if named {
            let mut bi = body.splitn(2, char::is_whitespace);
            (
                bi.next().unwrap_or("").to_string(),
                bi.next().unwrap_or("").trim().to_string(),
            )
        } else {
            (String::new(), body.trim().to_string())
        };
        out.push(DocTag {
            tag: strings.intern(&tag),
            arg: if arg.is_empty() {
                None
            } else {
                Some(strings.intern(&arg))
            },
            text: strings.intern(&desc),
        });
    }
    out
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
    sink.aux.sigs.push(TypeSig {
        owner,
        slot,
        pos,
        ty: strings.intern(name),
    });
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
        for child in node
            .children(&mut cursor)
            .filter(|n| n.kind() != "type_identifier")
        {
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
        "Int"
            | "Long"
            | "Short"
            | "Byte"
            | "Float"
            | "Double"
            | "Boolean"
            | "Char"
            | "String"
            | "Unit"
            | "Any"
            | "Nothing"
    )
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux).
//
// Ports v5 `kt_walk_call_defs` (defs, incl. ctors + lambda literals) +
// `kt_walk_call_sites`/`kt_callee` (sites). v5's `sym`/`end` line are dropped:
// a def is span + kind + name (the name is the bare identifier for callee
// resolution, NOT a qualified sym). The def span COVERS its body (decl start
// -> function_body end) so the seam's span-containment can bind a site's
// caller; the parity line reads `line_of(span.start)` = the decl start line
// (v5's `def.line`). Kind rules (v5-exact): a fun inside a class/object body
// is a Method (a nested LOCAL fun is Free - descending into a fn body resets
// the owner); primary/secondary constructors are Method rows named after the
// CLASS (so a `Widget(x)` call site name-matches here); a lambda literal
// inside a fn body is a nameless Lambda (a property-init lambda has no
// enclosing fn scope and is skipped). A fn with no name (an anonymous
// `fun(x) {}` expression) still mints a def with an empty name, like v5.
// ════════════════════════════════════════════════════════════════════════════

/// Project the CallF family: one def node per callable (Free / Method /
/// Lambda) + one site per call expression. Port of v5 `kt_walk_call_defs` +
/// `kt_walk_call_sites`.
fn project_call(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    kt_walk_call_defs(root, src, strings, sink, None, false);
    kt_walk_call_sites(root, src, strings, sink);
    kt_module_specifiers(root, src, strings, sink);
}

// ── module specifiers (CallFAux.specifiers) ─────────────────────────────────
// @comment-ok: the kind/name/module contract, pinned row-for-row by
// tests/26_kotlin_specifiers.rs. `Default`, `SideEffect` and `Reexport` are
// unreachable from kotlin.
//
// | kotlin source                    | kind      | name  | module                  |
// |----------------------------------|-----------|-------|-------------------------|
// | `import kotlin.collections.List` | Named     | List  | kotlin.collections.List |
// | `import java.util.Map as JMap`   | Named     | JMap  | java.util.Map           |
// | `import kotlin.text.*`           | Namespace | text  | kotlin.text             |
//
// Kotlin imports are symbol-level like rust's `use`, so `module` carries the
// full path as written (`src/graph/modgraph/kotlin.rs:203-207`). The alias
// overrides the bound name; a wildcard binds the last dotted segment.

/// Kotlin module specifiers: one row per `import_header`. Rides the one
/// tree-sitter parse `project_call` already holds. v5 reads the same facts
/// with a regex over stripped text (`src/graph/modgraph/kotlin.rs:19-26`).
fn kt_module_specifiers(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut rows = Vec::new();
    kt_walk_import_headers(root, src, strings, &mut rows);
    sink.aux.specifiers.extend(rows);
}

/// Recurse the tree for every `import_header` node, appending one row each.
pub(crate) fn kt_walk_import_headers(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    rows: &mut Vec<Specifier>,
) {
    if node.kind() == "import_header" {
        let identifier = kt_child_kind(node, "identifier");
        let path = identifier.map(|child| kt_text(child, src)).unwrap_or("");
        let span = match identifier {
            Some(identifier) => Span {
                start: identifier.start_byte() as u32,
                len: (node.end_byte() - identifier.start_byte()) as u32,
            },
            None => node_span(node),
        };
        let (kind, name) = if let Some(alias) = kt_child_kind(node, "import_alias") {
            let alias_text = kt_child_kind(alias, "type_identifier")
                .map(|child| kt_text(child, src))
                .unwrap_or("");
            (SpecifierKind::Named, alias_text)
        } else if kt_child_kind(node, "wildcard_import").is_some() {
            (SpecifierKind::Namespace, last_segment(path))
        } else {
            (SpecifierKind::Named, last_segment(path))
        };
        rows.push(Specifier {
            span,
            name: strings.intern(name),
            kind,
            module: Some(strings.intern(path)),
            imported: None,
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        kt_walk_import_headers(child, src, strings, rows);
    }
}

/// The first named child of `node` with `kind`.
pub(crate) fn kt_child_kind<'a>(
    node: tree_sitter::Node<'a>,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    node.named_children(&mut node.walk())
        .find(|child| child.kind() == kind)
}

/// The segment after the last `.` of a dotted path, or the whole text when the
/// path has no dot.
fn last_segment(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

/// Walk every callable declaration, minting one def node per Free function /
/// Method / Lambda. Port of v5 `kt_walk_call_defs`. `parent` is the enclosing
/// class/object name (v5's `parent`); `in_fn` is v5's `!enclosing.is_empty()`:
/// a lambda literal only mints a Lambda def when inside a fn/lambda body (a
/// property-init lambda has no enclosing scope to join). v6 drops the sym
/// strings themselves - only the gate survives, the def's span is its only
/// coordinate.
fn kt_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    parent: Option<&str>,
    in_fn: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "class_declaration" | "object_declaration" => {
                let owner = kt_first_child(child, "type_identifier").map(|n| kt_text(n, src));
                // A class body is not a fn scope: reset `in_fn` (v5 resets
                // `enclosing` to "") so a bare property-init lambda is skipped;
                // a member fun opens its own scope below.
                kt_walk_call_defs(child, src, strings, sink, owner, false);
            }
            // @callable kotlin function / @callable kotlin method
            "function_declaration" => {
                let name = kt_first_child(child, "simple_identifier")
                    .map(|n| kt_text(n, src).to_string())
                    .unwrap_or_default();
                let kind = match parent {
                    Some(_) => CallKind::Method,
                    None => CallKind::Free,
                };
                // The def span covers the whole callable body [decl.start,
                // body.end) for span-containment resolution; abstract/interface
                // funs have no body, so fall back to the decl end (v5's exact
                // fallback). line_of(span.start) == v5's def.line.
                let span = def_span(child);
                sink.nodes
                    .push(Node::new(span, kind).with_name(strings.intern(&name)));
                // v5 threads the fn's df_sym as `enclosing` and resets the
                // owner: a nested local fun is Free, not a method.
                kt_walk_call_defs(child, src, strings, sink, None, true);
            }
            // Primary/secondary constructors: Method rows named after the
            // class, so a `Widget(x)` call site resolves here via the bare-name
            // convention. Only minted inside a class/object body (parent Some).
            // @callable kotlin method
            "primary_constructor" | "secondary_constructor" => {
                if let Some(owner) = parent {
                    sink.nodes.push(
                        Node::new(node_span(child), CallKind::Method)
                            .with_name(strings.intern(owner)),
                    );
                }
                kt_walk_call_defs(child, src, strings, sink, parent, in_fn);
            }
            // `{ it + 1 }` inside a fn body: a nameless Lambda def (v5 keys it
            // by the same `lambda_sym` the df lift mints; v6 keeps only the
            // span - the df closure VALUE node carries that name instead).
            // @callable kotlin lambda
            "lambda_literal" if in_fn => {
                sink.nodes
                    .push(Node::new(node_span(child), CallKind::Lambda));
                kt_walk_call_defs(child, src, strings, sink, parent, true);
            }
            _ => kt_walk_call_defs(child, src, strings, sink, parent, in_fn),
        }
    }
}

/// The def span covers the whole callable `[child.start, body.end)` for
/// span-containment resolution. Port of v5's `end` computation (the
/// function_body end, or the decl end for a bodyless fun).
fn def_span(child: tree_sitter::Node) -> Span {
    let start = child.start_byte();
    let end = kt_first_child(child, "function_body")
        .unwrap_or(child)
        .end_byte();
    Span {
        start: start as u32,
        len: (end - start) as u32,
    }
}

/// Walk every call-shaped node, minting one call site per call. Port of v5
/// `kt_walk_call_sites` plus the operator/infix/invoke sites v5 dropped. The
/// site span is the LEAD callee node's span (line_of(span.start) = v5's
/// reported site line - for `recv.m()` the navigation_expression's start, NOT
/// the suffix's). Operator-shaped calls span their operator token (or the
/// infix name), so `--resolve` joins them to the `operator fun` /
/// `infix fun` def by name:
///  - `a infixName b`  -> infix_expression, callee = the infix name
///  - `a + b` etc.     -> additive/multiplicative/range/comparison/equality
///                        expression, callee = the operator-function name
///  - `a in b`         -> check_expression, callee = contains
///  - `-a` `!a` `++a`  -> prefix_expression (unaryMinus/unaryPlus/not/inc/dec)
///  - `a++` `a--`      -> postfix_expression (inc/dec)
///  - `a[i]`           -> indexing_suffix, callee = get (`a[i] = v` -> set)
///  - `a += b` etc.    -> assignment (plusAssign/minusAssign/...)
///  - `f(x)()`         -> call_expression over a call_expression, callee =
///                        invoke, span = the `()` call_suffix
fn kt_walk_call_sites(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "call_expression" => {
                if let Some((callee, lead)) = kt_callee(child, src) {
                    kt_push_site(node_span(lead), &callee, strings, sink);
                } else {
                    // An invoked expression value: `f(x)()` calls `invoke`
                    // on the result of the inner call.
                    let mut lead_cur = child.walk();
                    let lead_kind = child
                        .children(&mut lead_cur)
                        .find(|c| c.kind() != "call_suffix")
                        .map(|l| l.kind());
                    if lead_kind == Some("call_expression") {
                        if let Some(suffix) = kt_first_child(child, "call_suffix") {
                            kt_push_site(node_span(suffix), "invoke", strings, sink);
                        }
                    }
                }
            }
            // `1 plus2 2`: seq(expr, simple_identifier, expr) - the middle
            // child is the infix function name.
            "infix_expression" => {
                let mut infix = child.walk();
                let mid = child.children(&mut infix).nth(1);
                if let Some(name) = mid {
                    if name.kind() == "simple_identifier" {
                        let callee = kt_text(name, src).to_string();
                        kt_push_site(node_span(name), &callee, strings, sink);
                    }
                }
            }
            "additive_expression"
            | "multiplicative_expression"
            | "range_expression"
            | "comparison_expression"
            | "equality_expression" => {
                if let Some(callee) = kt_anon_token(child, src).and_then(|op| kt_operator_name(&op))
                {
                    kt_bin_site(child, callee, strings, sink);
                }
            }
            // `a in b` / `a !in b` both lower to a `contains` site. The `!` of
            // `!in` is its own anonymous token, so scan every anonymous child
            // for the `in` token instead of reading only the first one.
            // `is`/`!is` has no operator fun.
            "check_expression" => {
                let mut cursor = child.walk();
                let is_in = child
                    .children(&mut cursor)
                    .any(|c| !c.is_named() && matches!(kt_text(c, src), "in" | "!in"));
                if is_in {
                    kt_bin_site(child, "contains", strings, sink);
                }
            }
            "prefix_expression" => {
                if let Some(callee) = kt_anon_token(child, src).and_then(|op| kt_prefix_name(&op)) {
                    kt_bin_site(child, callee, strings, sink);
                }
            }
            "postfix_expression" => {
                if let Some(callee) = kt_anon_token(child, src).and_then(|op| kt_postfix_name(&op))
                {
                    kt_bin_site(child, callee, strings, sink);
                }
            }
            "indexing_expression" => {
                if let Some(suffix) = kt_first_child(child, "indexing_suffix") {
                    kt_push_site(node_span(suffix), "get", strings, sink);
                }
            }
            "assignment" => {
                if let Some(callee) = kt_anon_token(child, src).and_then(|op| kt_assign_name(&op)) {
                    kt_bin_site(child, callee, strings, sink);
                }
                // `a[i] = v` lowers to `set` on the index suffix (the lhs is
                // a directly_assignable_expression wrapping the suffix; there
                // is no indexing_expression node in the write position).
                if let Some(lhs) = kt_first_child(child, "directly_assignable_expression") {
                    if let Some(suffix) = kt_first_child(lhs, "indexing_suffix") {
                        kt_push_site(node_span(suffix), "set", strings, sink);
                    }
                }
            }
            _ => {}
        }
        kt_walk_call_sites(child, src, strings, sink);
    }
}

/// Push one call site onto the aux sink.
fn kt_push_site(span: Span, callee: &str, strings: &mut Strings, sink: &mut FamilyBundle<CallF>) {
    sink.aux.sites.push(CallSite {
        span,
        callee: strings.intern(callee),
        callee_path: None,
    });
}

/// Mint an operator site spanned by the node's anonymous operator token
/// (`a + b` spans the `+`); fall back to the whole node when the grammar
/// folds the operator into a named child.
fn kt_bin_site(
    expr: tree_sitter::Node,
    callee: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let span = kt_anon_token_node(expr)
        .map(node_span)
        .unwrap_or_else(|| node_span(expr));
    kt_push_site(span, callee, strings, sink);
}

/// The text of the node's first anonymous (non-named) child, if any.
fn kt_anon_token(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    Some(kt_text(kt_anon_token_node(node)?, src).to_string())
}

fn kt_anon_token_node<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| !c.is_named());
    found
}

/// Binary/infix operator token -> Kotlin operator-function name.
fn kt_operator_name(op: &str) -> Option<&'static str> {
    Some(match op {
        "+" => "plus",
        "-" => "minus",
        "*" => "times",
        "/" => "div",
        "%" => "rem",
        ".." => "rangeTo",
        "..<" => "rangeUntil",
        "in" => "contains",
        "==" | "!=" => "equals",
        "<" | ">" | "<=" | ">=" => "compareTo",
        _ => return None,
    })
}

/// Prefix unary operator token -> operator-function name.
fn kt_prefix_name(op: &str) -> Option<&'static str> {
    Some(match op {
        "-" => "unaryMinus",
        "+" => "unaryPlus",
        "!" => "not",
        "++" => "inc",
        "--" => "dec",
        _ => return None,
    })
}

/// Postfix unary operator token -> operator-function name (`!!` is notNull,
/// which has no operator fun).
fn kt_postfix_name(op: &str) -> Option<&'static str> {
    Some(match op {
        "++" => "inc",
        "--" => "dec",
        _ => return None,
    })
}

/// Compound-assignment operator token -> operator-function name.
fn kt_assign_name(op: &str) -> Option<&'static str> {
    Some(match op {
        "+=" => "plusAssign",
        "-=" => "minusAssign",
        "*=" => "timesAssign",
        "/=" => "divAssign",
        "%=" => "remAssign",
        _ => return None,
    })
}

/// (callee name, lead node) for a `call_expression`, or None when the callee
/// is not a plain/navigation name (e.g. an invoked lambda value). Port of v5
/// `kt_callee`: the lead is the call's first child that is not the
/// `call_suffix`; a bare `simple_identifier` is the callee, or the trailing
/// `simple_identifier` of a `navigation_expression` (`recv.qux()` -> "qux").
fn kt_callee<'a>(
    call: tree_sitter::Node<'a>,
    src: &[u8],
) -> Option<(String, tree_sitter::Node<'a>)> {
    let mut cursor = call.walk();
    let lead = call
        .children(&mut cursor)
        .find(|c| c.kind() != "call_suffix")?;
    match lead.kind() {
        "simple_identifier" => Some((kt_text(lead, src).to_string(), lead)),
        "navigation_expression" => {
            let nav = kt_first_child(lead, "navigation_suffix")?;
            let id = kt_first_child(nav, "simple_identifier")?;
            Some((kt_text(id, src).to_string(), lead))
        }
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// DfF: intra-procedural value flow (nodes + Direct edges).
//
// Ports v5 `kotlin_dataflow_from` (src/graph/typegraph/kotlin.rs:73): every
// value-bearing position in a callable's body becomes a NODE; local value flow
// becomes a Direct EDGE. The two-rule model (same as the go/rust lifts):
// value-bearing children flow into their parent, and a `val/var x = rhs`
// binds rhs -> x's slot with later reads flowing slot -> read.
//
// BYTE PARITY: v5 mints each node at `(node.start_position().row, .column)`,
// then bumps rows 1-based (`bump_node_lines_1based`); the oracle reconstructs
// the byte as `line_starts[row_0based] + col`, which equals tree-sitter's
// `node.start_byte()`. So v6 mints each node at the byte DIRECTLY (no line/col
// bridge, no post-pass) and the (kind, var, byte) triples + (from_byte,
// to_byte) edge pairs match v5 exactly. The lambda-tail `ret` sits at the
// lambda's END byte (v5's `node.end_position()`).
//
// What is DROPPED vs v5 (each deliberate, matching the TS/Rust/Go DfF ports):
//  - `fn_sym` ON NODES: the enclosing callable is not stored on every df node;
//    it is threaded through the walk (v5's own mechanism) purely so the
//    `closure` VALUE node carries v5's exact `lam_sym` name
//    (`{file}::function::{fn}::closure::{row}_{col}`, tree-sitter's 0-based
//    row/col of the lambda literal's start; nesting chains - EVERY fun, even a
//    method, roots at `{file}::function::{name}` per v5's kt_flow_fn). No sym
//    store: the name derives from the walk's containment path + span data.
//  - the enrichment aux: `args` (incl. the receiver slot -1 and named-arg
//    source positions), `fields` (named-arg labels), `param_pos`. The EDGES
//    already carry every value flow.
//  - `for`/`while`/`do-while` mint NO Loop node in v5 kotlin (only the aux loop
//    row lands); the body falls to the conservative recursion, and the for-loop
//    variable is NEVER scope-bound (v5 exact).  // @comment-ok: one pre-existing header prose run
// ════════════════════════════════════════════════════════════════════════════

/// Transient scope: a variable name -> its binding node (param or `let`).
/// v5 shares ONE scope through the whole fn walk (lambdas included): a capture
/// resolves, a lambda param shadows, and an `it` binding leaks past the lambda
/// body - all ported verbatim.
type Scope = std::collections::HashMap<String, NodeRef>;

/// Project the DfF family: each callable's body lifted to its value-flow
/// graph. Port of v5 `kotlin_dataflow_from` (the driver half). Unlike v5, no
/// post-pass bumps (v6 stores bytes directly, not 0-based rows). `file` roots
/// each fn_sym (the closure value node's name derives from it).
fn project_df(
    root: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    kt_walk_fns(root, src, file, strings, sink);
    sink.aux.nests = crate::types::compute_nests(&sink.nodes, &sink.aux.loops);
}

/// Walk every function_declaration, lifting each body. Port of v5
/// `kt_walk_fns`. Only function_declarations lift (ctors, property
/// initializers, and class bodies do not); nested funs are found by the
/// recursion.
fn kt_walk_fns(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_declaration" {
            kt_flow_fn(child, src, file, strings, sink);
        }
        kt_walk_fns(child, src, file, strings, sink);
    }
}

/// Seed `param` nodes from the parameter list, then walk the body. Port of v5
/// `kt_flow_fn` (the `param_pos` aux is emitted as DfParam rows). EVERY fun - method or not -
/// roots its fn_sym at `{file}::function::{name}` (v5's exact mint, so a
/// lambda inside a method still joins under the bare fun sym). The body's tail
/// value is the implicit return: it flows into a `ret` node minted at the
/// BODY's start byte (v5's `body.start_position()` - covers both the block
/// form and `fun f() = expr`). An explicit `return EXPR` mints its own ret in
/// the jump_expression arm.
fn kt_flow_fn(
    fn_node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let name = kt_first_child(fn_node, "simple_identifier")
        .map(|n| kt_text(n, src))
        .unwrap_or("");
    let fn_sym = format!("{file}::function::{name}");
    let mut scope = Scope::new();
    if let Some(params) = kt_first_child(fn_node, "function_value_parameters") {
        let mut param_pos = 0u32;
        let mut cursor = params.walk();
        for p in params
            .children(&mut cursor)
            .filter(|n| n.kind() == "parameter")
        {
            if let Some(idn) = kt_first_child(p, "simple_identifier") {
                let v = kt_text(idn, src).to_string();
                let id = df_push(
                    sink,
                    strings,
                    idn.start_byte() as u32,
                    idn.end_byte() as u32,
                    DfNodeKind::Param,
                    Some(&v),
                );
                sink.aux.params.push(DfParam {
                    node: id,
                    pos: param_pos,
                });
                scope.insert(v, id);
            }
            param_pos += 1;
        }
    }
    if let Some(body) = kt_first_child(fn_node, "function_body") {
        if let Some(tail) = flow_kt(body, src, &fn_sym, strings, &mut scope, sink) {
            let ret = df_push(
                sink,
                strings,
                body.start_byte() as u32,
                body.end_byte() as u32,
                DfNodeKind::Ret,
                None,
            );
            df_edge(sink, tail, ret);
        }
    }
}

/// Returns the node carrying the value of this subtree, or None when the
/// subtree is not value-bearing (statements, wrappers, bindings handled
/// inline). Conservative on unsupported constructs: may miss flows, never
/// invents. Port of v5 `flow_kt`; byte-exact (each node minted at
/// `node.start_byte()`, the lambda-tail ret at `node.end_byte()`).
fn flow_kt(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let start_byte = node.start_byte() as u32;
    match node.kind() {
        // A name in expression position is a read; the role is decided by its
        // parent: under variable_declaration it is a binding target, under
        // parameter a param, under call_expression the callee - never a read.
        "simple_identifier" => {
            let parent_kind = node.parent().map(|p| p.kind());
            match parent_kind.as_deref() {
                Some("variable_declaration") | Some("parameter") | Some("call_expression") => None,
                _ => {
                    let v = kt_text(node, src).to_string();
                    let id = df_push(
                        sink,
                        strings,
                        start_byte,
                        node.end_byte() as u32,
                        DfNodeKind::VarRead,
                        Some(&v),
                    );
                    if let Some(binding) = scope.get(&v) {
                        df_edge(sink, *binding, id);
                    }
                    Some(id)
                }
            }
        }
        // f(args): every argument value flows into the call result. A named
        // argument `f(x = v)` flows its VALUE (the name ident is a label, never
        // walked); a trailing lambda is the call's last positional argument
        // (the lambda_literal arm lifts it and returns its closure node). A
        // navigation callee `recv.m(a)` flows the receiver in too. A
        // capitalized callee is a constructor call (Kotlin classes are
        // UpperCamelCase), minted as a `new` node carrying the type name.
        "call_expression" => {
            let callee = node.child(0);
            let mut recv: Option<NodeRef> = None;
            let mut callee_name = String::new();
            match callee.map(|c| c.kind()) {
                Some("simple_identifier") => {
                    callee_name = kt_text(callee.unwrap(), src).to_string();
                }
                Some("navigation_expression") => {
                    let nav = callee.unwrap();
                    if let Some(obj) = nav.child(0) {
                        recv = flow_kt(obj, src, fn_sym, strings, scope, sink);
                    }
                    if let Some(idn) = kt_first_child(nav, "navigation_suffix")
                        .and_then(|s| kt_first_child(s, "simple_identifier"))
                    {
                        callee_name = kt_text(idn, src).to_string();
                    }
                }
                _ => {}
            }
            let mut arg_ids: Vec<(Option<String>, NodeRef)> = Vec::new();
            if let Some(suffix) = kt_first_child(node, "call_suffix") {
                if let Some(vargs) = kt_first_child(suffix, "value_arguments") {
                    let mut cursor = vargs.walk();
                    for va in vargs
                        .children(&mut cursor)
                        .filter(|n| n.kind() == "value_argument")
                    {
                        // Named form: value_argument = simple_identifier '=' expr.
                        let mut vc = va.walk();
                        let kids: Vec<tree_sitter::Node> = va.children(&mut vc).collect();
                        let eq_at = kids.iter().position(|k| k.kind() == "=");
                        let (name, val_node) = match eq_at {
                            Some(i) if i >= 1 && kids[i - 1].kind() == "simple_identifier" => (
                                Some(kt_text(kids[i - 1], src).to_string()),
                                kids.get(i + 1).copied(),
                            ),
                            _ => (None, None),
                        };
                        let vid = match val_node {
                            Some(v) => flow_kt(v, src, fn_sym, strings, scope, sink),
                            None => flow_kt(va, src, fn_sym, strings, scope, sink),
                        };
                        if let Some(vid) = vid {
                            arg_ids.push((name, vid));
                        }
                    }
                }
                // A trailing lambda (`xs.map { it + 1 }`) is the call's last
                // positional argument; the lambda_literal arm lifts it and
                // returns its `closure` value node.
                if let Some(al) = kt_first_child(suffix, "annotated_lambda") {
                    if let Some(ll) = kt_first_child(al, "lambda_literal") {
                        if let Some(vid) = flow_kt(ll, src, fn_sym, strings, scope, sink) {
                            arg_ids.push((None, vid));
                        }
                    }
                }
            }
            let is_ctor = callee_name.chars().next().is_some_and(|c| c.is_uppercase());
            let (kind, name) = if is_ctor {
                (DfNodeKind::New, Some(callee_name.as_str()))
            } else {
                (DfNodeKind::CallRes, None)
            };
            let id = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                kind,
                name,
            );
            if let Some(r) = recv {
                df_edge(sink, r, id);
                sink.aux.args.push(DfArg {
                    call: id,
                    pos: -1,
                    arg: r,
                });
            }
            for (pos, (name, arg_id)) in arg_ids.into_iter().enumerate() {
                df_edge(sink, arg_id, id);
                sink.aux.args.push(DfArg {
                    call: id,
                    pos: pos as i64,
                    arg: arg_id,
                });
                if let Some(n) = name {
                    sink.aux.fields.push(DfField {
                        owner: id,
                        name: n,
                        value: arg_id,
                    });
                }
            }
            Some(id)
        }
        // `base.f` outside a call: a member read (the base flows into a
        // `member` node carrying the accessed name). As a call's callee (parent
        // == call_expression) the call arm owns it instead.
        "navigation_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let obj = node
                .child(0)
                .and_then(|c| flow_kt(c, src, fn_sym, strings, scope, sink));
            let name = kt_first_child(node, "navigation_suffix")
                .and_then(|s| kt_first_child(s, "simple_identifier"))
                .map(|n| kt_text(n, src).to_string())
                .unwrap_or_default();
            let id = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Member,
                Some(&name),
            );
            if let Some(o) = obj {
                df_edge(sink, o, id);
            }
            Some(id)
        }
        // val/var x = rhs: mint the binding slot, flow rhs -> slot, register.
        "property_declaration" => {
            let mut bind: Option<(String, NodeRef)> = None;
            let mut rhs_id: Option<NodeRef> = None;
            let mut cursor = node.walk();
            for c in node.children(&mut cursor) {
                match c.kind() {
                    "variable_declaration" => {
                        if let Some(si) = kt_first_child(c, "simple_identifier") {
                            let v = kt_text(si, src).to_string();
                            let id = df_push(
                                sink,
                                strings,
                                si.start_byte() as u32,
                                si.end_byte() as u32,
                                DfNodeKind::LetBind,
                                Some(&v),
                            );
                            bind = Some((v, id));
                        }
                    }
                    "=" | "binding_pattern_kind" | "val" | "var" => {}
                    _ => {
                        if let Some(id) = flow_kt(c, src, fn_sym, strings, scope, sink) {
                            rhs_id = Some(id);
                        }
                    }
                }
            }
            if let (Some((v, bid)), Some(rhs)) = (bind, rhs_id) {
                df_edge(sink, rhs, bid);
                scope.insert(v, bid);
            }
            None
        }
        // Wrappers / statements: flow the last value-bearing child through.
        "value_argument" | "statements" | "function_body" | "source_file" => {
            kt_recurse_children(node, src, fn_sym, strings, scope, sink)
        }
        // `{ x -> body }` / `{ it + 1 }`: lift the lambda as its OWN fn scope
        // under v5's `lam_sym` (`{fn_sym}::closure::{row}_{col}`, tree-sitter's
        // 0-based row/col of the literal's start; chains when nested), same
        // shape as Go func literals. Declared lambda params bind by name; with
        // no declared parameter list Kotlin's implicit `it` binds at slot 0.
        // The enclosing `scope` is shared, so a captured outer variable's read
        // still resolves (and the `it` binding leaks past the body - v5 exact).
        // The tail value flows into a `ret` node at the lambda's END byte. The
        // `closure` VALUE node stays in the enclosing fn and carries the exact
        // sym as its name.
        "lambda_literal" => {
            let pos = node.start_position();
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            let mut seeded = false;
            if let Some(lp) = kt_first_child(node, "lambda_parameters") {
                let mut param_pos = 0u32;
                let mut cursor = lp.walk();
                for vd in lp
                    .children(&mut cursor)
                    .filter(|n| n.kind() == "variable_declaration")
                {
                    if let Some(idn) = kt_first_child(vd, "simple_identifier") {
                        let v = kt_text(idn, src).to_string();
                        let id = df_push(
                            sink,
                            strings,
                            idn.start_byte() as u32,
                            idn.end_byte() as u32,
                            DfNodeKind::Param,
                            Some(&v),
                        );
                        sink.aux.params.push(DfParam {
                            node: id,
                            pos: param_pos,
                        });
                        scope.insert(v, id);
                        seeded = true;
                        param_pos += 1;
                    }
                }
            }
            if !seeded {
                let id = df_push(
                    sink,
                    strings,
                    start_byte,
                    node.end_byte() as u32,
                    DfNodeKind::Param,
                    Some("it"),
                );
                sink.aux.params.push(DfParam { node: id, pos: 0 });
                scope.insert("it".into(), id);
            }
            let tail = kt_first_child(node, "statements")
                .and_then(|s| flow_kt(s, src, &lam_sym, strings, scope, sink));
            if let Some(t) = tail {
                let ret = df_push(
                    sink,
                    strings,
                    node.end_byte() as u32,
                    node.end_byte() as u32,
                    DfNodeKind::Ret,
                    None,
                );
                df_edge(sink, t, ret);
            }
            Some(df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Closure,
                Some(&lam_sym),
            ))
        }
        // return EXPR: the returned value flows into the fn's `ret` node.
        "jump_expression" => {
            let mut inner = None;
            let mut cursor = node.walk();
            for c in node.children(&mut cursor) {
                if c.kind() != "return" {
                    if let Some(id) = flow_kt(c, src, fn_sym, strings, scope, sink) {
                        inner = Some(id);
                    }
                }
            }
            let id = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Ret,
                None,
            );
            if let Some(v) = inner {
                df_edge(sink, v, id);
            }
            Some(id)
        }
        // a OP b: both operands taint the result (taint-vs-dataflow: `a + 1`
        // propagates `a` into the result). Kotlin splits operators across
        // additive/multiplicative/infix kinds (no named fields), so the first
        // and last NAMED children are the two operands; a single-named-child
        // form flows the same subtree twice, like v5.
        "additive_expression" | "multiplicative_expression" | "infix_expression" => {
            let mut cursor = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            let l = kids
                .first()
                .and_then(|n| flow_kt(*n, src, fn_sym, strings, scope, sink));
            let r = kids
                .last()
                .and_then(|n| flow_kt(*n, src, fn_sym, strings, scope, sink));
            let id = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Binop,
                None,
            );
            if let Some(lid) = l {
                df_edge(sink, lid, id);
            }
            if let Some(rid) = r {
                df_edge(sink, rid, id);
            }
            Some(id)
        }
        "string_literal" | "integer_literal" | "real_literal" | "boolean_literal"
        | "character_literal" | "long_literal" => Some(df_push(
            sink,
            strings,
            start_byte,
            node.end_byte() as u32,
            DfNodeKind::Lit,
            None,
        )),
        // `for (x in coll) body`: the loop row only. v5's own var lookup takes the
        // first named `simple_identifier` child, which IS the collection here.
        "for_statement" => {
            let mut cursor = node.walk();
            let mut var = None;
            let mut collection = None;
            let mut after_in = false;
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "variable_declaration" => var = Some(kt_text(child, src).to_string()),
                    "in" => after_in = true,
                    "control_structure_body" | ")" | "(" => {}
                    _ if after_in && collection.is_none() => {
                        collection = Some(kt_text(child, src).to_string());
                    }
                    _ => {}
                }
            }
            kt_loop_row(sink, node, var, collection);
            kt_recurse_children(node, src, fn_sym, strings, scope, sink)
        }
        "while_statement" | "do_while_statement" => {
            kt_loop_row(sink, node, None, None);
            kt_recurse_children(node, src, fn_sym, strings, scope, sink)
        }
        // Everything else (when-arms, if statements, elvis, index/range
        // expressions, ...): recurse conservatively, last value-bearing child.
        _ => kt_recurse_children(node, src, fn_sym, strings, scope, sink),
    }
}

/// One loop row. v5 kotlin mints NO df node for a loop and never scope-binds the
/// loop variable (kotlin.rs:561,573); only the aux row lands.
fn kt_loop_row(
    sink: &mut FamilyBundle<DfF>,
    node: tree_sitter::Node,
    var: Option<String>,
    collection: Option<String>,
) {
    sink.aux.loops.push(crate::types::DfLoop {
        span: node_span(node),
        var,
        collection,
    });
}

/// Walk all children of a node conservatively, surfacing the last
/// value-bearing child's node. Port of v5 `kt_recurse_children`.
fn kt_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let mut last = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(id) = flow_kt(child, src, fn_sym, strings, scope, sink) {
            last = Some(id);
        }
    }
    last
}

/// Push one df node, returning its `NodeRef` (the dense index edges reference).
/// The node carries its FULL syntactic extent: `FlatFact::Edge` carries endpoint
/// spans only, so a start-only anchor merges distinct value nodes. `end` is
/// exclusive (tree-sitter `end_byte()`); node STARTS are unchanged, so the v5
/// parity golden stays byte-exact (the lambda-tail `ret` stays a zero-width
/// anchor at the lambda's closing brace, where v5 puts it). Port of v5
/// `push_node` (minus fn_sym/file/aux).
fn df_push(
    sink: &mut FamilyBundle<DfF>,
    strings: &mut Strings,
    start: u32,
    end: u32,
    kind: DfNodeKind,
    name: Option<&str>,
) -> NodeRef {
    let node_ref = NodeRef(sink.nodes.len() as u32);
    let mut node = Node::new(
        Span {
            start,
            len: end.saturating_sub(start),
        },
        kind,
    );
    if let Some(name) = name.filter(|candidate| !candidate.is_empty()) {
        node = node.with_name(strings.intern(name));
    }
    sink.nodes.push(node);
    node_ref
}

/// One Direct value edge: `dst` receives the value of `src`.
fn df_edge(sink: &mut FamilyBundle<DfF>, src: NodeRef, dst: NodeRef) {
    sink.edges.push(Edge::new(src, dst, DfEdgeKind::Direct));
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
            let parsed = {
                let span = trace::parse_span("kotlin", "astgrep");
                let _entered = span.enter();
                AstGrepParser.parse(&arena, path, content).ok()
            };
            parsed.map(|parsed| {
                let span = trace::family_span("kotlin", "cst");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
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
                let tree = {
                    let span = trace::parse_span("kotlin", "tree-sitter");
                    let _entered = span.enter();
                    kt_parse(src)
                };
                if let Some(tree) = tree {
                    let root = tree.root_node();
                    let src_bytes = src.as_bytes();
                    if mask.types {
                        let span = trace::family_span("kotlin", "type");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(root, src_bytes, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let span = trace::family_span("kotlin", "call");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<CallF>::default();
                        project_call(root, src_bytes, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
                        call = Some(bundle);
                    }
                    if mask.df {
                        let span = trace::family_span("kotlin", "df");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<DfF>::default();
                        project_df(root, src_bytes, path, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        df = Some(bundle);
                    }
                }
            }
        }

        ExtractOutput {
            strings,
            cst,
            types,
            call,
            df,
            data: None,
        }
    }
}

impl KotlinSource {
    /// Name-only call target lookup for Kotlin. Same-file definitions win via
    /// their span; otherwise a single corpus blob supplies the target. Kotlin
    /// has no SCIP arm in this crate, so unresolved or ambiguous names emit no
    /// edge.
    pub fn call_name_match(
        output: &ExtractOutput,
        index: &DefIndex,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let call = output.call.as_ref()?;
        if let Some(r) = def_named(call, &output.strings, callee) {
            let span = call.node(r).span;
            if let Some(site) = corpus_defs(index, callee)
                .iter()
                .find(|site| site.span == span)
            {
                return Some((site.blob.clone(), site.span));
            }
        }
        let sites = corpus_defs(index, callee);
        let mut blobs: Vec<ContentId> = Vec::new();
        for site in sites {
            if !blobs.contains(&site.blob) {
                blobs.push(site.blob.clone());
            }
        }
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites
            .iter()
            .find(|site| site.family == FamilyTag::Call)
            .unwrap_or(&sites[0]);
        Some((blob.clone(), site.span))
    }
}

impl Resolve<CallF> for KotlinSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        let mut edges = Vec::new();
        for site in &call.aux.sites {
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let Some((dst_blob, dst_span)) =
                KotlinSource::call_name_match(output, def_index, callee)
            else {
                continue;
            };
            edges.push(
                ProjectEdge::new(
                    caller,
                    dst_blob,
                    dst_span,
                    CallEdgeKind::NameResolve,
                    ResolutionOrigin::CorpusUnique,
                )
                .with_call_site(site.span),
            );
        }
        edges
    }
}

impl KotlinSource {
    /// The deduped, deterministically-ordered candidate list (v5's BTreeSet
    /// shaping): the aux candidates, deduped on (owner, to, kind). `resolve`
    /// emits its edges in EXACTLY this order, one per candidate; the parity
    /// golden zips the two (the zip discipline: edge i resolves candidate i).
    // @comment-ok: method doc mirroring the go/rust candidate accessors
    pub fn type_edge_candidates(output: &ExtractOutput) -> Vec<TypeEdgeCandidate> {
        let mut set: BTreeSet<TypeEdgeCandidate> = BTreeSet::new();
        if let Some(types) = &output.types {
            for candidate in &types.aux.candidates {
                set.insert(candidate.clone());
            }
        }
        set.into_iter().collect()
    }
}

/// The dst leg of one candidate: same-file TypeF entity first (its span joined
/// through the `DefIndex` for the blob), else a unique corpus site, else None
/// (text stays text, the zero leg). Name-only resolution, per the 4a ADDENDUM
/// site-key discipline (no receiver typing).
// @comment-ok: helper doc mirroring the go/rust resolve_type_dst
fn resolve_type_dst(
    types: &FamilyBundle<TypeF>,
    strings: &Strings,
    index: Option<&DefIndex>,
    name: &str,
) -> Option<(ContentId, Span, ResolutionOrigin)> {
    let same_file = types
        .nodes
        .iter()
        .find(|node| node.name.map_or(false, |id| strings.lookup(id) == name));
    if let (Some(node), Some(index)) = (same_file, index) {
        return corpus_defs(index, name)
            .iter()
            .find(|site| site.span == node.span)
            .map(|site| (site.blob.clone(), site.span, ResolutionOrigin::SameFile));
    }
    let sites = index.map(|index| corpus_defs(index, name)).unwrap_or(&[]);
    match sites {
        [only] => Some((
            only.blob.clone(),
            only.span,
            ResolutionOrigin::CorpusUnique,
        )),
        _ => None,
    }
}

impl Resolve<TypeF> for KotlinSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let index = cx.indexes.def_index.get();
        let mut edges = Vec::new();
        for candidate in KotlinSource::type_edge_candidates(output) {
            // src: the TypeF entity at the owner span (a miss is a collection
            // bug, not hidden here: it would break the parity zip count).
            let Some(src_ix) = types
                .nodes
                .iter()
                .position(|node| node.span == candidate.owner)
            else {
                continue;
            };
            let (dst_blob, dst_span, origin) = resolve_type_dst(
                types,
                &output.strings,
                index,
                output.strings.lookup(candidate.to),
            )
            .unwrap_or((ZERO_CONTENT_ID, Span::empty(), ResolutionOrigin::Unresolved));
            edges.push(ProjectEdge::new(
                NodeRef(src_ix as u32),
                dst_blob,
                dst_span,
                candidate.kind,
                origin,
            ));
        }
        edges
    }
}
