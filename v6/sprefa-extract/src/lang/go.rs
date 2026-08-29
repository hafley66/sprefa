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
//! Direct edges). 4d-i-go ports `go_type_spec_edges` (type-edge candidates in
//! phase 1) + lands `Resolve<TypeF>`; 4d-ii-go lands `Resolve<CallF>` (the
//! scip-ratcheted twin of the TsSource arm).
//!
//! Deferred follow-ups: df literal/loop/nesting aux. Df argument slots,
//! parameter positions and field names emitted.
//! The const facet is
//! NOT ported: v5 go emits no const entities and no const_value rows
//! (`walk_go_entities` skips `const_declaration`; `extract` leaves `consts`
//! empty), so v6 matches by emitting none either.
// @comment-ok: the module header is a crate-level doc block predating the rail

use std::collections::BTreeSet;

use super::astgrep::{AstGrepParser, CstProjector};
use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstF, DfArg, DfEdgeKind, DfF, DfField, DfNodeKind,
    DfParam, DocFact, DocTag, ProjectEdge, SigSlot, Specifier, SpecifierKind, TypeEdgeCandidate,
    TypeEdgeKind, TypeEntityKind, TypeF, TypeSig,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::scip::{byte_range, definition_of, join_documents, site_occurrence};
use crate::seams::{
    containing_def_site, corpus_defs, covering_def, def_named, own_blob, DefIndex, Parser, Project,
    Resolve,
};
use crate::shape::{ContentId, FamilyTag, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::source::{ExtractOutput, FamilyMask, ProjectCx, Source};
use crate::trace;
use crate::types::ScipIndex;

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
// TypeF: entity nodes + arrow-type sigs + type-edge candidates. Commit B; the
// candidates land with 4d-i-go.
//
// Ports v5 `walk_go_entities` (the entity half) + `go_fn_type` (the arrow-type
// payload) + `go_type_spec_edges` (the UNRESOLVED type-edge candidates: struct
// field / struct embed / interface embed / generic constraint, owner span +
// to-name as written + kind - the 4b-iii TypeFAux.candidates pattern). The
// name-resolved type EDGES themselves land with `Resolve<TypeF>` (4d-i-go
// below); phase 1 stays pure-content span rows.
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
                    // A grouped `type ( ... )` decl carries its doc above the
                    // spec; a lone `type X struct{}` has it above the decl.
                    if let Some(text) =
                        go_leading_doc(spec, src).or_else(|| go_leading_doc(child, src))
                    {
                        push_go_doc(sink, strings, span, None, &text);
                    }
                    if spec.kind() == "type_spec" {
                        go_edge_candidates(spec, span, src, strings, sink);
                    }
                }
            }
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let span = node_span(child);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, TypeEntityKind::Function);
                    if let Some(text) = go_leading_doc(child, src) {
                        push_go_doc(sink, strings, span, None, &text);
                    }
                    fn_sigs(sink, strings, span, child, src);
                }
            }
            "method_declaration" => {
                // Gate on a resolvable receiver, matching v5: a malformed receiver
                // skips the entity (so v6 emits-or-skips exactly as v5 does). The
                // owner name itself is dropped (no parent sym in v6).
                if let (Some(name_node), Some(owner)) = (
                    child.child_by_field_name("name"),
                    go_receiver_type(child, src),
                ) {
                    let span = node_span(child);
                    let name = go_text(name_node, src).to_string();
                    push_entity(sink, strings, span, &name, TypeEntityKind::Method);
                    if let Some(text) = go_leading_doc(child, src) {
                        push_go_doc(sink, strings, span, Some(&owner), &text);
                    }
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
    sink.nodes
        .push(Node::new(span, kind).with_name(strings.intern(name)));
}

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
fn node_span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

// ── doc facet (port of v5 `walk_go_docs`) ────────────────────────────────────

/// The cleaned doc block directly above `node`, or None: walks BACKWARD over
/// `prev_sibling` comments with no blank-line gap, so multi-line godoc joins.
fn go_leading_doc(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut expected_row = node.start_position().row;
    let mut cur = node.prev_sibling()?;
    loop {
        if cur.kind() != "comment" || cur.end_position().row + 1 != expected_row {
            break;
        }
        let raw = go_text(cur, src);
        if raw.trim_start().starts_with("/*") {
            lines.insert(0, clean_block_comment(raw));
            break;
        }
        let body = raw
            .trim_start()
            .strip_prefix("//")
            .unwrap_or(raw)
            .trim_start()
            .to_string();
        lines.insert(0, body);
        expected_row = cur.start_position().row;
        let Some(prev) = cur.prev_sibling() else {
            break;
        };
        cur = prev;
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn push_go_doc(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    span: Span,
    parent: Option<&str>,
    text: &str,
) {
    sink.aux.docs.push(DocFact {
        owner: span,
        parent: parent.map(|name| strings.intern(name)),
        text: strings.intern(text),
        tags: parse_go_doc_tags(text, strings),
    });
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

/// godoc's one structured convention: a blank-line-separated paragraph starting
/// `Deprecated:` marks the decl deprecated. No `@`-tags exist in plain godoc.
fn parse_go_doc_tags(text: &str, strings: &mut Strings) -> Vec<DocTag> {
    let mut out = Vec::new();
    for para in text.split("\n\n") {
        if let Some(rest) = para.trim_start().strip_prefix("Deprecated:") {
            out.push(DocTag {
                tag: strings.intern("deprecated"),
                arg: None,
                text: strings.intern(rest.trim()),
            });
        }
    }
    out
}

// ── type-edge candidates (port of v5 `go_type_spec_edges`) ───────────────────

/// The type-edge candidates of one `type_spec`: struct fields of named types
/// (`field`), struct embeds (`impl` - a field_declaration with no name field),
/// interface `type_elem` embeds (`impl`; `method_elem` intentionally skipped:
/// no type_sig-equivalent exists for an interface's own method specs at the
/// type_edge level), and declared type-parameter constraints (`generic`). Port
/// of v5 `go_type_spec_edges` (src/graph/typegraph/go.rs:320); the
/// type_declaration/type_spec discovery rides the entity walk above, which
/// visits exactly the specs v5's `walk_go_types` visits (both recurse into
/// every child). Method/fn SIGNATURES are NOT edge sources for go (entity-level
/// `type_sig` covers callables; v5 go's type_edge is shape-only), so no
/// candidate is sig-sourced. The `to` is the name as written (a `pkg.Type`
/// qualified ref kept whole); `Resolve<TypeF>` binds it.
fn go_edge_candidates(
    spec: tree_sitter::Node,
    owner: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut params: BTreeSet<String> = BTreeSet::new();
    if let Some(tp_list) = spec.child_by_field_name("type_parameters") {
        let mut cursor = tp_list.walk();
        for tp in tp_list
            .children(&mut cursor)
            .filter(|n| n.kind() == "type_parameter_declaration")
        {
            // v5 accumulates the declared names left-to-right and filters each
            // constraint against the names seen SO FAR - ported verbatim.
            let mut cc = tp.walk();
            for n in tp.children(&mut cc).filter(|n| n.kind() == "identifier") {
                params.insert(go_text(n, src).to_string());
            }
            if let Some(constraint) = tp.child_by_field_name("type") {
                for to in go_type_refs(constraint, src, &params) {
                    push_candidate(sink, strings, owner, &to, TypeEdgeKind::Generic);
                }
            }
        }
    }

    let Some(ty) = spec.child_by_field_name("type") else {
        return;
    };
    match ty.kind() {
        "struct_type" => {
            let mut c = ty.walk();
            let Some(list) = ty
                .children(&mut c)
                .find(|n| n.kind() == "field_declaration_list")
            else {
                return;
            };
            let mut c2 = list.walk();
            for field in list
                .children(&mut c2)
                .filter(|n| n.kind() == "field_declaration")
            {
                let Some(ftype) = field.child_by_field_name("type") else {
                    continue;
                };
                let kind = if field.child_by_field_name("name").is_some() {
                    TypeEdgeKind::Field
                } else {
                    TypeEdgeKind::Impl
                };
                for to in go_type_refs(ftype, src, &params) {
                    push_candidate(sink, strings, owner, &to, kind);
                }
            }
        }
        "interface_type" => {
            let mut c = ty.walk();
            for elem in ty.children(&mut c).filter(|n| n.kind() == "type_elem") {
                for to in go_type_refs(elem, src, &params) {
                    push_candidate(sink, strings, owner, &to, TypeEdgeKind::Impl);
                }
            }
        }
        _ => {}
    }
}

fn push_candidate(
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
            if !matches!(
                param.kind(),
                "parameter_declaration" | "variadic_parameter_declaration"
            ) {
                continue;
            }
            let Some(ty) = param.child_by_field_name("type") else {
                continue;
            };
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
                if matches!(
                    param.kind(),
                    "parameter_declaration" | "variadic_parameter_declaration"
                ) {
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
    sink.aux.sigs.push(TypeSig {
        owner,
        slot,
        pos,
        ty: strings.intern(name),
    });
}

/// The callable's declared type-parameter names (the exclusion set: a generic
/// `[T]` referencing itself is not a sig). For a method this includes the
/// receiver's type arguments (`func (g Gen[T]) Get() T` declares T there, and
/// T in the result position is not a ref). Port of v5 `go_fn_type`'s tparams.
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
    if node.kind() == "method_declaration" {
        if let Some(recv_list) = node.child_by_field_name("receiver") {
            let mut cursor = recv_list.walk();
            for recv in recv_list
                .children(&mut cursor)
                .filter(|n| n.kind() == "parameter_declaration")
            {
                if let Some(ty) = recv.child_by_field_name("type") {
                    collect_identifiers(ty, src, &mut names);
                }
            }
        }
    }
    names
}

/// Every `type_identifier` name under `node`, recursive. Used for the
/// receiver's `type_arguments`, where the grammar nests the arg inside a
/// `type_elem`.
fn collect_identifiers(node: tree_sitter::Node, src: &[u8], names: &mut BTreeSet<String>) {
    if node.kind() == "type_identifier" {
        names.insert(go_text(node, src).to_string());
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_identifiers(child, src, names);
    }
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
            let pkg = node
                .child_by_field_name("package")
                .map(|n| go_text(n, src))
                .unwrap_or("");
            let name = node
                .child_by_field_name("name")
                .map(|n| go_text(n, src))
                .unwrap_or("");
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
        "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uintptr"
            | "float32"
            | "float64"
            | "complex64"
            | "complex128"
            | "bool"
            | "string"
            | "byte"
            | "rune"
            | "error"
            | "any"
            | "comparable"
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
        "qualified_type" => ty
            .child_by_field_name("name")
            .map(|n| go_text(n, src).to_string()),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════════
// CallF: callable definitions (nodes) + call sites (aux). Commit C.
//
// Ports v5 `go_walk_call_defs` (defs, incl. func_literal lambdas) +
// `go_walk_call_sites` (sites). v5's `mint_sym`/`lambda_sym`/`end` line are
// dropped: a def is span + kind + name (the name is the bare identifier for
// callee resolution, NOT a qualified sym). The def span COVERS its body (decl
// start -> block end) so the seam's span-containment can bind a site's caller;
// the parity line reads `line_of(span.start)` = the decl start line (v5's
// `def.line`). Lambda defs (func_literal inside a fn/method body) keep kind=
// Lambda, name=None (v5's empty name). A package-level func_literal
// (`var f = func(){}`) is skipped: v5's lift only walks fn/method bodies, so
// there is no enclosing scope to join (enclosing == "").
// ════════════════════════════════════════════════════════════════════════════

/// Project the CallF family: one def node per callable (Free / Method / Lambda)
/// + one site per call expression. Port of v5 `go_walk_call_defs` +
/// `go_walk_call_sites`.
fn project_call(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    go_walk_call_defs(root, src, strings, sink, false);
    // The import table is what a selector call's receiver is checked against,
    // so the specifiers land before the sites that read them.
    go_module_specifiers(root, src, strings, sink);
    let imports = go_import_bindings(sink, strings);
    go_walk_call_sites(root, src, strings, sink, &imports);
}

/// Qualifier -> import path, per the `go_module_specifiers` table above: a
/// plain spec binds its path's last segment, `_` and `.` bind no qualifier.
fn go_import_bindings(
    sink: &FamilyBundle<CallF>,
    strings: &Strings,
) -> std::collections::HashMap<String, String> {
    let mut bindings = std::collections::HashMap::new();
    for specifier in &sink.aux.specifiers {
        if !matches!(specifier.kind, SpecifierKind::Named) {
            continue;
        }
        let name = strings.lookup(specifier.name);
        let (binding, path) = match specifier.module {
            Some(module) => (name.to_string(), strings.lookup(module).to_string()),
            None => (
                name.rsplit('/').next().unwrap_or(name).to_string(),
                name.to_string(),
            ),
        };
        bindings.insert(binding, path);
    }
    bindings
}

// ── module specifiers (CallFAux.specifiers) ─────────────────────────────────
// @comment-ok: the kind/name/module contract, pinned row-for-row by
// tests/25_go_specifiers.rs. `Default` and `Reexport` are unreachable from go.
//
// | go source                 | kind       | name  | module          |
// |---------------------------|------------|-------|-----------------|
// | `import "fmt"`            | Named      | fmt   | None            |
// | `"os"` inside a block     | Named      | os    | None            |
// | `alias "path/filepath"`   | Named      | alias | path/filepath   |
// | `_ "embed"`               | SideEffect | embed | None            |
// | `. "strings"`             | Namespace  | strings | None        |
//
// The path-only form carries the path in `name` with `module` None
// (`src/types.rs:485-492` names go explicitly). Only the aliased form sets
// `module` to Some, because it is the only form where the path would
// otherwise be lost. v5 parses `_` and `.` in the same slot as an alias
// (`src/graph/modgraph/go.rs:37-59`).

/// Go module specifiers: one row per `import_spec`. Rides the one tree-sitter
/// parse `project_call` already holds. v5 reads the same facts with regexes
/// over stripped text (`src/graph/modgraph/go.rs:37-59`).
fn go_module_specifiers(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut rows = Vec::new();
    go_walk_import_specs(root, src, strings, &mut rows);
    sink.aux.specifiers.extend(rows);
}

/// Recurse the tree for every `import_spec` node, appending one row each.
fn go_walk_import_specs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    rows: &mut Vec<Specifier>,
) {
    if node.kind() == "import_spec" {
        let span = Span {
            start: node.start_byte() as u32,
            len: (node.end_byte() - node.start_byte()) as u32,
        };
        let path = path_of_import_spec(node, src);
        let (kind, name, module) = match leading_name(node) {
            Some(name_node) if name_node.kind() == "package_identifier" => (
                SpecifierKind::Named,
                go_text(name_node, src).to_string(),
                Some(path),
            ),
            Some(name_node) if name_node.kind() == "blank_identifier" => {
                (SpecifierKind::SideEffect, path, None)
            }
            Some(name_node) if name_node.kind() == "dot" => (SpecifierKind::Namespace, path, None),
            _ => (SpecifierKind::Named, path, None),
        };
        rows.push(Specifier {
            span,
            name: strings.intern(&name),
            kind,
            module: module.map(|text| strings.intern(&text)),
            imported: None,
        });
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        go_walk_import_specs(child, src, strings, rows);
    }
}

/// Optional leading name node of an `import_spec` (alias / blank / dot), if
/// present.
fn leading_name(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    node.named_children(&mut node.walk()).find(|child| {
        matches!(
            child.kind(),
            "package_identifier" | "blank_identifier" | "dot"
        )
    })
}

/// Path text of an `import_spec` from its `interpreted_string_literal_content`
/// descendant (quotes excluded).
fn path_of_import_spec(node: tree_sitter::Node, src: &[u8]) -> String {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "interpreted_string_literal" {
            let mut inner = child.walk();
            for grandchild in child.named_children(&mut inner) {
                if grandchild.kind() == "interpreted_string_literal_content" {
                    return go_text(grandchild, src).to_string();
                }
            }
        }
    }
    String::new()
}

/// The def span covers the whole callable body `[child.start, body.end)` for
/// span-containment resolution. Port of v5 `end_of(child)` (the body end line).
fn def_span(child: tree_sitter::Node) -> Span {
    let start = child.start_byte();
    let end = child
        .child_by_field_name("body")
        .unwrap_or(child)
        .end_byte();
    Span {
        start: start as u32,
        len: (end - start) as u32,
    }
}

/// Walk every callable declaration, minting one def node per Free function /
/// Method / Lambda. Port of v5 `go_walk_call_defs`. `in_fn` is v5's
/// `!enclosing.is_empty()`: a func_literal only mints a Lambda def when inside a
/// fn/method body (a package-level func literal has no enclosing scope to join).
fn go_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    in_fn: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // @callable go function
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let span = def_span(child);
                    let name = go_text(name_node, src).to_string();
                    sink.nodes
                        .push(Node::new(span, CallKind::Free).with_name(strings.intern(&name)));
                    go_walk_call_defs(child, src, strings, sink, true);
                    continue;
                }
            }
            // @callable go method
            "method_declaration" => {
                if let (Some(name_node), Some(_)) = (
                    child.child_by_field_name("name"),
                    go_receiver_type(child, src),
                ) {
                    let span = def_span(child);
                    let name = go_text(name_node, src).to_string();
                    sink.nodes
                        .push(Node::new(span, CallKind::Method).with_name(strings.intern(&name)));
                    go_walk_call_defs(child, src, strings, sink, true);
                    continue;
                }
            }
            // `func(...) {...}` inside a fn/method body: a Lambda. A package-level
            // `var f = func(){}` (in_fn == false) is skipped, matching v5
            // (enclosing == "" -> no Lambda def).
            // @callable go lambda
            "func_literal" if in_fn => {
                let span = def_span(child);
                sink.nodes.push(Node::new(span, CallKind::Lambda));
                go_walk_call_defs(child, src, strings, sink, true);
                continue;
            }
            _ => {}
        }
        go_walk_call_defs(child, src, strings, sink, in_fn);
    }
}

/// Walk every `call_expression`, minting one call site per call. The callee is
/// the trailing name: a bare `identifier`, or a `selector_expression`'s field
/// (`recv.M` -> "M"). A type conversion `T(x)` reads as an ordinary call (the
// syntactic tier can't tell a conversion from a call). Port of v5
/// `go_walk_call_sites` + `go_callee`. The site span is the CALLEE node's start
/// (line_of(span.start) = v5's reported site line).
/// `callee_path` is the import path when the selector's operand is a name an
/// import binds; any other receiver is a value, whose type nothing here knows.
fn go_walk_call_sites(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    imports: &std::collections::HashMap<String, String>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call_expression" {
            if let Some(func) = child.child_by_field_name("function") {
                let (callee, path) = match func.kind() {
                    "identifier" => (Some(go_text(func, src).to_string()), None),
                    "selector_expression" => (
                        func.child_by_field_name("field")
                            .map(|field| go_text(field, src).to_string()),
                        go_import_qualifier(func, src, imports),
                    ),
                    _ => (None, None),
                };
                if let Some(callee) = callee {
                    sink.aux.sites.push(CallSite {
                        span: node_span(func),
                        callee: strings.intern(&callee),
                        callee_path: path.map(|path| strings.intern(path)),
                    });
                }
            }
        }
        go_walk_call_sites(child, src, strings, sink, imports);
    }
}

/// The import path a selector's operand names, when the operand is a bare
/// identifier this file imported. `a.b.C()` and `value.M()` name none.
fn go_import_qualifier<'a>(
    selector: tree_sitter::Node,
    src: &[u8],
    imports: &'a std::collections::HashMap<String, String>,
) -> Option<&'a str> {
    let operand = selector.child_by_field_name("operand")?;
    if operand.kind() != "identifier" {
        return None;
    }
    imports.get(go_text(operand, src)).map(String::as_str)
}

// ════════════════════════════════════════════════════════════════════════════
// DfF: intra-procedural value flow (nodes + Direct edges). Commit D.
//
// Ports v5 `go_dataflow_from` (src/graph/typegraph/go.rs:576). Every value-bearing
// position in a callable's body becomes a NODE; local value flow becomes a Direct
// EDGE. The two are the dataflow graph the engine's `df_reaches` closure walks.
//
// BYTE PARITY: v5 mints each node at `(node.start_position().row, .column)` and
// the oracle reconstructs the byte as `line_starts[row] + col`, which equals
// tree-sitter's `node.start_byte()`. So v6 mints each node at `node.start_byte()`
// directly (no line/col bridge, unlike the syn front-end in rust.rs). The
// (kind, var, byte) triples and the (from_byte, to_byte) edge pairs match v5
// exactly.
//
// What is DROPPED vs v5 (each deliberate, matching the TS/Rust DfF ports):
//  - `fn_sym` ON NODES: the enclosing callable is not stored on every df node;
//    it is threaded through the walk (v5's own mechanism) purely so the
//    `closure` VALUE node carries v5's exact `lam_sym` name
//    (`{file}::function::{fn}::closure::{row}_{col}`, tree-sitter's 0-based
//    row/col; methods root at `{file}::method::{Recv}.{m}`). No sym store:
//    the name derives from the walk's containment path + the literal's start.
//  - the enrichment aux: `args`, `fields`, `lits`, `param_pos`. The EDGES
//    already carry every value flow.
// ════════════════════════════════════════════════════════════════════════════

/// Transient scope: a variable name -> its binding node (param or `let`).
type Scope = std::collections::HashMap<String, NodeRef>;

/// Project the DfF family: each callable's body lifted to its value-flow graph.
/// Port of v5 `go_dataflow_from` (the driver half). Unlike v5, no post-pass bumps
/// (v6 stores bytes directly, not 0-based rows). `file` roots each fn_sym (the
/// closure value node's name derives from it).
fn project_df(
    root: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    go_walk_fns(root, src, file, strings, sink);
    sink.aux.nests = crate::types::compute_nests(&sink.nodes, &sink.aux.loops);
}

/// Walk every function/method declaration, lifting each body. Port of v5
/// `go_walk_fns` (incl. its syms: `{file}::function::{name}` /
/// `{file}::method::{Recv}.{name}`). The receiver is NOT seeded as a param (it
/// lives in the `receiver` field, not `parameters`); a read of the receiver in
/// the body has no binding edge, matching v5.
fn go_walk_fns(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let fn_sym = format!("{file}::function::{}", go_text(name_node, src));
                    go_flow_fn(child, src, &fn_sym, strings, sink);
                }
            }
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) = (
                    child.child_by_field_name("name"),
                    go_receiver_type(child, src),
                ) {
                    let fn_sym = format!("{file}::method::{owner}.{}", go_text(name_node, src));
                    go_flow_fn(child, src, &fn_sym, strings, sink);
                }
            }
            _ => {}
        }
        go_walk_fns(child, src, file, strings, sink);
    }
}

/// Seed `param` nodes from the (non-receiver) parameter list, then walk the body.
/// A grouped parameter (`a, b int`) mints one param node PER declared name,
/// matching `go_fn_type`'s slot count; an unnamed parameter still advances the
/// position counter so later named params keep the right index. Port of v5
/// `go_flow_fn` (the `param_pos` aux is emitted as DfParam rows).
fn go_flow_fn(
    fn_node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut scope = Scope::new();
    if let Some(params) = fn_node.child_by_field_name("parameters") {
        let mut param_pos = 0u32;
        let mut cursor = params.walk();
        for param in params.children(&mut cursor) {
            if !matches!(
                param.kind(),
                "parameter_declaration" | "variadic_parameter_declaration"
            ) {
                continue;
            }
            let mut name_cursor = param.walk();
            let names: Vec<tree_sitter::Node> = param
                .children(&mut name_cursor)
                .filter(|n| n.kind() == "identifier")
                .collect();
            if names.is_empty() {
                param_pos += 1;
                continue;
            }
            for name_node in names {
                let name = go_text(name_node, src).to_string();
                let node = df_push(
                    sink,
                    strings,
                    name_node.start_byte() as u32,
                    name_node.end_byte() as u32,
                    DfNodeKind::Param,
                    Some(&name),
                );
                sink.aux.params.push(DfParam {
                    node,
                    pos: param_pos,
                });
                scope.insert(name, node);
                param_pos += 1;
            }
        }
    }
    if let Some(body) = fn_node.child_by_field_name("body") {
        flow_go(body, src, fn_sym, strings, &mut scope, sink);
    }
}

/// Returns the node carrying the value of this subtree, or None when the subtree
/// is a pure statement/binder handled inline (bindings, control-flow headers).
/// Unhandled node kinds fall through to `go_recurse_children`, conservative.
/// Port of v5 `flow_go`; byte-exact (each node minted at `node.start_byte()`).
fn flow_go(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let start_byte = node.start_byte() as u32;
    match node.kind() {
        "identifier" => {
            let name = go_text(node, src).to_string();
            let read = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::VarRead,
                Some(&name),
            );
            if let Some(binding) = scope.get(&name) {
                df_edge(sink, *binding, read);
            }
            Some(read)
        }
        "interpreted_string_literal"
        | "raw_string_literal"
        | "int_literal"
        | "float_literal"
        | "imaginary_literal"
        | "rune_literal"
        | "true"
        | "false"
        | "nil"
        | "iota" => Some(df_push(
            sink,
            strings,
            start_byte,
            node.end_byte() as u32,
            DfNodeKind::Lit,
            None,
        )),
        // f(args): every argument flows into the call result; a selector callee
        // `recv.M(args)` flows the receiver in too. Go has no syntactic ctor
        // marker (capitalization means EXPORTED), so every call is `call_res`.
        "call_expression" => {
            let func = node.child_by_field_name("function");
            let mut receiver: Option<NodeRef> = None;
            if let Some(func) = func {
                if func.kind() == "selector_expression" {
                    if let Some(operand) = func.child_by_field_name("operand") {
                        receiver = flow_go(operand, src, fn_sym, strings, scope, sink);
                    }
                }
            }
            let mut arg_ids = Vec::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cursor = args.walk();
                for arg in args.children(&mut cursor) {
                    if let Some(id) = flow_go(arg, src, fn_sym, strings, scope, sink) {
                        arg_ids.push(id);
                    }
                }
            }
            let call_res = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::CallRes,
                None,
            );
            if let Some(recv) = receiver {
                df_edge(sink, recv, call_res);
                sink.aux.args.push(DfArg {
                    call: call_res,
                    pos: -1,
                    arg: recv,
                });
            }
            for (pos, arg_id) in arg_ids.into_iter().enumerate() {
                df_edge(sink, arg_id, call_res);
                sink.aux.args.push(DfArg {
                    call: call_res,
                    pos: pos as i64,
                    arg: arg_id,
                });
            }
            Some(call_res)
        }
        // `base.Field` outside a call: a member read. As a call's callee (parent
        // is the enclosing call_expression) the call arm above owns it instead.
        "selector_expression" => {
            if node.parent().map(|p| p.kind()) == Some("call_expression") {
                return None;
            }
            let operand = node
                .child_by_field_name("operand")
                .and_then(|o| flow_go(o, src, fn_sym, strings, scope, sink));
            let name = node
                .child_by_field_name("field")
                .map(|f| go_text(f, src).to_string())
                .unwrap_or_default();
            let member = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Member,
                Some(&name),
            );
            if let Some(operand) = operand {
                df_edge(sink, operand, member);
            }
            Some(member)
        }
        // `T{...}` / `[]T{...}` / `map[K]V{...}`: an instantiation. Each element
        // flows into the `new` node (the keyed/positional field labels are dropped
        // aux). The key subtree of a `keyed_element` is a LABEL, never walked.
        "composite_literal" => {
            let type_name = node
                .child_by_field_name("type")
                .map(|t| go_type_name_text(t, src))
                .unwrap_or_default();
            let new_node = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::New,
                Some(&type_name),
            );
            if let Some(body) = node.child_by_field_name("body") {
                go_flow_literal_fields(body, src, fn_sym, strings, scope, sink, new_node);
            }
            Some(new_node)
        }
        // A `literal_value` reached directly (not via `composite_literal`): a
        // nested element literal whose type is implied by the enclosing composite.
        "literal_value" => {
            let new_node = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::New,
                None,
            );
            go_flow_literal_fields(node, src, fn_sym, strings, scope, sink, new_node);
            Some(new_node)
        }
        "binary_expression" => {
            let left = node
                .child_by_field_name("left")
                .and_then(|n| flow_go(n, src, fn_sym, strings, scope, sink));
            let right = node
                .child_by_field_name("right")
                .and_then(|n| flow_go(n, src, fn_sym, strings, scope, sink));
            let binop = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Binop,
                None,
            );
            if let Some(left) = left {
                df_edge(sink, left, binop);
            }
            if let Some(right) = right {
                df_edge(sink, right, binop);
            }
            Some(binop)
        }
        "unary_expression" => {
            let inner = node
                .child_by_field_name("operand")
                .and_then(|n| flow_go(n, src, fn_sym, strings, scope, sink));
            let unop = df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Unop,
                None,
            );
            if let Some(inner) = inner {
                df_edge(sink, inner, unop);
            }
            Some(unop)
        }
        // `x := rhs` (possibly multi-value): bind each declared name to a fresh
        // `let_bind` node. A matching-arity rhs pairs positionally; a mismatched
        // arity taints every target from the first rhs value conservatively.
        "short_var_declaration" => {
            let rhs_ids = node
                .child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, fn_sym, strings, scope, sink))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let names: Vec<tree_sitter::Node> = left
                    .children(&mut cursor)
                    .filter(|n| n.kind() == "identifier")
                    .collect();
                go_bind(
                    &names,
                    &rhs_ids,
                    DfNodeKind::LetBind,
                    src,
                    strings,
                    scope,
                    sink,
                );
            }
            None
        }
        "var_declaration" | "const_declaration" => {
            let mut cursor = node.walk();
            for spec in node
                .children(&mut cursor)
                .filter(|n| matches!(n.kind(), "var_spec" | "const_spec"))
            {
                go_flow_spec(spec, src, fn_sym, strings, scope, sink);
            }
            None
        }
        // `lhs = rhs` (incl. compound `+=`/etc): rebind so later reads see the
        // new value. Non-identifier targets (`x.Field = v`) still flow for side-
        // effect visibility without a scope rebind.
        "assignment_statement" => {
            let rhs_ids = node
                .child_by_field_name("right")
                .map(|right| go_flow_expr_list(right, src, fn_sym, strings, scope, sink))
                .unwrap_or_default();
            if let Some(left) = node.child_by_field_name("left") {
                let mut cursor = left.walk();
                let targets: Vec<tree_sitter::Node> = left.children(&mut cursor).collect();
                let names: Vec<tree_sitter::Node> = targets
                    .iter()
                    .filter(|n| n.kind() == "identifier")
                    .copied()
                    .collect();
                go_bind(
                    &names,
                    &rhs_ids,
                    DfNodeKind::VarWrite,
                    src,
                    strings,
                    scope,
                    sink,
                );
                for target in targets
                    .iter()
                    .filter(|n| n.kind() != "identifier" && n.kind() != ",")
                {
                    flow_go(*target, src, fn_sym, strings, scope, sink);
                }
            }
            None
        }
        // `return a, b`: one `ret` node PER returned value, each fed by its own
        // expression. A naked `return` mints one empty `ret` node so the fn has a
        // visible graph endpoint. The ret node sits at the EXPRESSION's byte (v5
        // uses the expression's position), not the return statement's.
        "return_statement" => {
            let mut cursor = node.walk();
            let list = node
                .children(&mut cursor)
                .find(|n| n.kind() == "expression_list");
            let mut minted = false;
            if let Some(list) = list {
                let mut list_cursor = list.walk();
                for expr in list.children(&mut list_cursor) {
                    if let Some(value) = flow_go(expr, src, fn_sym, strings, scope, sink) {
                        let ret = df_push(
                            sink,
                            strings,
                            expr.start_byte() as u32,
                            expr.end_byte() as u32,
                            DfNodeKind::Ret,
                            None,
                        );
                        df_edge(sink, value, ret);
                        minted = true;
                    }
                }
            }
            if !minted {
                df_push(
                    sink,
                    strings,
                    start_byte,
                    node.end_byte() as u32,
                    DfNodeKind::Ret,
                    None,
                );
            }
            None
        }
        "if_statement" => {
            if let Some(init) = node.child_by_field_name("initializer") {
                flow_go(init, src, fn_sym, strings, scope, sink);
            }
            if let Some(cond) = node.child_by_field_name("condition") {
                flow_go(cond, src, fn_sym, strings, scope, sink);
            }
            if let Some(cons) = node.child_by_field_name("consequence") {
                flow_go(cons, src, fn_sym, strings, scope, sink);
            }
            if let Some(alt) = node.child_by_field_name("alternative") {
                flow_go(alt, src, fn_sym, strings, scope, sink);
            }
            Some(df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::If,
                None,
            ))
        }
        // `for range/clause/cond { body }`: walk the header (binding the range
        // variable when present), then walk the body. A for_statement's non-`body`
        // child is at most ONE of {bare condition, `for_clause`, `range_clause`}.
        "for_statement" => {
            let mut loop_var = String::new();
            let mut collection = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "range_clause" => {
                        if let Some(right) = child.child_by_field_name("right") {
                            collection = Some(go_text(right, src).to_string());
                            flow_go(right, src, fn_sym, strings, scope, sink);
                        }
                        if let Some(left) = child.child_by_field_name("left") {
                            let mut left_cursor = left.walk();
                            let names: Vec<tree_sitter::Node> = left
                                .children(&mut left_cursor)
                                .filter(|n| n.kind() == "identifier")
                                .collect();
                            for name_node in &names {
                                let name = go_text(*name_node, src).to_string();
                                if name == "_" {
                                    continue;
                                }
                                let bind = df_push(
                                    sink,
                                    strings,
                                    name_node.start_byte() as u32,
                                    name_node.end_byte() as u32,
                                    DfNodeKind::LetBind,
                                    Some(&name),
                                );
                                scope.insert(name.clone(), bind);
                                if loop_var.is_empty() {
                                    loop_var = name;
                                }
                            }
                        }
                    }
                    "for_clause" => {
                        if let Some(init) = child.child_by_field_name("initializer") {
                            flow_go(init, src, fn_sym, strings, scope, sink);
                        }
                        if let Some(cond) = child.child_by_field_name("condition") {
                            flow_go(cond, src, fn_sym, strings, scope, sink);
                        }
                        if let Some(update) = child.child_by_field_name("update") {
                            flow_go(update, src, fn_sym, strings, scope, sink);
                        }
                    }
                    "block" | "for" => {}
                    _ => {
                        flow_go(child, src, fn_sym, strings, scope, sink);
                    }
                }
            }
            sink.aux.loops.push(crate::types::DfLoop {
                span: Span {
                    start: start_byte,
                    len: node.end_byte() as u32 - start_byte,
                },
                var: Some(loop_var.clone()).filter(|name| !name.is_empty()),
                collection,
            });
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, fn_sym, strings, scope, sink);
            }
            Some(df_push(
                sink,
                strings,
                start_byte,
                node.end_byte() as u32,
                DfNodeKind::Loop,
                Some(&loop_var),
            ))
        }
        // `func(...) {...}`: lift as its OWN fn scope under v5's `lam_sym`
        // (`{fn_sym}::closure::{row}_{col}`, tree-sitter's 0-based row/col of the
        // literal's start; chains when nested), same shape as Rust
        // closures/Kotlin lambda literals. The enclosing `scope` is shared, so a
        // captured outer variable's read still resolves. The `closure` VALUE node
        // stays in the enclosing fn and carries that exact sym as its name.
        "func_literal" => {
            let pos = node.start_position();
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut param_pos = 0u32;
                let mut cursor = params.walk();
                for param in params.children(&mut cursor) {
                    if !matches!(
                        param.kind(),
                        "parameter_declaration" | "variadic_parameter_declaration"
                    ) {
                        continue;
                    }
                    let mut name_cursor = param.walk();
                    let names: Vec<tree_sitter::Node> = param
                        .children(&mut name_cursor)
                        .filter(|n| n.kind() == "identifier")
                        .collect();
                    if names.is_empty() {
                        param_pos += 1;
                        continue;
                    }
                    for name_node in names {
                        let name = go_text(name_node, src).to_string();
                        let node_ref = df_push(
                            sink,
                            strings,
                            name_node.start_byte() as u32,
                            name_node.end_byte() as u32,
                            DfNodeKind::Param,
                            Some(&name),
                        );
                        sink.aux.params.push(DfParam {
                            node: node_ref,
                            pos: param_pos,
                        });
                        scope.insert(name, node_ref);
                        param_pos += 1;
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                flow_go(body, src, &lam_sym, strings, scope, sink);
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
        // everything else (blocks/statement lists, expression statements,
        // parenthesized/index/slice/type-assertion/conversion expressions,
        // go/defer/send/select/switch/labeled statements, ...): recurse
        // conservatively, surfacing the last value-bearing child.
        _ => go_recurse_children(node, src, fn_sym, strings, scope, sink),
    }
}

/// Flow every element of an `expression_list`, in source order, returning one
/// `Option<NodeRef>` per element. Port of v5 `go_flow_expr_list`.
fn go_flow_expr_list(
    list: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Vec<Option<NodeRef>> {
    let mut cursor = list.walk();
    list.children(&mut cursor)
        .map(|e| flow_go(e, src, fn_sym, strings, scope, sink))
        .collect()
}

/// Bind each name in `names` to a fresh node of `kind` ("let_bind" for a
/// declaration, "var_write" for a plain assignment), wiring the matching rhs
/// value when arity lines up (else every target derives from the first rhs value,
/// conservative). `_` binds nothing. Port of v5 `go_bind`.
fn go_bind(
    names: &[tree_sitter::Node],
    rhs_ids: &[Option<NodeRef>],
    kind: DfNodeKind,
    src: &[u8],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    for (i, name_node) in names.iter().enumerate() {
        let name = go_text(*name_node, src).to_string();
        if name == "_" {
            continue;
        }
        let bind = df_push(
            sink,
            strings,
            name_node.start_byte() as u32,
            name_node.end_byte() as u32,
            kind,
            Some(&name),
        );
        let rhs = if rhs_ids.len() == names.len() {
            rhs_ids.get(i).cloned().flatten()
        } else {
            rhs_ids.first().cloned().flatten()
        };
        if let Some(rhs) = rhs {
            df_edge(sink, rhs, bind);
        }
        scope.insert(name, bind);
    }
}

/// A `var_spec`/`const_spec`: bind each declared identifier to a `let_bind` node
/// fed by its initializer. Port of v5 `go_flow_spec`.
fn go_flow_spec(
    spec: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = spec.walk();
    let names: Vec<tree_sitter::Node> = spec
        .children(&mut cursor)
        .filter(|n| n.kind() == "identifier")
        .collect();
    let rhs_ids = spec
        .child_by_field_name("value")
        .map(|value| go_flow_expr_list(value, src, fn_sym, strings, scope, sink))
        .unwrap_or_default();
    go_bind(
        &names,
        &rhs_ids,
        DfNodeKind::LetBind,
        src,
        strings,
        scope,
        sink,
    );
}

/// A composite literal's body (`literal_value`): each `keyed_element`'s value
/// (and each bare `literal_element`'s value) flows into `owner`. The field labels
/// are dropped aux; the EDGES carry the flow. Port of v5
/// `go_flow_literal_fields`.
fn go_flow_literal_fields(
    lit: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
    owner: NodeRef,
) {
    let mut cursor = lit.walk();
    let mut pos_idx: usize = 0;
    for child in lit.children(&mut cursor) {
        let (key_text, value_wrap) = match child.kind() {
            "keyed_element" => {
                let key_text = child
                    .child_by_field_name("key")
                    .and_then(|key| key.named_child(0))
                    .filter(|inner| inner.kind() == "identifier")
                    .map(|inner| go_text(inner, src).to_string());
                (key_text, child.child_by_field_name("value"))
            }
            "literal_element" => (None, Some(child)),
            _ => continue,
        };
        let Some(value_wrap) = value_wrap else {
            continue;
        };
        let Some(inner) = value_wrap.named_child(0) else {
            continue;
        };
        if let Some(value) = flow_go(inner, src, fn_sym, strings, scope, sink) {
            df_edge(sink, value, owner);
            let field = key_text.unwrap_or_else(|| pos_idx.to_string());
            sink.aux.fields.push(DfField {
                owner,
                name: field,
                value,
            });
        }
        pos_idx += 1;
    }
}

/// The textual name of a composite literal's element type, for the `new` node's
/// name: a bare/qualified named type keeps its name; an anonymous array/slice/
/// map/struct literal type has no name (`""`). Port of v5 `go_type_name_text`.
fn go_type_name_text(node: tree_sitter::Node, src: &[u8]) -> String {
    match node.kind() {
        "type_identifier" => go_text(node, src).to_string(),
        "qualified_type" => node
            .child_by_field_name("name")
            .map(|n| go_text(n, src).to_string())
            .unwrap_or_default(),
        "generic_type" => node
            .child_by_field_name("type")
            .map(|t| go_type_name_text(t, src))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Walk all children conservatively, surfacing the last value-bearing child's
/// node. The generic fallback `flow_go` reaches for every node kind it doesn't
/// special-case. Port of v5 `go_recurse_children`.
fn go_recurse_children(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    let mut cursor = node.walk();
    let mut last = None;
    for child in node.children(&mut cursor) {
        if let Some(id) = flow_go(child, src, fn_sym, strings, scope, sink) {
            last = Some(id);
        }
    }
    last
}

/// Push one df node, returning its `NodeRef` (the dense index edges reference).
/// The node carries its FULL syntactic extent: `FlatFact::Edge` carries endpoint
/// spans only, so a start-only anchor merges distinct value nodes. `end` is
/// exclusive (tree-sitter `end_byte()`); node STARTS are unchanged, so the v5
/// parity golden stays byte-exact. Port of v5 `push_node` (minus
/// fn_sym/file/aux).
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
            let parsed = {
                let span = trace::parse_span("go", "astgrep");
                let _entered = span.enter();
                AstGrepParser.parse(&arena, path, content).ok()
            };
            parsed.map(|parsed| {
                let span = trace::family_span("go", "cst");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
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
                let tree = {
                    let span = trace::parse_span("go", "tree-sitter");
                    let _entered = span.enter();
                    go_parse(src)
                };
                if let Some(tree) = tree {
                    let root = tree.root_node();
                    let src_bytes = src.as_bytes();
                    if mask.types {
                        let span = trace::family_span("go", "type");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(root, src_bytes, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let span = trace::family_span("go", "call");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<CallF>::default();
                        project_call(root, src_bytes, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
                        call = Some(bundle);
                    }
                    if mask.df {
                        let span = trace::family_span("go", "df");
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

// ════════════════════════════════════════════════════════════════════════════
// Resolve<TypeF> for GoSource (commit 4d-i-go). The exact twin of the TsSource
// arm (4b-iii): candidates in, no AST. The candidate row IS the parity target
// (user ruling 2026-07-24, option (a)): v5's `type_edge.to` is free text, so
// text dsts STAY text — a candidate whose `to` names no corpus node (a
// qualified `pkg.Type` ref, a constraint naming no local decl) emits a ZERO dst
// leg (ZERO_CONTENT_ID + Span::default), never a fake node join. The
// genuinely-resolved span->blob legs are a v6-only ADDITIVE layer (reported,
// never asserted). Same-file blob leg: the TypeF node named `to` in THIS
// bundle gives the span, and the DefIndex span-join gives the blob (the output
// carries no hash of its own). Corpus fallback: a UNIQUE site only.
// The helper triplication with ts.rs (`type_edge_candidates` /
// `resolve_type_dst`) is DELIBERATE per the design audit's SEQUENCING RULING
// (2026-07-24): ALL dedup lands in ONE sweep AFTER the Resolve pass (4a-4d)
// fully lands, never interleaved with a resolve arm.
// ════════════════════════════════════════════════════════════════════════════

impl GoSource {
    /// The deduped, deterministically-ordered candidate list (v5's BTreeSet
    /// shaping): the aux candidates, deduped on (owner, to, kind). `resolve`
    /// emits its edges in EXACTLY this order, one per candidate — the parity
    /// golden zips the two (the zip discipline: edge i resolves candidate i).
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
/// (text stays text — the zero leg). Name-only resolution, per the 4a ADDENDUM
/// site-key discipline (no receiver typing anywhere in commit 4).
fn resolve_type_dst(
    types: &FamilyBundle<TypeF>,
    strings: &Strings,
    index: Option<&DefIndex>,
    name: &str,
) -> Option<(ContentId, Span)> {
    let same_file = types
        .nodes
        .iter()
        .find(|node| node.name.map_or(false, |id| strings.lookup(id) == name));
    if let (Some(node), Some(index)) = (same_file, index) {
        return corpus_defs(index, name)
            .iter()
            .find(|site| site.span == node.span)
            .map(|site| (site.blob.clone(), site.span));
    }
    let sites = index.map(|index| corpus_defs(index, name)).unwrap_or(&[]);
    match sites {
        [only] => Some((only.blob.clone(), only.span)),
        _ => None,
    }
}

impl Resolve<TypeF> for GoSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let index = cx.indexes.def_index.get();
        let mut edges = Vec::new();
        for candidate in GoSource::type_edge_candidates(output) {
            // src: the TypeF entity at the owner span. Exists by construction
            // (candidates are minted beside their entity); a miss would break
            // the parity golden's zip count loudly, so it is not hidden here.
            let Some(src_ix) = types
                .nodes
                .iter()
                .position(|node| node.span == candidate.owner)
            else {
                continue;
            };
            let (dst_blob, dst_span) = resolve_type_dst(
                types,
                &output.strings,
                index,
                output.strings.lookup(candidate.to),
            )
            .unwrap_or((ZERO_CONTENT_ID, Span::empty()));
            edges.push(ProjectEdge::new(
                NodeRef(src_ix as u32),
                dst_blob,
                dst_span,
                candidate.kind,
            ));
        }
        edges
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Resolve<CallF> for GoSource (commit 4d-ii-go). The exact twin of the TsSource
// arm (4c-ii), two legs per the user rulings (2026-07-24: scip-override
// ALLOWED; the v5-shaped name-match stays primary):
//   NameResolve — callee name -> unique def. Same-file WINS via the span-join
//     (def_named in THIS CallF bundle -> its span -> the DefIndex gives the
//     blob); cross-file a UNIQUE corpus blob (CallF facet preferred);
//     ambiguous/absent -> NO ROW (the 4b-iii discipline). For go the
//     cross-package ambiguity is the common case: two packages exporting the
//     same func name make the name-match abstain (exactly the case scip then
//     settles through the import).
//   ScipOverride — scip-go's occurrence resolution for the site disagrees with
//     the name-match outcome (a different corpus target, or any corpus target
//     where the name-match bound none): scip's target WINS the edge, the
//     name-match is displaced. The leg needs the corpus scip index
//     (cx.indexes.scip_index) AND the rev-correct reader (cx.reader); either
//     absent -> pure name-match (v5-shaped). scip-EXTERNAL (a stdlib symbol -
//     scip-go tags those `gomod github.com/golang/go/src ...` - an unresolved
//     reference, or no occurrence at the site) is NOT a corpus target: it
//     never displaces a NameResolve row and never mints one.
// The arm learns its own blob by the DefIndex span-join (`own_blob`) and its
// scip document by content hash (`join_documents`) — the resolve seam carries
// no path and no bytes (the 4b-i gap), so identity flows through content.
// Per-site edges, no dedup: two calls to one callee are two resolutions. A
// site outside every CallF def (package level) emits no row — v5's call_edge
// has no module caller. `callee_path` stays None for go: v5 go collects no
// path (go_callee returns name+line only, so V5-IS-CORRECT keeps it empty),
// the name-only + scip resolution does not need it, and filling it is the
// same declared-snapshot-increment catch-up ts deferred in 4c-ii.
// The helper triplication with ts.rs (`call_name_match` / `scip_call_target`)
// is DELIBERATE per the design audit's SEQUENCING RULING (2026-07-24): ALL
// dedup lands in ONE sweep AFTER the Resolve pass (4a-4d) fully lands.
// ════════════════════════════════════════════════════════════════════════════

impl GoSource {
    /// The name-match target of one callee (the NameResolve leg). Pub so the
    /// scip ratchet re-runs it to classify overrides — same discipline as
    /// `type_edge_candidates` in 4d-i-go. Same-file wins via the span-join;
    /// cross-file a unique corpus blob (the CallF facet's site preferred);
    /// ambiguous/absent -> None.
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
            .find(|s| s.family == FamilyTag::Call)
            .unwrap_or(&sites[0]);
        Some((blob.clone(), site.span))
    }
}

/// The scip-resolved corpus target of one call site: the site's occurrence
/// (the shared `site_occurrence` convention — for a selector callee
/// `recv.M`/`pkg.F` the occurrence whose text is the trailing field, inside
/// the whole-selector site span) -> its symbol's definition occurrence
/// (scip's own resolution; `local ` symbols document-scoped) -> the
/// containing DefSite (scip's def range marks the identifier, inside v6's
/// whole-decl span). None = scip has no corpus answer (a stdlib/external
/// symbol, an unresolved reference, no occurrence at the site, or the target
/// document is outside the corpus).
fn scip_call_target<'a>(
    index: &ScipIndex,
    joined: &[Option<(ContentId, Vec<u8>)>],
    doc_ix: usize,
    site: &CallSite,
    callee: &str,
    def_index: &'a DefIndex,
) -> Option<(ContentId, Span, &'a str)> {
    let doc = &index.documents[doc_ix];
    let (_, content) = joined[doc_ix].as_ref()?;
    let occ = site_occurrence(doc, content, site.span, callee)?;
    let (def_doc_ix, def_occ) = definition_of(index, doc_ix, &occ.symbol)?;
    let def_doc = &index.documents[def_doc_ix];
    let (def_blob, def_content) = joined[def_doc_ix].as_ref()?;
    let ident = byte_range(def_content, def_occ.range, def_doc.position_encoding)?;
    let (name, def_site) = containing_def_site(def_index, def_blob.clone(), ident)?;
    Some((def_blob.clone(), def_site.span, name))
}

impl Resolve<CallF> for GoSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        // The scip leg: the corpus index + the rev-correct reader + this
        // file's own document (found by content hash). Any missing piece ->
        // pure name-match (v5-shaped).
        let scip = cx
            .indexes
            .scip_index
            .get()
            .zip(cx.reader)
            .and_then(|(index, reader)| {
                let joined = cx
                    .indexes
                    .joined_documents
                    .get_or_init(|| join_documents(index, reader));
                let blob = own_blob(output, def_index)?;
                let doc_ix = joined
                    .iter()
                    .position(|j| j.as_ref().map_or(false, |(b, _)| *b == blob))?;
                Some((index, joined, doc_ix))
            });
        let mut edges = Vec::new();
        for site in &call.aux.sites {
            // The caller is the innermost covering CallF def (the 4a
            // caller-binding discipline); a package-level site has no caller
            // node and emits no row.
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let name_t = GoSource::call_name_match(output, def_index, callee);
            let scip_t = scip.as_ref().and_then(|(index, joined, doc_ix)| {
                scip_call_target(index, joined, *doc_ix, site, callee, def_index)
            });
            // Agreement is judged at (blob, name): the name-match binds the
            // call FACET (the callable def) while scip can name the type facet
            // (a conversion `Mode(0)`'s type) — one definition, two facet
            // coordinates (the ORACLE entry's "the models differ by
            // construction").
            let ((dst_blob, dst_span), kind) = match (name_t, scip_t) {
                (Some(n), Some(s)) if n.0 == s.0 && callee == s.2 => (n, CallEdgeKind::NameResolve),
                (_, Some(s)) => ((s.0, s.1), CallEdgeKind::ScipOverride),
                (Some(n), None) => (n, CallEdgeKind::NameResolve),
                (None, None) => continue,
            };
            edges
                .push(ProjectEdge::new(caller, dst_blob, dst_span, kind).with_call_site(site.span));
        }
        edges
    }
}
