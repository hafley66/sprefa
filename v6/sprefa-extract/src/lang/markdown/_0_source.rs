//! Markdown extraction over tree-sitter-md's block and inline grammars.
//!
//! The block grammar owns document structure. Each block `inline` node is
//! reparsed with the inline grammar, and its named children are attached below
//! that block node with file-relative byte spans. The result stays on the
//! existing CstF plane and uses the same node/child wire shape as every other
//! syntax source.

use crate::family::{CstEdgeKind, CstF};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::shape::{NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};

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
                let mut children: Vec<_> = inline_root
                    .children(&mut inline_root.walk())
                    .collect();
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
        if !mask.cst || std::str::from_utf8(content).is_err() {
            return output;
        }
        let language = tree_sitter::Language::new(tree_sitter_md::LANGUAGE);
        let Some(tree) = parse(content, language) else {
            return output;
        };
        let mut bundle = FamilyBundle::<CstF>::default();
        project_block_tree(
            tree.root_node(),
            content,
            &mut output.strings,
            &mut bundle,
        );
        output.cst = Some(bundle);
        output
    }
}
