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
    DfParam, DocFact, DocTag, ProjectEdge, SigSlot, Specifier, SpecifierKind, TypeEdgeCandidate,
    TypeEdgeKind, TypeEntityKind, TypeF, TypeSig,
};
use crate::lang::{AstGrepParser, CstProjector};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::scip::{byte_range_cached, definition_of, join_documents, site_occurrence};
use crate::seams::{
    containing_def_site, corpus_defs, covering_def, def_named, own_blob, DefIndex, Parser,
    Project, Resolve,
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
    let module_span = node_span(root);
    push_entity(sink, strings, module_span, "<module>", MODULE);
    if let Some(text) = py_docstring_of(root, src) {
        push_py_doc(sink, strings, module_span, None, &text);
    }
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
    py_walk_call_defs(root, src, strings, sink, None, false);
    py_walk_call_sites(root, src, strings, sink);
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
                sink.aux.params.push(DfParam { node: node_ref, pos });
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
            let is_ctor = callee_name
                .chars()
                .next()
                .is_some_and(|c| c.is_uppercase());
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
                None => rhs.unwrap_or_else(|| {
                    df_push_node(sink, strings, node, DfNodeKind::Expr, None)
                }),
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
) -> Option<(ContentId, Span)> {
    let same_file = types
        .nodes
        .iter()
        .find(|node| node.name.is_some_and(|id| strings.lookup(id) == name));
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

// ── Resolve<CallF>: the go arm's twin. NameResolve (same-file wins, else a
// unique corpus blob, else no row) with the scip-python override leg when the
// corpus scip index and a reader are both present. A site outside every def
// (module level) emits no row. ──────────────────────────────────────────────

impl PythonSource {
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
        let mut edges = Vec::new();
        for site in &call.aux.sites {
            let Some(caller) = covering_def(call, site.span) else {
                continue;
            };
            let callee = output.strings.lookup(site.callee);
            let name_t = PythonSource::call_name_match(output, def_index, callee);
            let scip_t = scip.as_ref().and_then(|(index, joined, doc_ix)| {
                scip_call_target(index, joined, *doc_ix, site, callee, def_index)
            });
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
