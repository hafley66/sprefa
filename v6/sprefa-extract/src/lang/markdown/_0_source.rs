//! Markdown extraction over tree-sitter-md's block and inline grammars.
//!
//! The block grammar owns document structure. Each block `inline` node is
//! reparsed with the inline grammar, and its named children are attached below
//! that block node with file-relative byte spans. The result stays on the
//! existing CstF plane and uses the same node/child wire shape as every other
//! syntax source.

use crate::family::{CstEdgeKind, CstF, TypeF};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{corpus_defs, ProjectCx, Resolve};
use crate::shape::{NameId, NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::trace;
use crate::types::{DocNode, DocNodeKind, ProjectEdge, TypeEdgeKind};

#[derive(Default)]
pub struct MarkdownSource;

fn parse(content: &[u8], language: tree_sitter::Language) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(content, None)
}

fn span(node: tree_sitter::Node, offset: u32) -> Span {
    Span {
        start: offset + node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

fn push_tree(
    root: tree_sitter::Node,
    offset: u32,
    parent: Option<NodeRef>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CstF>,
) {
    let mut stack = vec![(root, parent)];
    while let Some((node, parent)) = stack.pop() {
        let current = if node.is_named() {
            let node_ref = NodeRef(sink.nodes.len() as u32);
            sink.nodes
                .push(Node::new(span(node, offset), strings.intern(node.kind())));
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

fn project_block_tree(
    root: tree_sitter::Node,
    content: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CstF>,
) {
    let mut stack = vec![(root, None)];
    while let Some((node, parent)) = stack.pop() {
        let current = if node.is_named() {
            let node_ref = NodeRef(sink.nodes.len() as u32);
            sink.nodes
                .push(Node::new(span(node, 0), strings.intern(node.kind())));
            if let Some(parent) = parent {
                sink.edges
                    .push(Edge::new(parent, node_ref, CstEdgeKind::Child));
            }
            Some(node_ref)
        } else {
            parent
        };

        if node.kind() == "inline" {
            let start = node.start_byte();
            let end = node.end_byte();
            let language = tree_sitter::Language::new(tree_sitter_md::INLINE_LANGUAGE);
            if let Some(tree) = parse(&content[start..end], language) {
                let inline_root = tree.root_node();
                let mut children: Vec<_> = inline_root.children(&mut inline_root.walk()).collect();
                children.reverse();
                for child in children {
                    push_tree(child, start as u32, current, strings, sink);
                }
            }
            continue;
        }

        let mut children: Vec<_> = node.children(&mut node.walk()).collect();
        children.reverse();
        for child in children {
            stack.push((child, current));
        }
    }
}

impl Source for MarkdownSource {
    fn name(&self) -> &'static str {
        "markdown"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".md") || path.ends_with(".markdown")
    }

    fn extract(&self, _path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut output = ExtractOutput::default();
        if std::str::from_utf8(content).is_err() {
            return output;
        }
        let language = tree_sitter::Language::new(tree_sitter_md::LANGUAGE);
        let tree = {
            let span = trace::parse_span("markdown", "tree-sitter");
            let _entered = span.enter();
            parse(content, language)
        };
        let Some(tree) = tree else {
            return output;
        };
        if mask.cst {
            let span = trace::family_span("markdown", "cst");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<CstF>::default();
            project_block_tree(tree.root_node(), content, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.cst = Some(bundle);
        }
        if mask.types && !mask.cst {
            let span = trace::family_span("markdown", "type");
            let _entered = span.enter();
            let mut bundle = FamilyBundle::<TypeF>::default();
            project_doc_nodes(tree.root_node(), content, &mut output.strings, &mut bundle);
            trace::record_bundle(&span, &bundle, 0);
            output.types = Some(bundle);
        }
        output
    }
}

// The doc structure lives on the types plane only when cst is not requested:
// doc_nodes are a derived projection of the cst heading/fence nodes.

/// Project the heading stack + fenced code blocks into `TypeFAux.doc_nodes`.
/// A heading at level L pops stack entries with level >= L before pushing.
fn project_doc_nodes(
    root: tree_sitter::Node,
    content: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let mut stack: Vec<(u32, NameId)> = Vec::new();
    let mut to_visit: Vec<tree_sitter::Node> = vec![root];
    let mut cursor = root.walk();
    while let Some(n) = to_visit.pop() {
        cursor.reset(n);
        let mut children: Vec<tree_sitter::Node> = n.children(&mut cursor).collect();
        children.reverse();
        match n.kind() {
            "atx_heading" | "setext_heading" => {
                if let Some((level, title)) = heading_text(n, content, strings) {
                    while stack.last().map_or(false, |(l, _)| *l >= level) {
                        stack.pop();
                    }
                    let parent = stack.last().map(|(_, t)| *t);
                    sink.aux.doc_nodes.push(DocNode {
                        span: span(n, 0),
                        kind: DocNodeKind::Heading,
                        name: title,
                        parent,
                    });
                    stack.push((level, title));
                }
            }
            "fenced_code_block" => {
                let lang = fenced_code_block_lang(n, content, strings);
                let parent = stack.last().map(|(_, t)| *t);
                sink.aux.doc_nodes.push(DocNode {
                    span: span(n, 0),
                    kind: DocNodeKind::CodeBlock,
                    name: lang,
                    parent,
                });
            }
            _ => to_visit.extend(children),
        }
    }
}

/// `(level, interned title)` from an `atx_heading` / `setext_heading`; level
/// from the marker kind, title the inline text with the marker stripped.
fn heading_text(
    node: tree_sitter::Node,
    content: &[u8],
    strings: &mut Strings,
) -> Option<(u32, NameId)> {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
    let level = children.iter().find_map(|c| match c.kind() {
        "atx_h1_marker" | "setext_h1_underline" => Some(1u32),
        "atx_h2_marker" | "setext_h2_underline" => Some(2),
        "atx_h3_marker" | "setext_h3_underline" => Some(3),
        "atx_h4_marker" | "setext_h4_underline" => Some(4),
        "atx_h5_marker" | "setext_h5_underline" => Some(5),
        "atx_h6_marker" | "setext_h6_underline" => Some(6),
        _ => None,
    })?;
    let title = children
        .iter()
        .find(|c| c.kind() == "inline" || c.kind() == "paragraph")
        .map(|c| text_of(*c, content))
        .unwrap_or_default()
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_string();
    Some((level, strings.intern(&title)))
}

/// The fence language from a `fenced_code_block`, interned. Empty when no info
/// string names one.
fn fenced_code_block_lang(
    node: tree_sitter::Node,
    content: &[u8],
    strings: &mut Strings,
) -> NameId {
    let mut cursor = node.walk();
    let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
    let mut lang = String::new();
    for c in &children {
        if c.kind() == "info_string" {
            let mut sub = c.walk();
            let info_kids: Vec<tree_sitter::Node> = c.children(&mut sub).collect();
            if let Some(lc) = info_kids.iter().find(|x| x.kind() == "language") {
                lang = text_of(*lc, content);
            }
        }
    }
    strings.intern(&lang)
}

/// Slice the source bytes by a node's byte range.
fn text_of(node: tree_sitter::Node, content: &[u8]) -> String {
    let s = node.start_byte();
    let e = node.end_byte();
    if s <= e && e <= content.len() {
        String::from_utf8_lossy(&content[s..e]).to_string()
    } else {
        String::new()
    }
}

// For this arm `src` indexes `TypeFAux.doc_nodes`, not the node vec.
impl Resolve<TypeF> for MarkdownSource {
    fn resolve(&self, output: &ExtractOutput, cx: &ProjectCx) -> Vec<ProjectEdge<TypeF>> {
        let Some(types) = &output.types else {
            return Vec::new();
        };
        let Some(index) = cx.indexes.def_index.get() else {
            return Vec::new();
        };
        let mut edges = Vec::new();
        for (ix, node) in types.aux.doc_nodes.iter().enumerate() {
            if node.kind != DocNodeKind::Heading {
                continue;
            }
            let name = output.strings.lookup(node.name);
            let sites = corpus_defs(index, name);
            if let [site] = sites {
                edges.push(ProjectEdge::new(
                    NodeRef(ix as u32),
                    site.blob.clone(),
                    site.span,
                    TypeEdgeKind::DocRef,
                ));
            }
        }
        edges
    }
}
