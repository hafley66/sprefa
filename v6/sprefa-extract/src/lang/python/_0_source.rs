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

use std::collections::BTreeSet;

use crate::family::{CallF, CallKind, CallSite, CstF, SigSlot, TypeEntityKind, TypeF, TypeSig};
use crate::lang::{AstGrepParser, CstProjector};
use crate::rows::{FamilyBundle, Node};
use crate::seams::{Parser, Project};
use crate::shape::{Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::trace;

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

/// Unwrap `decorated_definition` to its inner `class`/`function_definition`;
/// any other node passes through. Port of v5 `py_unwrap_decorated`.
fn py_unwrap_decorated(node: tree_sitter::Node) -> tree_sitter::Node {
    if node.kind() == "decorated_definition" {
        node.child_by_field_name("definition").unwrap_or(node)
    } else {
        node
    }
}

// ── TypeF: entity nodes + arrow-type sigs (commit B) ───────────────────────

fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    walk_py_entities(root, src, strings, sink, None);
}

/// One entity per class/function/method; `class_owner` is the enclosing class's
/// name (method vs function) and resets to None inside a function body.
fn walk_py_entities(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
    class_owner: Option<&str>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                if let Some(name_node) = target.child_by_field_name("name") {
                    let name = py_text(name_node, src).to_string();
                    let span = node_span(target);
                    push_entity(sink, strings, span, &name, TypeEntityKind::Class);
                    if let Some(body) = target.child_by_field_name("body") {
                        walk_py_entities(body, src, strings, sink, Some(&name));
                    }
                }
            }
            "function_definition" => {
                if let Some(name_node) = target.child_by_field_name("name") {
                    let name = py_text(name_node, src).to_string();
                    let kind = if class_owner.is_some() {
                        TypeEntityKind::Method
                    } else {
                        TypeEntityKind::Function
                    };
                    let span = node_span(target);
                    push_entity(sink, strings, span, &name, kind);
                    fn_sigs(sink, strings, span, target, src);
                    if let Some(body) = target.child_by_field_name("body") {
                        walk_py_entities(body, src, strings, sink, None);
                    }
                }
            }
            _ => walk_py_entities(target, src, strings, sink, class_owner),
        }
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

/// Param type-refs (positional, `self`/`cls` receiver skipped) + return refs
/// (pos 0), one TypeSig per named ref. Port of v5 `py_fn_type`.
fn fn_sigs(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    node: tree_sitter::Node,
    src: &[u8],
) {
    let tparams = py_collect_type_params(node, src, "type_parameters");
    let mut pos: u32 = 0;
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cursor = plist.walk();
        let mut first = true;
        for param in plist.named_children(&mut cursor) {
            if matches!(param.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, type_node) = py_param_name_and_type(param, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            let refs = type_node
                .map(|ty| py_type_refs_collect(ty, src, &tparams))
                .unwrap_or_default();
            for name in refs {
                push_sig(sink, strings, owner, SigSlot::Param, pos, &name);
            }
            pos += 1;
        }
    }
    if let Some(ret) = node.child_by_field_name("return_type") {
        for name in py_type_refs_collect(ret, src, &tparams) {
            push_sig(sink, strings, owner, SigSlot::Ret, 0, &name);
        }
    }
}

fn push_sig(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    slot: SigSlot,
    pos: u32,
    ty: &str,
) {
    sink.aux.sigs.push(TypeSig {
        owner,
        slot,
        pos,
        ty: strings.intern(ty),
    });
}

/// (name, type-annotation node) for one `parameter` subtype; only
/// `typed_parameter`/`typed_default_parameter` carry a type. Port of v5.
fn py_param_name_and_type<'t>(
    param: tree_sitter::Node<'t>,
    src: &[u8],
) -> (Option<String>, Option<tree_sitter::Node<'t>>) {
    match param.kind() {
        "identifier" => (Some(py_text(param, src).to_string()), None),
        "typed_parameter" => {
            let mut cursor = param.walk();
            let name = param
                .named_children(&mut cursor)
                .find(|node| node.kind() == "identifier")
                .map(|node| py_text(node, src).to_string());
            (name, param.child_by_field_name("type"))
        }
        "default_parameter" => {
            let name = param
                .child_by_field_name("name")
                .filter(|node| node.kind() == "identifier")
                .map(|node| py_text(node, src).to_string());
            (name, None)
        }
        "typed_default_parameter" => {
            let name = param
                .child_by_field_name("name")
                .map(|node| py_text(node, src).to_string());
            (name, param.child_by_field_name("type"))
        }
        "list_splat_pattern" | "dictionary_splat_pattern" => {
            let mut cursor = param.walk();
            let name = param
                .named_children(&mut cursor)
                .find(|node| node.kind() == "identifier")
                .map(|node| py_text(node, src).to_string());
            (name, None)
        }
        _ => (None, None),
    }
}

/// PEP-695 type-parameter names under `field`, excluded from ref collection.
fn py_collect_type_params(node: tree_sitter::Node, src: &[u8], field: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(tp) = node.child_by_field_name(field) {
        py_collect_identifiers_rec(tp, src, &mut out);
    }
    out
}

fn py_collect_identifiers_rec(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<String>) {
    if node.kind() == "identifier" {
        out.insert(py_text(node, src).to_string());
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        py_collect_identifiers_rec(child, src, out);
    }
}

/// Named type refs under an annotation: `subscript` recurses into container +
/// args; `attribute` keeps only the trailing name. Port of v5 `py_type_refs`.
fn py_type_refs(
    node: tree_sitter::Node,
    src: &[u8],
    tparams: &BTreeSet<String>,
    out: &mut Vec<String>,
) {
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src).to_string();
            if !tparams.contains(&name) && !is_noise_python(&name) {
                out.push(name);
            }
        }
        "attribute" => {
            if let Some(attr) = node.child_by_field_name("attribute") {
                let name = py_text(attr, src).to_string();
                if !tparams.contains(&name) && !is_noise_python(&name) {
                    out.push(name);
                }
            }
        }
        "subscript" => {
            if let Some(value) = node.child_by_field_name("value") {
                py_type_refs(value, src, tparams, out);
            }
            let mut cursor = node.walk();
            for sub in node.children_by_field_name("subscript", &mut cursor) {
                py_type_refs(sub, src, tparams, out);
            }
        }
        "string" | "concatenated_string" => {}
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                py_type_refs(child, src, tparams, out);
            }
        }
    }
}

fn py_type_refs_collect(
    node: tree_sitter::Node,
    src: &[u8],
    tparams: &BTreeSet<String>,
) -> Vec<String> {
    let mut out = Vec::new();
    py_type_refs(node, src, tparams, &mut out);
    out.sort();
    out.dedup();
    out
}

/// Builtin scalar/container + `typing` wrapper names: noise for ref collection.
fn is_noise_python(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "str"
            | "float"
            | "bool"
            | "bytes"
            | "complex"
            | "object"
            | "type"
            | "list"
            | "dict"
            | "set"
            | "tuple"
            | "frozenset"
            | "None"
            | "Self"
            | "Any"
            | "Optional"
            | "Union"
            | "List"
            | "Dict"
            | "Tuple"
            | "Set"
            | "FrozenSet"
            | "Callable"
            | "ClassVar"
            | "Final"
            | "Type"
            | "Sequence"
            | "Iterable"
            | "Iterator"
            | "Mapping"
            | "Awaitable"
            | "Coroutine"
    )
}

// ── CallF: callable definitions (nodes) + call sites (aux, commit C) ───────

fn project_call(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    py_walk_call_defs(root, src, strings, sink, None, false);
    py_walk_call_sites(root, src, strings, sink);
}

/// The def span covers `[decl start, body end)` for span-containment caller
/// resolution; a lambda (no `body` field) covers its own extent.
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

/// One CallF def node per Free function / Method / Lambda. `parent` is the
/// enclosing class name (method vs free); `in_fn` gates lambda minting.
fn py_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    parent: Option<&str>,
    in_fn: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                let owner = target.child_by_field_name("name").map(|n| py_text(n, src));
                // A class body is not a fn scope: a bare class-attribute lambda
                // is skipped (in_fn reset), matching v5's enclosing == "".
                if let Some(body) = target.child_by_field_name("body") {
                    py_walk_call_defs(body, src, strings, sink, owner, false);
                }
            }
            "function_definition" => {
                if let Some(name_node) = target.child_by_field_name("name") {
                    let kind = match parent {
                        Some(_) => CallKind::Method,
                        None => CallKind::Free,
                    };
                    let span = def_span(target);
                    let name = py_text(name_node, src);
                    sink.nodes
                        .push(Node::new(span, kind).with_name(strings.intern(name)));
                    if let Some(body) = target.child_by_field_name("body") {
                        py_walk_call_defs(body, src, strings, sink, None, true);
                    }
                }
            }
            // `is_named` keeps the `lambda` KEYWORD token (same node kind) from
            // double-minting.
            "lambda" if in_fn && target.is_named() => {
                let span = def_span(target);
                sink.nodes.push(Node::new(span, CallKind::Lambda));
                py_walk_call_defs(target, src, strings, sink, parent, true);
            }
            _ => py_walk_call_defs(target, src, strings, sink, parent, in_fn),
        }
    }
}

/// One call site per `call`; the callee is the name as written (bare identifier
/// or trailing attribute name). The site span is the callee node's start.
fn py_walk_call_sites(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call" {
            if let Some((callee, span)) = py_callee(child, src) {
                sink.aux.sites.push(CallSite {
                    span,
                    callee: strings.intern(&callee),
                    callee_path: None,
                });
            }
        }
        py_walk_call_sites(child, src, strings, sink);
    }
}

/// (callee name, callee-node span) for a `call`, None for a non-identifier or
/// non-attribute callee. Port of v5 `py_callee`.
fn py_callee(call: tree_sitter::Node, src: &[u8]) -> Option<(String, Span)> {
    let func = call.child_by_field_name("function")?;
    let span = node_span(func);
    let callee = match func.kind() {
        "identifier" => py_text(func, src).to_string(),
        "attribute" => py_text(func.child_by_field_name("attribute")?, src).to_string(),
        _ => return None,
    };
    Some((callee, span))
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
            let parsed = {
                let span = trace::parse_span("python", "astgrep");
                let _entered = span.enter();
                AstGrepParser.parse(&arena, path, content).ok()
            };
            parsed.map(|parsed| {
                let span = trace::family_span("python", "cst");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
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
                let tree = {
                    let span = trace::parse_span("python", "tree-sitter");
                    let _entered = span.enter();
                    py_parse(src)
                };
                if let Some(tree) = tree {
                    let root = tree.root_node();
                    let src_bytes = src.as_bytes();
                    if mask.types {
                        let span = trace::family_span("python", "type");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<TypeF>::default();
                        project_types(root, src_bytes, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, 0);
                        types = Some(bundle);
                    }
                    if mask.call {
                        let span = trace::family_span("python", "call");
                        let _entered = span.enter();
                        let mut bundle = FamilyBundle::<CallF>::default();
                        project_call(root, src_bytes, &mut strings, &mut bundle);
                        trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
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
            data: None,
        }
    }
}
