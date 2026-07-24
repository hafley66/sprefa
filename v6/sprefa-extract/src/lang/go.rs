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

use std::collections::BTreeSet;

use crate::family::{CallF, CstF, DfF, SigSlot, TypeEntityKind, TypeF, TypeSig};
use crate::rows::{FamilyBundle, Node};
use crate::seams::{Parser, Project};
use crate::shape::{Span, Strings};
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
// No const facet (v5 go emits none: walk_go_entities skips const_declaration and
// extract leaves consts empty, so v6 matches by emitting none).
//
// v6 drops v5's `parent`/`sym`/`mint_sym`/`go_owner_kinds`: a node is span+kind+
// name; the parent linkage is span-containment at the seam. The method receiver
// (`go_receiver_type`) is kept ONLY as the gate v5 uses to emit-or-skip a method
// (a malformed receiver skips the entity); the resolved owner name is dropped.
// ════════════════════════════════════════════════════════════════════════════

/// Project the TypeF family: one entity node per type/function/method declaration
/// + an arrow-type sig per callable param/return type reference. Port of v5
/// `walk_go_entities` + `go_fn_type`.
fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    walk_go_entities(root, src, strings, sink);
}

/// Walk every type/function/method declaration, minting one entity node per decl
/// + one arrow-type sig per callable param/return type ref. Port of v5
/// `walk_go_entities`. The entity span is anchored at the spec/decl node's start
/// byte so `line_of(span.start)` equals v5's reported `entity.line` (the spec/
/// decl start row, 1-based).
fn walk_go_entities(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_declaration" => {
                let mut spec_cursor = child.walk();
                for spec in child.children(&mut spec_cursor) {
                    let (name_node, kind) = match spec.kind() {
                        "type_spec" => {
                            let k = match spec.child_by_field_name("type").map(|t| t.kind()) {
                                Some("struct_type") => TypeEntityKind::Struct,
                                Some("interface_type") => TypeEntityKind::Interface,
                                _ => TypeEntityKind::Alias,
                            };
                            (spec.child_by_field_name("name"), k)
                        }
                        "type_alias" => (spec.child_by_field_name("name"), TypeEntityKind::Alias),
                        _ => continue,
                    };
                    let Some(name_node) = name_node else { continue };
                    let span = node_span(spec);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, kind);
                }
            }
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let span = node_span(child);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, TypeEntityKind::Function);
                    fn_sigs(sink, strings, span, child, src);
                }
            }
            "method_declaration" => {
                // Gate on a resolvable receiver, matching v5: a malformed receiver
                // skips the entity (so v6 emits-or-skips exactly as v5 does). The
                // owner name itself is dropped (no parent sym in v6).
                if let (Some(name_node), Some(())) = (
                    child.child_by_field_name("name"),
                    go_receiver_type(child, src).map(|_| ()),
                ) {
                    let span = node_span(child);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, TypeEntityKind::Method);
                    fn_sigs(sink, strings, span, child, src);
                }
            }
            _ => {}
        }
        walk_go_entities(child, src, strings, sink);
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

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
fn node_span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

// ── arrow-type signatures (port of v5 `go_fn_type`) ──────────────────────────

/// The arrow-type sigs of one callable: param type-refs (positional, receiver
/// skipped, grouped params `a, b int` expanded to one slot per name) + return
/// type-refs (all unioned into slot pos=0, matching v5's flat `ret` list). Port
/// of v5 `go_fn_type`. `owner` is the callable entity's span (the sig join key).
fn fn_sigs(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    node: tree_sitter::Node,
    src: &[u8],
) {
    let tparams = go_type_param_names(node, src);
    let mut pos: u32 = 0;
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cursor = plist.walk();
        for param in plist.children(&mut cursor) {
            if !matches!(param.kind(), "parameter_declaration" | "variadic_parameter_declaration") {
                continue;
            }
            let Some(ty) = param.child_by_field_name("type") else { continue };
            // A grouped parameter (`a, b int`) is ONE grammar node but TWO slots;
            // each declared name gets its own slot sharing the group's type.
            let mut name_cursor = param.walk();
            let count = param
                .children(&mut name_cursor)
                .filter(|n| n.kind() == "identifier")
                .count()
                .max(1);
            let refs = go_type_refs(ty, src, &tparams);
            for _ in 0..count {
                for name in &refs {
                    push_sig(sink, strings, owner, SigSlot::Param, pos, name);
                }
                pos += 1;
            }
        }
    }
    // Go's multi-value return unions every result type ref into one flat list at
    // pos 0 (v5 stores one flat `ret` list regardless of slot count).
    if let Some(result) = node.child_by_field_name("result") {
        if result.kind() == "parameter_list" {
            let mut cursor = result.walk();
            for param in result.children(&mut cursor) {
                if matches!(param.kind(), "parameter_declaration" | "variadic_parameter_declaration") {
                    if let Some(ty) = param.child_by_field_name("type") {
                        for name in go_type_refs(ty, src, &tparams) {
                            push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
                        }
                    }
                }
            }
        } else {
            for name in go_type_refs(result, src, &tparams) {
                push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
            }
        }
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

/// The callable's declared type-parameter names (the exclusion set: a generic
/// `[T]` referencing itself is not a sig). Port of v5 `go_fn_type`'s tparams.
fn go_type_param_names(node: tree_sitter::Node, src: &[u8]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    if let Some(tp_list) = node.child_by_field_name("type_parameters") {
        let mut cursor = tp_list.walk();
        for tp in tp_list
            .children(&mut cursor)
            .filter(|n| n.kind() == "type_parameter_declaration")
        {
            let mut inner = tp.walk();
            for child in tp.children(&mut inner).filter(|n| n.kind() == "identifier") {
                names.insert(go_text(child, src).to_string());
            }
        }
    }
    names
}

/// Collect the named type references anywhere under `node`, de-duplicated and
/// sorted. A `qualified_type` (`pkg.Type`) is one ref kept as `pkg.Type` (NOT
/// recursed into; its inner `type_identifier` would double-count). A bare
/// `type_identifier` is a ref unless it names a type parameter or a predeclared/
/// builtin type. Port of v5 `go_type_refs`/`collect_go_refs`.
fn go_type_refs(node: tree_sitter::Node, src: &[u8], params: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    collect_go_refs(node, src, params, &mut out);
    out.sort();
    out.dedup();
    out
}

fn collect_go_refs(
    node: tree_sitter::Node,
    src: &[u8],
    params: &BTreeSet<String>,
    out: &mut Vec<String>,
) {
    match node.kind() {
        "type_identifier" => {
            let name = go_text(node, src).to_string();
            if !params.contains(&name) && !is_noise_go(&name) {
                out.push(name);
            }
        }
        "qualified_type" => {
            let pkg = node.child_by_field_name("package").map(|n| go_text(n, src)).unwrap_or("");
            let name = node.child_by_field_name("name").map(|n| go_text(n, src)).unwrap_or("");
            if !pkg.is_empty() && !name.is_empty() {
                out.push(format!("{pkg}.{name}"));
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_go_refs(child, src, params, out);
            }
        }
    }
}

/// Predeclared/builtin type filter: a reference to `int`/`string`/`error`/...
/// carries no resolvable declaration. Port of v5 `is_noise_go`.
fn is_noise_go(name: &str) -> bool {
    matches!(
        name,
        "int" | "int8" | "int16" | "int32" | "int64"
            | "uint" | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr"
            | "float32" | "float64" | "complex64" | "complex128"
            | "bool" | "string" | "byte" | "rune" | "error" | "any" | "comparable"
    )
}

/// A method's receiver base type name, `*`/generic-args stripped
/// (`(r *Repo[T])` -> `"Repo"`). None for a malformed/absent receiver. Port of v5
/// `go_receiver_type`; used here only as the emit-or-skip gate for a method entity.
fn go_receiver_type(method: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let recv_list = method.child_by_field_name("receiver")?;
    let mut cursor = recv_list.walk();
    let param = recv_list
        .children(&mut cursor)
        .find(|n| n.kind() == "parameter_declaration")?;
    let mut ty = param.child_by_field_name("type")?;
    loop {
        match ty.kind() {
            "pointer_type" => ty = ty.named_child(0)?,
            "generic_type" => ty = ty.child_by_field_name("type")?,
            _ => break,
        }
    }
    match ty.kind() {
        "type_identifier" => Some(go_text(ty, src).to_string()),
        "qualified_type" => ty.child_by_field_name("name").map(|n| go_text(n, src).to_string()),
        _ => None,
    }
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
