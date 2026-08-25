//! Prolog extraction over the DataGrout tree-sitter grammar.
//!
//! One parse feeds all four planes. Predicate identities include arity:
//! `name/2` for clauses and `name//2` for DCGs. The clause span is the
//! definition span, so body call sites are contained by their caller and the
//! shared `covering_def` resolver works without Prolog-specific phase-2 state.

use std::collections::HashMap;

use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstEdgeKind, CstF, DfEdgeKind, DfF, DfNodeKind,
    ProjectEdge, RefPosition, Reference, Specifier, SpecifierKind, TypeEntityKind, TypeF,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{corpus_defs, covering_def, ProjectCx, Resolve};
use crate::shape::{ContentId, FamilyTag, NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::trace;

#[derive(Default)]
pub struct PrologSource;

fn parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(tree_sitter_prolog::LANGUAGE);
    parser.set_language(&language).ok()?;
    parser.parse(content, None)
}

fn text<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

fn span(node: tree_sitter::Node) -> Span {
    Span {
        start: node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

fn field<'a>(node: tree_sitter::Node<'a>, name: &str) -> Option<tree_sitter::Node<'a>> {
    node.child_by_field_name(name)
}

fn operator<'a>(node: tree_sitter::Node, src: &'a [u8]) -> &'a str {
    field(node, "operator").map(|n| text(n, src)).unwrap_or("")
}

fn named_children(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn clause_term(clause: tree_sitter::Node) -> Option<tree_sitter::Node> {
    field(clause, "term").or_else(|| clause.named_child(0))
}

fn strip_annotation<'a>(mut node: tree_sitter::Node<'a>, src: &[u8]) -> tree_sitter::Node<'a> {
    while node.kind() == "binary_operation" && operator(node, src) == "::" {
        node = field(node, "right").unwrap_or(node);
    }
    node
}

fn callable_name_arity(node: tree_sitter::Node, src: &[u8]) -> Option<(String, usize)> {
    let node = strip_annotation(node, src);
    match node.kind() {
        "compound_term" => {
            let functor = field(node, "functor")?;
            let arity = node
                .children_by_field_name("argument", &mut node.walk())
                .count();
            Some((atom_text(functor, src), arity))
        }
        "atom" | "unquoted_atom" | "quoted_atom" | "operator_atom" => {
            Some((atom_text(node, src), 0))
        }
        _ => None,
    }
}

fn atom_text(node: tree_sitter::Node, src: &[u8]) -> String {
    let raw = text(node, src);
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        raw[1..raw.len() - 1].replace("''", "'")
    } else {
        raw.to_string()
    }
}

fn predicate_key(name: &str, arity: usize, dcg: bool) -> String {
    if dcg {
        format!("{name}//{arity}")
    } else {
        format!("{name}/{arity}")
    }
}

fn head_body<'a>(
    clause: tree_sitter::Node<'a>,
    src: &[u8],
) -> Option<(tree_sitter::Node<'a>, Option<tree_sitter::Node<'a>>, bool)> {
    let term = clause_term(clause)?;
    if term.kind() == "unary_operation" {
        return None;
    }
    if term.kind() == "binary_operation" {
        match operator(term, src) {
            ":-" => return Some((field(term, "left")?, field(term, "right"), false)),
            "-->" => return Some((field(term, "left")?, field(term, "right"), true)),
            _ => {}
        }
    }
    Some((strip_annotation(term, src), None, false))
}

fn project_cst(root: tree_sitter::Node, strings: &mut Strings, sink: &mut FamilyBundle<CstF>) {
    let mut stack = vec![(root, None)];
    while let Some((node, parent)) = stack.pop() {
        let current = if node.is_named() {
            let node_ref = NodeRef(sink.nodes.len() as u32);
            sink.nodes
                .push(Node::new(span(node), strings.intern(node.kind())));
            if let Some(parent) = parent {
                sink.edges
                    .push(Edge::new(parent, node_ref, CstEdgeKind::Child));
            }
            Some(node_ref)
        } else {
            parent
        };
        let mut children: Vec<_> = node.children(&mut node.walk()).collect();
        children.reverse();
        for child in children {
            stack.push((child, current));
        }
    }
}

fn clauses(root: tree_sitter::Node) -> impl Iterator<Item = tree_sitter::Node> {
    named_children(root)
        .into_iter()
        .filter(|node| node.kind() == "clause")
}

fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for clause in clauses(root) {
        let Some((head, _, dcg)) = head_body(clause, src) else {
            continue;
        };
        let Some((name, arity)) = callable_name_arity(head, src) else {
            continue;
        };
        sink.nodes.push(
            Node::new(span(clause), TypeEntityKind::Function)
                .with_name(strings.intern(&predicate_key(&name, arity, dcg))),
        );
    }
}

fn project_calls(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    for clause in clauses(root) {
        let Some((head, body, dcg)) = head_body(clause, src) else {
            project_directive(clause, src, strings, sink);
            walk_directive_refs(clause, src, strings, sink);
            continue;
        };
        let Some((name, arity)) = callable_name_arity(head, src) else {
            continue;
        };
        sink.nodes.push(
            Node::new(span(clause), CallKind::Free)
                .with_name(strings.intern(&predicate_key(&name, arity, dcg))),
        );
        walk_head_refs(head, src, strings, sink);
        if let Some(body) = body {
            walk_goals(body, src, dcg, strings, sink, None);
        }
    }
}

fn push_ref(
    node: tree_sitter::Node,
    functor: &str,
    position: RefPosition,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    sink.aux.refs.push(Reference {
        span: span(node),
        functor: strings.intern(functor),
        position,
    });
}

/// Every compound inside a clause HEAD's arguments is a `head_arg` reference.
/// The head's own functor is the definition, not a reference.
fn walk_head_refs(
    head: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    if head.kind() == "compound_term" {
        let mut cursor = head.walk();
        for arg in head.children_by_field_name("argument", &mut cursor) {
            walk_data_refs(arg, RefPosition::HeadArg, src, strings, sink);
        }
    }
}

/// Every compound anywhere in a data subtree is a reference at `position`.
fn walk_data_refs(
    node: tree_sitter::Node,
    position: RefPosition,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    match node.kind() {
        "compound_term" => {
            if let Some((name, arity)) = callable_name_arity(node, src) {
                push_ref(
                    node,
                    &predicate_key(&name, arity, false),
                    position,
                    strings,
                    sink,
                );
            }
            let mut cursor = node.walk();
            for arg in node.children_by_field_name("argument", &mut cursor) {
                walk_data_refs(arg, position, src, strings, sink);
            }
        }
        "atom" | "unquoted_atom" | "quoted_atom" | "operator_atom" | "variable" | "number"
        | "string" | "back_quoted_string" => {}
        _ => {
            for child in named_children(node) {
                walk_data_refs(child, position, src, strings, sink);
            }
        }
    }
}

/// Goal-position references: the top-level body conjuncts that are executed,
/// plus the data compounds nested inside their arguments (`term_arg`). Metacall
/// arguments (call/findall/maplist) are NOT unwrapped as goals; their contents
/// fall through to `term_arg`, matching the existing walker, which models no
/// metacalls.
fn walk_goals_refs(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    match node.kind() {
        "parenthesized" | "curly_block" => {
            for child in named_children(node) {
                walk_goals_refs(child, src, strings, sink);
            }
        }
        "unary_operation" => {
            let op = operator(node, src);
            if op == "\\+" {
                if let Some(operand) = field(node, "operand") {
                    walk_goals_refs(operand, src, strings, sink);
                }
            } else {
                // A prefix-operator goal (dynamic/1, initialization/1, ...).
                push_ref(node, &format!("{op}/1"), RefPosition::Goal, strings, sink);
                if let Some(operand) = field(node, "operand") {
                    walk_data_refs(operand, RefPosition::TermArg, src, strings, sink);
                }
            }
        }
        "binary_operation" => {
            let op = operator(node, src);
            match op {
                "," | ";" | "|" | "->" | "*->" => {
                    if let Some(left) = field(node, "left") {
                        walk_goals_refs(left, src, strings, sink);
                    }
                    if let Some(right) = field(node, "right") {
                        walk_goals_refs(right, src, strings, sink);
                    }
                }
                ":" => {
                    if let Some(right) = field(node, "right") {
                        walk_goals_refs(right, src, strings, sink);
                    }
                }
                ":-" | "-->" | "::" => {}
                _ => {
                    push_ref(node, &format!("{op}/2"), RefPosition::Goal, strings, sink);
                    for child in named_children(node) {
                        walk_data_refs(child, RefPosition::TermArg, src, strings, sink);
                    }
                }
            }
        }
        "compound_term" => {
            if let Some((name, arity)) = callable_name_arity(node, src) {
                push_ref(
                    node,
                    &predicate_key(&name, arity, false),
                    RefPosition::Goal,
                    strings,
                    sink,
                );
            }
            let mut cursor = node.walk();
            for arg in node.children_by_field_name("argument", &mut cursor) {
                walk_data_refs(arg, RefPosition::TermArg, src, strings, sink);
            }
        }
        "atom" | "unquoted_atom" | "quoted_atom" | "operator_atom" => {
            push_ref(
                node,
                &format!("{}/0", atom_text(node, src)),
                RefPosition::Goal,
                strings,
                sink,
            );
        }
        "cut" => push_ref(node, "!/0", RefPosition::Goal, strings, sink),
        _ => walk_data_refs(node, RefPosition::TermArg, src, strings, sink),
    }
}

/// A directive body (`:- Goal`) is executed at load time: same reference walk
/// as a clause body (goals `goal`, argument data `term_arg`).
fn walk_directive_refs(
    clause: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let Some(term) = clause_term(clause) else {
        return;
    };
    if term.kind() != "unary_operation" || operator(term, src) != ":-" {
        return;
    }
    if let Some(operand) = field(term, "operand") {
        walk_goals_refs(operand, src, strings, sink);
    }
}

fn project_directive(
    clause: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let Some(term) = clause_term(clause) else {
        return;
    };
    if term.kind() != "unary_operation" || operator(term, src) != ":-" {
        return;
    }
    let Some(operand) = field(term, "operand") else {
        return;
    };
    let Some((name, _)) = callable_name_arity(operand, src) else {
        return;
    };
    match name.as_str() {
        "use_module" | "ensure_loaded" | "consult" => import_directive(operand, src, strings, sink),
        "module" => module_declaration(operand, src, strings, sink),
        "include" => include_directive(operand, src, strings, sink),
        "reexport" => reexport_directive(operand, src, strings, sink),
        _ => (),
    }
}

// `use_module(Path)` names no predicate, so the path IS the specifier and rides
// `name` with no second copy; the two-argument form keys on (module, name).
fn import_directive(
    operand: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = operand.walk();
    let mut arguments = operand.children_by_field_name("argument", &mut cursor);
    let Some(source) = arguments.next() else {
        return;
    };
    let module_text = text(source, src).to_string();
    let Some(list) = arguments.next() else {
        sink.aux.specifiers.push(Specifier {
            span: span(source),
            name: strings.intern(&module_text),
            kind: SpecifierKind::SideEffect,
            module: None,
            imported: None,
        });
        return;
    };
    let module = strings.intern(&module_text);
    let mut named = 0usize;
    for indicator in predicate_indicators(list, src) {
        named += 1;
        sink.aux.specifiers.push(Specifier {
            span: indicator.span,
            name: strings.intern(&indicator.key),
            kind: SpecifierKind::Named,
            module: Some(module),
            imported: None,
        });
    }
    // `use_module(Path, [])` loads the file and imports nothing; without a
    // row the file-to-file edge is invisible to every consumer.
    if named == 0 {
        sink.aux.specifiers.push(Specifier {
            span: span(source),
            name: module,
            kind: SpecifierKind::SideEffect,
            module: None,
            imported: None,
        });
    }
}

// `include(Path)` pulls file text into the enclosing module at load time. The
// path IS the specifier and rides `name` with no module (an include names a
// part, never a module interface).
fn include_directive(
    operand: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = operand.walk();
    let mut arguments = operand.children_by_field_name("argument", &mut cursor);
    let Some(source) = arguments.next() else {
        return;
    };
    sink.aux.specifiers.push(Specifier {
        span: span(source),
        name: strings.intern(text(source, src)),
        kind: SpecifierKind::Include,
        module: None,
        imported: None,
    });
}

// `reexport(Path)` loads another module's exported interface; `reexport(Path,
// List)` narrows that to the named predicates. The one-argument form names no
// predicate, so the path IS the specifier and rides `name` with no module; the
// two-argument form keys on (module, name) like a module import.
fn reexport_directive(
    operand: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = operand.walk();
    let mut arguments = operand.children_by_field_name("argument", &mut cursor);
    let Some(source) = arguments.next() else {
        return;
    };
    let path_text = text(source, src).to_string();
    let Some(list) = arguments.next() else {
        sink.aux.specifiers.push(Specifier {
            span: span(source),
            name: strings.intern(&path_text),
            kind: SpecifierKind::ReexportModule,
            module: None,
            imported: None,
        });
        return;
    };
    let module = strings.intern(&path_text);
    for indicator in predicate_indicators(list, src) {
        sink.aux.specifiers.push(Specifier {
            span: indicator.span,
            name: strings.intern(&indicator.key),
            kind: SpecifierKind::ReexportModule,
            module: Some(module),
            imported: None,
        });
    }
}

// A module's export list is its interface. The rows carry the module's OWN name
// so an import row and the export row it resolves to join on (module, name).
fn module_declaration(
    operand: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = operand.walk();
    let mut arguments = operand.children_by_field_name("argument", &mut cursor);
    let Some(name_node) = arguments.next() else {
        return;
    };
    let Some(list) = arguments.next() else {
        return;
    };
    let module = strings.intern(&atom_text(name_node, src));
    for indicator in predicate_indicators(list, src) {
        sink.aux.specifiers.push(Specifier {
            span: indicator.span,
            name: strings.intern(&indicator.key),
            kind: SpecifierKind::Reexport,
            module: Some(module),
            imported: None,
        });
    }
}

struct PredicateIndicator {
    span: Span,
    key: String,
}

// `f/1`, `f//2` and `op(P, T, N)` are the three list entries SWI accepts. The
// operator form declares no predicate, so it yields no row.
fn predicate_indicators(list: tree_sitter::Node, src: &[u8]) -> Vec<PredicateIndicator> {
    let mut out = Vec::new();
    collect_predicate_indicators(list, src, &mut out);
    out
}

fn collect_predicate_indicators(
    node: tree_sitter::Node,
    src: &[u8],
    out: &mut Vec<PredicateIndicator>,
) {
    if node.kind() == "binary_operation" {
        let operator_text = operator(node, src);
        if operator_text == "/" || operator_text == "//" {
            if let (Some(name_node), Some(arity_node)) = (field(node, "left"), field(node, "right"))
            {
                if let Ok(arity) = text(arity_node, src).trim().parse::<usize>() {
                    out.push(PredicateIndicator {
                        span: span(node),
                        key: predicate_key(
                            &atom_text(name_node, src),
                            arity,
                            operator_text == "//",
                        ),
                    });
                }
            }
            return;
        }
    }
    for child in named_children(node) {
        collect_predicate_indicators(child, src, out);
    }
}

/// One spine pass pushing both `aux.sites` and `aux.refs`, replacing two
/// walks. Directives keep their own refs-only walk below.
fn walk_goals(
    node: tree_sitter::Node,
    src: &[u8],
    dcg: bool,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    module: Option<&str>,
) {
    match node.kind() {
        "parenthesized" | "curly_block" => {
            for child in named_children(node) {
                walk_goals(child, src, dcg, strings, sink, module);
            }
        }
        "unary_operation" => {
            let op = operator(node, src);
            if op == "\\+" {
                if let Some(operand) = field(node, "operand") {
                    walk_goals(operand, src, dcg, strings, sink, module);
                }
            } else {
                // A prefix-operator goal (dynamic/1, initialization/1, ...).
                push_ref(node, &format!("{op}/1"), RefPosition::Goal, strings, sink);
                if let Some(operand) = field(node, "operand") {
                    walk_data_refs(operand, RefPosition::TermArg, src, strings, sink);
                }
            }
        }
        "binary_operation" => {
            let op = operator(node, src);
            match op {
                "," | ";" | "|" | "->" | "*->" => {
                    if let Some(left) = field(node, "left") {
                        walk_goals(left, src, dcg, strings, sink, module);
                    }
                    if let Some(right) = field(node, "right") {
                        walk_goals(right, src, dcg, strings, sink, module);
                    }
                }
                ":" => {
                    let qualifier = field(node, "left").map(|n| atom_text(n, src));
                    if let Some(right) = field(node, "right") {
                        walk_goals(right, src, dcg, strings, sink, qualifier.as_deref());
                    }
                }
                ":-" | "-->" | "::" => {}
                _ => {
                    push_site(node, op, 2, false, strings, sink, module);
                    push_ref(node, &format!("{op}/2"), RefPosition::Goal, strings, sink);
                    for child in named_children(node) {
                        walk_data_refs(child, RefPosition::TermArg, src, strings, sink);
                    }
                }
            }
        }
        "compound_term" => {
            if let Some((name, arity)) = callable_name_arity(node, src) {
                push_site(node, &name, arity, dcg, strings, sink, module);
                push_ref(
                    node,
                    &predicate_key(&name, arity, false),
                    RefPosition::Goal,
                    strings,
                    sink,
                );
            }
            let mut cursor = node.walk();
            for arg in node.children_by_field_name("argument", &mut cursor) {
                walk_data_refs(arg, RefPosition::TermArg, src, strings, sink);
            }
        }
        "atom" | "unquoted_atom" | "quoted_atom" | "operator_atom" => {
            let name = atom_text(node, src);
            push_site(node, &name, 0, dcg, strings, sink, module);
            push_ref(node, &format!("{name}/0"), RefPosition::Goal, strings, sink);
        }
        "cut" => {
            push_site(node, "!", 0, false, strings, sink, module);
            push_ref(node, "!/0", RefPosition::Goal, strings, sink);
        }
        _ => walk_data_refs(node, RefPosition::TermArg, src, strings, sink),
    }
}

fn push_site(
    node: tree_sitter::Node,
    name: &str,
    arity: usize,
    dcg: bool,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
    module: Option<&str>,
) {
    let key = predicate_key(name, arity, dcg);
    let path = module.map(|module| strings.intern(&format!("{module}:{key}")));
    sink.aux.sites.push(CallSite {
        span: span(node),
        callee: strings.intern(&key),
        callee_path: path,
    });
}

#[derive(Default)]
struct Scope {
    vars: HashMap<String, NodeRef>,
}

fn project_df(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) {
    for clause in clauses(root) {
        let Some((head, body, _)) = head_body(clause, src) else {
            continue;
        };
        let mut scope = Scope::default();
        walk_variables(head, src, true, strings, &mut scope, sink);
        if let Some(body) = body {
            walk_df(body, src, strings, &mut scope, sink);
        }
    }
}

fn walk_variables(
    node: tree_sitter::Node,
    src: &[u8],
    params: bool,
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) {
    if node.kind() == "variable" {
        let name = text(node, src);
        if name == "_" {
            push_df(node, DfNodeKind::Param, None, strings, sink);
            return;
        }
        if let Some(origin) = scope.vars.get(name).copied() {
            let read = push_df(node, DfNodeKind::VarRead, Some(name), strings, sink);
            sink.edges.push(Edge::new(origin, read, DfEdgeKind::Direct));
        } else {
            let kind = if params {
                DfNodeKind::Param
            } else {
                DfNodeKind::LetBind
            };
            let binding = push_df(node, kind, Some(name), strings, sink);
            scope.vars.insert(name.to_string(), binding);
        }
        return;
    }
    for child in named_children(node) {
        walk_variables(child, src, params, strings, scope, sink);
    }
}

fn walk_df(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    scope: &mut Scope,
    sink: &mut FamilyBundle<DfF>,
) -> Option<NodeRef> {
    match node.kind() {
        "variable" => {
            let name = text(node, src);
            if name == "_" {
                return None;
            }
            if let Some(origin) = scope.vars.get(name).copied() {
                let read = push_df(node, DfNodeKind::VarRead, Some(name), strings, sink);
                sink.edges.push(Edge::new(origin, read, DfEdgeKind::Direct));
                Some(read)
            } else {
                let binding = push_df(node, DfNodeKind::LetBind, Some(name), strings, sink);
                scope.vars.insert(name.to_string(), binding);
                Some(binding)
            }
        }
        "compound_term" => {
            let mut args = Vec::new();
            let mut cursor = node.walk();
            for arg in node.children_by_field_name("argument", &mut cursor) {
                if let Some(value) = walk_df(arg, src, strings, scope, sink) {
                    args.push(value);
                }
            }
            let name = callable_name_arity(node, src)
                .map(|(name, arity)| predicate_key(&name, arity, false));
            let result = push_df(node, DfNodeKind::CallRes, name.as_deref(), strings, sink);
            for arg in args {
                sink.edges.push(Edge::new(arg, result, DfEdgeKind::Direct));
            }
            Some(result)
        }
        "number" | "string" | "back_quoted_string" | "atom" | "list" | "dict" => {
            for child in named_children(node) {
                walk_df(child, src, strings, scope, sink);
            }
            Some(push_df(node, DfNodeKind::Lit, None, strings, sink))
        }
        "binary_operation" | "unary_operation" => {
            let kind = match operator(node, src) {
                "," | ";" | "|" | "->" | "*->" => DfNodeKind::Logic,
                "\\+" | "-" | "+" | "\\" => DfNodeKind::Unop,
                _ => DfNodeKind::Binop,
            };
            let mut inputs = Vec::new();
            for child in named_children(node) {
                if child.field_name_for_child(0).is_some() {
                    continue;
                }
                if let Some(value) = walk_df(child, src, strings, scope, sink) {
                    inputs.push(value);
                }
            }
            let result = push_df(node, kind, None, strings, sink);
            for input in inputs {
                sink.edges
                    .push(Edge::new(input, result, DfEdgeKind::Direct));
            }
            Some(result)
        }
        _ => {
            let mut last = None;
            for child in named_children(node) {
                if let Some(value) = walk_df(child, src, strings, scope, sink) {
                    last = Some(value);
                }
            }
            last
        }
    }
}

fn push_df(
    node: tree_sitter::Node,
    kind: DfNodeKind,
    name: Option<&str>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<DfF>,
) -> NodeRef {
    let node_ref = NodeRef(sink.nodes.len() as u32);
    let mut row = Node::new(
        Span {
            start: node.start_byte() as u32,
            len: 0,
        },
        kind,
    );
    if let Some(name) = name {
        row = row.with_name(strings.intern(name));
    }
    sink.nodes.push(row);
    node_ref
}

impl PrologSource {
    fn call_name_match(
        output: &ExtractOutput,
        index: &crate::seams::DefIndex,
        callee: &str,
    ) -> Option<(ContentId, Span)> {
        let call = output.call.as_ref()?;
        if let Some(node) = call.nodes.iter().find(|node| {
            node.name
                .map(|name| output.strings.lookup(name) == callee)
                .unwrap_or(false)
        }) {
            for site in corpus_defs(index, callee) {
                if site.span == node.span && site.family == FamilyTag::Call {
                    return Some((site.blob.clone(), site.span));
                }
            }
        }
        let sites: Vec<_> = corpus_defs(index, callee)
            .iter()
            .filter(|site| site.family == FamilyTag::Call)
            .collect();
        let mut blobs: Vec<_> = sites.iter().map(|site| site.blob.clone()).collect();
        blobs.sort();
        blobs.dedup();
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites.iter().find(|site| site.blob == *blob)?;
        Some((site.blob.clone(), site.span))
    }
}

impl Source for PrologSource {
    fn name(&self) -> &'static str {
        "prolog"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".pl")
            || path.ends_with(".plt")
            || path.ends_with(".pro")
            || path.ends_with(".prolog")
            || path.ends_with(".datalog")
            || path.ends_with(".horn")
    }

    fn extract(&self, _path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut output = ExtractOutput::default();
        let Ok(src) = std::str::from_utf8(content) else {
            return output;
        };
        let tree = {
            let span = trace::parse_span("prolog", "tree-sitter");
            let _entered = span.enter();
            parse(src)
        };
        let Some(tree) = tree else {
            return output;
        };
        let root = tree.root_node();
        if mask.cst {
            let span = trace::family_span("prolog", "cst");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<CstF>::default();
            project_cst(root, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.cst = Some(bundle);
        }
        if mask.types {
            let span = trace::family_span("prolog", "type");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<TypeF>::default();
            project_types(root, content, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.types = Some(bundle);
        }
        if mask.call {
            let span = trace::family_span("prolog", "call");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<CallF>::default();
            project_calls(root, content, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
            output.call = Some(bundle);
        }
        if mask.df {
            let span = trace::family_span("prolog", "df");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<DfF>::default();
            project_df(root, content, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.df = Some(bundle);
        }
        output
    }
}

impl Resolve<CallF> for PrologSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<CallF>> {
        let Some(call) = &output.call else {
            return Vec::new();
        };
        let Some(index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        call.aux
            .sites
            .iter()
            .filter_map(|site| {
                let caller = covering_def(call, site.span)?;
                let callee = output.strings.lookup(site.callee);
                let (blob, target) = Self::call_name_match(output, index, callee)?;
                Some(
                    ProjectEdge::new(caller, blob, target, CallEdgeKind::NameResolve)
                        .with_call_site(site.span),
                )
            })
            .collect()
    }
}
