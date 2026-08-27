//! dl6 extraction over the tree-sitter-dl6 grammar.
//!
//! Planes:
//! - CstF: lossless CST node tree.
//! - CallF: rule heads as definitions (`CallKind::Free`), body goal atoms as call sites (`CallSite`),
//!   and `use` declarations as specifiers (`Specifier`).
//! - TypeF: `rel` declarations as type entities (`TypeEntityKind::Struct`), columns as fields (`TypeEdgeCandidate` / `TypeSig`).

use std::collections::BTreeSet;

use crate::family::{
    CallEdgeKind, CallF, CallKind, CallSite, CstEdgeKind, CstF, Family, ProjectEdge, SigSlot,
    Specifier, SpecifierKind, TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeF, TypeSig,
};
use crate::lang::extract_lang::ExtractLang;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{corpus_defs, covering_def, own_blob, ProjectCx, Resolve};
use crate::shape::{ContentId, FamilyTag, NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::trace;

#[derive(Default)]
pub struct DlSource;

fn parse(content: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(tree_sitter_dl6::LANGUAGE);
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

fn named_children(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
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

fn statements(root: tree_sitter::Node) -> impl Iterator<Item = tree_sitter::Node> {
    named_children(root)
        .into_iter()
        .filter(|node| node.kind() == "statement")
}

fn unwrap_statement(statement: tree_sitter::Node) -> Option<tree_sitter::Node> {
    statement.named_child(0)
}

fn unquote(raw: &str) -> String {
    if raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')))
    {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

fn project_types(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    for statement in statements(root) {
        let Some(inner) = unwrap_statement(statement) else {
            continue;
        };
        match inner.kind() {
            "relation_declaration" => {
                let Some(name_node) = field(inner, "name") else {
                    continue;
                };
                let rel_name = text(name_node, src);
                let rel_span = span(inner);
                let rel_name_id = strings.intern(rel_name);
                sink.nodes
                    .push(Node::new(rel_span, TypeEntityKind::Struct).with_name(rel_name_id));

                // The columns are not wrapped: every column binds to the same
                // `columns` field name, so `field()` would return only the first.
                walk_relation_columns(inner, rel_span, src, strings, sink);
            }
            _ => {}
        }
    }
}

fn push_column_sig(
    column: tree_sitter::Node,
    owner_span: Span,
    param_pos: u32,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let Some(type_node) = field(column, "type") else {
        return;
    };
    let type_name_id = strings.intern(text(type_node, src));
    sink.aux.sigs.push(TypeSig {
        owner: owner_span,
        slot: SigSlot::Param,
        pos: param_pos,
        ty: type_name_id,
    });
    sink.aux.candidates.push(TypeEdgeCandidate {
        owner: owner_span,
        to: type_name_id,
        kind: TypeEdgeKind::Field,
    });
}

fn walk_relation_columns(
    node: tree_sitter::Node,
    owner_span: Span,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut param_pos = 0u32;
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "declaration_parameter" | "column" => {
                push_column_sig(child, owner_span, param_pos, src, strings, sink);
                param_pos += 1;
            }
            "enum_variants" => {
                for variant in named_children(child) {
                    if variant.kind() == "enum_variant" {
                        walk_relation_columns(variant, owner_span, src, strings, sink);
                    }
                }
            }
            "enum_variant" => {
                for col in named_children(child) {
                    if col.kind() == "column" {
                        push_column_sig(col, owner_span, param_pos, src, strings, sink);
                        param_pos += 1;
                    }
                }
            }
            _ => {}
        }
    }
}

fn project_calls(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    for statement in statements(root) {
        let Some(inner) = unwrap_statement(statement) else {
            continue;
        };
        match inner.kind() {
            "rule" => {
                let rule_span = span(inner);
                let head = field(inner, "head");
                let head_name = head.and_then(|h| field(h, "name")).map(|n| text(n, src));
                if let Some(name) = head_name {
                    sink.nodes
                        .push(Node::new(rule_span, CallKind::Free).with_name(strings.intern(name)));
                }
                if let Some(body) = field(inner, "body") {
                    walk_goal_list_calls(body, src, strings, sink);
                }
            }
            "fact" => {
                if let Some(atom) = inner.named_child(0) {
                    if atom.kind() == "atom" {
                        if let Some(name_node) = field(atom, "name") {
                            let name = text(name_node, src);
                            sink.nodes.push(
                                Node::new(span(inner), CallKind::Free)
                                    .with_name(strings.intern(name)),
                            );
                        }
                    }
                }
            }
            "query" => {
                for child in named_children(inner) {
                    if child.kind() == "atom" {
                        push_atom_site(child, src, strings, sink);
                    }
                }
            }
            "match_statement" => {
                let scrutinee = field(inner, "scrutinee");
                if let Some(scrutinee) = scrutinee {
                    push_atom_site(scrutinee, src, strings, sink);
                }
                let mut cursor = inner.walk();
                for child in inner.named_children(&mut cursor) {
                    if child.kind() == "match_arm" {
                        let arm_span = span(child);
                        let head = field(child, "head");
                        let head_name = head.and_then(|h| field(h, "name")).map(|n| text(n, src));
                        if let Some(name) = head_name {
                            sink.nodes.push(
                                Node::new(arm_span, CallKind::Free).with_name(strings.intern(name)),
                            );
                        }
                        if let Some(guard) = field(child, "guard") {
                            walk_goal_list_calls(guard, src, strings, sink);
                        }
                    }
                }
            }
            "use_declaration" => {
                let path_node = field(inner, "path");
                let alias_node = field(inner, "alias");
                if let Some(path_node) = path_node {
                    let raw_path = text(path_node, src);
                    let clean_path = unquote(raw_path);
                    let path_id = strings.intern(&clean_path);
                    if let Some(alias_node) = alias_node {
                        let alias_name = text(alias_node, src);
                        sink.aux.specifiers.push(Specifier {
                            span: span(inner),
                            name: strings.intern(alias_name),
                            kind: SpecifierKind::Named,
                            module: Some(path_id),
                            imported: None,
                        });
                    } else {
                        sink.aux.specifiers.push(Specifier {
                            span: span(inner),
                            name: path_id,
                            kind: SpecifierKind::SideEffect,
                            module: None,
                            imported: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk_goal_list_calls(
    goal_list: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut cursor = goal_list.walk();
    for child in goal_list.named_children(&mut cursor) {
        walk_expr_calls(child, src, strings, sink);
    }
}

fn walk_expr_calls(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    match node.kind() {
        "atom" => {
            push_atom_site(node, src, strings, sink);
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "named_argument" {
                    if let Some(val) = field(child, "value") {
                        walk_expr_calls(val, src, strings, sink);
                    }
                } else if child != field(node, "name").unwrap_or(child) {
                    walk_expr_calls(child, src, strings, sink);
                }
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_expr_calls(child, src, strings, sink);
            }
        }
    }
}

fn push_atom_site(
    node: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let Some(name_node) = field(node, "name") else {
        return;
    };
    let path_text = text(name_node, src);
    let name_id = strings.intern(path_text);
    let (bare_name, callee_path) = if let Some(dot_idx) = path_text.rfind('.') {
        (strings.intern(&path_text[dot_idx + 1..]), Some(name_id))
    } else {
        (name_id, None)
    };
    sink.aux.sites.push(CallSite {
        span: span(node),
        callee: bare_name,
        callee_path,
    });
}

impl DlSource {
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

    pub fn type_edge_candidates(output: &ExtractOutput) -> Vec<TypeEdgeCandidate> {
        let mut set: BTreeSet<TypeEdgeCandidate> = BTreeSet::new();
        if let Some(types) = &output.types {
            for cand in &types.aux.candidates {
                set.insert(cand.clone());
            }
        }
        set.into_iter().collect()
    }
}

impl Source for DlSource {
    fn name(&self) -> &'static str {
        "dl6"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".dl6")
    }

    fn extract_lang(&self, _path: &str) -> Option<ExtractLang> {
        Some(ExtractLang::Dl6)
    }

    fn extract(&self, _path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut output = ExtractOutput::default();
        let Ok(src) = std::str::from_utf8(content) else {
            return output;
        };
        let tree = {
            let span = trace::parse_span("dl6", "tree-sitter");
            let _entered = span.enter();
            parse(src)
        };
        let Some(tree) = tree else {
            return output;
        };
        let root = tree.root_node();
        if mask.cst {
            let span = trace::family_span("dl6", "cst");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<CstF>::default();
            project_cst(root, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.cst = Some(bundle);
        }
        if mask.types {
            let span = trace::family_span("dl6", "type");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<TypeF>::default();
            project_types(root, content, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.types = Some(bundle);
        }
        if mask.call {
            let span = trace::family_span("dl6", "call");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<CallF>::default();
            project_calls(root, content, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, bundle.aux.sites.len());
            output.call = Some(bundle);
        }
        output
    }
}

impl Resolve<CallF> for DlSource {
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

impl Resolve<TypeF> for DlSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let Some(index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        let own = own_blob(output, index);
        let mut edges = Vec::new();
        for cand in Self::type_edge_candidates(output) {
            let Some(owner_ref) = types
                .nodes
                .iter()
                .position(|node| node.span == cand.owner)
                .map(|idx| NodeRef(idx as u32))
            else {
                continue;
            };
            let to_name = output.strings.lookup(cand.to);
            let sites: Vec<_> = corpus_defs(index, to_name)
                .iter()
                .filter(|site| site.family == TypeF::TAG)
                .collect();
            let matched_site = if sites.len() == 1 {
                Some(&sites[0])
            } else if let Some(own_hash) = &own {
                sites.iter().find(|site| site.blob == *own_hash)
            } else {
                None
            };
            if let Some(target) = matched_site {
                edges.push(ProjectEdge::new(
                    owner_ref,
                    target.blob.clone(),
                    target.span,
                    cand.kind,
                ));
            }
        }
        edges
    }
}
