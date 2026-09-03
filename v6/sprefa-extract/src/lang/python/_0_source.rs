//! The Python extractor arm: tree-sitter-python front-end for type/call/df,
//! ast-grep for cst. Mirrors GoSource (same shape, different front-end): cst via
//! ast-grep's python grammar + one tree-sitter-python parse feeding the
//! type/call/df projections, then `Resolve<TypeF>` and `Resolve<CallF>`.
//!
//! Span bridge: NONE needed (like go.rs, unlike rust.rs's syn line/col -> byte
//! table). tree-sitter nodes give raw byte offsets directly (`start_byte`/
//! `end_byte`), so `Span { start: node.start_byte(), len: end - start }` is the
//! whole story.
//!
//! v5 twin: `src/graph/typegraph/python.rs` (entities, sigs, edges, docs,
//! dataflow) and `src/graph/modgraph/python.rs` (import specifiers, read there
//! with regexes over stripped text; here off the same tree-sitter parse).
//!
//! Named stop against v5: `sys.path` mutation counting (v5
//! `count_sys_path_mutators`, a diagnostic line, never a fact).

use std::collections::BTreeSet;

use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstF, DfArg, DfEdgeKind, DfF, DfField, DfNodeKind,
    DfParam, DocFact, DocTag, ProjectEdge, PyBind, PyCallArg, PyCallBind, PyDecor, PyDefault,
    PyParam, PyRetCall, PyReturn, PySubCall, ResolutionOrigin, SigSlot, Specifier, SpecifierKind,
    TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeF, TypeSig,
};
use crate::lang::{AstGrepParser, CstProjector};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::scip::{byte_range_cached, definition_of, join_documents, site_occurrence};
use crate::seams::{
    containing_def_site, corpus_defs, covering_def, def_named, own_blob, DefIndex, Parser, Project,
    Resolve,
};
use crate::shape::{ContentId, FamilyTag, NodeRef, Span, Strings, ZERO_CONTENT_ID};
use crate::source::{ExtractOutput, FamilyMask, ProjectCx, Source};
use crate::trace;
use crate::types::{DfLoop, LangKind, ScipIndex};

/// Kinds only Python constructs: the core enums do not carry them
/// (tests/6_kind_vocab.rs). `cond` is `a if c else b`.
pub const COND: DfNodeKind = DfNodeKind::Ext(LangKind {
    lang: "python",
    tag: "cond",
});
/// The file's module scope, one entity named `<module>` over the whole file:
/// v5 mints it (`EntityKind::Module`) so a module docstring has an anchor.
pub const MODULE: TypeEntityKind = TypeEntityKind::Ext(LangKind {
    lang: "python",
    tag: "module",
});
/// The module as a CALL caller: a nameless whole-file cover def minted by
/// `project_call` so a module-level call site has a caller under
/// `Resolve<CallF>`. Not a call_def wire row (skipped in `flatten_call`, v5
/// emits no such def); `caller_name` answers null, the bench join's empty
/// src_name for module-level rows. Tag "module" collides with the TypeF ext
/// tag above only across families, which the vocab rail allows.
pub const MODULE_CALLER: CallKind = CallKind::Ext(LangKind {
    lang: "python",
    tag: "module",
});

// ── the tree-sitter-python parse (one parse feeds type/call) ─────────────────

/// Parse Python via tree-sitter-python (v5 `py_parse`). tree-sitter 0.25's
/// `Language::new` wraps tree-sitter-python 0.23's `LANGUAGE`.
pub(super) fn py_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_python::LANGUAGE);
    parser.set_language(&lang).ok()?;
    parser.parse(content, None)
}

/// UTF-8 text of a tree-sitter node. Port of v5 `py_text`.
pub(super) fn py_text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

/// The byte span of a tree-sitter node `[start_byte, end_byte)`.
pub(super) fn node_span(node: tree_sitter::Node) -> crate::shape::Span {
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
    let module_span = node_span(root);
    push_entity(sink, strings, module_span, "<module>", MODULE);
    if let Some(text) = py_docstring_of(root, src) {
        push_py_doc(sink, strings, module_span, None, &text);
    }
    walk_py_entities(root, src, strings, sink, None);
    super::_1_type_edges::tsi_rows(root, src, strings, sink);
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
                    py_class_candidates(target, span, src, strings, sink);
                    if let Some(body) = target.child_by_field_name("body") {
                        if let Some(text) = py_docstring_of(body, src) {
                            push_py_doc(sink, strings, span, None, &text);
                        }
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
                    py_function_candidates(target, span, src, strings, sink);
                    if let Some(body) = target.child_by_field_name("body") {
                        if let Some(text) = py_docstring_of(body, src) {
                            push_py_doc(sink, strings, span, class_owner, &text);
                        }
                        walk_py_entities(body, src, strings, sink, None);
                    }
                }
            }
            // PEP 695 `type X = ...` / `type X[T] = ...`: a named type wearing
            // a statement, so it declares an entity like a class does.
            "type_alias_statement" => {
                if let Some(name) = target
                    .child_by_field_name("left")
                    .and_then(|left| py_first_identifier(left))
                {
                    let name = py_text(name, src).to_string();
                    push_entity(
                        sink,
                        strings,
                        node_span(target),
                        &name,
                        TypeEntityKind::Alias,
                    );
                }
            }
            _ => walk_py_entities(target, src, strings, sink, class_owner),
        }
    }
}

/// The leading `identifier` of an alias head: `Alias` itself, or the container
/// name of a `generic_type` head like `Pair[T]`.
fn py_first_identifier(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() == "identifier" {
        return Some(node);
    }
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find_map(py_first_identifier);
    found
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
    // The module as a nameless covering def (whole-file span): a module-level
    // call site then has a caller for `Resolve<CallF>`'s covering-def join.
    // Skipped in call_def wire rows; MODULE_CALLER answers a null caller_name,
    // the bench join's empty src_name for module-level rows.
    sink.nodes.push(Node::new(node_span(root), MODULE_CALLER));
    let mut lambdas: Vec<(Span, String)> = Vec::new();
    py_walk_call_defs(root, src, strings, sink, None, true, &mut 0, &mut lambdas);
    py_walk_call_sites(root, src, strings, sink, &lambdas);
    py_walk_shapes(root, src, strings, sink, &lambdas);
    py_module_specifiers(root, src, strings, sink);
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
/// enclosing class name (method vs free); `in_scope` gates lambda minting (a
/// class body is not a scope that owns lambdas). A lambda is named
/// `<lambdaN>` by `counter`, which restarts per module, def and lambda body
/// (PyCG's per-scope numbering); `lambdas` collects (span, name) for the
/// shape rows that bind or pass a lambda by value.
fn py_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    parent: Option<&str>,
    in_scope: bool,
    counter: &mut u32,
    lambdas: &mut Vec<(Span, String)>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                let owner = target.child_by_field_name("name").map(|n| py_text(n, src));
                // A class body is not a fn scope: a bare class-attribute lambda
                // is skipped (in_scope reset), matching v5's enclosing == "".
                if let Some(body) = target.child_by_field_name("body") {
                    py_walk_call_defs(body, src, strings, sink, owner, false, &mut 0, lambdas);
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
                        py_walk_call_defs(body, src, strings, sink, None, true, &mut 0, lambdas);
                    }
                }
            }
            // `is_named` keeps the `lambda` KEYWORD token (same node kind) from
            // double-minting.
            "lambda" if in_scope && target.is_named() => {
                let span = def_span(target);
                *counter += 1;
                let name = format!("<lambda{counter}>");
                sink.nodes
                    .push(Node::new(span, CallKind::Lambda).with_name(strings.intern(&name)));
                lambdas.push((span, name));
                py_walk_call_defs(target, src, strings, sink, parent, true, &mut 0, lambdas);
            }
            _ => py_walk_call_defs(
                target, src, strings, sink, parent, in_scope, counter, lambdas,
            ),
        }
    }
}

/// The value spelling of a minted lambda: `<lambdaN>@<def start>`. The
/// `<lambdaN>` def name repeats per scope, so a value row names the def by
/// its span start and `py_lambda_value_span` reads it back.
fn py_lambda_name(lambdas: &[(Span, String)], node: tree_sitter::Node) -> Option<String> {
    let span = def_span(node);
    lambdas
        .iter()
        .find(|(candidate, _)| *candidate == span)
        .map(|(_, name)| format!("{name}@{}", span.start))
}

/// The def start a lambda value spelling carries, None for any other name.
fn py_lambda_value_start(name: &str) -> Option<u32> {
    let (head, start) = name.rsplit_once('@')?;
    if !head.starts_with("<lambda") {
        return None;
    }
    start.parse().ok()
}

/// The name a value expression carries into a bind, argument or return row:
/// a bare identifier, the trailing name of an attribute (`a.func` names
/// `func`, the same spelling a call through it resolves by), a minted
/// lambda, or a parenthesized one of those. Anything else carries no name.
fn py_value_name(
    node: tree_sitter::Node,
    src: &[u8],
    lambdas: &[(Span, String)],
) -> Option<String> {
    match node.kind() {
        "identifier" => Some(py_text(node, src).to_string()),
        "attribute" => node
            .child_by_field_name("attribute")
            .map(|attr| py_text(attr, src).to_string()),
        "lambda" => py_lambda_name(lambdas, node),
        "parenthesized_expression" => node
            .named_child(0)
            .and_then(|inner| py_value_name(inner, src, lambdas)),
        _ => None,
    }
}

/// One call site per `call`; the callee is the name as written (bare identifier
/// or trailing attribute name). The site span is the callee node's span. A
/// `call`-or-`subscript` function has no name: the site is emitted with an
/// empty callee and its shape row (`PyRetCall` / `PySubCall`) drives
/// resolution. Every site's named arguments land in `py_args`, keyed by the
/// site span.
fn py_walk_call_sites(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "call" {
            if let Some(func) = child.child_by_field_name("function") {
                let span = node_span(func);
                if let Some((callee, span)) = py_callee(child, src) {
                    sink.aux.sites.push(CallSite {
                        span,
                        callee: strings.intern(&callee),
                        callee_path: None,
                    });
                } else {
                    match func.kind() {
                        "call" => {
                            sink.aux.sites.push(CallSite {
                                span,
                                callee: strings.intern(""),
                                callee_path: None,
                            });
                            if let Some(inner) = func.child_by_field_name("function") {
                                sink.aux.py_ret_calls.push(PyRetCall {
                                    span,
                                    inner: node_span(inner),
                                });
                            }
                        }
                        "subscript" => {
                            if let Some((base, key)) = py_literal_subscript(func, src) {
                                sink.aux.sites.push(CallSite {
                                    span,
                                    callee: strings.intern(""),
                                    callee_path: None,
                                });
                                sink.aux.py_sub_calls.push(PySubCall {
                                    span,
                                    base: strings.intern(&base),
                                    key: strings.intern(&key),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                py_collect_call_args(child, span, src, strings, sink, lambdas);
            }
        }
        py_walk_call_sites(child, src, strings, sink, lambdas);
    }
}

/// Nested key-path levels of one `PyBind.key` / `PySubCall.key` are joined by
/// this separator (a control byte no python key literal spells).
const PY_KEY_SEP: &str = "\x1f";

/// The key text of one literal subscript or dict-literal key: unquoted string
/// content, or `#` + decimal for an integer (`d[1]` and `d["1"]` are two
/// keys). None for a computed key.
fn py_key_text(key: tree_sitter::Node, src: &[u8]) -> Option<String> {
    match key.kind() {
        "string" => Some(py_string_content(key, src)),
        "integer" => Some(format!("#{}", py_text(key, src))),
        _ => None,
    }
}

/// The (base, key path) of a literal subscript chain over an identifier:
/// `base["a"]`, `base[0]`, `base["a"][0]`. None for a computed key at any
/// level or any other shape.
fn py_literal_subscript(node: tree_sitter::Node, src: &[u8]) -> Option<(String, String)> {
    let mut keys: Vec<String> = Vec::new();
    let mut current = node;
    while current.kind() == "subscript" {
        let mut cursor = current.walk();
        let key = current
            .children_by_field_name("subscript", &mut cursor)
            .next()?;
        keys.push(py_key_text(key, src)?);
        current = current.child_by_field_name("value")?;
    }
    if current.kind() != "identifier" {
        return None;
    }
    keys.reverse();
    Some((py_text(current, src).to_string(), keys.join(PY_KEY_SEP)))
}

/// The string content of a `"..."` literal (the `string_content` child).
fn py_string_content(node: tree_sitter::Node, src: &[u8]) -> String {
    let mut cursor = node.walk();
    let kids: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    kids.into_iter()
        .find(|child| child.kind() == "string_content")
        .map(|child| py_text(child, src).to_string())
        .unwrap_or_default()
}

/// (pos, keyword name, value name) for every named-value argument of one
/// call, keyed by the call's site span.
fn py_collect_call_args(
    call: tree_sitter::Node,
    site: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let Some(args) = call.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = args.walk();
    for (pos, arg) in args.named_children(&mut cursor).enumerate() {
        let (kw, value) = match arg.kind() {
            "keyword_argument" => {
                let kw = arg.child_by_field_name("name").map(|n| py_text(n, src));
                let Some(value) = arg.child_by_field_name("value") else {
                    continue;
                };
                (kw, value)
            }
            _ => (None, arg),
        };
        let Some(name) = py_value_name(value, src, lambdas) else {
            continue;
        };
        sink.aux.py_args.push(PyCallArg {
            site,
            pos: pos as i64,
            kw: kw.map(|text| strings.intern(text)),
            value: strings.intern(&name),
        });
    }
}

// ── dynamic-shape rows (py_* aux; the syntax an honest tier can carry) ──────
//
// One full-tree walk in file order collecting the value bindings, params,
// single returns, decorator applications, and raise sites that
// `Resolve<CallF>` needs beyond a bare callee name. All rows are same-file,
// byte-ordered facts; nothing here traces flow beyond "the last binding
// before the site".

fn py_walk_shapes(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "expression_statement" => {
                if let Some(inner) = child.named_child(0) {
                    if inner.kind() == "assignment" {
                        py_collect_assignment(inner, src, strings, sink, lambdas);
                    }
                }
            }
            "assignment" => py_collect_assignment(child, src, strings, sink, lambdas),
            "function_definition" => py_collect_fn_shapes(child, src, strings, sink, lambdas),
            "lambda" if child.is_named() => {
                py_collect_fn_shapes(child, src, strings, sink, lambdas)
            }
            "call" => py_collect_mutation(child, src, strings, sink, lambdas),
            "decorated_definition" => py_collect_decorators(child, src, strings, sink),
            "raise_statement" => py_collect_raise(child, src, strings, sink),
            _ => {}
        }
        py_walk_shapes(child, src, strings, sink, lambdas);
    }
}

/// One `assignment` statement: bind (or kill) every target against its value.
/// A chained `a = b = <v>` binds the intermediate targets first, then pairs
/// the outer targets with the same value node.
fn py_collect_assignment(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let mut value = node.child_by_field_name("right");
    while value.is_some_and(|v| v.kind() == "assignment") {
        let inner = value.unwrap();
        py_collect_assignment(inner, src, strings, sink, lambdas);
        value = inner.child_by_field_name("right");
    }
    let Some(value) = value else {
        return;
    };
    let span = node_span(node);
    py_shape_bind_pattern(left, value, span, src, strings, sink, lambdas);
}

/// Pair one assignment-target pattern with its value node: `identifier =
/// <value name>` mints a name binding (a `call` value adds the call-result
/// row; a container literal adds one element binding per literal slot);
/// positional patterns pair element-wise (a splat takes the middle run as
/// list slots); a `subscript` target mints an element binding; a `self.<attr>`
/// target binds the attribute name. A value carrying no name is a KILL
/// (value None) so an earlier binding for the same name cannot survive it.
fn py_shape_bind_pattern(
    left: tree_sitter::Node,
    rhs: tree_sitter::Node,
    span: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    match left.kind() {
        "identifier" => {
            let target = strings.intern(&py_text(left, src));
            py_bind_named(target, None, rhs, span, src, strings, sink, lambdas);
        }
        "attribute" => {
            let is_self = left.child_by_field_name("object").is_some_and(|object| {
                object.kind() == "identifier" && py_text(object, src) == "self"
            });
            let Some(attr) = left.child_by_field_name("attribute").filter(|_| is_self) else {
                return;
            };
            let target = strings.intern(&py_text(attr, src));
            py_bind_named(target, None, rhs, span, src, strings, sink, lambdas);
        }
        "pattern_list" | "tuple_pattern" | "list_pattern" => {
            let elements = py_sequence_elements(rhs);
            let mut cursor = left.walk();
            let items: Vec<tree_sitter::Node> = left.named_children(&mut cursor).collect();
            let splat_at = items.iter().position(|i| i.kind() == "list_splat_pattern");
            let mut index = 0usize;
            for (item_ix, item) in items.iter().enumerate() {
                if Some(item_ix) == splat_at {
                    // The splat takes every element the tail will not: the
                    // middle run of the value sequence, as list slots 0..take.
                    let after = items.len() - item_ix - 1;
                    let take = elements_len(&elements).saturating_sub(index + after);
                    let name = item
                        .named_child(0)
                        .filter(|n| n.kind() == "identifier")
                        .map(|n| py_text(n, src).to_string());
                    for slot in 0..take {
                        let Some(element) = py_element_at(&elements, index + slot) else {
                            continue;
                        };
                        if let (Some(base), Some(text)) =
                            (name.as_ref(), py_value_name(element, src, lambdas))
                        {
                            sink.aux.py_binds.push(PyBind {
                                span,
                                target: strings.intern(base),
                                key: Some(strings.intern(&format!("#{slot}"))),
                                value: Some(strings.intern(&text)),
                            });
                        }
                    }
                    index += take;
                    continue;
                }
                let Some(element) = py_element_at(&elements, index) else {
                    break;
                };
                index += 1;
                py_bind_one(*item, element, span, src, strings, sink, lambdas);
            }
        }
        "subscript" => {
            if let Some((base, key)) = py_literal_subscript(left, src) {
                let target = strings.intern(&base);
                py_bind_named(target, Some(key), rhs, span, src, strings, sink, lambdas);
            }
        }
        _ => {}
    }
}

/// One (target, key path) bound to a value node: the name row (or KILL), the
/// call-result row for a `call` value, and one element row per slot of a
/// container literal, nested literals extending the key path.
fn py_bind_named(
    target: crate::shape::NameId,
    key: Option<String>,
    rhs: tree_sitter::Node,
    span: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let value = py_value_name(rhs, src, lambdas).map(|text| strings.intern(&text));
    sink.aux.py_binds.push(PyBind {
        span,
        target,
        key: key.as_deref().map(|text| strings.intern(text)),
        value,
    });
    if key.is_none() && rhs.kind() == "call" {
        if let Some(func) = rhs.child_by_field_name("function") {
            sink.aux.py_call_binds.push(PyCallBind {
                span,
                target,
                site: node_span(func),
            });
        }
    }
    py_bind_slots(target, key, rhs, span, src, strings, sink, lambdas);
}

/// One element row per slot of a container literal, nested literals
/// extending the key path under `key`.
fn py_bind_slots(
    target: crate::shape::NameId,
    key: Option<String>,
    rhs: tree_sitter::Node,
    span: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let slots: Vec<(String, tree_sitter::Node)> = match rhs.kind() {
        "dictionary" => {
            let mut cursor = rhs.walk();
            rhs.named_children(&mut cursor)
                .filter(|pair| pair.kind() == "pair")
                .filter_map(|pair| {
                    let key = py_key_text(pair.child_by_field_name("key")?, src)?;
                    Some((key, pair.child_by_field_name("value")?))
                })
                .collect()
        }
        "list" | "tuple" => {
            let mut cursor = rhs.walk();
            rhs.named_children(&mut cursor)
                .enumerate()
                .map(|(slot, element)| (format!("#{slot}"), element))
                .collect()
        }
        _ => Vec::new(),
    };
    for (slot, element) in slots {
        let path = match &key {
            Some(prefix) => format!("{prefix}{PY_KEY_SEP}{slot}"),
            None => slot,
        };
        py_bind_named(
            target,
            Some(path),
            element,
            span,
            src,
            strings,
            sink,
            lambdas,
        );
    }
}

/// The value side of an unpacking: `expression_list` (top-level tuple rhs),
/// `tuple` / `list` literals, or the single expression itself, as a uniform
/// list of expression nodes.
fn py_sequence_elements(node: tree_sitter::Node) -> PySeq {
    match node.kind() {
        "expression_list" | "tuple" | "list" => {
            let mut cursor = node.walk();
            PySeq::Many(node.named_children(&mut cursor).collect::<Vec<_>>())
        }
        _ => PySeq::One(node),
    }
}

enum PySeq<'t> {
    One(tree_sitter::Node<'t>),
    Many(Vec<tree_sitter::Node<'t>>),
}

fn elements_len(seq: &PySeq) -> usize {
    match seq {
        PySeq::One(_) => 1,
        PySeq::Many(nodes) => nodes.len(),
    }
}

fn py_element_at<'t>(seq: &'t PySeq, index: usize) -> Option<tree_sitter::Node<'t>> {
    match seq {
        PySeq::One(node) => (index == 0).then_some(*node),
        PySeq::Many(nodes) => nodes.get(index).copied(),
    }
}

/// One positional element pair (pattern item, value expression): a named
/// value or a nested pattern; a value carrying no name kills the name.
fn py_bind_one(
    item: tree_sitter::Node,
    value: tree_sitter::Node,
    span: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    match item.kind() {
        "identifier" => {
            let target = strings.intern(&py_text(item, src));
            py_bind_named(target, None, value, span, src, strings, sink, lambdas);
        }
        "tuple_pattern" | "list_pattern" => {
            py_shape_bind_pattern(item, value, span, src, strings, sink, lambdas)
        }
        _ => {}
    }
}

/// Params (slot positions, receiver skipped), bare-identifier defaults, and
/// the single-named-value return shape of one function def.
fn py_collect_fn_shapes(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let def = node_span(node);
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        let mut pos: u32 = 0;
        let mut first = true;
        for param in params.named_children(&mut cursor) {
            if matches!(param.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, _ty) = py_param_name_and_type(param, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            let Some(pname) = name_opt else {
                continue;
            };
            sink.aux.py_params.push(PyParam {
                def,
                name: strings.intern(&pname),
                pos,
            });
            if param.kind() == "default_parameter" {
                if let Some(value) = param.child_by_field_name("value") {
                    if value.kind() == "identifier" {
                        sink.aux.py_defaults.push(PyDefault {
                            def,
                            name: strings.intern(&pname),
                            value: strings.intern(&py_text(value, src)),
                        });
                    }
                }
            }
            pos += 1;
        }
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    // A lambda's body IS its return.
    let returned = if node.kind() == "lambda" {
        py_value_name(body, src, lambdas)
    } else if py_count_returns(body) == 1 {
        py_single_return(body, src, lambdas)
    } else {
        None
    };
    if let Some(value) = returned {
        sink.aux.py_returns.push(PyReturn {
            def,
            value: strings.intern(&value),
        });
    }
}

/// Methods that rewrite a container in place. `update` with a dict literal
/// binds the literal's slots on the receiver; every other one KILLs the
/// receiver's element bindings (a container-level KILL row, key None).
const PY_MUTATORS: &[&str] = &[
    "update",
    "append",
    "insert",
    "extend",
    "pop",
    "popitem",
    "clear",
    "setdefault",
    "remove",
    "sort",
    "reverse",
];

/// `name.<mutator>(...)`: the receiver's element bindings after this call.
fn py_collect_mutation(
    call: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    lambdas: &[(Span, String)],
) {
    let Some(func) = call.child_by_field_name("function") else {
        return;
    };
    if func.kind() != "attribute" {
        return;
    }
    let (Some(object), Some(attr)) = (
        func.child_by_field_name("object"),
        func.child_by_field_name("attribute"),
    ) else {
        return;
    };
    if object.kind() != "identifier" || !PY_MUTATORS.contains(&py_text(attr, src)) {
        return;
    }
    let target = strings.intern(&py_text(object, src));
    let span = node_span(call);
    sink.aux.py_binds.push(PyBind {
        span,
        target,
        key: None,
        value: None,
    });
    if py_text(attr, src) != "update" {
        return;
    }
    let Some(args) = call.child_by_field_name("arguments") else {
        return;
    };
    let literal = args.named_child(0).filter(|arg| arg.kind() == "dictionary");
    if let Some(literal) = literal {
        py_bind_slots(target, None, literal, span, src, strings, sink, lambdas);
    }
}

/// The number of `return` statements directly in one body block, nested
/// defs/classes excluded (their returns belong to their own defs).
fn py_count_returns(node: tree_sitter::Node) -> usize {
    if node.kind() == "return_statement" {
        return 1;
    }
    if matches!(
        node.kind(),
        "function_definition" | "class_definition" | "lambda"
    ) {
        return 0;
    }
    let mut count = 0;
    let mut cursor = node.walk();
    let kids: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    for child in kids {
        count += py_count_returns(child);
    }
    count
}

/// The value name of a body's single `return`, when it carries one.
fn py_single_return(
    node: tree_sitter::Node,
    src: &[u8],
    lambdas: &[(Span, String)],
) -> Option<String> {
    if node.kind() == "return_statement" {
        let value = node.named_child(0)?;
        return py_value_name(value, src, lambdas);
    }
    if matches!(
        node.kind(),
        "function_definition" | "class_definition" | "lambda"
    ) {
        return None;
    }
    let mut cursor = node.walk();
    let kids: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    kids.into_iter()
        .find_map(|child| py_single_return(child, src, lambdas))
}

/// Decorator applications: every decorator expression emits a call site (the
/// decorator IS called, with the decorated def as its slot-0 argument); only
/// the OUTERMOST decorator may rebind the decorated name, and that check
/// happens at resolve time on the `PyDecor` row.
fn py_collect_decorators(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let Some(target) = node.child_by_field_name("definition") else {
        return;
    };
    let Some(name_node) = target.child_by_field_name("name") else {
        return;
    };
    let decorated = py_text(name_node, src).to_string();
    let mut cursor = node.walk();
    let mut first = true;
    for child in node.children(&mut cursor) {
        if child.kind() != "decorator" {
            continue;
        }
        let mut decor_cursor = child.walk();
        let Some(expr) = child.named_children(&mut decor_cursor).next() else {
            continue;
        };
        let (callee, span) = match expr.kind() {
            "identifier" => (py_text(expr, src).to_string(), node_span(expr)),
            "call" => match py_callee(expr, src) {
                Some(found) => found,
                None => continue,
            },
            // a bare attribute keeps its trailing name; the class it names
            // (cross-file included) is what the oracle's edge targets.
            "attribute" => {
                let Some(attr) = expr.child_by_field_name("attribute") else {
                    continue;
                };
                (py_text(attr, src).to_string(), node_span(expr))
            }
            _ => continue,
        };
        let callee_id = strings.intern(&callee);
        sink.aux.sites.push(CallSite {
            span,
            callee: callee_id,
            callee_path: None,
        });
        sink.aux.py_args.push(PyCallArg {
            site: span,
            pos: 0,
            kw: None,
            value: strings.intern(&decorated),
        });
        if first {
            first = false;
            sink.aux.py_decorators.push(PyDecor {
                span,
                callee: callee_id,
                decorated: strings.intern(&decorated),
                call_expr: expr.kind() == "call",
            });
        }
    }
}

/// `raise <expr>` is a call to the expr's class: a bare name, a name alias, or
/// a trailing attribute (whose class carries the `__init__` the oracle wants).
fn py_collect_raise(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = node.walk();
    let Some(value) = node.named_children(&mut cursor).next() else {
        return;
    };
    let (callee, span) = match value.kind() {
        "identifier" => (py_text(value, src).to_string(), node_span(value)),
        "attribute" => {
            let Some(attr) = value.child_by_field_name("attribute") else {
                return;
            };
            (py_text(attr, src).to_string(), node_span(value))
        }
        _ => return,
    };
    sink.aux.sites.push(CallSite {
        span,
        callee: strings.intern(&callee),
        callee_path: None,
    });
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

// ── docs facet (TypeFAux.docs): PEP 257 docstrings + Sphinx field tags ───────

fn push_py_doc(
    sink: &mut FamilyBundle<TypeF>,
    strings: &mut Strings,
    owner: Span,
    parent: Option<&str>,
    text: &str,
) {
    let tags = py_parse_sphinx_tags(text, strings);
    sink.aux.docs.push(DocFact {
        owner,
        parent: parent.map(|name| strings.intern(name)),
        text: strings.intern(text),
        tags,
    });
}

/// The docstring at the head of a class/def body block: the block's first
/// named child must be a bare `string` expression statement. Port of v5
/// `py_docstring_of`.
fn py_docstring_of(body: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut cursor = body.walk();
    let first = body.named_children(&mut cursor).next()?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let inner = first.named_child(0)?;
    if inner.kind() != "string" {
        return None;
    }
    Some(py_clean_docstring(py_text(inner, src)))
}

/// Strip an optional `r`/`b`/`f`/`u` prefix and the enclosing quotes, then
/// dedent. Escapes are kept as written. Port of v5 `py_clean_docstring`.
fn py_clean_docstring(raw: &str) -> String {
    let trimmed = raw.trim();
    let quote_at = trimmed.find(['"', '\'']).unwrap_or(0);
    let body = &trimmed[quote_at..];
    let quote = if body.starts_with("\"\"\"") {
        "\"\"\""
    } else if body.starts_with("'''") {
        "'''"
    } else if body.starts_with('"') {
        "\""
    } else if body.starts_with('\'') {
        "'"
    } else {
        return trimmed.to_string();
    };
    let inner = body
        .strip_prefix(quote)
        .and_then(|rest| rest.strip_suffix(quote))
        .unwrap_or(body);
    py_dedent(inner)
}

/// PEP 257 dedent: the minimum indent over every non-blank line AFTER the
/// first is stripped from every subsequent line. Port of v5 `py_dedent`.
fn py_dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return text.trim().to_string();
    }
    let min_indent = lines
        .iter()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        if index == 0 {
            out.push(line.trim().to_string());
        } else {
            out.push(
                line.get(min_indent.min(line.len())..)
                    .unwrap_or("")
                    .to_string(),
            );
        }
    }
    out.join("\n").trim().to_string()
}

/// Sphinx field-list tags: `:param name: text` -> tag `param` arg `name`;
/// `:return:`/`:returns:` -> tag `returns`, no arg; any other `:tag:` passes
/// through; a continuation line appends to the previous tag's text.
/// Google-style `Args:` sections are not recognized (v5 neither). Port of v5
/// `py_parse_sphinx_tags`.
fn py_parse_sphinx_tags(text: &str, strings: &mut Strings) -> Vec<DocTag> {
    let mut out: Vec<(&'static str, String, String)> = Vec::new();
    let mut owned_tags: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(':') {
            if let Some(colon) = rest.find(':') {
                let head = rest[..colon].trim();
                let body = rest[colon + 1..].trim().to_string();
                let mut parts = head.splitn(2, char::is_whitespace);
                let tag_word = parts.next().unwrap_or("");
                let head_arg = parts.next().unwrap_or("").trim();
                let (tag, arg): (&'static str, &str) = match tag_word {
                    "param" | "parameter" => ("param", head_arg),
                    "return" | "returns" => ("returns", ""),
                    other => {
                        owned_tags.push(other.to_string());
                        ("", head_arg)
                    }
                };
                out.push((tag, arg.to_string(), body));
                continue;
            }
        }
        if let Some(last) = out.last_mut() {
            if !trimmed.is_empty() {
                if !last.2.is_empty() {
                    last.2.push(' ');
                }
                last.2.push_str(trimmed);
            }
        }
    }
    let mut owned = owned_tags.into_iter();
    out.into_iter()
        .map(|(tag, arg, body)| {
            let tag_text = if tag.is_empty() {
                owned.next().unwrap_or_default()
            } else {
                tag.to_string()
            };
            DocTag {
                tag: strings.intern(&tag_text),
                arg: (!arg.is_empty()).then(|| strings.intern(&arg)),
                text: strings.intern(&body),
            }
        })
        .collect()
}

// ── type-edge candidates (TypeFAux.candidates, port of v5 `py_edges_from`) ──

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

/// A class's candidates: each superclass (`impl`; a `metaclass=` keyword arg
/// is not a base) and each annotated class attribute (`field`). Port of v5
/// `py_class_edges`.
fn py_class_candidates(
    node: tree_sitter::Node,
    owner: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let tparams = py_collect_type_params(node, src, "type_parameters");
    if let Some(supers) = node.child_by_field_name("superclasses") {
        let mut cursor = supers.walk();
        for arg in supers.named_children(&mut cursor) {
            if arg.kind() == "keyword_argument" {
                continue;
            }
            for to in py_type_refs_collect(arg, src, &tparams) {
                push_candidate(sink, strings, owner, &to, TypeEdgeKind::Impl);
            }
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        let mut cursor = body.walk();
        for stmt in body.named_children(&mut cursor) {
            if stmt.kind() != "expression_statement" {
                continue;
            }
            let Some(inner) = stmt.named_child(0) else {
                continue;
            };
            if inner.kind() != "assignment" {
                continue;
            }
            if let Some(ty) = inner.child_by_field_name("type") {
                for to in py_type_refs_collect(ty, src, &tparams) {
                    push_candidate(sink, strings, owner, &to, TypeEdgeKind::Field);
                }
            }
        }
    }
}

/// A def's candidates: param annotations (`param`, receiver skipped), the
/// return annotation (`returns`), and every annotated local assignment under
/// the body, nested defs included (`uses`). Port of v5 `py_function_edges`.
fn py_function_candidates(
    node: tree_sitter::Node,
    owner: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let tparams = py_collect_type_params(node, src, "type_parameters");
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
            if let Some(ty) = type_node {
                for to in py_type_refs_collect(ty, src, &tparams) {
                    push_candidate(sink, strings, owner, &to, TypeEdgeKind::Param);
                }
            }
        }
    }
    if let Some(ret) = node.child_by_field_name("return_type") {
        for to in py_type_refs_collect(ret, src, &tparams) {
            push_candidate(sink, strings, owner, &to, TypeEdgeKind::Returns);
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        let mut uses = Vec::new();
        py_collect_body_annotation_refs(body, src, &tparams, &mut uses);
        uses.sort();
        uses.dedup();
        for to in uses {
            push_candidate(sink, strings, owner, &to, TypeEdgeKind::Uses);
        }
    }
}

fn py_collect_body_annotation_refs(
    node: tree_sitter::Node,
    src: &[u8],
    tparams: &BTreeSet<String>,
    out: &mut Vec<String>,
) {
    if node.kind() == "assignment" {
        if let Some(ty) = node.child_by_field_name("type") {
            out.extend(py_type_refs_collect(ty, src, tparams));
        }
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        py_collect_body_annotation_refs(child, src, tparams, out);
    }
}

// ── module specifiers (CallFAux.specifiers) ─────────────────────────────────
// @comment-ok: the kind/name/module/imported contract, pinned row-for-row by
// tests/16_python.rs. `Default`/`Reexport` are unreachable from python.
//
// | python source                | kind      | name    | module   | imported |
// |------------------------------|-----------|---------|----------|----------|
// | `import os`                  | Named     | os      | None     | None     |
// | `import os.path as osp`      | Named     | osp     | os.path  | None     |
// | `from x import a`            | Named     | a       | x        | None     |
// | `from x import a as b`       | Named     | b       | x        | a        |
// | `from . import sibling`      | Named     | sibling | .        | None     |
// | `from .p.q import t as u`    | Named     | u       | .p.q     | t        |
// | `from x import *`            | Namespace | x       | None     | None     |
//
// The path-only form carries the path in `name` with `module` None (the go
// convention, `src/types.rs` names the path-shaped languages). One row per
// imported name; a relative module keeps its leading dots in `module` so the
// resolver can pop directories exactly as v5 `py_resolve_relative` does.

fn py_module_specifiers(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut rows = Vec::new();
    py_walk_imports(root, src, strings, &mut rows);
    sink.aux.specifiers.extend(rows);
}

fn py_walk_imports(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    rows: &mut Vec<Specifier>,
) {
    match node.kind() {
        "import_statement" => {
            let mut cursor = node.walk();
            for item in node.named_children(&mut cursor) {
                let (name, module) = match item.kind() {
                    "dotted_name" => (py_text(item, src).to_string(), None),
                    "aliased_import" => {
                        let path = item
                            .child_by_field_name("name")
                            .map(|n| py_text(n, src).to_string())
                            .unwrap_or_default();
                        let alias = item
                            .child_by_field_name("alias")
                            .map(|n| py_text(n, src).to_string())
                            .unwrap_or_else(|| path.clone());
                        (alias, Some(path))
                    }
                    _ => continue,
                };
                rows.push(Specifier {
                    span: node_span(item),
                    name: strings.intern(&name),
                    kind: SpecifierKind::Named,
                    module: module.map(|text| strings.intern(&text)),
                    imported: None,
                });
            }
        }
        // `from __future__ import x` is its OWN node kind, and the module name
        // is a keyword the grammar leaves off the field table.
        "import_from_statement" | "future_import_statement" => {
            let module = if node.kind() == "future_import_statement" {
                "__future__".to_string()
            } else {
                node.child_by_field_name("module_name")
                    .map(|n| py_text(n, src).to_string())
                    .unwrap_or_default()
            };
            let mut cursor = node.walk();
            let mut saw_name = false;
            for item in node.children_by_field_name("name", &mut cursor) {
                saw_name = true;
                let (name, imported) = match item.kind() {
                    "dotted_name" => (py_text(item, src).to_string(), None),
                    "aliased_import" => {
                        let source = item
                            .child_by_field_name("name")
                            .map(|n| py_text(n, src).to_string())
                            .unwrap_or_default();
                        let alias = item
                            .child_by_field_name("alias")
                            .map(|n| py_text(n, src).to_string())
                            .unwrap_or_else(|| source.clone());
                        let imported = (alias != source).then_some(source);
                        (alias, imported)
                    }
                    _ => continue,
                };
                rows.push(Specifier {
                    span: node_span(item),
                    name: strings.intern(&name),
                    kind: SpecifierKind::Named,
                    module: Some(strings.intern(&module)),
                    imported: imported.map(|text| strings.intern(&text)),
                });
            }
            if !saw_name {
                // `from x import *`: the wildcard is the only nameless form.
                rows.push(Specifier {
                    span: node_span(node),
                    name: strings.intern(&module),
                    kind: SpecifierKind::Namespace,
                    module: None,
                    imported: None,
                });
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        py_walk_imports(child, src, strings, rows);
    }
}

// ── DfF: intra-procedural value flow (port of v5 `py_dataflow_from`) ────────
//
// Same two-rule model as go/kotlin: value-bearing children flow into their
// parent, and a bound name (assignment target, param, loop variable,
// comprehension variable, lambda param) registers a scope slot a later read
// flows from. Every named `def` (top-level, method, nested) is discovered by
// one full-tree walk and flowed with a FRESH scope; only a `lambda` shares the
// enclosing scope. `self`/`cls` are skipped as params so `param.pos` aligns
// with `sig.pos`.

type Scope = std::collections::HashMap<String, NodeRef>;

fn project_df(
    root: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    py_walk_fns(root, src, file, strings, sink);
    sink.aux.nests = crate::types::compute_nests(&sink.nodes, &sink.aux.loops);
}

fn py_walk_fns(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let target = py_unwrap_decorated(child);
        if target.kind() == "function_definition" {
            py_flow_fn(target, src, file, strings, sink);
        }
        py_walk_fns(target, src, file, strings, sink);
    }
}

/// Push one df node over its FULL syntactic extent, returning its `NodeRef`.
/// Port of v5 `push_node` (minus fn_sym/file/aux).
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

fn df_push_node(
    sink: &mut FamilyBundle<DfF>,
    strings: &mut Strings,
    node: tree_sitter::Node,
    kind: DfNodeKind,
    name: Option<&str>,
) -> NodeRef {
    df_push(
        sink,
        strings,
        node.start_byte() as u32,
        node.end_byte() as u32,
        kind,
        name,
    )
}

fn df_edge(sink: &mut FamilyBundle<DfF>, src: NodeRef, dst: NodeRef) {
    sink.edges.push(Edge::new(src, dst, DfEdgeKind::Direct));
}

/// v5 `mint_sym(file, Function, name, None)`: the grouping key a closure's
/// `lam_sym` chains from. A method mints the same `function` shape (v5
/// `py_flow_fn`).
fn py_fn_sym(file: &str, name: &str) -> String {
    format!("{file}::function::{name}")
}

/// Seed non-receiver param nodes into a fresh scope, then flow the body. A
/// Python body has no implicit tail-return: only an explicit `return` reaches
/// a `ret` node. Port of v5 `py_flow_fn`.
fn py_flow_fn(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let fn_sym = py_fn_sym(file, py_text(name_node, src));
    let mut scope = Scope::new();
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut cursor = params.walk();
        let mut pos: u32 = 0;
        let mut first = true;
        for param in params.named_children(&mut cursor) {
            if matches!(param.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, _ty) = py_param_name_and_type(param, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            if let Some(pname) = name_opt {
                let node_ref = df_push_node(sink, strings, param, DfNodeKind::Param, Some(&pname));
                sink.aux.params.push(DfParam {
                    node: node_ref,
                    pos,
                });
                scope.insert(pname, node_ref);
                pos += 1;
            }
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, &fn_sym, strings, &mut scope, sink);
    }
}

/// Flow one statement. A nested def/class is SKIPPED here: `py_walk_fns`
/// discovers and flows it with its own fresh scope. Port of v5 `py_flow_stmt`.
fn py_flow_stmt(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    match node.kind() {
        "function_definition" | "decorated_definition" | "class_definition" => {}
        "expression_statement" => {
            if let Some(inner) = node.named_child(0) {
                match inner.kind() {
                    "assignment" => {
                        py_flow_assignment(inner, src, fn_sym, strings, scope, sink);
                    }
                    "augmented_assignment" => {
                        py_flow_augmented(inner, src, fn_sym, strings, scope, sink);
                    }
                    _ => {
                        py_flow_expr(inner, src, fn_sym, strings, scope, sink);
                    }
                }
            }
        }
        "assignment" => py_flow_assignment(node, src, fn_sym, strings, scope, sink),
        "augmented_assignment" => py_flow_augmented(node, src, fn_sym, strings, scope, sink),
        "return_statement" => {
            let ret = df_push_node(sink, strings, node, DfNodeKind::Ret, None);
            if let Some(value) = node.named_child(0) {
                let value_ref = py_flow_expr(value, src, fn_sym, strings, scope, sink);
                df_edge(sink, value_ref, ret);
            }
        }
        "for_statement" => py_flow_for(node, src, fn_sym, strings, scope, sink),
        "while_statement" => py_flow_while(node, src, fn_sym, strings, scope, sink),
        // The condition is an EXPRESSION; every other named child is a suite.
        "if_statement" | "elif_clause" => {
            let cond = node.child_by_field_name("condition");
            if let Some(cond) = cond {
                py_flow_expr(cond, src, fn_sym, strings, scope, sink);
            }
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if cond.is_some_and(|c| c.id() == child.id()) {
                    continue;
                }
                py_flow_stmt(child, src, fn_sym, strings, scope, sink);
            }
        }
        "assert_statement" | "raise_statement" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                py_flow_expr(child, src, fn_sym, strings, scope, sink);
            }
        }
        "with_statement" => py_flow_with(node, src, fn_sym, strings, scope, sink),
        "except_clause" | "except_group_clause" => {
            py_flow_except(node, src, fn_sym, strings, scope, sink)
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                py_flow_stmt(child, src, fn_sym, strings, scope, sink);
            }
        }
    }
}

/// `x += e` rebinds `x` from its own read and from `e`. The rebind carries the
/// STATEMENT span: the target identifier already carries the read.
fn py_flow_augmented(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let left = node.child_by_field_name("left");
    let mut sources = Vec::new();
    if let Some(left) = left {
        sources.push(py_flow_expr(left, src, fn_sym, strings, scope, sink));
    }
    if let Some(right) = node.child_by_field_name("right") {
        sources.push(py_flow_expr(right, src, fn_sym, strings, scope, sink));
    }
    let Some(left) = left.filter(|target| target.kind() == "identifier") else {
        return;
    };
    let name = py_text(left, src).to_string();
    let bind = df_push_node(sink, strings, node, DfNodeKind::LetBind, Some(&name));
    for source in sources {
        df_edge(sink, source, bind);
    }
    scope.insert(name, bind);
}

/// Each `with_item` flows its context expression; an `as` target binds from
/// that value, exactly as an assignment target binds from its rhs.
fn py_flow_with(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    let clauses: Vec<tree_sitter::Node> = node
        .named_children(&mut cursor)
        .filter(|child| child.kind() == "with_clause")
        .collect();
    for clause in clauses {
        let mut item_cursor = clause.walk();
        let items: Vec<tree_sitter::Node> = clause
            .named_children(&mut item_cursor)
            .filter(|item| item.kind() == "with_item")
            .collect();
        for item in items {
            let Some(value) = item
                .child_by_field_name("value")
                .or_else(|| item.named_child(0))
            else {
                continue;
            };
            if value.kind() != "as_pattern" {
                py_flow_expr(value, src, fn_sym, strings, scope, sink);
                continue;
            }
            let Some(inner) = value.named_child(0) else {
                continue;
            };
            let context = py_flow_expr(inner, src, fn_sym, strings, scope, sink);
            py_bind_as_target(value, Some(context), src, strings, scope, sink);
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, fn_sym, strings, scope, sink);
    }
}

/// `except E as name` binds `name` with NO incoming edge: the caught exception
/// has no producer inside the function, and `E` is a type, never a value read.
fn py_flow_except(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    for child in children {
        match child.kind() {
            "as_pattern" => py_bind_as_target(child, None, src, strings, scope, sink),
            "block" => py_flow_stmt(child, src, fn_sym, strings, scope, sink),
            _ => {}
        }
    }
}

/// Bind the `as NAME` half of an `as_pattern`, optionally fed by the value the
/// pattern destructures.
fn py_bind_as_target(
    pattern: tree_sitter::Node,
    value: Option<NodeRef>,
    src: &[u8],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = pattern.walk();
    let Some(target) = pattern
        .child_by_field_name("alias")
        .or_else(|| {
            pattern
                .named_children(&mut cursor)
                .find(|child| child.kind() == "as_pattern_target")
        })
        .and_then(py_first_identifier)
    else {
        return;
    };
    let name = py_text(target, src).to_string();
    let bind = df_push_node(sink, strings, target, DfNodeKind::LetBind, Some(&name));
    if let Some(value) = value {
        df_edge(sink, value, bind);
    }
    scope.insert(name, bind);
}

fn py_flow_assignment(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    let rhs = py_flow_expr(right, src, fn_sym, strings, scope, sink);
    if let Some(left) = node.child_by_field_name("left") {
        py_bind_pattern(left, rhs, src, strings, scope, sink);
    }
}

/// `identifier` mints a `let_bind` fed by the rhs; tuple/list unpacking mints
/// one slot PER identifier, each fed by the SAME rhs; `attribute` and
/// `subscript` targets track no local binding. Port of v5 `py_bind_pattern`.
fn py_bind_pattern(
    node: tree_sitter::Node,
    rhs: NodeRef,
    src: &[u8],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src).to_string();
            let bind = df_push_node(sink, strings, node, DfNodeKind::LetBind, Some(&name));
            df_edge(sink, rhs, bind);
            scope.insert(name, bind);
        }
        "tuple_pattern" | "list_pattern" | "pattern_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                py_bind_pattern(child, rhs, src, strings, scope, sink);
            }
        }
        _ => {}
    }
}

/// `(name, identifier node)` for every leaf identifier a for/comprehension
/// pattern binds. Port of v5 `py_pattern_identifiers`.
fn py_pattern_identifiers<'t>(
    node: tree_sitter::Node<'t>,
    src: &[u8],
    out: &mut Vec<(String, tree_sitter::Node<'t>)>,
) {
    match node.kind() {
        "identifier" => out.push((py_text(node, src).to_string(), node)),
        "tuple_pattern" | "list_pattern" | "pattern_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                py_pattern_identifiers(child, src, out);
            }
        }
        _ => {}
    }
}

/// Bind each pattern identifier to a `let_bind` fed by the iterable's value,
/// returning the first bound name (the loop's `var`).
fn py_bind_loop_targets(
    left: tree_sitter::Node,
    collection: Option<NodeRef>,
    src: &[u8],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<String> {
    let mut names = Vec::new();
    py_pattern_identifiers(left, src, &mut names);
    let mut var_name = None;
    for (name, name_node) in &names {
        let bind = df_push_node(sink, strings, *name_node, DfNodeKind::LetBind, Some(name));
        if let Some(collection) = collection {
            df_edge(sink, collection, bind);
        }
        scope.insert(name.clone(), bind);
        if var_name.is_none() {
            var_name = Some(name.clone());
        }
    }
    var_name
}

fn push_loop(sink: &mut FamilyBundle<DfF>, node: tree_sitter::Node, var: Option<String>) {
    sink.aux.loops.push(DfLoop {
        span: node_span(node),
        var,
        collection: None,
    });
}

fn py_flow_for(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    let mut cursor = node.walk();
    let iter_expr = node
        .children_by_field_name("right", &mut cursor)
        .find(|n| n.is_named());
    let collection = iter_expr.map(|expr| py_flow_expr(expr, src, fn_sym, strings, scope, sink));
    let var = node
        .child_by_field_name("left")
        .and_then(|left| py_bind_loop_targets(left, collection, src, strings, scope, sink));
    push_loop(sink, node, var);
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, fn_sym, strings, scope, sink);
    }
}

fn py_flow_while(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    if let Some(cond) = node.child_by_field_name("condition") {
        py_flow_expr(cond, src, fn_sym, strings, scope, sink);
    }
    push_loop(sink, node, None);
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, fn_sym, strings, scope, sink);
    }
}

/// A comprehension walks its `for_in_clause`s and `if_clause`s in the
/// ENCLOSING scope, binds each loop variable from its iterable, then flows the
/// body (both halves of a dict comprehension's `pair`) into a `new` node; its
/// own span is a loop so `nest` counts calls per iteration. Port of v5
/// `py_comprehension_flow`.
fn py_comprehension_flow(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let mut loop_var: Option<String> = None;
    let mut cursor = node.walk();
    let clauses: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
    for clause in &clauses {
        match clause.kind() {
            "for_in_clause" => {
                let mut right_cursor = clause.walk();
                let iter_expr = clause
                    .children_by_field_name("right", &mut right_cursor)
                    .find(|n| n.is_named());
                let collection =
                    iter_expr.map(|expr| py_flow_expr(expr, src, fn_sym, strings, scope, sink));
                if let Some(left) = clause.child_by_field_name("left") {
                    let first = py_bind_loop_targets(left, collection, src, strings, scope, sink);
                    if loop_var.is_none() {
                        loop_var = first;
                    }
                }
            }
            "if_clause" => {
                let mut clause_cursor = clause.walk();
                for expr in clause.named_children(&mut clause_cursor) {
                    py_flow_expr(expr, src, fn_sym, strings, scope, sink);
                }
            }
            _ => {}
        }
    }
    let mut fill = Vec::new();
    if node.kind() == "dictionary_comprehension" {
        if let Some(pair) = node.child_by_field_name("body") {
            if let Some(key) = pair.child_by_field_name("key") {
                fill.push(py_flow_expr(key, src, fn_sym, strings, scope, sink));
            }
            if let Some(value) = pair.child_by_field_name("value") {
                fill.push(py_flow_expr(value, src, fn_sym, strings, scope, sink));
            }
        }
    } else if let Some(body) = node.child_by_field_name("body") {
        fill.push(py_flow_expr(body, src, fn_sym, strings, scope, sink));
    }
    let new_node = df_push_node(sink, strings, node, DfNodeKind::New, None);
    for value in fill {
        df_edge(sink, value, new_node);
    }
    push_loop(sink, node, loop_var);
    new_node
}

/// Post-order value flow for one expression, returning the node carrying its
/// value. Unhandled shapes recurse and surface the last value-bearing child,
/// or a generic `expr` node. Port of v5 `py_flow_expr`.
fn py_flow_expr(
    node: tree_sitter::Node,
    src: &[u8],
    fn_sym: &str,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src).to_string();
            let read = df_push_node(sink, strings, node, DfNodeKind::VarRead, Some(&name));
            if let Some(binding) = scope.get(&name) {
                df_edge(sink, *binding, read);
            }
            read
        }
        "true" | "false" | "none" | "integer" | "float" | "string" | "concatenated_string" => {
            df_push_node(sink, strings, node, DfNodeKind::Lit, None)
        }
        // f(args) / recv.method(args): each positional argument flows into the
        // call result at its 0-based slot; a keyword argument ALSO lands in
        // `fields` under its name; a member callee flows the receiver in at
        // slot -1; a CAPITALIZED bare callee is a constructor (PEP 8), minted
        // as `new` carrying the type name.
        "call" => {
            let func = node.child_by_field_name("function");
            let mut receiver: Option<NodeRef> = None;
            let mut callee_name = String::new();
            match func.map(|f| f.kind()) {
                Some("identifier") => {
                    callee_name = py_text(func.unwrap(), src).to_string();
                }
                Some("attribute") => {
                    let attr = func.unwrap();
                    if let Some(object) = attr.child_by_field_name("object") {
                        receiver = Some(py_flow_expr(object, src, fn_sym, strings, scope, sink));
                    }
                    if let Some(name) = attr.child_by_field_name("attribute") {
                        callee_name = py_text(name, src).to_string();
                    }
                }
                _ => {
                    if let Some(f) = func {
                        py_flow_expr(f, src, fn_sym, strings, scope, sink);
                    }
                }
            }
            let mut arg_ids: Vec<(Option<String>, NodeRef)> = Vec::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cursor = args.walk();
                for arg in args.named_children(&mut cursor) {
                    match arg.kind() {
                        "keyword_argument" => {
                            let name = arg
                                .child_by_field_name("name")
                                .map(|n| py_text(n, src).to_string());
                            if let Some(value) = arg.child_by_field_name("value") {
                                let value_ref =
                                    py_flow_expr(value, src, fn_sym, strings, scope, sink);
                                arg_ids.push((name, value_ref));
                            }
                        }
                        "dictionary_splat" | "list_splat" => {
                            if let Some(inner) = arg.named_child(0) {
                                let value_ref =
                                    py_flow_expr(inner, src, fn_sym, strings, scope, sink);
                                arg_ids.push((None, value_ref));
                            }
                        }
                        _ => {
                            let value_ref = py_flow_expr(arg, src, fn_sym, strings, scope, sink);
                            arg_ids.push((None, value_ref));
                        }
                    }
                }
            }
            let is_ctor = callee_name.chars().next().is_some_and(|c| c.is_uppercase());
            let call_node = if is_ctor {
                df_push_node(sink, strings, node, DfNodeKind::New, Some(&callee_name))
            } else {
                df_push_node(sink, strings, node, DfNodeKind::CallRes, None)
            };
            if let Some(receiver) = receiver {
                df_edge(sink, receiver, call_node);
                sink.aux.args.push(DfArg {
                    call: call_node,
                    pos: -1,
                    arg: receiver,
                });
            }
            for (pos, (name, value_ref)) in arg_ids.into_iter().enumerate() {
                df_edge(sink, value_ref, call_node);
                sink.aux.args.push(DfArg {
                    call: call_node,
                    pos: pos as i64,
                    arg: value_ref,
                });
                if let Some(name) = name {
                    sink.aux.fields.push(DfField {
                        owner: call_node,
                        name,
                        value: value_ref,
                    });
                }
            }
            call_node
        }
        "attribute" => {
            let object = node
                .child_by_field_name("object")
                .map(|o| py_flow_expr(o, src, fn_sym, strings, scope, sink));
            let name = node
                .child_by_field_name("attribute")
                .map(|a| py_text(a, src).to_string())
                .unwrap_or_default();
            let member = df_push_node(sink, strings, node, DfNodeKind::Member, Some(&name));
            if let Some(object) = object {
                df_edge(sink, object, member);
            }
            member
        }
        "subscript" => {
            let value = node
                .child_by_field_name("value")
                .map(|v| py_flow_expr(v, src, fn_sym, strings, scope, sink));
            let mut cursor = node.walk();
            for sub in node.children_by_field_name("subscript", &mut cursor) {
                py_flow_expr(sub, src, fn_sym, strings, scope, sink);
            }
            let member = df_push_node(sink, strings, node, DfNodeKind::Member, None);
            if let Some(value) = value {
                df_edge(sink, value, member);
            }
            member
        }
        "binary_operator" | "boolean_operator" | "comparison_operator" => {
            let mut cursor = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            let left = kids
                .first()
                .map(|n| py_flow_expr(*n, src, fn_sym, strings, scope, sink));
            let right = kids
                .last()
                .map(|n| py_flow_expr(*n, src, fn_sym, strings, scope, sink));
            let binop = df_push_node(sink, strings, node, DfNodeKind::Binop, None);
            if let Some(left) = left {
                df_edge(sink, left, binop);
            }
            if let Some(right) = right {
                df_edge(sink, right, binop);
            }
            binop
        }
        "not_operator" | "unary_operator" => {
            let inner = node
                .named_child(0)
                .map(|n| py_flow_expr(n, src, fn_sym, strings, scope, sink));
            let unop = df_push_node(sink, strings, node, DfNodeKind::Unop, None);
            if let Some(inner) = inner {
                df_edge(sink, inner, unop);
            }
            unop
        }
        // `<cons> if <cond> else <alt>`: the value is EITHER branch; the
        // condition is walked for its own facts, never edged in as a value.
        "conditional_expression" => {
            let mut cursor = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cursor).collect();
            let cons = kids
                .first()
                .map(|n| py_flow_expr(*n, src, fn_sym, strings, scope, sink));
            if let Some(cond) = kids.get(1) {
                py_flow_expr(*cond, src, fn_sym, strings, scope, sink);
            }
            let alt = kids
                .get(2)
                .map(|n| py_flow_expr(*n, src, fn_sym, strings, scope, sink));
            let cond_node = df_push_node(sink, strings, node, COND, None);
            if let Some(cons) = cons {
                df_edge(sink, cons, cond_node);
            }
            if let Some(alt) = alt {
                df_edge(sink, alt, cond_node);
            }
            cond_node
        }
        "parenthesized_expression" | "await" => match node.named_child(0) {
            Some(inner) => py_flow_expr(inner, src, fn_sym, strings, scope, sink),
            None => df_push_node(sink, strings, node, DfNodeKind::Expr, None),
        },
        // PEP 572 `name := value`: a binding whose VALUE is the binding, so the
        // enclosing expression consumes the bound slot.
        "named_expression" => {
            let value = node
                .child_by_field_name("value")
                .or_else(|| node.named_child(1));
            let rhs = value.map(|expr| py_flow_expr(expr, src, fn_sym, strings, scope, sink));
            let target = node
                .child_by_field_name("name")
                .or_else(|| node.named_child(0))
                .filter(|target| target.kind() == "identifier");
            match target {
                Some(target) => {
                    let name = py_text(target, src).to_string();
                    let bind =
                        df_push_node(sink, strings, target, DfNodeKind::LetBind, Some(&name));
                    if let Some(rhs) = rhs {
                        df_edge(sink, rhs, bind);
                    }
                    scope.insert(name, bind);
                    bind
                }
                None => {
                    rhs.unwrap_or_else(|| df_push_node(sink, strings, node, DfNodeKind::Expr, None))
                }
            }
        }
        // `lambda params: body`: its OWN fn scope under `{fn_sym}::closure::
        // {row}_{col}` (tree-sitter's 0-based start), params + one `ret` at the
        // lambda's END for the body value; the enclosing scope is shared so
        // captures resolve. The `closure` VALUE node carries the sym.
        "lambda" => {
            let pos = node.start_position();
            let lam_sym = format!("{fn_sym}::closure::{}_{}", pos.row, pos.column);
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for (index, param) in params.named_children(&mut cursor).enumerate() {
                    let (name_opt, _ty) = py_param_name_and_type(param, src);
                    if let Some(pname) = name_opt {
                        let node_ref =
                            df_push_node(sink, strings, param, DfNodeKind::Param, Some(&pname));
                        sink.aux.params.push(DfParam {
                            node: node_ref,
                            pos: index as u32,
                        });
                        scope.insert(pname, node_ref);
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                let value = py_flow_expr(body, src, &lam_sym, strings, scope, sink);
                let end = node.end_byte() as u32;
                let ret = df_push(sink, strings, end, end, DfNodeKind::Ret, None);
                df_edge(sink, value, ret);
            }
            df_push_node(sink, strings, node, DfNodeKind::Closure, Some(&lam_sym))
        }
        "list_comprehension"
        | "set_comprehension"
        | "generator_expression"
        | "dictionary_comprehension" => {
            py_comprehension_flow(node, src, fn_sym, strings, scope, sink)
        }
        "list" | "set" | "tuple" => {
            let mut cursor = node.walk();
            let values: Vec<NodeRef> = node
                .named_children(&mut cursor)
                .map(|element| py_flow_expr(element, src, fn_sym, strings, scope, sink))
                .collect();
            let new_node = df_push_node(sink, strings, node, DfNodeKind::New, None);
            for value in values {
                df_edge(sink, value, new_node);
            }
            new_node
        }
        // `{...}`: each pair's value flows into a `new` node; a plain-string
        // key is the field name; `**spread` lands under the `..` pseudo-field.
        "dictionary" => {
            let mut cursor = node.walk();
            let mut filled: Vec<(String, NodeRef)> = Vec::new();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "pair" => {
                        let key = child.child_by_field_name("key");
                        let value = child
                            .child_by_field_name("value")
                            .map(|v| py_flow_expr(v, src, fn_sym, strings, scope, sink));
                        let name = key
                            .filter(|k| k.kind() == "string")
                            .map(|k| py_text(k, src).trim_matches(['"', '\'']).to_string())
                            .unwrap_or_default();
                        if let Some(value) = value {
                            filled.push((name, value));
                        }
                    }
                    "dictionary_splat" => {
                        if let Some(inner) = child.named_child(0) {
                            let value = py_flow_expr(inner, src, fn_sym, strings, scope, sink);
                            filled.push(("..".to_string(), value));
                        }
                    }
                    _ => {}
                }
            }
            let new_node = df_push_node(sink, strings, node, DfNodeKind::New, None);
            for (name, value) in filled {
                df_edge(sink, value, new_node);
                if !name.is_empty() {
                    sink.aux.fields.push(DfField {
                        owner: new_node,
                        name,
                        value,
                    });
                }
            }
            new_node
        }
        _ => {
            let mut cursor = node.walk();
            let mut last = None;
            for child in node.named_children(&mut cursor) {
                last = Some(py_flow_expr(child, src, fn_sym, strings, scope, sink));
            }
            last.unwrap_or_else(|| df_push_node(sink, strings, node, DfNodeKind::Expr, None))
        }
    }
}

// ── PythonSource: cst via ast-grep + type/call/df via tree-sitter-python ────

/// `matches` = `.py`/`.pyi` (SupportLang maps both to Python). cst via ast-grep;
/// type/call/df via one tree-sitter-python parse.
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

        // type/call/df via ONE tree-sitter-python parse (masked). Byte spans
        // come straight off the tree-sitter nodes. A failed parse leaves all
        // three None.
        let mut types = None;
        let mut call = None;
        let mut df = None;
        if mask.types || mask.call || mask.df {
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
                    if mask.df {
                        let span = trace::family_span("python", "df");
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

// ── Resolve<TypeF>: the go arm's twin. Candidates in, no AST. A `to` naming
// no corpus node keeps a ZERO dst leg (text stays text); same-file entity
// first, else a unique corpus site. ─────────────────────────────────────────

impl PythonSource {
    /// The deduped, deterministically-ordered candidate list; `resolve` emits
    /// one edge per candidate in EXACTLY this order.
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

fn resolve_type_dst(
    types: &FamilyBundle<TypeF>,
    strings: &Strings,
    index: Option<&DefIndex>,
    name: &str,
) -> Option<(ContentId, Span, ResolutionOrigin)> {
    let same_file = types
        .nodes
        .iter()
        .find(|node| node.name.is_some_and(|id| strings.lookup(id) == name));
    if let (Some(node), Some(index)) = (same_file, index) {
        return corpus_defs(index, name)
            .iter()
            .find(|site| site.span == node.span)
            .map(|site| (site.blob.clone(), site.span, ResolutionOrigin::SameFile));
    }
    let sites = index.map(|index| corpus_defs(index, name)).unwrap_or(&[]);
    match sites {
        [only] => Some((only.blob.clone(), only.span, ResolutionOrigin::CorpusUnique)),
        _ => None,
    }
}

impl Resolve<TypeF> for PythonSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let index = cx.indexes.def_index.get();
        let mut edges = Vec::new();
        for candidate in PythonSource::type_edge_candidates(output) {
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

// ── Resolve<CallF>: the go arm's twin. NameResolve (same-file wins, else a
// unique corpus blob, else no row) with the scip-python override leg when the
// corpus scip index and a reader are both present. A site outside every
// function def falls to the module def node minted by `project_call`
// (CallKind::Module): the module is the caller of top-level code. ────────────

impl PythonSource {
    pub fn call_name_match(
        output: &ExtractOutput,
        index: &DefIndex,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let call = output.call.as_ref()?;
        let sites = corpus_defs(index, callee);
        let mut blobs: Vec<ContentId> = Vec::new();
        for site in sites.iter().filter(|s| s.family == FamilyTag::Call) {
            if !blobs.contains(&site.blob) {
                blobs.push(site.blob.clone());
            }
        }
        // Every leg below needs the name to own exactly one corpus file: a
        // duplicate def anywhere makes any dst, same-file or not, a guess.
        if blobs.len() == 1 {
            if let Some(r) = def_named(call, &output.strings, callee) {
                let span = call.node(r).span;
                if let Some(site) = sites
                    .iter()
                    .find(|site| site.span == span && site.family == FamilyTag::Call)
                {
                    return Some((site.blob.clone(), site.span));
                }
            }
        }
        // `Callee()` is a call to `Callee.__init__` (PyCG's oracle semantics);
        // the class itself is a TypeF def and never the oracle's edge target.
        // Same file first, then a unique corpus blob. A class with no
        // `__init__` resolves to nothing (imported_call_without_init).
        if let Some((blob, span)) = init_of_class(output, index, callee) {
            return Some((blob, span));
        }
        // Bare-name fallback over CALL defs only: a class name reaching here
        // took the ctor leg above; resolving it to the TypeF class def minted
        // class-name rows the PyCG oracle never has (37/136 rows, precision
        // 71.32 -> floor breach, 2026-08-31 rescore).
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites
            .iter()
            .find(|s| s.family == FamilyTag::Call && s.blob == *blob)
            .unwrap_or(&sites[0]);
        Some((blob.clone(), site.span))
    }
}

/// The `__init__` call def of a class named `callee`, same file first, then a
/// unique corpus blob. `None` when the name is not a class or the class has no
/// `__init__`.
fn init_of_class(
    output: &ExtractOutput,
    index: &DefIndex,
    callee: &str,
) -> Option<(ContentId, Span)> {
    // Same file: a TypeF class def named `callee`, and a Method call def named
    // `__init__` whose span sits inside the class span, else the first base
    // class (left to right, depth first) that carries one.
    if let Some(types) = &output.types {
        if types.nodes.iter().any(|n| {
            n.name
                .map_or(false, |id| output.strings.lookup(id) == callee)
        }) {
            return same_file_init(output, index, callee, &mut Vec::new());
        }
    }
    // Cross-file: corpus TypeF defs named `callee`; the `__init__` call def in
    // the same blob whose span the class span contains. One unique blob wins.
    let mut hits: Vec<(ContentId, Span)> = Vec::new();
    for class in corpus_defs(index, callee)
        .iter()
        .filter(|s| s.family == FamilyTag::Type)
    {
        for init in corpus_defs(index, "__init__")
            .iter()
            .filter(|s| s.family == FamilyTag::Call && s.blob == class.blob)
        {
            if init.span.start >= class.span.start && init.span.end() <= class.span.end() {
                if !hits.iter().any(|(b, _)| *b == class.blob) {
                    hits.push((class.blob.clone(), init.span));
                }
                break;
            }
        }
    }
    let mut blobs: Vec<&ContentId> = Vec::new();
    for (blob, _) in &hits {
        if !blobs.contains(&blob) {
            blobs.push(blob);
        }
    }
    let [blob] = blobs.as_slice() else {
        return None;
    };
    hits.iter()
        .find(|(b, _)| b == *blob)
        .map(|(b, s)| (b.clone(), *s))
}

/// The `__init__` call def a same-file class named `class_name` constructs
/// with: its own, else its bases' in declaration order (cycle-guarded).
fn same_file_init(
    output: &ExtractOutput,
    index: &DefIndex,
    class_name: &str,
    seen: &mut Vec<String>,
) -> Option<(ContentId, Span)> {
    if seen.iter().any(|s| s == class_name) {
        return None;
    }
    seen.push(class_name.to_string());
    let strings = &output.strings;
    let types = output.types.as_ref()?;
    let class_span = types
        .nodes
        .iter()
        .find(|n| n.name.map_or(false, |id| strings.lookup(id) == class_name))
        .map(|n| n.span)?;
    let init = output.call.as_ref().and_then(|call| {
        call.nodes.iter().find(|n| {
            n.name.map_or(false, |id| strings.lookup(id) == "__init__")
                && n.span.start >= class_span.start
                && n.span.end() <= class_span.end()
        })
    });
    if let Some(init) = init {
        let span = init.span;
        return corpus_defs(index, "__init__")
            .iter()
            .find(|s| s.span == span)
            .map(|site| (site.blob.clone(), site.span));
    }
    types
        .aux
        .candidates
        .iter()
        .filter(|c| c.owner == class_span && c.kind == TypeEdgeKind::Impl)
        .find_map(|c| same_file_init(output, index, strings.lookup(c.to), seen))
}

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

impl Resolve<CallF> for PythonSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(def_index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
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
                let blob = own_blob(cx, output)?;
                let doc_ix = joined
                    .iter()
                    .position(|j| j.as_ref().is_some_and(|(b, _)| *b == blob))?;
                Some((index, joined, doc_ix))
            });
        let own = own_blob(cx, output);
        let mut resolver = PyResolver {
            output,
            index: def_index,
            call,
            own: own.clone(),
            decor_binds: Vec::new(),
            decor_extras: Vec::new(),
            active: std::cell::RefCell::new(Vec::new()),
            param_memo: std::cell::RefCell::new(std::collections::HashMap::new()),
            param_scratch: std::cell::RefCell::new(std::collections::HashMap::new()),
            callee_memo: std::cell::RefCell::new(std::collections::HashMap::new()),
            callee_scratch: std::cell::RefCell::new(std::collections::HashMap::new()),
            cuts: std::cell::Cell::new(0),
        };
        // A decorator whose def's single return names a same-file def rebinds
        // the decorated name to it (`func()` then calls the wrapper). A
        // `@factory()` decorator's def may return the APPLIED decorator: that
        // application is its own call edge, and the applied def carries the
        // wrapper check.
        for decor in &call.aux.py_decorators {
            let callee = output.strings.lookup(decor.callee);
            let mut seen = Vec::new();
            let Some((blob, dspan)) = resolver.name_target(callee, decor.span, &mut seen) else {
                continue;
            };
            if Some(&blob) != own.as_ref() {
                continue;
            }
            let mut applied = (blob, dspan);
            if decor.call_expr {
                // `@factory()`: the def returns a bare identifier naming a
                // same-file def -- the applied decorator is THAT def.
                if let Some(ret) = call.aux.py_returns.iter().find(|r| r.def == dspan) {
                    let value = output.strings.lookup(ret.value);
                    let mut ret_seen = vec![callee.to_string()];
                    if let Some(t) = resolver.name_target(value, decor.span, &mut ret_seen) {
                        resolver.decor_extras.push((decor.span, t.clone()));
                        applied = t;
                    }
                }
            }
            let Some(ret) = call.aux.py_returns.iter().find(|r| r.def == applied.1) else {
                continue;
            };
            let wrapper = output.strings.lookup(ret.value);
            if wrapper == output.strings.lookup(decor.decorated) {
                continue;
            }
            // The wrapper must be a same-file call def; otherwise no bind.
            if PythonSource::call_name_match(output, def_index, wrapper).is_none() {
                continue;
            }
            resolver.decor_binds.push(PyBind {
                span: decor.span,
                target: decor.decorated,
                key: None,
                value: Some(ret.value),
            });
        }
        let mut edges = Vec::new();
        for site in &call.aux.sites {
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            resolver.clear_scratch();
            let callee = output.strings.lookup(site.callee);
            let push = |edges: &mut Vec<ProjectEdge<CallF>>,
                        dst_blob: ContentId,
                        dst_span: Span,
                        kind: CallEdgeKind,
                        origin: ResolutionOrigin| {
                edges.push(
                    ProjectEdge::new(caller, dst_blob, dst_span, kind, origin)
                        .with_call_site(site.span),
                );
            };
            // Tier order: scip (compiler) -> dynamic shapes -> name-match.
            // A wrapper bind or a shadowing parameter is what the name means
            // there; the corpus name-match is the last resort.
            let scip_t = scip.as_ref().and_then(|(index, joined, doc_ix)| {
                scip_call_target(index, joined, *doc_ix, site, callee, def_index)
            });
            let scip_hit = scip_t.is_some();
            if let Some((dst_blob, dst_span, _)) = scip_t {
                push(
                    &mut edges,
                    dst_blob,
                    dst_span,
                    CallEdgeKind::ScipOverride,
                    ResolutionOrigin::Scip,
                );
            } else if let Some((dst_blob, dst_span, origin)) =
                resolver.resolve_site(site, &mut Vec::new())
            {
                push(
                    &mut edges,
                    dst_blob,
                    dst_span,
                    CallEdgeKind::NameResolve,
                    origin,
                );
            } else if !resolver.shadowed(callee, site.span) {
                // Shadowed names stay dropped: the corpus match must not
                // resurrect the module-level binding resolve_site declined.
                if let Some((dst_blob, dst_span)) =
                    PythonSource::call_name_match(output, def_index, callee)
                {
                    push(
                        &mut edges,
                        dst_blob,
                        dst_span,
                        CallEdgeKind::NameResolve,
                        ResolutionOrigin::CorpusUnique,
                    );
                }
            }
            // A calling builtin (`map(f, xs)`) with no def of its own in the
            // corpus: each named argument that reaches a def is called.
            if !scip_hit
                && PY_CALLING_BUILTINS.contains(&callee)
                && PythonSource::call_name_match(output, def_index, callee).is_none()
            {
                let mut targets: Vec<(ContentId, Span)> = Vec::new();
                for arg in call.aux.py_args.iter().filter(|a| a.site == site.span) {
                    let value = output.strings.lookup(arg.value);
                    if let Some(t) = resolver.name_target(value, site.span, &mut Vec::new()) {
                        if !targets.contains(&t) {
                            targets.push(t);
                        }
                    }
                }
                for (dst_blob, dst_span) in targets {
                    push(
                        &mut edges,
                        dst_blob,
                        dst_span,
                        CallEdgeKind::NameResolve,
                        ResolutionOrigin::Param,
                    );
                }
            }
            // The applied decorator of a `@factory()` site: a second edge.
            if let Some((_, t)) = resolver
                .decor_extras
                .iter()
                .find(|(span, _)| *span == site.span)
            {
                push(
                    &mut edges,
                    t.0.clone(),
                    t.1,
                    CallEdgeKind::NameResolve,
                    ResolutionOrigin::Decorator,
                );
            }
        }
        edges
    }
}

/// The same-file dynamic-shape resolver: reads the python-only aux rows
/// (`py_binds`, `py_call_binds`, `py_args`, `py_params`, `py_defaults`,
/// `py_returns`, `py_sub_calls`, `py_ret_calls`) and answers the sites a bare
/// callee name cannot. Every leg is unique-candidate or syntactic-only: a
/// shape that cannot be resolved honestly resolves to nothing.
struct PyResolver<'a> {
    output: &'a ExtractOutput,
    index: &'a DefIndex,
    call: &'a FamilyBundle<CallF>,
    /// This file's blob, when its own bytes are part of the run's file set.
    own: Option<ContentId>,
    /// Binds synthesized from decorator applications (decorated name ->
    /// wrapper name), consulted BEFORE the file's own rows: the outermost
    /// decorator's return is what the decorated name means from then on.
    decor_binds: Vec<PyBind>,
    /// `@factory()` sites: the applied decorator's (blob, span), emitted as a
    /// second edge on the same site.
    decor_extras: Vec<(Span, (ContentId, Span))>,
    /// Call-bind sites and param-rule defs on the active resolution path;
    /// a re-entry on the same span is a cycle and resolves to nothing, and a
    /// path deeper than `PY_RESOLVE_DEPTH` stops.
    active: std::cell::RefCell<Vec<Span>>,
    /// Complete answers of `param_target` by (callee, site span) and of
    /// `callee_def` by site span: the param rule scans every site and recurses
    /// per bound-name callee, so without a memo the walk is factorial in the
    /// defs on the path (click/core.py hung past 15 s, then overflowed a
    /// worker stack). An answer computed under a cycle cut is complete only
    /// for the top-level site being resolved: it lives in the scratch maps,
    /// cleared per site, where an in-progress entry reads as nothing.
    param_memo:
        std::cell::RefCell<std::collections::HashMap<(String, Span), Option<(ContentId, Span)>>>,
    param_scratch:
        std::cell::RefCell<std::collections::HashMap<(String, Span), Option<(ContentId, Span)>>>,
    callee_memo: std::cell::RefCell<std::collections::HashMap<Span, Option<(ContentId, Span)>>>,
    callee_scratch: std::cell::RefCell<std::collections::HashMap<Span, Option<(ContentId, Span)>>>,
    /// Bumped on every cycle cut (active-path re-entry, depth cap, scratch
    /// read); an answer whose computation bumped it is not complete.
    cuts: std::cell::Cell<u64>,
}

const PY_RESOLVE_DEPTH: usize = 12;

/// What a name is bound to at a point: another value name, or the result of
/// the call whose site (function-node span) is carried.
#[derive(Clone, Copy, Debug)]
enum PyBound {
    Name(crate::shape::NameId),
    Call(Span),
}

/// The builtins that call the callable they are handed: every named
/// argument of such a site is a callee of the site's caller.
const PY_CALLING_BUILTINS: &[&str] = &["map", "filter"];

impl<'a> PyResolver<'a> {
    /// The active binding for (`target`, `key`): the LAST row in file order
    /// with span.start < at.start, else the last row overall (a def body can
    /// sit textually before the module-level binding that is live when the
    /// call runs). A KILL row (value None) clears the name at its own byte
    /// order; a call-result row or an element row at the same span outranks
    /// the container-level KILL beside it. `within` restricts the rows to one
    /// def's span (a def-local binding).
    fn latest_bind(
        &self,
        target: &str,
        key: Option<&str>,
        at: Span,
        within: Option<Span>,
    ) -> Option<PyBound> {
        let strings = &self.output.strings;
        let inside = |span: Span| {
            within.map_or(true, |def| {
                span.start >= def.start && span.end() <= def.end()
            })
        };
        let rows = self
            .call
            .aux
            .py_binds
            .iter()
            .chain(self.decor_binds.iter())
            .filter(|bind| strings.lookup(bind.target) == target)
            .filter(|bind| match (&bind.key, key) {
                (None, None) => true,
                (Some(k), Some(want)) => strings.lookup(*k) == want,
                // A container-level KILL (the name rebound, or mutated in
                // place) clears every element binding before it.
                (None, Some(_)) => bind.value.is_none(),
                _ => false,
            })
            .filter(|bind| inside(bind.span))
            .map(|bind| {
                let rank = u8::from(bind.key.is_some());
                (bind.span, rank, bind.value.map(PyBound::Name))
            })
            .chain(
                self.call
                    .aux
                    .py_call_binds
                    .iter()
                    .filter(|_| key.is_none())
                    .filter(|bind| strings.lookup(bind.target) == target)
                    .filter(|bind| inside(bind.span))
                    .map(|bind| (bind.span, 1u8, Some(PyBound::Call(bind.site)))),
            );
        let mut latest_before: Option<(Span, u8, Option<PyBound>)> = None;
        let mut latest: Option<(Span, u8, Option<PyBound>)> = None;
        for row in rows {
            let order = (row.0.start, row.1);
            if row.0.start < at.start {
                match latest_before {
                    Some(prev) if (prev.0.start, prev.1) >= order => {}
                    _ => latest_before = Some(row),
                }
            }
            match latest {
                Some(prev) if (prev.0.start, prev.1) >= order => {}
                _ => latest = Some(row),
            }
        }
        latest_before.or(latest)?.2
    }

    /// A name to a def: corpus name-match first, then the same-file binding
    /// chain (cycle-guarded).
    fn name_target(
        &self,
        name: &str,
        at: Span,
        seen: &mut Vec<String>,
    ) -> Option<(ContentId, Span)> {
        if seen.iter().any(|s| s == name) {
            return None;
        }
        seen.push(name.to_string());
        if let Some(start) = py_lambda_value_start(name) {
            let own = self.own.clone()?;
            let node = self
                .call
                .nodes
                .iter()
                .find(|n| n.span.start == start && n.kind == CallKind::Lambda)?;
            return Some((own, node.span));
        }
        if let Some(t) = PythonSource::call_name_match(self.output, self.index, name) {
            return Some(t);
        }
        let bound = self.latest_bind(name, None, at, None)?;
        self.bound_target(bound, at, seen)
    }

    /// One binding to a def: a value name resolves as a name; a call result
    /// resolves through the call's def and its single return.
    fn bound_target(
        &self,
        bound: PyBound,
        at: Span,
        seen: &mut Vec<String>,
    ) -> Option<(ContentId, Span)> {
        match bound {
            PyBound::Name(value) => self.name_target(self.output.strings.lookup(value), at, seen),
            PyBound::Call(site_span) => {
                let (blob, dspan) = self.call_site_def(site_span)?;
                self.returned_target(blob, dspan, site_span, at)
            }
        }
    }

    /// The def the call site at `site_span` (function-node span) resolves to,
    /// cycle-guarded on the site.
    fn call_site_def(&self, site_span: Span) -> Option<(ContentId, Span)> {
        if self.active.borrow().contains(&site_span)
            || self.active.borrow().len() >= PY_RESOLVE_DEPTH
        {
            self.cut();
            return None;
        }
        let site = self.call.aux.sites.iter().find(|s| s.span == site_span)?;
        self.active.borrow_mut().push(site_span);
        let found = self.resolve_site(site, &mut Vec::new());
        self.active.borrow_mut().pop();
        found.map(|(blob, span, _)| (blob, span))
    }

    /// What the same-file def at `dspan` hands back through its single
    /// return: a def-local binding of the returned name first, then the name
    /// itself, then the argument the call at `inner` passed for a returned
    /// parameter.
    fn returned_target(
        &self,
        blob: ContentId,
        dspan: Span,
        inner: Span,
        at: Span,
    ) -> Option<(ContentId, Span)> {
        if Some(&blob) != self.own.as_ref() {
            return None;
        }
        let strings = &self.output.strings;
        let ret = self.call.aux.py_returns.iter().find(|r| r.def == dspan)?;
        let value = strings.lookup(ret.value);
        if let Some(local) = self.latest_bind(
            value,
            None,
            Span {
                start: dspan.end(),
                len: 0,
            },
            Some(dspan),
        ) {
            if let Some(t) = self.bound_target(local, at, &mut vec![value.to_string()]) {
                return Some(t);
            }
        }
        if let Some(t) = self.name_target(value, at, &mut Vec::new()) {
            return Some(t);
        }
        let p = self
            .call
            .aux
            .py_params
            .iter()
            .find(|p| p.def == dspan && strings.lookup(p.name) == value)?;
        let arg = self
            .call
            .aux
            .py_args
            .iter()
            .find(|a| a.site == inner && a.kw.is_none() && a.pos == p.pos as i64)?;
        self.name_target(strings.lookup(arg.value), at, &mut Vec::new())
    }

    /// The element binding `base[key]` at a point: a keyed row on the base;
    /// else the base as an alias of another container, a parameter of the
    /// enclosing def whose slot every same-file call fills with one container
    /// name, or a call result whose def returns a container it bound locally.
    fn element_bound(
        &self,
        base: &str,
        key: &str,
        at: Span,
        seen: &mut Vec<String>,
    ) -> Option<PyBound> {
        if seen.iter().any(|s| s == base) {
            return None;
        }
        seen.push(base.to_string());
        if let Some(bound) = self.latest_bind(base, Some(key), at, None) {
            return Some(bound);
        }
        if let Some(args) = self.param_args(base, at) {
            let mut found: Vec<PyBound> = Vec::new();
            for (name, arg_at) in args {
                if let Some(bound) = self.element_bound(&name, key, arg_at, seen) {
                    if !found.iter().any(|prev| py_bound_eq(*prev, bound)) {
                        found.push(bound);
                    }
                }
            }
            return match found.as_slice() {
                [one] => Some(*one),
                _ => None,
            };
        }
        match self.latest_bind(base, None, at, None)? {
            PyBound::Name(alias) => {
                self.element_bound(self.output.strings.lookup(alias), key, at, seen)
            }
            PyBound::Call(site_span) => {
                let (blob, dspan) = self.call_site_def(site_span)?;
                if Some(&blob) != self.own.as_ref() {
                    return None;
                }
                let ret = self.call.aux.py_returns.iter().find(|r| r.def == dspan)?;
                let value = self.output.strings.lookup(ret.value);
                self.latest_bind(
                    value,
                    Some(key),
                    Span {
                        start: dspan.end(),
                        len: 0,
                    },
                    Some(dspan),
                )
            }
        }
    }

    /// One resolved site, by shape: wrapper bind -> param -> corpus name-match
    /// -> alias, else subscript element, else return-of-call. A callee
    /// shadowed by an enclosing def's parameter never falls through to the
    /// module-level alias: the parameter is what that name means there.
    fn resolve_site(
        &self,
        site: &CallSite,
        visited: &mut Vec<Span>,
    ) -> Option<(ContentId, Span, ResolutionOrigin)> {
        if visited.contains(&site.span) {
            return None;
        }
        visited.push(site.span);
        let strings = &self.output.strings;
        let callee = strings.lookup(site.callee);
        if !callee.is_empty() {
            if let Some(t) = self.decor_target(callee, site.span) {
                return Some((t.0, t.1, ResolutionOrigin::Decorator));
            }
            if let Some(t) = self.param_target(callee, site.span) {
                return Some((t.0, t.1, ResolutionOrigin::Param));
            }
            if self.shadowed(callee, site.span) {
                return None;
            }
            return PythonSource::call_name_match(self.output, self.index, callee)
                .map(|(blob, span)| (blob, span, ResolutionOrigin::CorpusUnique))
                .or_else(|| {
                    self.alias_target(callee, site.span)
                        .map(|(blob, span)| (blob, span, ResolutionOrigin::AliasChain))
                });
        }
        if let Some(sub) = self
            .call
            .aux
            .py_sub_calls
            .iter()
            .find(|s| s.span == site.span)
        {
            let base = strings.lookup(sub.base);
            let key = strings.lookup(sub.key);
            let bound = self.element_bound(base, key, site.span, &mut Vec::new())?;
            return self
                .bound_target(bound, site.span, &mut Vec::new())
                .map(|(blob, span)| (blob, span, ResolutionOrigin::Subscript));
        }
        if let Some(rc) = self
            .call
            .aux
            .py_ret_calls
            .iter()
            .find(|rc| rc.span == site.span)
        {
            return self
                .retcall_target(rc, visited)
                .map(|(blob, span)| (blob, span, ResolutionOrigin::ReturnCall));
        }
        None
    }

    /// The wrapper rebind for a decorated name, when its outermost decorator's
    /// def returns a same-file def other than the decorated one.
    fn decor_target(&self, callee: &str, at: Span) -> Option<(ContentId, Span)> {
        let strings = &self.output.strings;
        let decor = self
            .call
            .aux
            .py_decorators
            .iter()
            .filter(|d| strings.lookup(d.decorated) == callee && d.span.start < at.start)
            .max_by_key(|d| d.span.start)?;
        let bind = self.decor_binds.iter().find(|b| b.span == decor.span)?;
        let value = strings.lookup(bind.value?);
        self.name_target(value, decor.span, &mut vec![callee.to_string()])
    }

    /// The binding for `name`, resolved as a value (one hop; deeper chains go
    /// through `name_target`).
    fn alias_target(&self, name: &str, at: Span) -> Option<(ContentId, Span)> {
        let bound = self.latest_bind(name, None, at, None)?;
        self.bound_target(bound, at, &mut vec![name.to_string()])
    }

    /// `callee` names a parameter of an enclosing def (tightest cover first).
    fn shadowed(&self, callee: &str, site_span: Span) -> bool {
        let strings = &self.output.strings;
        self.call.aux.py_params.iter().any(|p| {
            p.def.start <= site_span.start
                && site_span.end() <= p.def.end()
                && strings.lookup(p.name) == callee
        })
    }

    /// The def a call site's callee names, for the param rule's "every call
    /// to this def" scan: a parameter-bound callee, else the name itself or
    /// its binding chain.
    fn callee_def(&self, site: &CallSite) -> Option<(ContentId, Span)> {
        if let Some(found) = self.callee_memo.borrow().get(&site.span) {
            return found.clone();
        }
        if let Some(found) = self.callee_scratch.borrow().get(&site.span) {
            self.cut();
            return found.clone();
        }
        self.callee_scratch.borrow_mut().insert(site.span, None);
        let cuts_before = self.cuts.get();
        let callee = self.output.strings.lookup(site.callee);
        let found = if callee.is_empty() {
            None
        } else if let Some(t) = self.param_target(callee, site.span) {
            Some(t)
        } else if self.shadowed(callee, site.span) {
            None
        } else {
            self.name_target(callee, site.span, &mut Vec::new())
        };
        if self.cuts.get() == cuts_before {
            self.callee_memo
                .borrow_mut()
                .insert(site.span, found.clone());
        }
        self.callee_scratch
            .borrow_mut()
            .insert(site.span, found.clone());
        found
    }

    /// One cycle cut on the active path.
    fn cut(&self) {
        self.cuts.set(self.cuts.get() + 1);
    }

    /// Drop the per-site scratch answers before resolving the next site.
    fn clear_scratch(&self) {
        self.param_scratch.borrow_mut().clear();
        self.callee_scratch.borrow_mut().clear();
    }

    /// The param rule's inputs: `callee` names a parameter of an enclosing
    /// def (tightest cover first), and the answer is every (argument name,
    /// site) that same-file calls to that def pass in the slot, through the
    /// def's own name or a callee bound to it; when no call fills the slot,
    /// the parameter's bare-identifier default. None when `callee` is no
    /// parameter, or the def is already on the active path (a cycle).
    fn param_args(&self, callee: &str, site_span: Span) -> Option<Vec<(String, Span)>> {
        let strings = &self.output.strings;
        let call = self.call;
        let mut enclosing: Vec<(Span, String)> = call
            .nodes
            .iter()
            .filter(|n| {
                n.span.start <= site_span.start
                    && site_span.end() <= n.span.end()
                    && n.name.is_some()
            })
            .map(|n| (n.span, strings.lookup(n.name.unwrap()).to_string()))
            .collect();
        enclosing.sort_by_key(|(span, _)| (span.end() - span.start, span.start));
        enclosing.dedup();
        for (dspan, dname) in enclosing {
            let Some(param) = call
                .aux
                .py_params
                .iter()
                .find(|p| p.def == dspan && strings.lookup(p.name) == callee)
            else {
                continue;
            };
            if self.active.borrow().contains(&dspan)
                || self.active.borrow().len() >= PY_RESOLVE_DEPTH
            {
                self.cut();
                return None;
            }
            self.active.borrow_mut().push(dspan);
            let mut args: Vec<(String, Span)> = Vec::new();
            for s in &call.aux.sites {
                if s.span == site_span {
                    continue;
                }
                let site_callee = strings.lookup(s.callee);
                let calls_def = site_callee == dname
                    || (self.is_bound_name(site_callee)
                        && self.callee_def(s).is_some_and(|(blob, span)| {
                            Some(&blob) == self.own.as_ref() && span == dspan
                        }));
                if !calls_def {
                    continue;
                }
                for arg in &call.aux.py_args {
                    if arg.site != s.span {
                        continue;
                    }
                    let kw_hit = arg.kw.is_some_and(|kw| strings.lookup(kw) == callee);
                    let pos_hit = arg.kw.is_none() && arg.pos == param.pos as i64;
                    if kw_hit || pos_hit {
                        args.push((strings.lookup(arg.value).to_string(), s.span));
                    }
                }
            }
            self.active.borrow_mut().pop();
            if args.is_empty() {
                if let Some(default) = call
                    .aux
                    .py_defaults
                    .iter()
                    .find(|d| d.def == dspan && strings.lookup(d.name) == callee)
                {
                    args.push((strings.lookup(default.value).to_string(), site_span));
                }
            }
            return Some(args);
        }
        None
    }

    /// Whether `name` is a binding target or a parameter anywhere in the file
    /// (the only callee spellings that can reach a def by a name other than
    /// its own).
    fn is_bound_name(&self, name: &str) -> bool {
        let strings = &self.output.strings;
        self.call
            .aux
            .py_binds
            .iter()
            .any(|b| b.key.is_none() && strings.lookup(b.target) == name)
            || self
                .call
                .aux
                .py_call_binds
                .iter()
                .any(|b| strings.lookup(b.target) == name)
            || self
                .call
                .aux
                .py_params
                .iter()
                .any(|p| strings.lookup(p.name) == name)
    }

    /// The param rule: every same-file call to the enclosing def passes the
    /// same single named function in `callee`'s slot (or nothing does and the
    /// default names one). Emit only on uniqueness; a parameter that exists
    /// but resolves nowhere is shadowed, silent.
    fn param_target(&self, callee: &str, site_span: Span) -> Option<(ContentId, Span)> {
        let memo_key = (callee.to_string(), site_span);
        if let Some(found) = self.param_memo.borrow().get(&memo_key) {
            return found.clone();
        }
        if let Some(found) = self.param_scratch.borrow().get(&memo_key) {
            self.cut();
            return found.clone();
        }
        self.param_scratch
            .borrow_mut()
            .insert(memo_key.clone(), None);
        let cuts_before = self.cuts.get();
        let found = self.param_target_uncached(callee, site_span);
        if self.cuts.get() == cuts_before {
            self.param_memo
                .borrow_mut()
                .insert(memo_key.clone(), found.clone());
        }
        self.param_scratch
            .borrow_mut()
            .insert(memo_key, found.clone());
        found
    }

    fn param_target_uncached(&self, callee: &str, site_span: Span) -> Option<(ContentId, Span)> {
        let args = self.param_args(callee, site_span)?;
        let mut targets: Vec<(ContentId, Span)> = Vec::new();
        for (value, at) in args {
            // The argument may itself be a parameter of the calling def.
            let found = self
                .param_target(&value, at)
                .or_else(|| self.name_target(&value, at, &mut Vec::new()));
            if let Some(t) = found {
                if !targets.contains(&t) {
                    targets.push(t);
                }
            }
        }
        match targets.as_slice() {
            [_] => targets.pop(),
            _ => None,
        }
    }

    /// The return-of-call rule: the inner call resolves to a def whose single
    /// return names a value; see `returned_target`.
    fn retcall_target(&self, rc: &PyRetCall, visited: &mut Vec<Span>) -> Option<(ContentId, Span)> {
        let inner_site = self.call.aux.sites.iter().find(|s| s.span == rc.inner)?;
        let (blob, dspan, _) = self.resolve_site(inner_site, visited)?;
        self.returned_target(blob, dspan, rc.inner, rc.span)
    }
}

fn py_bound_eq(a: PyBound, b: PyBound) -> bool {
    match (a, b) {
        (PyBound::Name(x), PyBound::Name(y)) => x == y,
        (PyBound::Call(x), PyBound::Call(y)) => x == y,
        _ => false,
    }
}
