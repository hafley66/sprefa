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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use super::astgrep::{AstGrepParser, CstProjector};
use super::go_modules::{is_exported, GoModuleIndex};
use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstF, DfArg, DfEdgeKind, DfF, DfField, DfNodeKind,
    DfParam, DocFact, DocTag, MethodOwner, ProjectEdge, ReceiverBinding, ReceiverOutcome, SigSlot,
    Specifier, SpecifierKind, TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeF, TypeSig,
};
use crate::project::ResolveDrop;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::scip::{byte_range_cached, definition_of, join_documents, site_occurrence};
use crate::seams::{
    containing_def_site, corpus_defs, covering_def, def_named, own_blob, DefIndex, DefSite, Parser,
    Project, Resolve,
};
use crate::shape::{ContentId, FamilyTag, NameId, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::source::{ExtractOutput, FamilyMask, ProjectCx, Source};
use crate::trace;
use crate::types::{PathIndex, ScipIndex, UnresolvedReason};

// ── the tree-sitter-go parse (one parse feeds type/call/df) ──────────────────

/// Parse Go source via tree-sitter-go. Port of v5 `go_parse`
/// (src/graph/typegraph/go.rs:41). tree-sitter 0.25's `Language::new` wraps the
/// `LanguageFn` tree-sitter-go 0.23 exports as `LANGUAGE`; the versions unify
/// with what ast-grep-language already transitively pulls.
pub(crate) fn go_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_go::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// Parse Go source, reusing the tree the extract pass already produced for
/// these exact bytes on this thread (task: one parse per file per language).
/// The single-entry handoff: `dispatch` parses and stores, the module plane
/// consumes on the same worker thread.
pub(crate) fn go_parse_shared(content: &str) -> Option<std::sync::Arc<tree_sitter::Tree>> {
    use crate::shape::content_id_of;
    thread_local! {
        static LAST: std::cell::RefCell<Option<(crate::shape::ContentId, std::sync::Arc<tree_sitter::Tree>)>> =
            const { std::cell::RefCell::new(None) };
    }
    let id = content_id_of(content.as_bytes());
    if let Some((_, tree)) = LAST.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|(cached, _)| cached == &id)
            .cloned()
    }) {
        return Some(tree);
    }
    let tree = std::sync::Arc::new(go_parse(content)?);
    LAST.with(|slot| *slot.borrow_mut() = Some((id, tree.clone())));
    Some(tree)
}

/// UTF-8 text of a tree-sitter node. Port of v5 `go_text`.
pub(crate) fn go_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
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
                    let span = go_node_span(spec);
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
                    let span = go_node_span(child);
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
                    let span = go_node_span(child);
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
pub(crate) fn go_node_span(node: tree_sitter::Node) -> Span {
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
    let field_types = go_field_types(root, src);
    go_collect_receivers(root, src, strings, sink, &imports, &field_types);
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
/// `pub(crate)`: `go_modules.rs`'s own dedicated parse reuses this walk
/// directly rather than re-deriving the kind/name/module table a second time.
pub(crate) fn go_walk_import_specs(
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
                if let (Some(name_node), Some(owner)) = (
                    child.child_by_field_name("name"),
                    go_receiver_type(child, src),
                ) {
                    let span = def_span(child);
                    let name = go_text(name_node, src).to_string();
                    sink.nodes
                        .push(Node::new(span, CallKind::Method).with_name(strings.intern(&name)));
                    sink.aux.method_owners.push(MethodOwner {
                        span,
                        self_type: Some(strings.intern(&owner)),
                        trait_name: None,
                    });
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
            // An interface method spec is its own callable def, owner = the
            // interface name (leg-2 dispatch treats it like a concrete method).
            "type_declaration" => {
                let mut sc = child.walk();
                for spec in child.children(&mut sc).filter(|n| n.kind() == "type_spec") {
                    go_mint_interface_methods(spec, src, strings, sink);
                }
                continue;
            }
            _ => {}
        }
        go_walk_call_defs(child, src, strings, sink, in_fn);
    }
}

fn go_mint_interface_methods(
    spec: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let Some(name_node) = spec.child_by_field_name("name") else {
        return;
    };
    let Some(iface) = spec
        .child_by_field_name("type")
        .filter(|t| t.kind() == "interface_type")
    else {
        return;
    };
    let iname = go_text(name_node, src).to_string();
    let mut mc = iface.walk();
    for elem in iface
        .children(&mut mc)
        .filter(|n| n.kind() == "method_elem")
    {
        let Some(mname) = elem.child_by_field_name("name") else {
            continue;
        };
        let span = go_node_span(elem);
        let name = go_text(mname, src).to_string();
        sink.nodes
            .push(Node::new(span, CallKind::Method).with_name(strings.intern(&name)));
        sink.aux.method_owners.push(MethodOwner {
            span,
            self_type: Some(strings.intern(&iname)),
            trait_name: None,
        });
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
                        span: go_node_span(func),
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

// Receiver types (CallFAux.receivers): `x.M()` binds through x's declared
// type T; `:=` from a call result is Inferred, a rebind conflict is Ambiguous.

/// Phase-1 record of every `x := f()` / `var x = f()` binding site plus every
/// receiver site whose operand's binding came from one. Keyed by spans, so the
/// resolve phase can join it to the call-site stream in source order and give
/// each name the callee's declared result type (one hop, no fixpoint).
#[derive(Default)]
struct GoBindPlan {
    /// rhs call START byte -> (enclosing top-level fn span, bound names in
    /// order). A call site's span is its function part, whose start is the
    /// call's own start for both `f()` and `recv.M()`; the key is start-only.
    binds: HashMap<u32, ((u32, u32), Vec<String>)>,
    /// receiver site span -> (enclosing top-level fn span, the operand name).
    inferred_recv: HashMap<(u32, u32), ((u32, u32), String)>,
    /// receiver site span -> (enclosing top-level fn span, the chain). A chain
    /// site is one whose operand is a selector chain with at least one call
    /// hop; the final `.c()` is the site's own callee.
    multihop: HashMap<(u32, u32), ((u32, u32), GoChain)>,
    /// This file's struct field types, so the resolve phase can fold `.field`
    /// hops whose struct is declared here.
    fields: HashMap<(String, String), DeclType>,
}

/// The leftmost value of a selector chain: a name in scope, or an
/// import-qualified func call `pkg.F()` whose result type resolve re-derives.
#[derive(Clone, Debug, PartialEq, Eq)]
enum GoChainBase {
    Var { name: String, decl: Option<String> },
    Import { callee: String, path: String },
}

/// One hop of a chain, in source order. `Field` folds through a struct field's
/// declared type, `Call` through the method's declared first result, `Elem`
/// through a slice/array/map's element.
#[derive(Clone, Debug, PartialEq, Eq)]
enum GoChainStep {
    Field(String),
    Call(String),
    Elem,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GoChain {
    base: GoChainBase,
    steps: Vec<GoChainStep>,
}

/// The maximum number of hops a chain may carry, the site's own final call
/// included; deeper chains keep their current outcome.
const GO_CHAIN_MAX_STEPS: usize = 8;

/// Decompose a chain operand into (base, hops). The hops are appended in
/// source order; a call hop's own method name is one hop, the site's final
/// callee is NOT a hop here. Returns None for anything the tier cannot type
/// one pass (a local call `f()`, an import qualifier mid-chain, a field on an
/// unknown type, an index, a type assertion).
fn go_chain_of(
    expr: tree_sitter::Node,
    src: &[u8],
    scope: &TypeScope,
    imports: &HashMap<String, String>,
    steps: &mut Vec<GoChainStep>,
) -> Option<GoChainBase> {
    match expr.kind() {
        "identifier" => {
            let name = go_text(expr, src);
            match scope_lookup(scope, name) {
                Some(TypeBinding::Decl(DeclType::Named(t))) => Some(GoChainBase::Var {
                    name: name.to_string(),
                    decl: Some(t.clone()),
                }),
                Some(TypeBinding::Chained(recorded)) => {
                    steps.splice(0..0, recorded.steps.iter().cloned());
                    Some(recorded.base.clone())
                }
                Some(TypeBinding::Inferred) | None => {
                    if imports.contains_key(name) {
                        None
                    } else {
                        Some(GoChainBase::Var {
                            name: name.to_string(),
                            decl: None,
                        })
                    }
                }
                _ => None,
            }
        }
        "selector_expression" => {
            let operand = expr.child_by_field_name("operand")?;
            let field = expr.child_by_field_name("field")?;
            if operand.kind() == "identifier" && imports.contains_key(go_text(operand, src)) {
                return None;
            }
            let base = go_chain_of(operand, src, scope, imports, steps)?;
            steps.push(GoChainStep::Field(go_text(field, src).to_string()));
            Some(base)
        }
        "index_expression" => {
            let operand = expr.child_by_field_name("operand")?;
            let base = go_chain_of(operand, src, scope, imports, steps)?;
            steps.push(GoChainStep::Elem);
            Some(base)
        }
        "parenthesized_expression" => go_chain_of(expr.named_child(0)?, src, scope, imports, steps),
        "call_expression" => {
            let function = expr.child_by_field_name("function")?;
            if function.kind() != "selector_expression" {
                return None;
            }
            let operand = function.child_by_field_name("operand")?;
            let field = function.child_by_field_name("field")?;
            if operand.kind() == "identifier" && imports.contains_key(go_text(operand, src)) {
                // `pkg.F()` is only a chain ROOT: its result type is
                // re-derived at resolve through the import leg.
                return if steps.is_empty() {
                    Some(GoChainBase::Import {
                        callee: go_text(field, src).to_string(),
                        path: imports.get(go_text(operand, src))?.clone(),
                    })
                } else {
                    None
                };
            }
            let base = go_chain_of(operand, src, scope, imports, steps)?;
            steps.push(GoChainStep::Call(go_text(field, src).to_string()));
            Some(base)
        }
        _ => None,
    }
}

/// The per-process plan cache, keyed by content (the extraction cache never
/// skips the first extraction of a process, so every resolved file has a row).
fn plan_cache() -> &'static Mutex<HashMap<ContentId, Arc<GoBindPlan>>> {
    static CACHE: OnceLock<Mutex<HashMap<ContentId, Arc<GoBindPlan>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn go_bind_plan_of(blob: &ContentId) -> Option<Arc<GoBindPlan>> {
    let guard = plan_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    guard.get(blob).cloned()
}

fn go_bind_plan_store(blob: ContentId, plan: GoBindPlan) {
    let mut guard = plan_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    guard.insert(blob, Arc::new(plan));
}

/// A declared type, unwrapped one level. `Indexable` is a slice/array/map's
/// element/value type, reachable via `s[i]` or a two-name `range`, never
/// through `s` itself; `Streamed` is a channel's, reachable by `range` alone.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DeclType {
    Named(String),
    Indexable(String),
    Streamed(String),
}

/// One name's binding within the innermost enclosing scope frame.
#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeBinding {
    Decl(DeclType),
    /// `x := <selector chain>` whose type this file cannot name: the chain is
    /// kept verbatim and replayed at the USE site, where the corpus is joined.
    Chained(GoChain),
    Inferred,
    Ambiguous,
}

/// A stack of block-scoped frames, innermost last (an inner `x` shadows an
/// outer one; lookup walks top-down).
type TypeScope = Vec<HashMap<String, TypeBinding>>;

fn scope_insert(scope: &mut TypeScope, name: String, binding: TypeBinding) {
    let Some(frame) = scope.last_mut() else {
        return;
    };
    match frame.get(&name) {
        Some(existing) if *existing != binding => {
            frame.insert(name, TypeBinding::Ambiguous);
        }
        _ => {
            frame.insert(name, binding);
        }
    }
}

fn scope_lookup<'a>(scope: &'a TypeScope, name: &str) -> Option<&'a TypeBinding> {
    scope.iter().rev().find_map(|frame| frame.get(name))
}

/// The declared type of a `_type` node, pointer-stripped, slice/array/map
/// unwrapped to `Indexable`.
fn go_decl_type_of(ty: tree_sitter::Node, src: &[u8]) -> Option<DeclType> {
    let ty = if ty.kind() == "pointer_type" {
        ty.named_child(0)?
    } else {
        ty
    };
    match ty.kind() {
        "type_identifier" | "qualified_type" => Some(DeclType::Named(go_text(ty, src).to_string())),
        "generic_type" => ty
            .child_by_field_name("type")
            .map(|t| DeclType::Named(go_text(t, src).to_string())),
        "slice_type" | "array_type" => {
            let elem = ty.child_by_field_name("element")?;
            match go_decl_type_of(elem, src)? {
                DeclType::Named(name) => Some(DeclType::Indexable(name)),
                _ => None,
            }
        }
        "map_type" => {
            let value = ty.child_by_field_name("value")?;
            match go_decl_type_of(value, src)? {
                DeclType::Named(name) => Some(DeclType::Indexable(name)),
                _ => None,
            }
        }
        "channel_type" => {
            let value = ty.child_by_field_name("value")?;
            match go_decl_type_of(value, src)? {
                DeclType::Named(name) => Some(DeclType::Streamed(name)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// (struct name, field name) -> the field's declared type, this file's own
/// struct declarations only (no false edge, just no binding, otherwise).
fn go_field_types(root: tree_sitter::Node, src: &[u8]) -> HashMap<(String, String), DeclType> {
    let mut out = HashMap::new();
    go_walk_field_types(root, src, &mut out);
    out
}

fn go_walk_field_types(
    node: tree_sitter::Node,
    src: &[u8],
    out: &mut HashMap<(String, String), DeclType>,
) {
    if node.kind() == "type_declaration" {
        let mut sc = node.walk();
        for spec in node.children(&mut sc).filter(|n| n.kind() == "type_spec") {
            let Some(name_node) = spec.child_by_field_name("name") else {
                continue;
            };
            let Some(struct_ty) = spec.child_by_field_name("type") else {
                continue;
            };
            if struct_ty.kind() != "struct_type" {
                continue;
            }
            let struct_name = go_text(name_node, src).to_string();
            let mut lc = struct_ty.walk();
            let Some(list) = struct_ty
                .children(&mut lc)
                .find(|n| n.kind() == "field_declaration_list")
            else {
                continue;
            };
            let mut fc = list.walk();
            for field in list
                .children(&mut fc)
                .filter(|n| n.kind() == "field_declaration")
            {
                let Some(ftype) = field.child_by_field_name("type") else {
                    continue;
                };
                let Some(decl) = go_decl_type_of(ftype, src) else {
                    continue;
                };
                let mut nc = field.walk();
                for name_node in field
                    .children(&mut nc)
                    .filter(|n| n.kind() == "field_identifier")
                {
                    out.insert(
                        (struct_name.clone(), go_text(name_node, src).to_string()),
                        decl.clone(),
                    );
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        go_walk_field_types(child, src, out);
    }
}

/// Seed one frame from a parameter list (or a receiver's own single param).
/// A variadic `xs ...T` behaves like `[]T`: indexable, not itself a receiver.
fn go_seed_params(params: tree_sitter::Node, src: &[u8], scope: &mut TypeScope) {
    let mut cursor = params.walk();
    for param in params.children(&mut cursor) {
        if !matches!(
            param.kind(),
            "parameter_declaration" | "variadic_parameter_declaration"
        ) {
            continue;
        }
        let Some(ty) = param.child_by_field_name("type") else {
            continue;
        };
        let Some(decl) = go_decl_type_of(ty, src) else {
            continue;
        };
        let decl = if param.kind() == "variadic_parameter_declaration" {
            match decl {
                DeclType::Named(name) => DeclType::Indexable(name),
                indexable => indexable,
            }
        } else {
            decl
        };
        let mut nc = param.walk();
        for name_node in param.children(&mut nc).filter(|n| n.kind() == "identifier") {
            scope_insert(
                scope,
                go_text(name_node, src).to_string(),
                TypeBinding::Decl(decl.clone()),
            );
        }
    }
}

/// A method/function's opening frame: the receiver (base type, `*` stripped)
/// plus every parameter's declared type.
fn go_seed_top_scope(fn_node: tree_sitter::Node, src: &[u8]) -> TypeScope {
    let mut scope: TypeScope = vec![HashMap::new()];
    if fn_node.kind() == "method_declaration" {
        if let Some(recv_list) = fn_node.child_by_field_name("receiver") {
            let mut rc = recv_list.walk();
            let param = recv_list
                .children(&mut rc)
                .find(|n| n.kind() == "parameter_declaration");
            if let Some(param) = param {
                let mut pc = param.walk();
                let recv_name = param.children(&mut pc).find(|n| n.kind() == "identifier");
                if let (Some(recv_name), Some(owner)) = (recv_name, go_receiver_type(fn_node, src))
                {
                    scope_insert(
                        &mut scope,
                        go_text(recv_name, src).to_string(),
                        TypeBinding::Decl(DeclType::Named(owner)),
                    );
                }
            }
        }
    }
    if let Some(params) = fn_node.child_by_field_name("parameters") {
        go_seed_params(params, src, &mut scope);
    }
    scope
}

/// The rhs binding a `:=`/`var` name gets: a (possibly `&`-taken) composite
/// literal names its type; a call result is `Inferred`; every other shape is
/// whatever `go_operand_decl` can type (a name, a field read, an index read).
fn go_binding_of_rhs(
    rhs: tree_sitter::Node,
    src: &[u8],
    scope: &TypeScope,
    imports: &HashMap<String, String>,
    field_types: &HashMap<(String, String), DeclType>,
) -> Option<TypeBinding> {
    match rhs.kind() {
        "composite_literal" => {
            let ty = rhs.child_by_field_name("type")?;
            go_decl_type_of(ty, src).map(TypeBinding::Decl)
        }
        "unary_expression" => {
            let op = rhs.child_by_field_name("operator")?;
            if go_text(op, src) != "&" {
                return None;
            }
            let operand = rhs.child_by_field_name("operand")?;
            go_binding_of_rhs(operand, src, scope, imports, field_types)
        }
        "call_expression" => Some(match go_paren_conversion_type(rhs, src) {
            Some(name) => TypeBinding::Decl(DeclType::Named(name)),
            None => TypeBinding::Inferred,
        }),
        "type_assertion_expression" => {
            let ty = rhs.child_by_field_name("type")?;
            go_decl_type_of(ty, src).map(TypeBinding::Decl)
        }
        _ => go_operand_decl(rhs, src, scope, field_types).or_else(|| {
            let mut steps = Vec::new();
            let base = go_chain_of(rhs, src, scope, imports, &mut steps)?;
            (!steps.is_empty() && steps.len() < GO_CHAIN_MAX_STEPS)
                .then_some(TypeBinding::Chained(GoChain { base, steps }))
        }),
    }
}

/// The type a PARENTHESIZED conversion `(*T)(x)` names. Bare `T(x)` is a call
/// to the parser; the resolve phase settles that one by the target's decl kind.
fn go_paren_conversion_type(call: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let function = call.child_by_field_name("function")?;
    if function.kind() != "parenthesized_expression" {
        return None;
    }
    let inner = function.named_child(0)?;
    let named = match inner.kind() {
        "unary_expression" if go_text(inner.child_by_field_name("operator")?, src) == "*" => {
            inner.child_by_field_name("operand")?
        }
        "identifier" | "selector_expression" => inner,
        _ => return None,
    };
    match named.kind() {
        "identifier" | "selector_expression" => Some(go_text(named, src).to_string()),
        _ => None,
    }
}

/// The type an operand expression carries: an identifier, an index into a
/// slice/array/map, or a field read. Collection shapes survive; the gate cuts.
fn go_operand_decl(
    operand: tree_sitter::Node,
    src: &[u8],
    scope: &TypeScope,
    field_types: &HashMap<(String, String), DeclType>,
) -> Option<TypeBinding> {
    match operand.kind() {
        "identifier" => scope_lookup(scope, go_text(operand, src)).cloned(),
        "parenthesized_expression" => {
            go_operand_decl(operand.named_child(0)?, src, scope, field_types)
        }
        "index_expression" => {
            let base = operand.child_by_field_name("operand")?;
            match go_operand_decl(base, src, scope, field_types)? {
                TypeBinding::Decl(DeclType::Indexable(t)) => {
                    Some(TypeBinding::Decl(DeclType::Named(t)))
                }
                _ => None,
            }
        }
        "selector_expression" => {
            let base = operand.child_by_field_name("operand")?;
            let field = operand.child_by_field_name("field")?;
            match go_operand_decl(base, src, scope, field_types)? {
                TypeBinding::Decl(DeclType::Named(struct_name)) => field_types
                    .get(&(struct_name, go_text(field, src).to_string()))
                    .cloned()
                    .map(TypeBinding::Decl),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `go_operand_decl` under the receiver gate: a slice/map/channel var is no
/// receiver, so it binds nothing rather than its element type.
fn go_receiver_binding(
    operand: tree_sitter::Node,
    src: &[u8],
    scope: &TypeScope,
    field_types: &HashMap<(String, String), DeclType>,
) -> Option<TypeBinding> {
    match go_operand_decl(operand, src, scope, field_types)? {
        TypeBinding::Decl(DeclType::Indexable(_))
        | TypeBinding::Decl(DeclType::Streamed(_))
        | TypeBinding::Chained(_) => None,
        other => Some(other),
    }
}

/// Walk one callable body, threading a block-scoped `TypeScope`, recording a
/// `(span, TypeBinding)` row per non-import selector call site.
fn go_walk_receivers(
    node: tree_sitter::Node,
    src: &[u8],
    scope: &mut TypeScope,
    imports: &HashMap<String, String>,
    field_types: &HashMap<(String, String), DeclType>,
    out: &mut Vec<(Span, TypeBinding)>,
    plan: &mut GoBindPlan,
    top: (u32, u32),
) {
    match node.kind() {
        "block" | "if_statement" | "for_statement" | "expression_switch_statement" => {
            scope.push(HashMap::new());
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                go_walk_receivers(child, src, scope, imports, field_types, out, plan, top);
            }
            scope.pop();
        }
        "range_clause" => {
            if let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                let mut lc = left.walk();
                let idents: Vec<tree_sitter::Node> = left
                    .children(&mut lc)
                    .filter(|n| n.kind() == "identifier")
                    .collect();
                // Go's arity rule: a slice/map puts the element in a SECOND
                // name, a channel in the first.
                go_walk_receivers(right, src, scope, imports, field_types, out, plan, top);
                let local = match go_operand_decl(right, src, scope, field_types) {
                    Some(TypeBinding::Decl(DeclType::Indexable(t))) if idents.len() == 2 => {
                        Some((idents[1], t))
                    }
                    Some(TypeBinding::Decl(DeclType::Streamed(t))) if idents.len() == 1 => {
                        Some((idents[0], t))
                    }
                    _ => None,
                };
                match local {
                    Some((name_node, t)) => scope_insert(
                        scope,
                        go_text(name_node, src).to_string(),
                        TypeBinding::Decl(DeclType::Named(t)),
                    ),
                    // A two-name range over a chain this file cannot type:
                    // record the chain plus one `Elem` hop for the use site.
                    None if idents.len() == 2 => {
                        let mut steps = Vec::new();
                        if let Some(base) = go_chain_of(right, src, scope, imports, &mut steps) {
                            steps.push(GoChainStep::Elem);
                            if steps.len() < GO_CHAIN_MAX_STEPS {
                                scope_insert(
                                    scope,
                                    go_text(idents[1], src).to_string(),
                                    TypeBinding::Chained(GoChain { base, steps }),
                                );
                            }
                        }
                    }
                    None => {}
                }
            }
        }
        "type_switch_statement" => {
            go_walk_type_switch(node, src, scope, imports, field_types, out, plan, top);
        }
        "var_declaration" => {
            let mut cursor = node.walk();
            for spec in node
                .children(&mut cursor)
                .filter(|n| n.kind() == "var_spec")
            {
                if let Some(ty) = spec.child_by_field_name("type") {
                    if let Some(decl) = go_decl_type_of(ty, src) {
                        let mut nc = spec.walk();
                        for name_node in spec.children(&mut nc).filter(|n| n.kind() == "identifier")
                        {
                            scope_insert(
                                scope,
                                go_text(name_node, src).to_string(),
                                TypeBinding::Decl(decl.clone()),
                            );
                        }
                    }
                } else if let Some(value) = spec.child_by_field_name("value") {
                    if value.kind() == "call_expression" {
                        let mut nc = spec.walk();
                        let names: Vec<String> = spec
                            .children(&mut nc)
                            .filter(|n| n.kind() == "identifier")
                            .map(|n| go_text(n, src).to_string())
                            .collect();
                        // One name, one call value (`var x = f()`); a grouped
                        // `var a, b = f(), g()` pairs positionally and is
                        // out of this tier.
                        if let [name] = names.as_slice() {
                            scope_insert(scope, name.clone(), TypeBinding::Inferred);
                            plan.binds.insert(go_node_span(value).start, (top, names));
                        }
                    }
                }
                if let Some(value) = spec.child_by_field_name("value") {
                    go_walk_receivers(value, src, scope, imports, field_types, out, plan, top);
                }
            }
        }
        "short_var_declaration" => {
            if let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) {
                // Walked BEFORE the names bind: a `:=` name's scope starts
                // after the statement, so `x := x.M()` reads the OUTER x.
                go_walk_receivers(right, src, scope, imports, field_types, out, plan, top);
                let mut lc = left.walk();
                let names: Vec<tree_sitter::Node> = left
                    .children(&mut lc)
                    .filter(|n| n.kind() == "identifier")
                    .collect();
                let mut rc = right.walk();
                let rhss: Vec<tree_sitter::Node> = right.children(&mut rc).collect();
                if names.len() == rhss.len() {
                    for (name_node, rhs) in names.iter().zip(rhss.iter()) {
                        if let Some(binding) =
                            go_binding_of_rhs(*rhs, src, scope, imports, field_types)
                        {
                            scope_insert(scope, go_text(*name_node, src).to_string(), binding);
                        }
                        if rhs.kind() == "call_expression" {
                            plan.binds.insert(
                                go_node_span(*rhs).start,
                                (top, vec![go_text(*name_node, src).to_string()]),
                            );
                        }
                    }
                } else if let [rhs] = rhss.as_slice() {
                    if rhs.kind() == "call_expression" {
                        let bound: Vec<String> = names
                            .iter()
                            .map(|name_node| go_text(*name_node, src).to_string())
                            .collect();
                        for name_node in &names {
                            scope_insert(
                                scope,
                                go_text(*name_node, src).to_string(),
                                TypeBinding::Inferred,
                            );
                        }
                        plan.binds.insert(go_node_span(*rhs).start, (top, bound));
                    }
                }
            }
        }
        "func_literal" => {
            scope.push(HashMap::new());
            if let Some(params) = node.child_by_field_name("parameters") {
                go_seed_params(params, src, scope);
            }
            if let Some(body) = node.child_by_field_name("body") {
                go_walk_receivers(body, src, scope, imports, field_types, out, plan, top);
            }
            scope.pop();
        }
        "call_expression" => {
            if let Some(func) = node.child_by_field_name("function") {
                if func.kind() == "selector_expression" {
                    if let Some(operand) = func.child_by_field_name("operand") {
                        let is_import = operand.kind() == "identifier"
                            && imports.contains_key(go_text(operand, src))
                            && scope_lookup(scope, go_text(operand, src)).is_none();
                        if !is_import {
                            match go_receiver_binding(operand, src, scope, field_types) {
                                Some(binding) => {
                                    if binding == TypeBinding::Inferred
                                        && operand.kind() == "identifier"
                                    {
                                        let span = go_node_span(func);
                                        plan.inferred_recv.insert(
                                            (span.start, span.end()),
                                            (top, go_text(operand, src).to_string()),
                                        );
                                    }
                                    out.push((go_node_span(func), binding));
                                }
                                None => {
                                    let mut steps = Vec::new();
                                    if let Some(base) =
                                        go_chain_of(operand, src, scope, imports, &mut steps)
                                    {
                                        // A hopless `Var` base is the one-hop
                                        // leg's own job, and it already declined.
                                        let hopless = steps.is_empty()
                                            && matches!(base, GoChainBase::Var { .. });
                                        if steps.len() < GO_CHAIN_MAX_STEPS && !hopless {
                                            let span = go_node_span(func);
                                            plan.multihop.insert(
                                                (span.start, span.end()),
                                                (top, GoChain { base, steps }),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                go_walk_receivers(child, src, scope, imports, field_types, out, plan, top);
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                go_walk_receivers(child, src, scope, imports, field_types, out, plan, top);
            }
        }
    }
}

/// `switch alias := value.(type)`: inside a case naming exactly ONE type, the
/// alias HAS that type. A multi-type case or `default` leaves it untyped.
#[allow(clippy::too_many_arguments)]
fn go_walk_type_switch(
    node: tree_sitter::Node,
    src: &[u8],
    scope: &mut TypeScope,
    imports: &HashMap<String, String>,
    field_types: &HashMap<(String, String), DeclType>,
    out: &mut Vec<(Span, TypeBinding)>,
    plan: &mut GoBindPlan,
    top: (u32, u32),
) {
    scope.push(HashMap::new());
    let mut alias: Option<String> = None;
    if let Some(list) = node.child_by_field_name("alias") {
        let mut c = list.walk();
        alias = list
            .children(&mut c)
            .find(|n| n.kind() == "identifier")
            .map(|n| go_text(n, src).to_string());
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "type_case" {
            go_walk_receivers(child, src, scope, imports, field_types, out, plan, top);
            continue;
        }
        scope.push(HashMap::new());
        let mut tc = child.walk();
        let types: Vec<tree_sitter::Node> = child.children_by_field_name("type", &mut tc).collect();
        if let (Some(name), [ty]) = (alias.as_ref(), types.as_slice()) {
            if let Some(decl) = go_decl_type_of(*ty, src) {
                scope_insert(scope, name.clone(), TypeBinding::Decl(decl));
            }
        }
        let mut cc = child.walk();
        for stmt in child.children(&mut cc) {
            go_walk_receivers(stmt, src, scope, imports, field_types, out, plan, top);
        }
        scope.pop();
    }
    scope.pop();
}

/// Drive `go_walk_receivers` over every top-level function/method, appending
/// one `ReceiverBinding` per traceable call site to `sink.aux.receivers`, and
/// store the file's bind plan for the resolve phase (keyed by content).
fn go_collect_receivers(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    imports: &HashMap<String, String>,
    field_types: &HashMap<(String, String), DeclType>,
) {
    let mut plan = GoBindPlan::default();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if !matches!(child.kind(), "function_declaration" | "method_declaration") {
            continue;
        }
        let Some(body) = child.child_by_field_name("body") else {
            continue;
        };
        let top = {
            let span = def_span(child);
            (span.start, span.end())
        };
        let mut scope = go_seed_top_scope(child, src);
        let mut out = Vec::new();
        go_walk_receivers(
            body,
            src,
            &mut scope,
            imports,
            field_types,
            &mut out,
            &mut plan,
            top,
        );
        for (span, binding) in out {
            let outcome = match binding {
                TypeBinding::Decl(DeclType::Named(name)) => {
                    ReceiverOutcome::Named(strings.intern(&name))
                }
                TypeBinding::Decl(_) | TypeBinding::Chained(_) => continue,
                TypeBinding::Inferred => ReceiverOutcome::Inferred,
                TypeBinding::Ambiguous => ReceiverOutcome::Ambiguous,
            };
            sink.aux.receivers.push(ReceiverBinding {
                call_site: span,
                outcome,
            });
        }
    }
    go_bind_plan_store(
        crate::content_id_of(src),
        GoBindPlan {
            binds: plan.binds,
            inferred_recv: plan.inferred_recv,
            multihop: plan.multihop,
            fields: field_types.clone(),
        },
    );
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
                    go_parse_shared(src)
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

/// Go's `type` entity kinds. Only a type declaration can be a type reference's
/// target; a `Function`/`Method` entity shares the name index with it.
fn is_go_type_decl(kind: TypeEntityKind) -> bool {
    matches!(
        kind,
        TypeEntityKind::Struct | TypeEntityKind::Interface | TypeEntityKind::Alias
    )
}

/// A type reference's candidate sites: type declarations only. With no module
/// plane in hand (a hand-built cx) nothing can tell the kinds apart, so all pass.
fn type_decl_sites<'a>(
    sites: &'a [DefSite],
    modules: Option<&GoModuleIndex>,
    paths: Option<&PathIndex>,
) -> Vec<&'a DefSite> {
    let (Some(modules), Some(paths)) = (modules, paths) else {
        return sites.iter().collect();
    };
    sites
        .iter()
        .filter(|site| {
            paths
                .get(&site.blob)
                .is_some_and(|path| modules.is_type_decl(path, site.span))
        })
        .collect()
}

/// A `pkg.Name` ref resolves through the go module plane; a bare name tries
/// same-file then a unique corpus site, else None (text stays text). Every leg
/// sees type declarations only (`type_decl_sites`).
fn resolve_type_dst(
    types: &FamilyBundle<TypeF>,
    strings: &Strings,
    index: Option<&DefIndex>,
    modules: Option<&GoModuleIndex>,
    paths: Option<&PathIndex>,
    own_path: Option<&str>,
    name: &str,
) -> Option<(ContentId, Span)> {
    if let Some((pkg, bare)) = name.split_once('.') {
        let modules = modules?;
        let own_path = own_path?;
        let module = go_module_of(own_path)?;
        let import_path = modules.import_path_for(own_path, pkg)?;
        let dir = go_package_dir(&module, &import_path)?;
        return modules.resolve_type_in_dir(&dir, index?, paths?, bare);
    }
    let same_file = types.nodes.iter().find(|node| {
        is_go_type_decl(node.kind) && node.name.map_or(false, |id| strings.lookup(id) == name)
    });
    if let (Some(node), Some(index)) = (same_file, index) {
        return corpus_defs(index, name)
            .iter()
            .find(|site| site.span == node.span)
            .map(|site| (site.blob.clone(), site.span));
    }
    // Go's package scope: a bare name binds in the referring file's own
    // directory before any corpus-wide name match is allowed to guess.
    if let (Some(modules), Some(index), Some(paths), Some(dir)) = (
        modules,
        index,
        paths,
        own_path.and_then(|path| Path::new(path).parent()),
    ) {
        if let Some(hit) = modules.resolve_type_in_own_dir(dir, index, paths, name) {
            return Some(hit);
        }
    }
    let sites = index.map(|index| corpus_defs(index, name)).unwrap_or(&[]);
    match type_decl_sites(sites, modules, paths).as_slice() {
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
        let modules = cx.indexes.go_modules.get();
        let paths = cx.indexes.paths.get();
        let own_path = own_blob(cx, output)
            .zip(paths)
            .and_then(|(blob, paths)| paths.get(&blob));
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
                modules,
                paths,
                own_path,
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
// has no module caller. A site whose `callee_path` names an import takes the
// IMPORTED leg: go binds `pkg.F` in pkg, never in the file that writes it.
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

    // The `pkg.F` leg resolves through `go_modules::GoModuleIndex::resolve_in_dir`
    // now, the plane's own directory-scoped, exported-only lookup.
}

/// The one blob `sites` name, with the CallF facet's span preferred; two blobs
/// are an ambiguity this tier does not settle. `pub(crate)`: `go_modules.rs`'s
/// package-qualified leg reuses this join rather than re-deriving it.
pub(crate) fn unique_blob(sites: &[&DefSite]) -> Option<(ContentId, Span)> {
    let mut blobs: Vec<&ContentId> = Vec::new();
    for site in sites {
        if !blobs.contains(&&site.blob) {
            blobs.push(&site.blob);
        }
    }
    let [blob] = blobs.as_slice() else {
        return None;
    };
    let site = sites
        .iter()
        .find(|s| s.family == FamilyTag::Call)
        .unwrap_or(&sites[0]);
    Some(((*blob).clone(), site.span))
}

/// The go module owning a file: the nearest ancestor directory holding a
/// `go.mod`, with that file's `module` line. `pub(crate)`: `go_modules.rs`
/// reuses this rather than re-walking the filesystem with its own copy.
pub(crate) struct GoModule {
    root: PathBuf,
    module: String,
}

/// Walk up from `path` for the `go.mod` that names the module the file is in.
/// None: no ancestor has one, or the one found declares no module.
pub(crate) fn go_module_of(path: &str) -> Option<GoModule> {
    let mut dir = Path::new(path).parent()?;
    loop {
        if let Ok(text) = std::fs::read_to_string(dir.join("go.mod")) {
            let module = text
                .lines()
                .find_map(|line| line.trim().strip_prefix("module "))?;
            return Some(GoModule {
                root: dir.to_path_buf(),
                module: module.trim().to_string(),
            });
        }
        dir = dir.parent()?;
    }
}

/// The directory an import path names inside `module`. None = outside the
/// module (stdlib or third-party), which no corpus file declares.
pub(crate) fn go_package_dir(module: &GoModule, import_path: &str) -> Option<PathBuf> {
    if import_path == module.module {
        return Some(module.root.clone());
    }
    let rel = import_path
        .strip_prefix(&module.module)?
        .strip_prefix('/')?;
    Some(module.root.join(rel))
}

/// Directory equality over supplied paths: `./a/x.go` and `a/x.go` name one
/// directory, and no arm may resolve on the spelling difference.
pub(crate) fn same_dir(left: &Path, right: &Path) -> bool {
    let strip = |path: &Path| -> Vec<std::ffi::OsString> {
        path.components()
            .filter(|part| !matches!(part, Component::CurDir))
            .map(|part| part.as_os_str().to_os_string())
            .collect()
    };
    strip(left) == strip(right)
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
    let (def_doc_ix, def_range) = definition_of(index, doc_ix, occ)?;
    let def_doc = &index.documents[def_doc_ix];
    let (def_blob, def_content) = joined[def_doc_ix].as_ref()?;
    let ident = byte_range_cached(def_doc, def_content, def_range, def_doc.position_encoding)?;
    let (name, def_site) = containing_def_site(def_index, def_blob.clone(), ident)?;
    Some((def_blob.clone(), def_site.span, name))
}

// One file's method/interface facts, computed during the module plane's own
// pass over the shared parse and published for the resolve arms (the
// process-global `GO_FILE_FACTS` store below); `paths` names the real path.
#[derive(Default)]
pub(crate) struct GoFileFacts {
    owner_of: HashMap<(u32, u32), String>,
    methods_of: HashMap<String, BTreeSet<String>>,
    /// Names declared as interface types (method specs, never struct methods).
    ifaces: BTreeSet<String>,
    /// def_span -> the declared result types in order (pointer-stripped, type
    /// arguments cut). Feeds the one-hop return-type inference of `x := f()`.
    ret_of: HashMap<(u32, u32), Vec<String>>,
    /// def_spans whose result is a generic instantiation; a multi-hop chain
    /// stops there (a cut type-argument result is not a type this tier names).
    generic: std::collections::HashSet<(u32, u32)>,
    /// (struct, field) -> the field's declared type, this file's own struct
    /// declarations. A collection field keeps its shape: no receiver itself,
    /// but a `range` or an index over it names its element.
    fields: HashMap<(String, String), DeclType>,
    /// struct -> its EMBEDDED types as written, `*` and type arguments cut, a
    /// `pkg.T` embed keeping its qualifier. Go's method promotion walks these.
    embeds: HashMap<String, Vec<String>>,
    /// Import qualifier -> import path, so a `pkg.T` embed resolves through
    /// the DECLARING file's own imports rather than the resolving file's.
    imports: HashMap<String, String>,
    /// `type A = B` alias name -> the type it names, as written. Go makes A and
    /// B one type, so A's method set is B's.
    aliases: HashMap<String, String>,
}

/// The resolve arms' read side for per-file facts. The module plane publishes
/// every corpus file's facts BEFORE the resolve loop starts (single writer,
/// then read-only), so the hot path is a read-guard lookup with no parse. The
/// parse fallback covers library/test use where no module plane ran.
struct GoFileFactsStore {
    by_path: RwLock<HashMap<String, Arc<GoFileFacts>>>,
    by_blob: RwLock<HashMap<ContentId, Arc<GoFileFacts>>>,
}

static GO_FILE_FACTS: OnceLock<GoFileFactsStore> = OnceLock::new();

fn go_file_facts_store() -> &'static GoFileFactsStore {
    GO_FILE_FACTS.get_or_init(|| GoFileFactsStore {
        by_path: RwLock::new(HashMap::new()),
        by_blob: RwLock::new(HashMap::new()),
    })
}

impl GoFileFactsStore {
    fn get_path(&self, path: &str) -> Option<Arc<GoFileFacts>> {
        self.by_path
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(path)
            .cloned()
    }

    fn get_blob(&self, blob: &ContentId) -> Option<Arc<GoFileFacts>> {
        self.by_blob
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(blob)
            .cloned()
    }

    /// The module plane's publish step: single-threaded, before resolve.
    fn publish(&self, path: &str, blob: Option<&ContentId>, facts: Arc<GoFileFacts>) {
        self.by_path
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(path.to_string(), facts.clone());
        if let Some(blob) = blob {
            self.by_blob
                .write()
                .unwrap_or_else(|poison| poison.into_inner())
                .insert(blob.clone(), facts);
        }
    }
}

/// Publish one file's facts computed from the module plane's shared parse.
/// `blob` is `None` for a path outside the supplied corpus.
pub(crate) fn go_publish_file_facts(
    path: &str,
    blob: Option<&ContentId>,
    facts: Arc<GoFileFacts>,
) {
    go_file_facts_store().publish(path, blob, facts);
}

/// Facts for a resolve-side query, keyed by the file's content id: the
/// published module-plane facts first, the parse fallback second.
fn go_file_facts(blob: &ContentId, path: &str) -> Arc<GoFileFacts> {
    if let Some(published) = go_file_facts_store().get_blob(blob) {
        return published;
    }
    let facts = match go_file_facts_store().get_path(path) {
        Some(published) => published,
        None => Arc::new(go_parse_file_facts(path)),
    };
    go_file_facts_store()
        .by_blob
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .entry(blob.clone())
        .or_insert(facts.clone());
    facts
}

/// Path-keyed twin: files resolved by name (the `go_facts_of_path` callers)
/// before any blob is known for them.
fn go_facts_of_path(path: &str) -> Arc<GoFileFacts> {
    if let Some(published) = go_file_facts_store().get_path(path) {
        return published;
    }
    let facts = Arc::new(go_parse_file_facts(path));
    go_file_facts_store()
        .by_path
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .entry(path.to_string())
        .or_insert(facts.clone());
    facts
}

/// Is the def at `(path, span)` a METHOD (or an interface method spec)? A
/// free-function leg must never take one: `f()` cannot name `(r T) f()`.
pub(crate) fn go_is_method_def(path: &str, span: Span) -> bool {
    go_facts_of_path(path)
        .owner_of
        .contains_key(&(span.start, span.end()))
}

fn go_parse_file_facts(path: &str) -> GoFileFacts {
    let Ok(bytes) = std::fs::read(path) else {
        return GoFileFacts::default();
    };
    let Ok(src) = std::str::from_utf8(&bytes) else {
        return GoFileFacts::default();
    };
    match go_parse_shared(src) {
        Some(tree) => go_file_facts_of_source(&tree, src),
        None => GoFileFacts::default(),
    }
}

/// The file facts off an already-parsed tree, so the module plane's shared
/// parse serves the resolve arms without a second parse.
pub(crate) fn go_file_facts_of_source(
    tree: &tree_sitter::Tree,
    src: &str,
) -> GoFileFacts {
    let mut facts = GoFileFacts::default();
    let src = src.as_bytes();
    go_collect_file_facts(tree.root_node(), src, &mut facts);
    go_collect_file_imports(tree.root_node(), src, &mut facts.imports);
    facts.fields = go_field_types(tree.root_node(), src);
    facts
}

/// Every method declaration's (span -> owner) + (owner -> method names), and
/// every interface's method_elem the same way (owner = the interface name).
fn go_collect_file_facts(node: tree_sitter::Node, src: &[u8], facts: &mut GoFileFacts) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "method_declaration" => {
                if let (Some(name_node), Some(owner)) = (
                    child.child_by_field_name("name"),
                    go_receiver_type(child, src),
                ) {
                    let span = def_span(child);
                    facts
                        .owner_of
                        .insert((span.start, span.end()), owner.clone());
                    facts
                        .methods_of
                        .entry(owner)
                        .or_default()
                        .insert(go_text(name_node, src).to_string());
                    facts.insert_ret(child, src);
                }
            }
            "function_declaration" => {
                facts.insert_ret(child, src);
            }
            "type_declaration" => {
                let mut sc = child.walk();
                for spec in child.children(&mut sc) {
                    match spec.kind() {
                        "type_spec" => {
                            go_collect_interface_facts(spec, src, facts);
                            go_collect_embed_facts(spec, src, facts);
                        }
                        "type_alias" => go_collect_alias_facts(spec, src, facts),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        go_collect_file_facts(child, src, facts);
    }
}

impl GoFileFacts {
    /// The callable's ordered declared result types, keyed by its def span.
    /// A `*T` names T; type arguments are cut (`Wrapper[T]` -> `Wrapper`); a
    /// func literal or a type-parameter result never lands here.
    fn insert_ret(&mut self, decl: tree_sitter::Node, src: &[u8]) {
        let Some(result) = decl.child_by_field_name("result") else {
            return;
        };
        let span = def_span(decl);
        let mut rets = Vec::new();
        if result.kind() == "parameter_list" {
            let mut cursor = result.walk();
            for param in result.children(&mut cursor) {
                if let Some(ty) = param
                    .child_by_field_name("type")
                    .filter(|_| matches!(param.kind(), "parameter_declaration"))
                {
                    rets.push(go_named_type_text(ty, src));
                }
            }
        } else {
            rets.push(go_named_type_text(result, src));
        }
        rets.retain(|t| !t.is_empty());
        if rets.is_empty() {
            return;
        }
        if is_generic_ret(decl) {
            self.generic.insert((span.start, span.end()));
        }
        self.ret_of.insert((span.start, span.end()), rets);
    }
}

/// Whether the callable's declared result is (or contains) a generic
/// instantiation, `Wrapper[T]`.
fn is_generic_ret(decl: tree_sitter::Node) -> bool {
    let Some(result) = decl.child_by_field_name("result") else {
        return false;
    };
    let mut types: Vec<tree_sitter::Node> = Vec::new();
    if result.kind() == "parameter_list" {
        let mut cursor = result.walk();
        for param in result.children(&mut cursor) {
            if let Some(ty) = param.child_by_field_name("type") {
                types.push(ty);
            }
        }
    } else {
        types.push(result);
    }
    types.iter().any(|ty| {
        ty.kind() == "generic_type"
            || ty
                .children(&mut ty.walk())
                .any(|child| child.kind() == "generic_type")
    })
}

/// The declared type's name text: `*T` -> `T`, `pkg.T` kept whole, type
/// arguments cut, anything else (array, func, chan, interface literal) empty.
fn go_named_type_text(ty: tree_sitter::Node, src: &[u8]) -> String {
    let ty = if ty.kind() == "pointer_type" {
        match ty.named_child(0) {
            Some(inner) => inner,
            None => return String::new(),
        }
    } else {
        ty
    };
    match ty.kind() {
        "type_identifier" | "qualified_type" | "generic_type" => {
            let text = go_text(ty, src);
            match text.split_once('[') {
                Some((base, _)) => base.to_string(),
                None => text.to_string(),
            }
        }
        _ => String::new(),
    }
}

fn go_collect_interface_facts(spec: tree_sitter::Node, src: &[u8], facts: &mut GoFileFacts) {
    let Some(name_node) = spec.child_by_field_name("name") else {
        return;
    };
    let Some(iface) = spec
        .child_by_field_name("type")
        .filter(|t| t.kind() == "interface_type")
    else {
        return;
    };
    let iname = go_text(name_node, src).to_string();
    facts.ifaces.insert(iname.clone());
    let mut mc = iface.walk();
    for elem in iface
        .children(&mut mc)
        .filter(|n| n.kind() == "method_elem")
    {
        let Some(mname) = elem.child_by_field_name("name") else {
            continue;
        };
        let span = go_node_span(elem);
        facts
            .owner_of
            .insert((span.start, span.end()), iname.clone());
        facts
            .methods_of
            .entry(iname.clone())
            .or_default()
            .insert(go_text(mname, src).to_string());
        facts.insert_ret(elem, src);
    }
}

/// `type A = B`: tree-sitter-go spells an alias `type_alias`, never `type_spec`,
/// so the two never collide in one table.
fn go_collect_alias_facts(spec: tree_sitter::Node, src: &[u8], facts: &mut GoFileFacts) {
    let (Some(name_node), Some(ty)) = (
        spec.child_by_field_name("name"),
        spec.child_by_field_name("type"),
    ) else {
        return;
    };
    let target = go_named_type_text(ty, src);
    if target.is_empty() {
        return;
    }
    facts
        .aliases
        .insert(go_text(name_node, src).to_string(), target);
}

/// A struct's embedded fields: tree-sitter-go's `field_declaration` carries a
/// `type` and NO `field_identifier` for exactly those.
fn go_collect_embed_facts(spec: tree_sitter::Node, src: &[u8], facts: &mut GoFileFacts) {
    let Some(name_node) = spec.child_by_field_name("name") else {
        return;
    };
    let Some(struct_ty) = spec
        .child_by_field_name("type")
        .filter(|t| t.kind() == "struct_type")
    else {
        return;
    };
    let mut lc = struct_ty.walk();
    let Some(list) = struct_ty
        .children(&mut lc)
        .find(|n| n.kind() == "field_declaration_list")
    else {
        return;
    };
    let mut embeds = Vec::new();
    let mut fc = list.walk();
    for field in list
        .children(&mut fc)
        .filter(|n| n.kind() == "field_declaration")
    {
        let mut nc = field.walk();
        if field
            .children(&mut nc)
            .any(|n| n.kind() == "field_identifier")
        {
            continue;
        }
        let Some(ty) = field.child_by_field_name("type") else {
            continue;
        };
        let name = go_named_type_text(ty, src);
        if !name.is_empty() {
            embeds.push(name);
        }
    }
    if !embeds.is_empty() {
        facts
            .embeds
            .insert(go_text(name_node, src).to_string(), embeds);
    }
}

/// The `go_import_bindings` table off a dedicated parse: a plain spec binds
/// its path's last segment, `_` and `.` bind no qualifier.
fn go_collect_file_imports(node: tree_sitter::Node, src: &[u8], out: &mut HashMap<String, String>) {
    if node.kind() == "import_spec" {
        let path = path_of_import_spec(node, src);
        match leading_name(node) {
            Some(name_node) if name_node.kind() == "package_identifier" => {
                out.insert(go_text(name_node, src).to_string(), path);
            }
            Some(_) => {}
            None => {
                let tail = path.rsplit('/').next().unwrap_or(&path).to_string();
                out.insert(tail, path);
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        go_collect_file_imports(child, src, out);
    }
}

/// A bound result type from another package is stored qualified by the
/// caller's own import of that package (`Sub` -> `sub.Sub`), so the receiver
/// leg's directory lookup can find it. Same-package results stay bare.
fn go_qualify_bound_type(
    module: &Option<GoModule>,
    paths: Option<&PathIndex>,
    imports: &HashMap<String, String>,
    own: Option<&ContentId>,
    dst_blob: &ContentId,
    type_name: &str,
) -> String {
    let (Some(module), Some(paths)) = (module, paths) else {
        return type_name.to_string();
    };
    let (Some(dst_path), Some(own_path)) = (paths.get(dst_blob), own.and_then(|b| paths.get(b)))
    else {
        return type_name.to_string();
    };
    let (Some(dst_dir), Some(own_dir)) =
        (Path::new(dst_path).parent(), Path::new(own_path).parent())
    else {
        return type_name.to_string();
    };
    if same_dir(dst_dir, own_dir) {
        return type_name.to_string();
    }
    let qualifier = imports.iter().find_map(|(qualifier, import)| {
        go_package_dir(module, import)
            .filter(|dir| same_dir(dir, dst_dir))
            .map(|_| qualifier)
    });
    match qualifier {
        Some(q) => format!("{q}.{type_name}"),
        None => type_name.to_string(),
    }
}

/// A named type's identity: its DECLARING package directory plus its bare name.
/// `pkg.T` and the same type written bare in its own package are one key here.
type GoTypeId = (PathBuf, String);

/// A type name as written in the file `own` names, resolved to a `GoTypeId`
/// through that file's own import bindings.
fn go_type_id(
    module: &Option<GoModule>,
    paths: &PathIndex,
    own: Option<&ContentId>,
    imports: &HashMap<String, String>,
    type_name: &str,
) -> Option<GoTypeId> {
    match type_name.split_once('.') {
        Some((pkg, bare)) => Some((
            go_package_dir(module.as_ref()?, imports.get(pkg)?)?,
            bare.to_string(),
        )),
        None => Some((
            Path::new(paths.get(own?)?)
                .parent()
                .map(Path::to_path_buf)?,
            type_name.to_string(),
        )),
    }
}

/// The same for a type name read out of `path`'s OWN declarations: the
/// qualifier binds through `path`'s imports, never the resolving file's.
fn go_type_id_in_file(path: &str, type_name: &str) -> Option<GoTypeId> {
    match type_name.split_once('.') {
        Some((pkg, bare)) => {
            let facts = go_facts_of_path(path);
            let import = facts.imports.get(pkg)?;
            Some((
                go_package_dir(&go_module_of(path)?, import)?,
                bare.to_string(),
            ))
        }
        None => Some((
            Path::new(path).parent().map(Path::to_path_buf)?,
            type_name.to_string(),
        )),
    }
}

/// `pkg.M()` where a LOCAL named `pkg` shadows the import: phase 1 records a
/// receiver for exactly those, so a binding here means the local wins.
#[allow(clippy::too_many_arguments)]
fn go_shadowing_receiver_target(
    call: &FamilyBundle<CallF>,
    def_index: &DefIndex,
    paths: Option<&PathIndex>,
    module: &Option<GoModule>,
    own: Option<&ContentId>,
    imports: &HashMap<String, String>,
    output: &ExtractOutput,
    plan: Option<&GoBindPlan>,
    bound_types: &HashMap<(u32, u32), HashMap<String, String>>,
    site: &CallSite,
    callee: &str,
) -> Option<(ContentId, Span)> {
    let binding = call
        .aux
        .receivers
        .iter()
        .find(|r| r.call_site == site.span)?;
    let type_name = match &binding.outcome {
        ReceiverOutcome::Named(type_id) => output.strings.lookup(*type_id).to_string(),
        ReceiverOutcome::Inferred => plan?
            .inferred_recv
            .get(&(site.span.start, site.span.end()))
            .and_then(|(top, var)| bound_types.get(top)?.get(var))?
            .clone(),
        ReceiverOutcome::Ambiguous => return None,
    };
    go_receiver_target(def_index, paths, module, own, imports, &type_name, callee)
}

/// Leg 1: `callee` among `type_name`'s methods, same package first else the
/// package the `pkg.` qualifier names, and only then the promotion walk.
fn go_receiver_target(
    def_index: &DefIndex,
    paths: Option<&PathIndex>,
    module: &Option<GoModule>,
    own: Option<&ContentId>,
    imports: &HashMap<String, String>,
    type_name: &str,
    callee: &str,
) -> Option<(ContentId, Span)> {
    let paths = paths?;
    let ty = go_type_id(module, paths, own, imports, type_name)?;
    go_method_on_type(def_index, paths, &ty, callee)
}

/// Alias hops the method lookup takes before it stops.
const GO_ALIAS_DEPTH: usize = 4;

/// `callee` among the type's own methods, the ones it promotes, then the same
/// two on what `type A = B` names it (`owner_of` holds the WRITTEN receiver).
fn go_method_on_type(
    def_index: &DefIndex,
    paths: &PathIndex,
    ty: &GoTypeId,
    callee: &str,
) -> Option<(ContentId, Span)> {
    let mut cur = ty.clone();
    let mut seen: std::collections::HashSet<GoTypeId> =
        std::collections::HashSet::from([cur.clone()]);
    for _ in 0..=GO_ALIAS_DEPTH {
        if let Some(hit) = go_method_in_dir(def_index, paths, &cur.0, &cur.1, callee)
            .or_else(|| go_promoted_method(def_index, paths, &cur.0, &cur.1, callee))
        {
            return Some(hit);
        }
        let next = go_aliases_of_dir(&cur.0, paths).get(&cur.1)?.clone();
        if !seen.insert(next.clone()) {
            return None;
        }
        cur = next;
    }
    None
}

/// One directory's `type A = B` table, alias name -> the aliased type's id. A
/// `pkg.T` target resolves through the DECLARING file's imports, like an embed.
fn go_aliases_of_dir(dir: &Path, paths: &PathIndex) -> Arc<AliasesOfDir> {
    static CACHE: OnceLock<Mutex<AliasesCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (std::ptr::from_ref(paths) as usize, normalize_dir(dir));
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(hit) = guard.get(&key) {
        return hit.clone();
    }
    let mut out: AliasesOfDir = HashMap::new();
    for path in go_dir_index(paths).get(&key.1).into_iter().flatten() {
        for (alias, target) in &go_facts_of_path(path).aliases {
            if let Some(id) = go_type_id_in_file(path, target) {
                out.insert(alias.clone(), id);
            }
        }
    }
    let out = Arc::new(out);
    guard.insert(key, out.clone());
    out
}

/// Alias name -> the aliased type's id, one directory.
type AliasesOfDir = HashMap<String, GoTypeId>;

/// (resolve-run identity, normalized dir) -> that dir's alias table.
type AliasesCache = HashMap<(usize, PathBuf), Arc<AliasesOfDir>>;

/// `callee` among the methods declared on `(dir, type_name)`.
fn go_method_in_dir(
    def_index: &DefIndex,
    paths: &PathIndex,
    dir: &Path,
    type_name: &str,
    callee: &str,
) -> Option<(ContentId, Span)> {
    let matches: Vec<&DefSite> = corpus_defs(def_index, callee)
        .iter()
        .filter(|site| {
            site.family == FamilyTag::Call
                && paths.get(&site.blob).is_some_and(|p| {
                    Path::new(p)
                        .parent()
                        .is_some_and(|parent| same_dir(parent, dir))
                        && go_file_facts(&site.blob, p)
                            .owner_of
                            .get(&(site.span.start, site.span.end()))
                            .map(String::as_str)
                            == Some(type_name)
                })
        })
        .collect();
    // Exactly one, order-independent: two matches is a corpus ambiguity this
    // tier does not settle, never a coin flip on Vec insertion order.
    match matches.as_slice() {
        [site] => Some((site.blob.clone(), site.span)),
        _ => None,
    }
}

/// Embedded-field hops the promotion walk takes before it stops.
const GO_EMBED_DEPTH: usize = 4;

/// `callee` among the methods `(dir, type_name)` promotes through its embedded
/// fields. Go's rule: shallowest wins, a tie at one depth binds nothing.
fn go_promoted_method(
    def_index: &DefIndex,
    paths: &PathIndex,
    dir: &Path,
    type_name: &str,
    callee: &str,
) -> Option<(ContentId, Span)> {
    let start = (dir.to_path_buf(), type_name.to_string());
    let mut seen: std::collections::HashSet<(PathBuf, String)> =
        std::collections::HashSet::from([start.clone()]);
    let mut frontier = vec![start];
    for _ in 0..GO_EMBED_DEPTH {
        let mut next = Vec::new();
        for (owner_dir, owner) in &frontier {
            let embeds = go_embeds_of_dir(owner_dir, paths);
            for embed in embeds.get(owner).into_iter().flatten() {
                if seen.insert(embed.clone()) {
                    next.push(embed.clone());
                }
            }
        }
        if next.is_empty() {
            return None;
        }
        let hits: BTreeSet<(ContentId, Span)> = next
            .iter()
            .filter_map(|(embed_dir, embed)| {
                go_method_in_dir(def_index, paths, embed_dir, embed, callee)
            })
            .collect();
        let mut found = hits.into_iter();
        match (found.next(), found.next()) {
            (Some(hit), None) => return Some(hit),
            (Some(_), Some(_)) => return None,
            _ => {}
        }
        frontier = next;
    }
    None
}

/// `same_dir`'s component strip as an owned key, so `./a/x` and `a/x` name one
/// directory in a hash map rather than under a per-pair comparison.
fn normalize_dir(dir: &Path) -> PathBuf {
    dir.components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect()
}

/// The resolve universe's paths grouped by directory, ONE pass per run. A
/// per-(dir, name) scan of the whole path list is what kink 1 was.
fn go_dir_index(paths: &PathIndex) -> Arc<DirIndex> {
    static CACHE: OnceLock<Mutex<HashMap<usize, Arc<DirIndex>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = std::ptr::from_ref(paths) as usize;
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(hit) = guard.get(&key) {
        return hit.clone();
    }
    let mut index: DirIndex = HashMap::new();
    for path in paths.map.values() {
        if let Some(parent) = Path::new(path).parent() {
            index
                .entry(normalize_dir(parent))
                .or_default()
                .push(path.clone());
        }
    }
    let index = Arc::new(index);
    guard.insert(key, index.clone());
    index
}

/// One directory's structs -> their embedded types as (declaring dir, bare
/// name). A `pkg.T` embed resolves through the DECLARING file's imports.
fn go_embeds_of_dir(dir: &Path, paths: &PathIndex) -> Arc<EmbedsOfDir> {
    static CACHE: OnceLock<Mutex<EmbedsCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (std::ptr::from_ref(paths) as usize, normalize_dir(dir));
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(hit) = guard.get(&key) {
        return hit.clone();
    }
    let mut out: EmbedsOfDir = HashMap::new();
    for path in go_dir_index(paths).get(&key.1).into_iter().flatten() {
        let facts = go_facts_of_path(path);
        for (struct_name, embeds) in &facts.embeds {
            let resolved: Vec<(PathBuf, String)> = embeds
                .iter()
                .filter_map(|embed| match embed.split_once('.') {
                    Some((qualifier, bare)) => {
                        let import = facts.imports.get(qualifier)?;
                        let module = go_module_of(path)?;
                        Some((go_package_dir(&module, import)?, bare.to_string()))
                    }
                    None => Some((dir.to_path_buf(), embed.clone())),
                })
                .collect();
            out.entry(struct_name.clone()).or_default().extend(resolved);
        }
    }
    let out = Arc::new(out);
    guard.insert(key, out.clone());
    out
}

/// Normalized directory -> the resolve universe's `.go` paths under it.
type DirIndex = HashMap<PathBuf, Vec<String>>;

/// Struct name -> its embedded types as (declaring dir, bare name).
type EmbedsOfDir = HashMap<String, Vec<(PathBuf, String)>>;

/// (resolve-run identity, normalized dir) -> that dir's embed table.
type EmbedsCache = HashMap<(usize, PathBuf), Arc<EmbedsOfDir>>;

/// The multi-hop replay: type the chain's operand left to right as `GoTypeId`s,
/// then bind `callee` on it. Third result: the fan-out gate's receiver name.
#[allow(clippy::too_many_arguments)]
fn go_chain_receiver_target(
    def_index: &DefIndex,
    paths: Option<&PathIndex>,
    module: &Option<GoModule>,
    own: Option<&ContentId>,
    imports: &HashMap<String, String>,
    modules: Option<&GoModuleIndex>,
    plan: &GoBindPlan,
    bound_types: &HashMap<(u32, u32), HashMap<String, String>>,
    site_span: (u32, u32),
    callee: &str,
) -> Option<(ContentId, Span, String)> {
    let paths = paths?;
    let (top, chain) = plan.multihop.get(&site_span)?;
    // `bool`: the type is a slice/array/map OF that element, so only an `Elem`
    // hop may follow it.
    let mut ty: Option<(GoTypeId, bool)> = match &chain.base {
        GoChainBase::Var { name, decl } => {
            let written = match decl {
                Some(t) => t.clone(),
                None => bound_types.get(top)?.get(name)?.clone(),
            };
            go_type_id(module, paths, own, imports, &written).map(|id| (id, false))
        }
        GoChainBase::Import {
            callee: base_callee,
            path,
        } => {
            let (blob, span) = match (module, modules) {
                (Some(module), Some(modules)) => go_package_dir(module, path)
                    .and_then(|dir| modules.resolve_in_dir(&dir, def_index, paths, base_callee))?,
                _ => return None,
            };
            go_ret_type_id(&blob, span, paths).map(|id| (id, false))
        }
    };
    for step in &chain.steps {
        let (cur, collection) = ty?;
        if is_noise_go(&cur.1) {
            return None;
        }
        ty = match step {
            GoChainStep::Elem if collection => Some((cur, false)),
            _ if collection => return None,
            GoChainStep::Elem => return None,
            GoChainStep::Field(field) => go_field_type_of(&cur, field, paths),
            GoChainStep::Call(method) => {
                let (blob, span) = go_method_on_type(def_index, paths, &cur, method)?;
                go_ret_type_id(&blob, span, paths).map(|id| (id, false))
            }
        };
    }
    let (ty, collection) = ty?;
    if collection || is_noise_go(&ty.1) {
        return None;
    }
    let (blob, span) = go_method_on_type(def_index, paths, &ty, callee)?;
    Some((blob, span, ty.1))
}

/// A def's declared first result as a `GoTypeId`, read through the DECLARING
/// file's imports. None when the def declares none or its result is generic.
fn go_ret_type_id(blob: &ContentId, span: Span, paths: &PathIndex) -> Option<GoTypeId> {
    let path = paths.get(blob)?;
    let written = ret_first_of(blob, span, paths)?;
    go_type_id_in_file(path, &written)
}

/// A def's declared first result type from its file facts; None when the def
/// declares none or its result is generic (a chain stops at both).
fn ret_first_of(blob: &ContentId, span: Span, paths: &PathIndex) -> Option<String> {
    let path = paths.get(blob)?;
    let facts = go_file_facts(blob, path);
    let key = (span.start, span.end());
    if facts.generic.contains(&key) {
        return None;
    }
    facts.ret_of.get(&key)?.first().cloned()
}

/// `ty`'s `field`, as (the field type's id, is it a collection OF that type).
/// Reads the whole DECLARING package, so a struct split across files answers.
fn go_field_type_of(ty: &GoTypeId, field: &str, paths: &PathIndex) -> Option<(GoTypeId, bool)> {
    go_fields_of_dir(&ty.0, paths)
        .get(&(ty.1.clone(), field.to_string()))
        .cloned()
}

/// One directory's (struct, field) -> the field type's id, ONE pass per run.
fn go_fields_of_dir(dir: &Path, paths: &PathIndex) -> Arc<FieldsOfDir> {
    static CACHE: OnceLock<Mutex<FieldsCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (std::ptr::from_ref(paths) as usize, normalize_dir(dir));
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(hit) = guard.get(&key) {
        return hit.clone();
    }
    let mut out: FieldsOfDir = HashMap::new();
    for path in go_dir_index(paths).get(&key.1).into_iter().flatten() {
        for (owner_field, decl) in &go_facts_of_path(path).fields {
            let (written, collection) = match decl {
                DeclType::Named(name) => (name, false),
                DeclType::Indexable(name) => (name, true),
                DeclType::Streamed(name) => (name, true),
            };
            if let Some(id) = go_type_id_in_file(path, written) {
                out.insert(owner_field.clone(), (id, collection));
            }
        }
    }
    let out = Arc::new(out);
    guard.insert(key, out.clone());
    out
}

/// (struct, field) -> (the field type's id, is it a collection), one directory.
type FieldsOfDir = HashMap<(String, String), (GoTypeId, bool)>;

/// (resolve-run identity, normalized dir) -> that dir's field table.
type FieldsCache = HashMap<(usize, PathBuf), Arc<FieldsOfDir>>;

impl Resolve<CallF> for GoSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        // One join per FILE: every leg below asks which blob this output is,
        // and the go.mod walk is per file, never per call site.
        let own = own_blob(cx, output);
        let paths = cx.indexes.paths.get();
        let own_path = own
            .as_ref()
            .zip(paths)
            .and_then(|(blob, paths)| paths.get(blob));
        let module = own_path.and_then(go_module_of);
        let modules = cx.indexes.go_modules.get();
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
                let blob = own.clone()?;
                let doc_ix = joined
                    .iter()
                    .position(|j| j.as_ref().map_or(false, |(b, _)| *b == blob))?;
                Some((index, joined, doc_ix))
            });
        let imports = go_import_bindings(call, &output.strings);
        // The one-hop return-type inference: phase 1 recorded every `x := f()`
        // bind site and every receiver site whose operand it bound. Resolution
        // runs in SOURCE ORDER (a chain `b := a.M()` needs `a` bound first), so
        // the sites are processed sorted and the edges emitted in file order.
        let plan = own.as_ref().and_then(|blob| go_bind_plan_of(blob));
        let sites = &call.aux.sites;
        let mut order: Vec<usize> = (0..sites.len()).collect();
        order.sort_by_key(|&ix| (sites[ix].span.start, sites[ix].span.end()));
        let mut bound_types: HashMap<(u32, u32), HashMap<String, String>> = HashMap::new();
        // Per-site fan-out source: the interface a Named receiver resolved
        // through, when that receiver type is an interface.
        let mut iface_recv: Vec<Option<String>> = vec![None; sites.len()];
        let mut results: Vec<Option<(NodeRef, ContentId, Span, CallEdgeKind)>> =
            vec![None; sites.len()];
        for ix in order {
            let site = &sites[ix];
            // The caller is the innermost covering CallF def (the 4a
            // caller-binding discipline); a package-level site has no caller
            // node and emits no row.
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let receiver = if site.callee_path.is_none() {
                call.aux
                    .receivers
                    .iter()
                    .find(|r| r.call_site == site.span)
                    .map(|r| &r.outcome)
            } else {
                None
            };
            let name_t: Option<(ContentId, Span, CallEdgeKind)> = match receiver {
                Some(ReceiverOutcome::Named(type_id)) => {
                    let type_name = output.strings.lookup(*type_id);
                    let target = go_receiver_target(
                        def_index,
                        paths,
                        &module,
                        own.as_ref(),
                        &imports,
                        type_name,
                        callee,
                    );
                    if target.is_some() {
                        let bare = type_name.split('.').next_back().unwrap_or(type_name);
                        if go_iface_fanout(def_index, paths, bare).is_iface {
                            iface_recv[ix] = Some(bare.to_string());
                        }
                    }
                    target.map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                }
                Some(ReceiverOutcome::Inferred) => {
                    let bound = plan
                        .as_ref()
                        .and_then(|plan| {
                            plan.inferred_recv.get(&(site.span.start, site.span.end()))
                        })
                        .and_then(|(top, var)| {
                            bound_types.get(top).and_then(|names| names.get(var))
                        });
                    match bound {
                        Some(type_name) => go_receiver_target(
                            def_index,
                            paths,
                            &module,
                            own.as_ref(),
                            &imports,
                            type_name,
                            callee,
                        )
                        .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve)),
                        None => None,
                    }
                }
                Some(ReceiverOutcome::Ambiguous) => None,
                None => match site.callee_path.map(|id| output.strings.lookup(id)) {
                    // A local shadowing the package name wins; the directory
                    // leg is exported-only, corpus-wide name match last.
                    Some(import) => go_shadowing_receiver_target(
                        call,
                        def_index,
                        paths,
                        &module,
                        own.as_ref(),
                        &imports,
                        output,
                        plan.as_deref(),
                        &bound_types,
                        site,
                        callee,
                    )
                    .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                    .or_else(|| match (&module, paths, modules) {
                        (Some(module), Some(paths), Some(modules)) => {
                            go_package_dir(module, import).and_then(|dir| {
                                modules
                                    .resolve_in_dir(&dir, def_index, paths, callee)
                                    .map(|(blob, span)| (blob, span, CallEdgeKind::ImportResolve))
                                    .or_else(|| {
                                        if !is_exported(callee) {
                                            return None;
                                        }
                                        GoSource::call_name_match(output, def_index, callee).map(
                                            |(blob, span)| (blob, span, CallEdgeKind::NameResolve),
                                        )
                                    })
                            })
                        }
                        _ => None,
                    }),
                    None => {
                        // The multi-hop chain leg: replay the operand's hops
                        // left to right and bind the final `.c()` the way a
                        // one-hop receiver binds. Only when the chain gives
                        // nothing does the bare name-match leg run.
                        let chained = plan.as_ref().and_then(|plan| {
                            go_chain_receiver_target(
                                def_index,
                                paths,
                                &module,
                                own.as_ref(),
                                &imports,
                                modules,
                                plan,
                                &bound_types,
                                (site.span.start, site.span.end()),
                                callee,
                            )
                        });
                        if let Some((blob, span, ty)) = chained {
                            let bare = ty.rsplit('.').next_back().unwrap_or(&ty);
                            if go_iface_fanout(def_index, paths, bare).is_iface {
                                iface_recv[ix] = Some(bare.to_string());
                            }
                            Some((blob, span, CallEdgeKind::NameResolve))
                        } else {
                            // Go's package block first: a same-package func
                            // shadows every corpus-wide name guess.
                            modules
                                .zip(own_path)
                                .and_then(|(modules, path)| {
                                    let dir = Path::new(path).parent()?;
                                    modules.resolve_call_in_own_dir(dir, def_index, paths?, callee)
                                })
                                .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                                .or_else(|| {
                                    GoSource::call_name_match(output, def_index, callee)
                                        .map(|(blob, span)| (blob, span, CallEdgeKind::NameResolve))
                                })
                                .or_else(|| {
                                    modules.zip(own_path).and_then(|(modules, path)| {
                                        modules
                                            .resolve_dot_imported(path, def_index, paths?, callee)
                                            .map(|(blob, span)| {
                                                (blob, span, CallEdgeKind::ImportResolve)
                                            })
                                    })
                                })
                        }
                    }
                },
            };
            let scip_t = scip.as_ref().and_then(|(index, joined, doc_ix)| {
                scip_call_target(index, joined, *doc_ix, site, callee, def_index)
            });
            // Agreement is judged at (blob, name): the name-match binds the
            // call FACET (the callable def) while scip can name the type facet
            // (a conversion `Mode(0)`'s type) — one definition, two facet
            // coordinates (the ORACLE entry's "the models differ by
            // construction").
            let final_t = match (name_t, scip_t) {
                (Some((blob, span, _)), Some(s)) if blob == s.0 && callee == s.2 => {
                    Some(((blob, span), CallEdgeKind::NameResolve))
                }
                (_, Some(s)) => Some(((s.0, s.1), CallEdgeKind::ScipOverride)),
                (Some((blob, span, kind)), None) => Some(((blob, span), kind)),
                (None, None) => None,
            };
            if let (Some(plan), Some(((dst_blob, dst_span), _))) = (plan.as_ref(), final_t.as_ref())
            {
                if let Some((top, names)) = plan.binds.get(&site.span.start) {
                    let rets = paths
                        .and_then(|p| p.get(dst_blob))
                        .map(|path| {
                            go_file_facts(dst_blob, path)
                                .ret_of
                                .get(&(dst_span.start, dst_span.end()))
                                .cloned()
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    // A conversion `T(x)` parses as a call; a target that is a
                    // TYPE decl with no result list is what names it one.
                    let converted = rets.is_empty()
                        && paths
                            .zip(modules)
                            .and_then(|(paths, modules)| {
                                let path = paths.get(dst_blob)?;
                                Some(modules.is_type_decl(path, *dst_span))
                            })
                            .unwrap_or(false);
                    let rets = if converted {
                        vec![callee.to_string()]
                    } else {
                        rets
                    };
                    let entry = bound_types.entry(*top).or_default();
                    for (slot, name) in names.iter().enumerate() {
                        if let Some(t) = rets.get(slot) {
                            let t = go_qualify_bound_type(
                                &module,
                                paths,
                                &imports,
                                own.as_ref(),
                                dst_blob,
                                t,
                            );
                            entry.insert(name.clone(), t);
                        }
                    }
                }
            }
            results[ix] = final_t.map(|((blob, span), kind)| (caller, blob, span, kind));
        }
        // The closure-caller mirror: a caller whose def is a Lambda
        // (`closure@<n>`) gets ONE extra edge onto the innermost NAMED def
        // covering the site (the enclosing fn/method), same shape and kind as
        // the rust arm (52_rust_crawl_kinks kink 3). Package-level func
        // literals mint no def, so their sites have no caller and no mirror.
        // Sorted once per file: the mirror lookup runs per closure-caller
        // site, and a per-site scan of the def table is the shape kink 1 was.
        let named = go_named_def_spans(call);
        let mut edges = Vec::new();
        for (ix, (site, result)) in sites.iter().zip(results).enumerate() {
            let Some((caller, dst_blob, dst_span, kind)) = result else {
                continue;
            };
            if call.node(caller).name.is_none() {
                if let Some(enclosing) = go_enclosing_named_def(&named, site.span) {
                    edges.push(
                        ProjectEdge::new(
                            enclosing,
                            dst_blob.clone(),
                            dst_span,
                            CallEdgeKind::NameResolve,
                        )
                        .with_call_site(site.span),
                    );
                }
            }
            edges
                .push(ProjectEdge::new(caller, dst_blob, dst_span, kind).with_call_site(site.span));
            let Some(iface) = &iface_recv[ix] else {
                continue;
            };
            let fanout = go_iface_fanout(def_index, paths, iface);
            if fanout.impls.len() > GO_FANOUT_CAP {
                fanout_cap_registry()
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .entry(corpus_key(def_index))
                    .or_default()
                    .insert((site.span.start, site.span.end()), fanout.impls.len());
                continue;
            }
            let callee = output.strings.lookup(site.callee);
            for methods in &fanout.impls {
                if let Some((blob, span)) = methods.get(callee) {
                    edges.push(
                        ProjectEdge::new(caller, blob.clone(), *span, CallEdgeKind::Implements)
                            .with_call_site(site.span),
                    );
                }
            }
        }
        let interfaces: BTreeSet<NameId> = output
            .types
            .as_ref()
            .map(|types| {
                types
                    .nodes
                    .iter()
                    .filter(|n| n.kind == TypeEntityKind::Interface)
                    .filter_map(|n| n.name)
                    .collect()
            })
            .unwrap_or_default();
        let mut interface_specs: HashMap<String, Vec<(NodeRef, String)>> = HashMap::new();
        for (ix, node) in call.nodes.iter().enumerate() {
            if node.kind != CallKind::Method {
                continue;
            }
            let Some(owner) = call.aux.method_owners.iter().find(|o| o.span == node.span) else {
                continue;
            };
            let Some(self_type) = owner.self_type.filter(|t| interfaces.contains(t)) else {
                continue;
            };
            let Some(name_id) = node.name else { continue };
            interface_specs
                .entry(output.strings.lookup(self_type).to_string())
                .or_default()
                .push((
                    NodeRef(ix as u32),
                    output.strings.lookup(name_id).to_string(),
                ));
        }
        for (iface_name, specs) in &interface_specs {
            edges.extend(go_interface_implements(def_index, paths, iface_name, specs));
        }
        edges
    }
}

/// Leg 2: for every named type whose method set covers ALL of `specs`, one
/// `Implements` edge per spec. One pass per interface, never per call site.
/// Every NAMED CallF def as (span, ref), sorted by (start, end) for the
/// `go_enclosing_named_def` binary search.
fn go_named_def_spans(defs: &FamilyBundle<CallF>) -> Vec<(Span, NodeRef)> {
    let mut sorted: Vec<(Span, NodeRef)> = defs
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.name.is_some())
        .map(|(ix, node)| (node.span, NodeRef(ix as u32)))
        .collect();
    sorted.sort_by_key(|(span, _)| (span.start, span.end()));
    sorted
}

/// The innermost NAMED def covering `site`. `covering_def` takes the innermost
/// def of any kind, which is the closure wherever one is in the way.
fn go_enclosing_named_def(sorted: &[(Span, NodeRef)], site: Span) -> Option<NodeRef> {
    let cut = sorted.partition_point(|(span, _)| span.start <= site.start);
    let mut best: Option<(Span, NodeRef)> = None;
    for &(span, r) in &sorted[..cut] {
        if site.end() <= span.end()
            && best.map_or(true, |(b, _)| span.end() - span.start < b.end() - b.start)
        {
            best = Some((span, r));
        }
    }
    best.map(|(_, r)| r)
}

fn go_interface_implements(
    def_index: &DefIndex,
    paths: Option<&PathIndex>,
    iface_name: &str,
    specs: &[(NodeRef, String)],
) -> Vec<ProjectEdge<CallF>> {
    let methods: BTreeSet<String> = specs.iter().map(|(_, name)| name.clone()).collect();
    go_iface_candidate_maps(def_index, paths, iface_name, &methods)
        .into_iter()
        .flat_map(|methods| {
            specs.iter().filter_map(move |(node_ref, spec_name)| {
                methods.get(spec_name).map(|(blob, span)| {
                    ProjectEdge::new(*node_ref, blob.clone(), *span, CallEdgeKind::Implements)
                })
            })
        })
        .collect()
}

/// A corpus identity for the resolve-phase static caches: the `DefIndex`'s
/// address, stable for the resolve run that owns it.
fn corpus_key(def_index: &DefIndex) -> usize {
    std::ptr::from_ref(def_index) as usize
}

/// The fan-out-capped site spans per corpus (def_index address): site span ->
/// implementer count. `resolve` fills it; `call_drops` emits one
/// `fanout_cap` row per entry.
fn fanout_cap_registry() -> &'static Mutex<HashMap<usize, HashMap<(u32, u32), usize>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<usize, HashMap<(u32, u32), usize>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// One implementer of an interface: every required method -> its def site.
type IfaceImplementers = Vec<HashMap<String, (ContentId, Span)>>;

/// The corpus-wide fan-out facts for one interface NAME: whether a type of
/// this name is declared as an interface, and the implementer set. Keyed by
/// name (same convention as `go_interface_implements`); two packages naming a
/// type the same merge into one candidate set, never a conflation inside a
/// single (owner, dir).
struct IfaceFanout {
    is_iface: bool,
    impls: IfaceImplementers,
}

/// Fan-out cap: an interface with more implementers than this emits the
/// `I.M` spec edge only plus one `unresolved` row reason `fanout_cap`.
const GO_FANOUT_CAP: usize = 64;

/// Per-corpus cache, keyed by (def_index address, interface name): the
/// implementer set is built once per corpus, never per call site (the same
/// discipline as `plan_cache`; the corpus is identified by its `DefIndex`,
/// whose address is stable for the resolve run that owns it).
fn iface_fanout_cache() -> &'static Mutex<HashMap<(usize, String), Arc<IfaceFanout>>> {
    static CACHE: OnceLock<Mutex<HashMap<(usize, String), Arc<IfaceFanout>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn go_iface_fanout(
    def_index: &DefIndex,
    paths: Option<&PathIndex>,
    iface: &str,
) -> Arc<IfaceFanout> {
    let key = (corpus_key(def_index), iface.to_string());
    let cache = iface_fanout_cache();
    let mut guard = cache.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(hit) = guard.get(&key) {
        return hit.clone();
    }
    // The interface's required method set comes from every corpus TypeF site
    // of this name that facts say IS an interface (a struct with methods can
    // share the name; its method set is not a spec).
    let mut is_iface = false;
    let mut methods: BTreeSet<String> = BTreeSet::new();
    if let Some(paths) = paths {
        for site in corpus_defs(def_index, iface) {
            if site.family != FamilyTag::Type {
                continue;
            }
            let Some(path) = paths.get(&site.blob) else {
                continue;
            };
            let facts = go_file_facts(&site.blob, path);
            if !facts.ifaces.contains(iface) {
                continue;
            }
            is_iface = true;
            if let Some(names) = facts.methods_of.get(iface) {
                methods.extend(names.iter().cloned());
            }
        }
    }
    let fanout = Arc::new(IfaceFanout {
        is_iface,
        impls: go_iface_candidate_maps(def_index, paths, iface, &methods),
    });
    guard.insert(key, fanout.clone());
    fanout
}

/// Every named type whose method set covers ALL of `methods`, as one map per
/// implementer. One pass per interface, never per call site.
fn go_iface_candidate_maps(
    def_index: &DefIndex,
    paths: Option<&PathIndex>,
    iface_name: &str,
    methods: &BTreeSet<String>,
) -> IfaceImplementers {
    let Some(paths) = paths else {
        return Vec::new();
    };
    // Keyed by (owner name, declaring dir): two packages can name a type the
    // same, and that must stay two candidates, never one conflated identity.
    let mut candidates: BTreeMap<(String, String), HashMap<&str, (ContentId, Span)>> =
        BTreeMap::new();
    for method in methods {
        for site in corpus_defs(def_index, method) {
            if site.family != FamilyTag::Call {
                continue;
            }
            let Some(path) = paths.get(&site.blob) else {
                continue;
            };
            let Some(dir) = Path::new(path).parent() else {
                continue;
            };
            let facts = go_file_facts(&site.blob, path);
            let Some(owner) = facts.owner_of.get(&(site.span.start, site.span.end())) else {
                continue;
            };
            if owner == iface_name {
                continue;
            }
            candidates
                .entry((owner.clone(), dir.to_string_lossy().into_owned()))
                .or_default()
                .insert(method.as_str(), (site.blob.clone(), site.span));
        }
    }
    candidates
        .into_values()
        .filter(|set| methods.iter().all(|name| set.contains_key(name.as_str())))
        .map(|set| {
            set.into_iter()
                .map(|(name, site)| (name.to_string(), site))
                .collect()
        })
        .collect()
}

// call_drops: a bare callee matching the table below drops reason `builtin`; a
// local def sharing the name already won NameResolve, so it is in `bound`.

/// Go's predeclared function identifiers (functions only).
const GO_BUILTIN_FUNCS: &[&str] = &[
    "append", "cap", "clear", "close", "complex", "copy", "delete", "imag", "len", "make", "max",
    "min", "new", "panic", "print", "println", "real", "recover",
];

/// Predeclared TYPE identifiers used as a conversion call (`int32(x)`).
const GO_BUILTIN_TYPES: &[&str] = &[
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
    "float32",
    "float64",
    "complex64",
    "complex128",
    "bool",
    "string",
    "byte",
    "rune",
    "error",
    "any",
];

fn is_go_builtin_call(name: &str) -> bool {
    GO_BUILTIN_FUNCS.contains(&name) || GO_BUILTIN_TYPES.contains(&name)
}

/// One `unresolved` row per dropped predeclared-callee site, plus one per
/// import spec outside the corpus (reason `external`, import-spec-level).
pub fn call_drops(
    output: &ExtractOutput,
    cx: &ProjectCx,
    edges: &[ProjectEdge<CallF>],
) -> Vec<ResolveDrop> {
    let Some(call) = &output.call else {
        return Vec::new();
    };
    let bound: BTreeSet<(u32, u32)> = edges
        .iter()
        .filter_map(|edge| edge.call_site.map(|span| (span.start, span.end())))
        .collect();
    let mut drops: Vec<ResolveDrop> = call
        .aux
        .sites
        .iter()
        .filter(|site| !bound.contains(&(site.span.start, site.span.end())))
        .filter_map(|site| {
            if site.callee_path.is_some() {
                return None;
            }
            let callee = output.strings.lookup(site.callee);
            let receiver = call
                .aux
                .receivers
                .iter()
                .find(|r| r.call_site == site.span)
                .map(|r| &r.outcome);
            let reason = match receiver {
                Some(ReceiverOutcome::Inferred) => UnresolvedReason::Inferred,
                Some(ReceiverOutcome::Ambiguous) => UnresolvedReason::Ambiguous,
                _ if is_go_builtin_call(callee) => UnresolvedReason::Builtin,
                _ => return None,
            };
            Some(ResolveDrop {
                span: site.span,
                reason,
                detail: callee.to_string(),
            })
        })
        .collect();
    // The fan-out cap rows are minted by `resolve` (which knows the
    // implementer counts); the registry carries them across phases.
    if let Some(def_index) = cx.indexes.def_index.get() {
        let capped = fanout_cap_registry()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .get(&corpus_key(def_index))
            .cloned()
            .unwrap_or_default();
        for site in &call.aux.sites {
            if let Some(count) = capped.get(&(site.span.start, site.span.end())) {
                drops.push(ResolveDrop {
                    span: site.span,
                    reason: UnresolvedReason::FanoutCap,
                    detail: format!("{count} implementers"),
                });
            }
        }
    }
    if let Some(modules) = cx.indexes.go_modules.get() {
        let own_path = own_blob(cx, output)
            .zip(cx.indexes.paths.get())
            .and_then(|(blob, paths)| paths.get(&blob));
        if let Some(path) = own_path {
            drops.extend(
                modules
                    .external_drops(path)
                    .into_iter()
                    .map(|(span, import_path)| ResolveDrop {
                        span,
                        reason: UnresolvedReason::External,
                        detail: import_path,
                    }),
            );
        }
    }
    drops
}
