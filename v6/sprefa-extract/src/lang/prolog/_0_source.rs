//! Prolog extraction over the DataGrout tree-sitter grammar.
//!
//! One parse feeds all four planes. Predicate identities include arity:
//! `name/2` for clauses and `name//2` for DCGs. The clause span is the
//! definition span, so body call sites are contained by their caller and the
//! shared `covering_def` resolver works without Prolog-specific phase-2 state.

use std::collections::HashMap;

use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstEdgeKind, CstF, DfEdgeKind, DfF, DfNodeKind,
    ProjectEdge, Specifier, SpecifierKind, TypeEntityKind, TypeF,
};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{corpus_defs, covering_def, ProjectCx, Resolve};
use crate::shape::{BlobHash, FamilyTag, NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};

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
            continue;
        };
        let Some((name, arity)) = callable_name_arity(head, src) else {
            continue;
        };
        sink.nodes.push(
            Node::new(span(clause), CallKind::Free)
                .with_name(strings.intern(&predicate_key(&name, arity, dcg))),
        );
        if let Some(body) = body {
            walk_goals(body, src, dcg, strings, sink, None);
        }
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
    if name != "use_module" && name != "ensure_loaded" && name != "consult" {
        return;
    }
    let mut cursor = operand.walk();
    let Some(source) = operand
        .children_by_field_name("argument", &mut cursor)
        .next()
    else {
        return;
    };
    sink.aux.specifiers.push(Specifier {
        span: span(source),
        name: strings.intern(text(source, src)),
        kind: SpecifierKind::SideEffect,
    });
}

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
            if operator(node, src) == "\\+" {
                if let Some(operand) = field(node, "operand") {
                    walk_goals(operand, src, dcg, strings, sink, module);
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
                _ => push_site(node, op, 2, false, strings, sink, module),
            }
        }
        "compound_term" => {
            if let Some((name, arity)) = callable_name_arity(node, src) {
                push_site(node, &name, arity, dcg, strings, sink, module);
            }
        }
        "atom" | "unquoted_atom" | "quoted_atom" | "operator_atom" => {
            push_site(node, &atom_text(node, src), 0, dcg, strings, sink, module);
        }
        "cut" => push_site(node, "!", 0, false, strings, sink, module),
        _ => {}
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
    ) -> Option<(BlobHash, Span)> {
        let call = output.call.as_ref()?;
        if let Some(node) = call.nodes.iter().find(|node| {
            node.name
                .map(|name| output.strings.lookup(name) == callee)
                .unwrap_or(false)
        }) {
            for site in corpus_defs(index, callee) {
                if site.span == node.span && site.family == FamilyTag::Call {
                    return Some((site.blob, site.span));
                }
            }
        }
        let sites: Vec<_> = corpus_defs(index, callee)
            .iter()
            .filter(|site| site.family == FamilyTag::Call)
            .collect();
        let mut blobs: Vec<_> = sites.iter().map(|site| site.blob).collect();
        blobs.sort_by_key(|blob| blob.0);
        blobs.dedup();
        let [blob] = blobs.as_slice() else {
            return None;
        };
        let site = sites.iter().find(|site| site.blob == *blob)?;
        Some((site.blob, site.span))
    }
}

impl Source for PrologSource {
    fn name(&self) -> &'static str {
        "prolog"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".pl")
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
        let Some(tree) = parse(src) else {
            return output;
        };
        let root = tree.root_node();
        if mask.cst {
            let mut bundle = FamilyBundle::<CstF>::default();
            project_cst(root, &mut output.strings, &mut bundle);
            output.cst = Some(bundle);
        }
        if mask.types {
            let mut bundle = FamilyBundle::<TypeF>::default();
            project_types(root, content, &mut output.strings, &mut bundle);
            output.types = Some(bundle);
        }
        if mask.call {
            let mut bundle = FamilyBundle::<CallF>::default();
            project_calls(root, content, &mut output.strings, &mut bundle);
            output.call = Some(bundle);
        }
        if mask.df {
            let mut bundle = FamilyBundle::<DfF>::default();
            project_df(root, content, &mut output.strings, &mut bundle);
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
                Some(ProjectEdge::new(
                    caller,
                    blob,
                    target,
                    CallEdgeKind::NameResolve,
                ).with_call_site(site.span))
            })
            .collect()
    }
}
