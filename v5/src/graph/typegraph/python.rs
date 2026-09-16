//! Python extractor arm (tree-sitter-python front-end): TypeLang impl,
//! type edges, entities, call defs/sites, docs, dataflow. Pure code motion
//! out of the former single typegraph.rs; zero behavior change.

use std::collections::BTreeSet;

use super::*;

fn py_parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let lang = tree_sitter::Language::new(tree_sitter_python::LANGUAGE);
    if parser.set_language(&lang).is_err() {
        return None;
    }
    parser.parse(content, None)
}

impl TypeLang for PyTypes {
    fn name(&self) -> &'static str {
        "python"
    }
    fn matches(&self, path: &str) -> bool {
        path.ends_with(".py")
    }
    // One tree-sitter parse feeds entities + edges + docs.
    fn extract(&self, file: &str, content: &str) -> TypeFacts {
        let Some(tree) = py_parse(content) else {
            return TypeFacts::default();
        };
        let src = content.as_bytes();
        let root = tree.root_node();
        TypeFacts {
            entities: py_entities_from(root, src, file),
            edges: py_edges_from(root, src),
            docs: py_docs_from(root, src, file),
            ..Default::default()
        }
    }
    // A second tree-sitter parse feeds defs + sites, same shape as Kotlin.
    fn extract_calls(&self, file: &str, content: &str) -> CallFacts {
        let Some(tree) = py_parse(content) else {
            return CallFacts::default();
        };
        let src = content.as_bytes();
        let root = tree.root_node();
        CallFacts {
            defs: py_call_defs_from(root, src, file),
            sites: py_call_sites_from(root, src, file),
        }
    }
    fn extract_dataflow(&self, file: &str, content: &str) -> DataflowFacts {
        let Some(tree) = py_parse(content) else {
            return DataflowFacts::default();
        };
        py_dataflow_from(tree.root_node(), content.as_bytes(), file)
    }
}

fn py_text(node: tree_sitter::Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or("").to_string()
}

fn py_row1(node: tree_sitter::Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// Unwrap a `decorated_definition` down to its inner `class_definition` /
/// `function_definition` (a decorated def still emits its entity/edges/calls;
/// decorator identity rewriting is a stated non-goal). Any other node passes
/// through unchanged.
fn py_unwrap_decorated(node: tree_sitter::Node) -> tree_sitter::Node {
    if node.kind() == "decorated_definition" {
        node.child_by_field_name("definition").unwrap_or(node)
    } else {
        node
    }
}

/// (name, type-annotation node) for one `parameter` subtype. `self`/`cls`
/// receivers are plain `identifier` params like any other — the caller decides
/// whether to skip the first one. Lambda params (always untyped) reuse the
/// `identifier`/`default_parameter`/splat arms; only `typed_parameter`/
/// `typed_default_parameter` (regular-function-only syntax) carry a type.
fn py_param_name_and_type<'t>(
    p: tree_sitter::Node<'t>,
    src: &[u8],
) -> (Option<String>, Option<tree_sitter::Node<'t>>) {
    match p.kind() {
        "identifier" => (Some(py_text(p, src)), None),
        "typed_parameter" => {
            let mut cur = p.walk();
            let name = p
                .named_children(&mut cur)
                .find(|n| n.kind() == "identifier")
                .map(|n| py_text(n, src));
            (name, p.child_by_field_name("type"))
        }
        "default_parameter" => {
            let name = p
                .child_by_field_name("name")
                .filter(|n| n.kind() == "identifier")
                .map(|n| py_text(n, src));
            (name, None)
        }
        "typed_default_parameter" => {
            let name = p.child_by_field_name("name").map(|n| py_text(n, src));
            (name, p.child_by_field_name("type"))
        }
        "list_splat_pattern" | "dictionary_splat_pattern" => {
            let mut cur = p.walk();
            let name = p
                .named_children(&mut cur)
                .find(|n| n.kind() == "identifier")
                .map(|n| py_text(n, src));
            (name, None)
        }
        _ => (None, None),
    }
}

/// Declared PEP-695 type-parameter names (`def f[T](...)` / `class C[T]:`),
/// excluded from ref collection like Kotlin/TS's declared-generic exclusion.
/// Broad by design: every identifier under the `type_parameters` field counts,
/// including bound expressions — over-excluding a rare bound name is harmless.
fn py_collect_type_params(node: tree_sitter::Node, src: &[u8], field: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let Some(tp) = node.child_by_field_name(field) {
        py_collect_identifiers_rec(tp, src, &mut out);
    }
    out
}

fn py_collect_identifiers_rec(node: tree_sitter::Node, src: &[u8], out: &mut BTreeSet<String>) {
    if node.kind() == "identifier" {
        out.insert(py_text(node, src));
    }
    let mut cur = node.walk();
    for c in node.named_children(&mut cur) {
        py_collect_identifiers_rec(c, src, out);
    }
}

/// Collect every type name referenced under an annotation node. `subscript`
/// (`Optional[Foo]`, `list[Bar]`) recurses into BOTH the container (`Optional`/
/// `list`, itself noise-filtered) and each subscripted argument — never the raw
/// subscript text — so `Optional[Foo]` yields `Foo` (and `Optional` is dropped
/// as noise). `attribute` (`typing.Optional`, `module.Class`) keeps only the
/// trailing bare name, matching the callee-resolution convention elsewhere.
/// A string forward-ref (`"Foo"`) is not parsed (non-goal).
fn py_type_refs(
    node: tree_sitter::Node,
    src: &[u8],
    tparams: &BTreeSet<String>,
    out: &mut Vec<String>,
) {
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src);
            if !tparams.contains(&name) && !is_noise_python(&name) {
                out.push(name);
            }
        }
        "attribute" => {
            if let Some(attr) = node.child_by_field_name("attribute") {
                let name = py_text(attr, src);
                if !tparams.contains(&name) && !is_noise_python(&name) {
                    out.push(name);
                }
            }
        }
        "subscript" => {
            if let Some(value) = node.child_by_field_name("value") {
                py_type_refs(value, src, tparams, out);
            }
            let mut cur = node.walk();
            for sub in node.children_by_field_name("subscript", &mut cur) {
                py_type_refs(sub, src, tparams, out);
            }
        }
        "string" | "concatenated_string" => {}
        _ => {
            let mut cur = node.walk();
            for child in node.named_children(&mut cur) {
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

/// Builtin scalar/container names and common `typing` wrapper names: noise for
/// ref collection so `Optional[Foo]`/`list[Bar]` surface the inner `Foo`/`Bar`
/// without also emitting an edge to the wrapper itself.
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

/// Build the arrow `[...A] => B` for a `def`. Each declared parameter is a slot
/// (untyped slots stay empty, matching the TS/Kotlin convention); `self`/`cls`
/// are dropped entirely (not even an empty slot), mirroring Rust's receiver
/// skip so positions align with `type_sig.pos`/`df_param.pos`.
fn py_fn_type(node: tree_sitter::Node, src: &[u8]) -> TypeExpr {
    let tparams = py_collect_type_params(node, src, "type_parameters");
    let mut params = Vec::new();
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cur = plist.walk();
        let mut first = true;
        for p in plist.named_children(&mut cur) {
            if matches!(p.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, type_node) = py_param_name_and_type(p, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            let refs = type_node
                .map(|t| py_type_refs_collect(t, src, &tparams))
                .unwrap_or_default();
            params.push(refs.into_iter().map(TypeRef::Named).collect());
        }
    }
    let ret = node
        .child_by_field_name("return_type")
        .map(|rt| py_type_refs_collect(rt, src, &tparams))
        .unwrap_or_default()
        .into_iter()
        .map(TypeRef::Named)
        .collect();
    TypeExpr { params, ret }
}

// --- Python entity pass: module + class + function/method, functions carrying
// their arrow type like Rust/Kotlin/TS. `class_owner` threads the enclosing
// class's bare name while walking a class body's DIRECT statements (including
// through pass-through compound statements like `if`/`try`/`with`) and resets
// to None on entering ANY function body, so a def nested inside a method is a
// free function, not a second-level method (matches Kotlin/TS). ---

fn py_entities_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<TypeEntity> {
    let mut out = vec![TypeEntity {
        sym: mint_sym(file, EntityKind::Module, "<module>", None),
        name: "<module>".to_string(),
        kind: EntityKind::Module,
        parent: None,
        file: file.to_string(),
        line: 1,
        ty: None,
    }];
    walk_py_entities(root, src, file, None, &mut out);
    out
}

fn walk_py_entities(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    class_owner: Option<&str>,
    out: &mut Vec<TypeEntity>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    out.push(TypeEntity {
                        sym: mint_sym(file, EntityKind::Class, &name, None),
                        name: name.clone(),
                        kind: EntityKind::Class,
                        parent: None,
                        file: file.to_string(),
                        line: py_row1(target),
                        ty: None,
                    });
                    if let Some(body) = target.child_by_field_name("body") {
                        walk_py_entities(body, src, file, Some(&name), out);
                    }
                }
            }
            "function_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    let (kind, parent_name) = match class_owner {
                        Some(o) => (EntityKind::Method, Some(o)),
                        None => (EntityKind::Function, None),
                    };
                    out.push(TypeEntity {
                        sym: mint_sym(file, kind, &name, parent_name),
                        name,
                        kind,
                        parent: parent_name.map(|p| mint_sym(file, EntityKind::Class, p, None)),
                        file: file.to_string(),
                        line: py_row1(target),
                        ty: Some(py_fn_type(target, src)),
                    });
                    if let Some(body) = target.child_by_field_name("body") {
                        walk_py_entities(body, src, file, None, out);
                    }
                }
            }
            _ => walk_py_entities(target, src, file, class_owner, out),
        }
    }
}

// --- Python type_edge pass: class bases = "impl"; annotated class-body
// attributes = "field"; def param annotations = "param", return annotation =
// "returns", annotations on locally-annotated assignments IN the body =
// "uses" (the TS function-edge vocabulary, applied to the closest Python
// analogue of "types mentioned in the body"). ---

fn py_edges_from(root: tree_sitter::Node, src: &[u8]) -> Vec<TypeEdge> {
    let mut out: BTreeSet<(String, String, &'static str)> = BTreeSet::new();
    walk_py_edges(root, src, &mut out);
    out.into_iter()
        .map(|(from, to, kind)| TypeEdge { from, to, kind })
        .collect()
}

fn walk_py_edges(
    node: tree_sitter::Node,
    src: &[u8],
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                py_class_edges(target, src, out);
                if let Some(body) = target.child_by_field_name("body") {
                    walk_py_edges(body, src, out);
                }
            }
            "function_definition" => {
                py_function_edges(target, src, out);
                if let Some(body) = target.child_by_field_name("body") {
                    walk_py_edges(body, src, out);
                }
            }
            _ => walk_py_edges(target, src, out),
        }
    }
}

fn py_class_edges(
    node: tree_sitter::Node,
    src: &[u8],
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let Some(owner) = node.child_by_field_name("name").map(|n| py_text(n, src)) else {
        return;
    };
    let tparams = py_collect_type_params(node, src, "type_parameters");
    if let Some(supers) = node.child_by_field_name("superclasses") {
        let mut cur = supers.walk();
        for arg in supers.named_children(&mut cur) {
            // `metaclass=Foo` is a keyword arg, not a base type.
            if arg.kind() == "keyword_argument" {
                continue;
            }
            for to in py_type_refs_collect(arg, src, &tparams) {
                push(out, &owner, &to, "impl");
            }
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        let mut cur = body.walk();
        for stmt in body.named_children(&mut cur) {
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
                    push(out, &owner, &to, "field");
                }
            }
        }
    }
}

fn py_function_edges(
    node: tree_sitter::Node,
    src: &[u8],
    out: &mut BTreeSet<(String, String, &'static str)>,
) {
    let Some(owner) = node.child_by_field_name("name").map(|n| py_text(n, src)) else {
        return;
    };
    let tparams = py_collect_type_params(node, src, "type_parameters");
    if let Some(plist) = node.child_by_field_name("parameters") {
        let mut cur = plist.walk();
        let mut first = true;
        for p in plist.named_children(&mut cur) {
            if matches!(p.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, type_node) = py_param_name_and_type(p, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            if let Some(t) = type_node {
                for to in py_type_refs_collect(t, src, &tparams) {
                    push(out, &owner, &to, "param");
                }
            }
        }
    }
    if let Some(rt) = node.child_by_field_name("return_type") {
        for to in py_type_refs_collect(rt, src, &tparams) {
            push(out, &owner, &to, "returns");
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        let mut uses = Vec::new();
        py_collect_body_annotation_refs(body, src, &tparams, &mut uses);
        uses.sort();
        uses.dedup();
        for to in uses {
            push(out, &owner, &to, "uses");
        }
    }
}

/// Every annotated local assignment (`x: Foo = ...`) anywhere under a function
/// body, including inside nested defs (same imprecision TS accepts: its body
/// visitor doesn't stop at a nested closure either).
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
    let mut cur = node.walk();
    for child in node.named_children(&mut cur) {
        py_collect_body_annotation_refs(child, src, tparams, out);
    }
}

// --- Python call-graph pass: `function_definition` nodes become CallDefs (a
// def inside a class body is a Method keyed to the enclosing class, a
// top-level or nested-in-function def is Free); every `call` node becomes a
// CallSite whose callee is the called name as written — a bare identifier, or
// the trailing attribute name of `recv.method(...)` (the bare-attribute
// convention; attribute-chain resolution is scoped out). ---

fn py_call_defs_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<CallDef> {
    let mut out = Vec::new();
    py_walk_call_defs(root, src, file, None, "", &mut out);
    out
}

fn py_walk_call_defs(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    parent: Option<&str>,
    enclosing: &str,
    out: &mut Vec<CallDef>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                let owner = target.child_by_field_name("name").map(|n| py_text(n, src));
                // A class body is not a fn scope: reset `enclosing` to "" so a
                // bare class-attribute lambda (`x = lambda: 1`) is skipped, as df
                // does — only its methods open new (Function/None) scopes.
                if let Some(body) = target.child_by_field_name("body") {
                    py_walk_call_defs(body, src, file, owner.as_deref(), "", out);
                }
            }
            // @callable python function
            // @callable python method
            "function_definition" => {
                let name = target
                    .child_by_field_name("name")
                    .map(|n| py_text(n, src))
                    .unwrap_or_default();
                let (kind, ekind) = match parent {
                    Some(_) => (CallKind::Method, EntityKind::Method),
                    None => (CallKind::Free, EntityKind::Function),
                };
                let end = target
                    .child_by_field_name("body")
                    .unwrap_or(target)
                    .end_position()
                    .row as u32
                    + 1;
                // `py_flow_fn` lifts EVERY function_definition as Function/None
                // (even a method), so a lambda in this body joins df under that
                // sym. A nested `def` is Free (parent None).
                let df_sym = mint_sym(file, EntityKind::Function, &name, None);
                out.push(CallDef {
                    sym: mint_sym(file, ekind, &name, parent),
                    name,
                    kind,
                    file: file.to_string(),
                    line: py_row1(target),
                    end,
                });
                if let Some(body) = target.child_by_field_name("body") {
                    py_walk_call_defs(body, src, file, None, &df_sym, out);
                }
            }
            // `lambda x: ...` inside a fn body: Lambda with the SAME
            // `lambda_sym(enclosing, "<row>_<col>")` `py_dataflow_from` mints.
            // `is_named` gate: the `lambda` KEYWORD token shares the node kind
            // "lambda" with the expression, so without it a descent re-matches
            // the keyword at the same coord and double-emits.
            // @callable python lambda
            "lambda" if !enclosing.is_empty() && target.is_named() => {
                let pos = target.start_position();
                let sym = lambda_sym(enclosing, &format!("{}_{}", pos.row, pos.column));
                out.push(CallDef {
                    sym: sym.clone(),
                    name: String::new(),
                    kind: CallKind::Lambda,
                    file: file.to_string(),
                    line: pos.row as u32 + 1,
                    end: target.end_position().row as u32 + 1,
                });
                py_walk_call_defs(target, src, file, parent, &sym, out);
            }
            _ => py_walk_call_defs(target, src, file, parent, enclosing, out),
        }
    }
}

fn py_call_sites_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<CallSite> {
    let mut out = Vec::new();
    py_walk_call_sites(root, src, file, &mut out);
    out
}

fn py_walk_call_sites(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut Vec<CallSite>) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "call" {
            if let Some((callee, line)) = py_callee(child, src) {
                out.push(CallSite {
                    caller_sym: None,
                    callee,
                    callee_path: None,
                    file: file.to_string(),
                    line,
                });
            }
        }
        py_walk_call_sites(child, src, file, out);
    }
}

/// (callee name, 1-based call line) for a `call` node, or None when the callee
/// is not a plain identifier or attribute access (e.g. an invoked subscript or
/// a called lambda expression).
fn py_callee(call: tree_sitter::Node, src: &[u8]) -> Option<(String, u32)> {
    let func = call.child_by_field_name("function")?;
    let line = py_row1(func);
    match func.kind() {
        "identifier" => Some((py_text(func, src), line)),
        "attribute" => {
            let attr = func.child_by_field_name("attribute")?;
            Some((py_text(attr, src), line))
        }
        _ => None,
    }
}

// --- Python doc pass: the docstring is the first expression-statement STRING
// of a module/class/def body (PEP 257); quote/prefix-stripped and dedented.
// Sphinx-field tags only (`:param name: text`, `:return:`/`:returns: text`) —
// Google-style (`Args:` sections) is a stated non-goal. ---

fn py_docs_from(root: tree_sitter::Node, src: &[u8], file: &str) -> Vec<DocFact> {
    let mut out = Vec::new();
    if let Some(text) = py_docstring_of(root, src) {
        out.push(DocFact {
            sym: mint_sym(file, EntityKind::Module, "<module>", None),
            line: 1,
            tags: py_parse_sphinx_tags(&text),
            text,
        });
    }
    walk_py_docs(root, src, file, None, &mut out);
    out
}

fn walk_py_docs(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    class_owner: Option<&str>,
    out: &mut Vec<DocFact>,
) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        match target.kind() {
            "class_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    if let Some(body) = target.child_by_field_name("body") {
                        if let Some(text) = py_docstring_of(body, src) {
                            out.push(DocFact {
                                sym: mint_sym(file, EntityKind::Class, &name, None),
                                line: py_row1(target),
                                tags: py_parse_sphinx_tags(&text),
                                text,
                            });
                        }
                        walk_py_docs(body, src, file, Some(&name), out);
                    }
                }
            }
            "function_definition" => {
                if let Some(name) = target.child_by_field_name("name").map(|n| py_text(n, src)) {
                    let kind = if class_owner.is_some() {
                        EntityKind::Method
                    } else {
                        EntityKind::Function
                    };
                    if let Some(body) = target.child_by_field_name("body") {
                        if let Some(text) = py_docstring_of(body, src) {
                            out.push(DocFact {
                                sym: mint_sym(file, kind, &name, class_owner),
                                line: py_row1(target),
                                tags: py_parse_sphinx_tags(&text),
                                text,
                            });
                        }
                        walk_py_docs(body, src, file, None, out);
                    }
                }
            }
            _ => walk_py_docs(target, src, file, class_owner, out),
        }
    }
}

/// The docstring at the head of a module/class/def body block: the block's
/// first named child must be a bare `string` expression statement.
fn py_docstring_of(body: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut cur = body.walk();
    let first = body.named_children(&mut cur).next()?;
    if first.kind() != "expression_statement" {
        return None;
    }
    let inner = first.named_child(0)?;
    if inner.kind() != "string" {
        return None;
    }
    let raw = inner.utf8_text(src).ok()?;
    Some(py_clean_docstring(raw))
}

/// Strip an (optional) `r`/`b`/`f`/`u` prefix and the enclosing quotes (`"""`/
/// `'''`/`"`/`'`), then dedent. Raw-string escapes are not unescaped (honest:
/// the doc text keeps whatever backslash sequences the source has).
fn py_clean_docstring(raw: &str) -> String {
    let trimmed = raw.trim();
    let quote_at = trimmed.find(['"', '\'']).unwrap_or(0);
    let body = &trimmed[quote_at..];
    let (quote, _) = if body.starts_with("\"\"\"") {
        ("\"\"\"", 3)
    } else if body.starts_with("'''") {
        ("'''", 3)
    } else if body.starts_with('"') {
        ("\"", 1)
    } else if body.starts_with('\'') {
        ("'", 1)
    } else {
        return trimmed.to_string();
    };
    let inner = body
        .strip_prefix(quote)
        .and_then(|r| r.strip_suffix(quote))
        .unwrap_or(body);
    py_dedent(inner)
}

/// PEP 257 dedent: the minimum leading whitespace over every non-blank line
/// AFTER the first (which sits right after the opening quote, so it carries no
/// meaningful indent) is stripped from every subsequent line.
fn py_dedent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= 1 {
        return text.trim().to_string();
    }
    let min_indent = lines
        .iter()
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
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

/// Sphinx field-list tags: `:param name: text` -> tag "param" arg "name";
/// `:return:`/`:returns: text` -> tag "returns" (no arg). Any other `:tag:`
/// passes through with its raw arg/body; a continuation line (no leading `:`)
/// appends to the previous tag's text. Google-style (`Args:` sections) is a
/// stated non-goal — not recognized here.
fn py_parse_sphinx_tags(text: &str) -> Vec<DocTag> {
    let mut out: Vec<DocTag> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(':') {
            if let Some(colon) = rest.find(':') {
                let head = rest[..colon].trim();
                let body = rest[colon + 1..].trim().to_string();
                let mut it = head.splitn(2, char::is_whitespace);
                let tag_word = it.next().unwrap_or("");
                let head_arg = it.next().unwrap_or("").trim();
                let (tag, arg) = match tag_word {
                    "param" | "parameter" => ("param", head_arg),
                    "return" | "returns" => ("returns", ""),
                    other => (other, head_arg),
                };
                out.push(DocTag {
                    tag: tag.to_string(),
                    arg: arg.to_string(),
                    text: body,
                });
                continue;
            }
        }
        if let Some(last) = out.last_mut() {
            if !trimmed.is_empty() {
                if !last.text.is_empty() {
                    last.text.push(' ');
                }
                last.text.push_str(trimmed);
            }
        }
    }
    out
}

// --- Python intra-procedural dataflow lift (tree-sitter). Same two-rule model
// as Kotlin/Rust: value-bearing children flow into their parent, and a bound
// name (assignment target, param, loop variable, comprehension variable, or
// lambda param) registers a scope slot that a later read flows from. Node id
// is `file:row:col:kind` (`push_node`'s shared format); rows are 0-based from
// tree-sitter and bumped +1 at the end, matching Kotlin exactly. Every named
// `def` (top-level, method, or nested) is discovered by one full-tree walk
// (`py_walk_fns`, mirrors `kt_walk_fns`) and flowed with a FRESH, unshared
// scope — captures are only modeled for LAMBDAS, which explicitly share the
// enclosing `scope` map. `self`/`cls` are skipped as params so `df_param.pos`
// aligns with `type_sig.pos`. ---

fn py_dataflow_from(root: tree_sitter::Node, src: &[u8], file: &str) -> DataflowFacts {
    let mut out = DataflowFacts::default();
    py_walk_fns(root, src, file, &mut out);
    // tree-sitter rows are 0-based -> 1-based; `bump_node_lines_1based` also
    // rebuilds each node id so it reconstructs from the stored columns (the
    // coordinate de-intern contract). Loops bump first; nests recompute after.
    for l in &mut out.loops {
        l.start += 1;
        l.end += 1;
    }
    bump_node_lines_1based(&mut out);
    out.nests = compute_nests(&out.nodes, &out.loops);
    out
}

fn py_walk_fns(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        let target = py_unwrap_decorated(child);
        if target.kind() == "function_definition" {
            py_flow_fn(target, src, file, out);
        }
        py_walk_fns(target, src, file, out);
    }
}

/// Seed non-receiver param nodes into a fresh scope, then flow the body's
/// statements. Unlike Rust/Kotlin, a Python function body has no implicit
/// tail-return: only an explicit `return` (handled in `py_flow_stmt`) reaches
/// the fn's `ret` sink. `fn_sym` always mints `EntityKind::Function` with no
/// parent, even for a method — matching Kotlin's `kt_flow_fn` exactly (the
/// dataflow fn_sym is a grouping key, not the entity/call_def sym).
fn py_flow_fn(node: tree_sitter::Node, src: &[u8], file: &str, out: &mut DataflowFacts) {
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let name = py_text(name_node, src);
    let fn_sym = mint_sym(file, EntityKind::Function, &name, None);
    let mut scope: std::collections::HashMap<String, NodeIdx> = std::collections::HashMap::new();
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut cur = params.walk();
        let mut pos: u32 = 0;
        let mut first = true;
        for p in params.named_children(&mut cur) {
            if matches!(p.kind(), "keyword_separator" | "positional_separator") {
                continue;
            }
            let (name_opt, _ty) = py_param_name_and_type(p, src);
            if first {
                first = false;
                if matches!(name_opt.as_deref(), Some("self") | Some("cls")) {
                    continue;
                }
            }
            if let Some(pname) = name_opt {
                let ppos = p.start_position();
                let id = push_node(
                    out,
                    file,
                    ppos.row as u32,
                    ppos.column as u32,
                    "param",
                    &pname,
                    &fn_sym,
                );
                out.param_pos.push((id.clone(), pos));
                scope.insert(pname, id);
                pos += 1;
            }
        }
    }
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, file, &fn_sym, &mut scope, out);
    }
}

/// Flow one statement. A nested `function_definition`/`decorated_definition`/
/// `class_definition` is deliberately SKIPPED here (not recursed into): the
/// top-level `py_walk_fns` full-tree walk independently discovers and flows it
/// with its own fresh scope, so flowing it again here would double-count.
fn py_flow_stmt(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) {
    match node.kind() {
        "function_definition" | "decorated_definition" | "class_definition" => {}
        "expression_statement" => {
            if let Some(inner) = node.named_child(0) {
                if inner.kind() == "assignment" {
                    py_flow_assignment(inner, src, file, fn_sym, scope, out);
                } else {
                    let _ = py_flow_expr(inner, src, file, fn_sym, scope, out);
                }
            }
        }
        "assignment" => py_flow_assignment(node, src, file, fn_sym, scope, out),
        "return_statement" => {
            let pos = node.start_position();
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "ret",
                "",
                fn_sym,
            );
            if let Some(val) = node.named_child(0) {
                let v = py_flow_expr(val, src, file, fn_sym, scope, out);
                out.edges.push(DfEdge { from: v, to: id });
            }
        }
        "for_statement" => py_flow_for(node, src, file, fn_sym, scope, out),
        "while_statement" => py_flow_while(node, src, file, fn_sym, scope, out),
        _ => {
            // block/if_statement/try_statement/with_statement/else_clause/... :
            // conservative pass-through recursion into every named child.
            let mut cur = node.walk();
            for c in node.named_children(&mut cur) {
                py_flow_stmt(c, src, file, fn_sym, scope, out);
            }
        }
    }
}

fn py_flow_assignment(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) {
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };
    let rhs = py_flow_expr(right, src, file, fn_sym, scope, out);
    if let Some(left) = node.child_by_field_name("left") {
        py_bind_pattern(left, rhs, src, file, fn_sym, scope, out);
    }
}

/// Bind an assignment target. `identifier` mints a `let_bind` slot edged from
/// the rhs; tuple/list unpacking mints one slot PER identifier, each edged
/// from the SAME rhs value (kept simple, no per-position slicing); `attribute`
/// (`self.x = ...`) and `subscript` (`d[k] = ...`) track no local binding
/// (honest limit — attribute-chain flow is scoped out).
fn py_bind_pattern(
    node: tree_sitter::Node,
    rhs: NodeIdx,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) {
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src);
            let pos = node.start_position();
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "let_bind",
                &name,
                fn_sym,
            );
            out.edges.push(DfEdge { from: rhs, to: id });
            scope.insert(name, id);
        }
        "tuple_pattern" | "list_pattern" | "pattern_list" => {
            let mut cur = node.walk();
            for child in node.named_children(&mut cur) {
                py_bind_pattern(child, rhs, src, file, fn_sym, scope, out);
            }
        }
        _ => {}
    }
}

/// Identifiers bound by a `for`/comprehension pattern (tuple unpacking flattens
/// to every leaf identifier); returns `(name, the identifier's own node)` pairs
/// so the caller can mint a correctly-positioned `let_bind`.
fn py_pattern_identifiers<'t>(
    node: tree_sitter::Node<'t>,
    src: &[u8],
    out: &mut Vec<(String, tree_sitter::Node<'t>)>,
) {
    match node.kind() {
        "identifier" => out.push((py_text(node, src), node)),
        "tuple_pattern" | "list_pattern" | "pattern_list" => {
            let mut cur = node.walk();
            for c in node.named_children(&mut cur) {
                py_pattern_identifiers(c, src, out);
            }
        }
        _ => {}
    }
}

fn py_flow_for(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) {
    let pos = node.start_position();
    let mut rcur = node.walk();
    let iter_expr = node
        .children_by_field_name("right", &mut rcur)
        .find(|n| n.is_named());
    let coll = iter_expr.map(|e| py_flow_expr(e, src, file, fn_sym, scope, out));
    let mut var_name = String::new();
    if let Some(left) = node.child_by_field_name("left") {
        let mut names = Vec::new();
        py_pattern_identifiers(left, src, &mut names);
        for (i, (name, nnode)) in names.iter().enumerate() {
            let npos = nnode.start_position();
            let id = push_node(
                out,
                file,
                npos.row as u32,
                npos.column as u32,
                "let_bind",
                name,
                fn_sym,
            );
            if let Some(c) = &coll {
                out.edges.push(DfEdge {
                    from: c.clone(),
                    to: id.clone(),
                });
            }
            scope.insert(name.clone(), id);
            if i == 0 {
                var_name = name.clone();
            }
        }
    }
    let end = node.end_position();
    out.loops.push(LoopFact {
        file: file.into(),
        start: pos.row as u32,
        end: end.row as u32,
        var: var_name,
        collection: String::new(),
        fn_sym: fn_sym.into(),
    });
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, file, fn_sym, scope, out);
    }
}

fn py_flow_while(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) {
    let pos = node.start_position();
    if let Some(cond) = node.child_by_field_name("condition") {
        let _ = py_flow_expr(cond, src, file, fn_sym, scope, out);
    }
    let end = node.end_position();
    out.loops.push(LoopFact {
        file: file.into(),
        start: pos.row as u32,
        end: end.row as u32,
        var: String::new(),
        collection: String::new(),
        fn_sym: fn_sym.into(),
    });
    if let Some(body) = node.child_by_field_name("body") {
        py_flow_stmt(body, src, file, fn_sym, scope, out);
    }
}

/// Comprehensions/generator expressions walk their `for_in_clause`(s) and
/// `if_clause`(s) IN THE ENCLOSING SCOPE (Python creates its own comprehension
/// scope at runtime; this diet lift shares the caller's `scope` map instead,
/// same simplification as everywhere else here), binding each loop variable
/// from its iterable, then flows the body (or, for a dict comprehension, both
/// the key and value of its `pair`) into a `new` node representing the
/// assembled collection. Also records the comprehension's own span as a loop
/// fact so `nest` counts calls made per iteration.
fn py_comprehension_flow(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) -> NodeIdx {
    let pos = node.start_position();
    let mut loop_var = String::new();
    let mut cur = node.walk();
    let clauses: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
    for clause in &clauses {
        match clause.kind() {
            "for_in_clause" => {
                let mut rcur = clause.walk();
                let iter_expr = clause
                    .children_by_field_name("right", &mut rcur)
                    .find(|n| n.is_named());
                let coll = iter_expr.map(|e| py_flow_expr(e, src, file, fn_sym, scope, out));
                if let Some(left) = clause.child_by_field_name("left") {
                    let mut names = Vec::new();
                    py_pattern_identifiers(left, src, &mut names);
                    for (name, nnode) in &names {
                        if loop_var.is_empty() {
                            loop_var = name.clone();
                        }
                        let npos = nnode.start_position();
                        let id = push_node(
                            out,
                            file,
                            npos.row as u32,
                            npos.column as u32,
                            "let_bind",
                            name,
                            fn_sym,
                        );
                        if let Some(c) = &coll {
                            out.edges.push(DfEdge {
                                from: c.clone(),
                                to: id.clone(),
                            });
                        }
                        scope.insert(name.clone(), id);
                    }
                }
            }
            "if_clause" => {
                let mut ccur = clause.walk();
                for e in clause.named_children(&mut ccur) {
                    let _ = py_flow_expr(e, src, file, fn_sym, scope, out);
                }
            }
            _ => {}
        }
    }
    let mut fill_ids = Vec::new();
    if node.kind() == "dictionary_comprehension" {
        if let Some(pair) = node.child_by_field_name("body") {
            if let Some(k) = pair.child_by_field_name("key") {
                fill_ids.push(py_flow_expr(k, src, file, fn_sym, scope, out));
            }
            if let Some(v) = pair.child_by_field_name("value") {
                fill_ids.push(py_flow_expr(v, src, file, fn_sym, scope, out));
            }
        }
    } else if let Some(body_expr) = node.child_by_field_name("body") {
        fill_ids.push(py_flow_expr(body_expr, src, file, fn_sym, scope, out));
    }
    let id = push_node(
        out,
        file,
        pos.row as u32,
        pos.column as u32,
        "new",
        "",
        fn_sym,
    );
    for f in fill_ids {
        out.edges.push(DfEdge {
            from: f,
            to: id.clone(),
        });
    }
    let end = node.end_position();
    out.loops.push(LoopFact {
        file: file.into(),
        start: pos.row as u32,
        end: end.row as u32,
        var: loop_var,
        collection: String::new(),
        fn_sym: fn_sym.into(),
    });
    id
}

/// Post-order value flow for one Python expression. Returns the node id
/// carrying its value; unhandled shapes fall through to a conservative
/// catch-all that recurses and surfaces the last value-bearing child (or, if
/// none, a generic `expr` node) — may miss flows, never invents one.
fn py_flow_expr(
    node: tree_sitter::Node,
    src: &[u8],
    file: &str,
    fn_sym: &str,
    scope: &mut std::collections::HashMap<String, NodeIdx>,
    out: &mut DataflowFacts,
) -> NodeIdx {
    let pos = node.start_position();
    match node.kind() {
        "identifier" => {
            let name = py_text(node, src);
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "var_read",
                &name,
                fn_sym,
            );
            if let Some(b) = scope.get(&name) {
                out.edges.push(DfEdge {
                    from: b.clone(),
                    to: id.clone(),
                });
            }
            id
        }
        "true" | "false" | "none" | "integer" | "float" | "string" | "concatenated_string" => {
            push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "lit",
                "",
                fn_sym,
            )
        }
        // f(args) / recv.method(args): each positional argument flows into the
        // call result with `df_arg` recording its 0-based slot; a keyword
        // argument ALSO lands in `df_field` under its name (the Kotlin
        // named-arg precedent); a member callee flows the receiver in at slot
        // -1; a CAPITALIZED bare callee is a constructor call (PEP 8
        // convention), minted as a `new` node carrying the type name.
        "call" => {
            let func = node.child_by_field_name("function");
            let mut recv: Option<NodeIdx> = None;
            let mut callee_name = String::new();
            match func.map(|f| f.kind()) {
                Some("identifier") => {
                    callee_name = py_text(func.unwrap(), src);
                }
                Some("attribute") => {
                    let f = func.unwrap();
                    if let Some(obj) = f.child_by_field_name("object") {
                        recv = Some(py_flow_expr(obj, src, file, fn_sym, scope, out));
                    }
                    if let Some(attr) = f.child_by_field_name("attribute") {
                        callee_name = py_text(attr, src);
                    }
                }
                _ => {
                    if let Some(f) = func {
                        let _ = py_flow_expr(f, src, file, fn_sym, scope, out);
                    }
                }
            }
            let mut arg_ids: Vec<(Option<String>, NodeIdx)> = Vec::new();
            if let Some(args) = node.child_by_field_name("arguments") {
                let mut cur = args.walk();
                for a in args.named_children(&mut cur) {
                    match a.kind() {
                        "keyword_argument" => {
                            let name = a.child_by_field_name("name").map(|n| py_text(n, src));
                            if let Some(val) = a.child_by_field_name("value") {
                                let vid = py_flow_expr(val, src, file, fn_sym, scope, out);
                                arg_ids.push((name, vid));
                            }
                        }
                        "dictionary_splat" | "list_splat" => {
                            if let Some(inner) = a.named_child(0) {
                                let vid = py_flow_expr(inner, src, file, fn_sym, scope, out);
                                arg_ids.push((None, vid));
                            }
                        }
                        _ => {
                            let vid = py_flow_expr(a, src, file, fn_sym, scope, out);
                            arg_ids.push((None, vid));
                        }
                    }
                }
            }
            let is_ctor = callee_name.chars().next().is_some_and(|c| c.is_uppercase());
            let (kind, var) = if is_ctor {
                ("new", callee_name.as_str())
            } else {
                ("call_res", "")
            };
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                kind,
                var,
                fn_sym,
            );
            if let Some(r) = recv {
                out.edges.push(DfEdge {
                    from: r.clone(),
                    to: id.clone(),
                });
                out.args.push((id.clone(), -1, r));
            }
            for (p, (name, vid)) in arg_ids.into_iter().enumerate() {
                out.edges.push(DfEdge {
                    from: vid.clone(),
                    to: id.clone(),
                });
                out.args.push((id.clone(), p as i64, vid.clone()));
                if let Some(n) = name {
                    out.fields.push((id.clone(), n, vid));
                }
            }
            id
        }
        // `base.name` outside call-callee position: a member read, `var` is
        // the accessed name so a `df_field` write can be matched against a
        // read of the same field.
        "attribute" => {
            let obj = node
                .child_by_field_name("object")
                .map(|o| py_flow_expr(o, src, file, fn_sym, scope, out));
            let name = node
                .child_by_field_name("attribute")
                .map(|a| py_text(a, src))
                .unwrap_or_default();
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "member",
                &name,
                fn_sym,
            );
            if let Some(o) = obj {
                out.edges.push(DfEdge {
                    from: o,
                    to: id.clone(),
                });
            }
            id
        }
        "subscript" => {
            let val = node
                .child_by_field_name("value")
                .map(|v| py_flow_expr(v, src, file, fn_sym, scope, out));
            let mut cur = node.walk();
            for sub in node.children_by_field_name("subscript", &mut cur) {
                let _ = py_flow_expr(sub, src, file, fn_sym, scope, out);
            }
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "member",
                "",
                fn_sym,
            );
            if let Some(v) = val {
                out.edges.push(DfEdge {
                    from: v,
                    to: id.clone(),
                });
            }
            id
        }
        "binary_operator" | "boolean_operator" | "comparison_operator" => {
            let mut cur = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
            let l = kids
                .first()
                .map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            let r = kids
                .last()
                .map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "binop",
                "",
                fn_sym,
            );
            if let Some(l) = l {
                out.edges.push(DfEdge {
                    from: l,
                    to: id.clone(),
                });
            }
            if let Some(r) = r {
                out.edges.push(DfEdge {
                    from: r,
                    to: id.clone(),
                });
            }
            id
        }
        "not_operator" | "unary_operator" => {
            let mut cur = node.walk();
            let v = node
                .named_children(&mut cur)
                .next()
                .map(|n| py_flow_expr(n, src, file, fn_sym, scope, out));
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "unop",
                "",
                fn_sym,
            );
            if let Some(v) = v {
                out.edges.push(DfEdge {
                    from: v,
                    to: id.clone(),
                });
            }
            id
        }
        // `<true_expr> if <cond> else <false_expr>`: the value is EITHER
        // branch; the condition is walked for its own nested facts, never
        // edged in as a value (mirrors TS's ternary).
        "conditional_expression" => {
            let mut cur = node.walk();
            let kids: Vec<tree_sitter::Node> = node.named_children(&mut cur).collect();
            let cons = kids
                .first()
                .map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            if let Some(cond) = kids.get(1) {
                let _ = py_flow_expr(*cond, src, file, fn_sym, scope, out);
            }
            let alt = kids
                .get(2)
                .map(|n| py_flow_expr(*n, src, file, fn_sym, scope, out));
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "cond",
                "",
                fn_sym,
            );
            if let Some(c) = cons {
                out.edges.push(DfEdge {
                    from: c,
                    to: id.clone(),
                });
            }
            if let Some(a) = alt {
                out.edges.push(DfEdge {
                    from: a,
                    to: id.clone(),
                });
            }
            id
        }
        "parenthesized_expression" | "await" => {
            let mut cur = node.walk();
            let inner = node.named_children(&mut cur).next();
            match inner {
                Some(inner) => py_flow_expr(inner, src, file, fn_sym, scope, out),
                None => push_node(
                    out,
                    file,
                    pos.row as u32,
                    pos.column as u32,
                    "expr",
                    "",
                    fn_sym,
                ),
            }
        }
        // `lambda params: body`: lift as its OWN fn scope under a synthetic
        // `<enclosing>::closure::<row>_<col>` sym (mirrors Kotlin/TS inline
        // lambdas exactly) — param nodes + a `ret` node for the single body
        // expression (a lambda has no `return`, its body IS the return value)
        // — and mint the `closure` VALUE node here, carrying the lifted sym in
        // `var` (the join key `std/flow.dl`'s higher-order hop reads). The
        // enclosing `scope` is shared so captures resolve.
        "lambda" => {
            let lam_sym = lambda_sym(fn_sym, &format!("{}_{}", pos.row, pos.column));
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cur = params.walk();
                for (i, p) in params.named_children(&mut cur).enumerate() {
                    let (name_opt, _ty) = py_param_name_and_type(p, src);
                    if let Some(pname) = name_opt {
                        let ppos = p.start_position();
                        let id = push_node(
                            out,
                            file,
                            ppos.row as u32,
                            ppos.column as u32,
                            "param",
                            &pname,
                            &lam_sym,
                        );
                        out.param_pos.push((id.clone(), i as u32));
                        scope.insert(pname, id);
                    }
                }
            }
            if let Some(body) = node.child_by_field_name("body") {
                let v = py_flow_expr(body, src, file, &lam_sym, scope, out);
                let end = node.end_position();
                let ret = push_node(
                    out,
                    file,
                    end.row as u32,
                    end.column as u32,
                    "ret",
                    "",
                    &lam_sym,
                );
                out.edges.push(DfEdge { from: v, to: ret });
            }
            push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "closure",
                &lam_sym,
                fn_sym,
            )
        }
        "list_comprehension"
        | "set_comprehension"
        | "generator_expression"
        | "dictionary_comprehension" => py_comprehension_flow(node, src, file, fn_sym, scope, out),
        "list" | "set" | "tuple" => {
            let mut cur = node.walk();
            let ids: Vec<NodeIdx> = node
                .named_children(&mut cur)
                .map(|el| py_flow_expr(el, src, file, fn_sym, scope, out))
                .collect();
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "new",
                "",
                fn_sym,
            );
            for v in ids {
                out.edges.push(DfEdge {
                    from: v,
                    to: id.clone(),
                });
            }
            id
        }
        // `{...}`: each `pair`'s value flows into a `new` node; a plain-string
        // key becomes the `df_field` name (mirrors TS's ObjectExpression);
        // `**spread` lands under the ".." pseudo-field (the FRU convention).
        "dictionary" => {
            let mut cur = node.walk();
            let mut filled: Vec<(String, NodeIdx)> = Vec::new();
            for child in node.named_children(&mut cur) {
                match child.kind() {
                    "pair" => {
                        let key = child.child_by_field_name("key");
                        let val = child
                            .child_by_field_name("value")
                            .map(|v| py_flow_expr(v, src, file, fn_sym, scope, out));
                        let name = key
                            .filter(|k| k.kind() == "string")
                            .and_then(|k| k.utf8_text(src).ok())
                            .map(|s| s.trim_matches(['"', '\'']).to_string())
                            .unwrap_or_default();
                        if let Some(v) = val {
                            filled.push((name, v));
                        }
                    }
                    "dictionary_splat" => {
                        if let Some(inner) = child.named_child(0) {
                            let v = py_flow_expr(inner, src, file, fn_sym, scope, out);
                            filled.push(("..".into(), v));
                        }
                    }
                    _ => {}
                }
            }
            let id = push_node(
                out,
                file,
                pos.row as u32,
                pos.column as u32,
                "new",
                "",
                fn_sym,
            );
            for (name, v) in filled {
                out.edges.push(DfEdge {
                    from: v.clone(),
                    to: id.clone(),
                });
                if !name.is_empty() {
                    out.fields.push((id.clone(), name, v));
                }
            }
            id
        }
        _ => {
            let mut cur = node.walk();
            let mut last = None;
            for c in node.named_children(&mut cur) {
                last = Some(py_flow_expr(c, src, file, fn_sym, scope, out));
            }
            last.unwrap_or_else(|| {
                push_node(
                    out,
                    file,
                    pos.row as u32,
                    pos.column as u32,
                    "expr",
                    "",
                    fn_sym,
                )
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_entities_class_method_function_module() {
        let src = "\"\"\"Module doc.\"\"\"\n\n\nclass Repo:\n    def fetch(self, id: int) -> Report:\n        return Report()\n\n\ndef helper(n: int) -> int:\n    return n\n";
        let es = PyTypes.extract("app.py", src).entities;
        let module = es
            .iter()
            .find(|e| e.kind == EntityKind::Module)
            .expect("module entity");
        assert_eq!(module.line, 1);
        let repo = es.iter().find(|e| e.name == "Repo").expect("class entity");
        assert_eq!(repo.kind, EntityKind::Class);
        assert!(repo.parent.is_none());
        let fetch = es
            .iter()
            .find(|e| e.name == "fetch")
            .expect("method entity");
        assert_eq!(fetch.kind, EntityKind::Method);
        assert_eq!(fetch.parent.as_deref(), Some(repo.sym.as_str()));
        // self dropped: one param slot only for `id`, which is a builtin (no ref).
        let ty = fetch.ty.as_ref().unwrap();
        assert_eq!(ty.params.len(), 1, "{ty:?}");
        assert!(ty.params[0].is_empty(), "int is a builtin, no ref: {ty:?}");
        assert_eq!(ty.ret, vec![TypeRef::Named("Report".into())]);
        let helper = es
            .iter()
            .find(|e| e.name == "helper")
            .expect("function entity");
        assert_eq!(helper.kind, EntityKind::Function);
        assert!(helper.parent.is_none());
    }

    #[test]
    fn python_edges_bases_fields_params_returns_and_subscript_inner() {
        let src = "\
from typing import Optional


class Base:
    pass


class Widget(Base):
    label: Optional[str]

    def render(self, item: Optional[Widget]) -> Optional[Report]:
        note: Widget = item
        return note
";
        let facts = PyTypes.extract("app.py", src);
        let got = &facts.edges;
        assert!(has(got, "Widget", "Base", "impl"), "{got:?}");
        // Optional[str] -> "str" is noise-filtered (builtin), so no field edge
        // to str, but "Optional" itself must never appear as a ref either.
        assert!(!got.iter().any(|e| e.to == "Optional"), "{got:?}");
        assert!(has(got, "render", "Widget", "param"), "{got:?}");
        assert!(has(got, "render", "Report", "returns"), "{got:?}");
        assert!(has(got, "render", "Widget", "uses"), "{got:?}");
    }

    #[test]
    fn python_calls_ctor_and_attribute_callee() {
        let src = "\
class Widget:
    def render(self):
        return self.helper()


def make(store):
    w = Widget()
    return store.save(w)
";
        let facts = PyTypes.extract_calls("app.py", src);
        let render_def = facts
            .defs
            .iter()
            .find(|d| d.name == "render")
            .expect("render def");
        assert_eq!(render_def.kind, CallKind::Method);
        let make_def = facts
            .defs
            .iter()
            .find(|d| d.name == "make")
            .expect("make def");
        assert_eq!(make_def.kind, CallKind::Free);
        // attribute call: bare trailing name only.
        assert!(
            facts.sites.iter().any(|s| s.callee == "helper"),
            "{:?}",
            facts.sites
        );
        assert!(
            facts.sites.iter().any(|s| s.callee == "save"),
            "{:?}",
            facts.sites
        );
        // capitalized bare call is present as a call site too (ctor df_node is
        // a dataflow-layer concept, checked separately below).
        assert!(
            facts.sites.iter().any(|s| s.callee == "Widget"),
            "{:?}",
            facts.sites
        );
    }

    #[test]
    fn python_dataflow_ctor_kwarg_lambda_and_comprehension_loop_span() {
        let src = "\
def build(xs):
    item = Widget(label=\"x\")
    doubled = [n * 2 for n in xs]
    fn = lambda value: value + 1
    return fn(item)
";
        let df = PyTypes.extract_dataflow("app.py", src);
        // capitalized call mints a `new` node carrying the type name.
        let ctor = df
            .nodes
            .iter()
            .find(|n| n.kind == "new" && n.var == "Widget")
            .expect("ctor node");
        // keyword argument also lands in df_field under its name.
        assert!(
            df.fields
                .iter()
                .any(|(id, name, _)| id == &ctor.id && name == "label"),
            "{:?}",
            df.fields
        );
        // list comprehension records a loop span with its loop variable.
        assert!(df.loops.iter().any(|l| l.var == "n"), "{:?}", df.loops);
        // lambda lifts as its own closure scope with a param node.
        let closure = df
            .nodes
            .iter()
            .find(|n| n.kind == "closure")
            .expect("closure node");
        let lam_sym = closure.var.clone();
        assert!(
            df.nodes
                .iter()
                .any(|n| n.kind == "param" && n.fn_sym == lam_sym && n.var == "value"),
            "{:?}",
            df.nodes
        );
    }

    #[test]
    fn python_docstring_and_sphinx_tags() {
        let src = "\
def compute(count):
    \"\"\"Compute a thing.

    :param count: how many
    :returns: the result
    \"\"\"
    return count
";
        let docs = PyTypes.extract("app.py", src).docs;
        let doc = docs
            .iter()
            .find(|d| d.text.starts_with("Compute a thing"))
            .expect("docstring");
        let param_tag = doc
            .tags
            .iter()
            .find(|t| t.tag == "param")
            .expect("param tag");
        assert_eq!(param_tag.arg, "count");
        assert_eq!(param_tag.text, "how many");
        assert!(
            doc.tags.iter().any(|t| t.tag == "returns"),
            "{:?}",
            doc.tags
        );
    }

    // ── template_parts ──────────────────────────────────────────────────────
}
