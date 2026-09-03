//! Markdown extraction over tree-sitter-md's block and inline grammars.
//!
//! The block grammar owns document structure. Each block `inline` node is
//! reparsed with the inline grammar, and its named children are attached below
//! that block node with file-relative byte spans. The result stays on the
//! existing CstF plane and uses the same node/child wire shape as every other
//! syntax source.

use crate::family::{CstEdgeKind, CstF, TypeF};
use crate::lang::extract_lang::ExtractLang;
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{corpus_defs, ProjectCx, Resolve};
use crate::shape::{NameId, NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::trace;
use crate::types::{DocNode, DocNodeKind, ProjectEdge, ResolutionOrigin, TypeEdgeKind};

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

    fn extract_lang(&self, _path: &str) -> Option<ExtractLang> {
        Some(ExtractLang::Markdown)
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
// doc_nodes are a derived projection of the cst heading/fence/link nodes.

/// A heading at level L pops stack entries with level >= L before pushing.
/// Reference definitions are gathered first: a definition may follow its use.
fn project_doc_nodes(
    root: tree_sitter::Node,
    content: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let definitions = link_reference_definitions(root, content);
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
                        target: None,
                        title: None,
                        body: None,
                    });
                    stack.push((level, title));
                }
                if let Some(inline) = find_inline(n) {
                    let parent = stack.last().map(|(_, t)| *t);
                    project_inline_links(inline, content, &definitions, parent, strings, sink);
                }
            }
            "fenced_code_block" => {
                let lang = fenced_code_block_lang(n, content, strings);
                let parent = stack.last().map(|(_, t)| *t);
                let body = children
                    .iter()
                    .find(|c| c.kind() == "code_fence_content")
                    .map(|c| span(*c, 0));
                sink.aux.doc_nodes.push(DocNode {
                    span: span(n, 0),
                    kind: DocNodeKind::CodeBlock,
                    name: lang,
                    parent,
                    target: None,
                    title: None,
                    body,
                });
            }
            "indented_code_block" => {
                let parent = stack.last().map(|(_, t)| *t);
                sink.aux.doc_nodes.push(DocNode {
                    span: span(n, 0),
                    kind: DocNodeKind::CodeBlock,
                    name: strings.intern(""),
                    parent,
                    target: None,
                    title: None,
                    body: Some(span(n, 0)),
                });
            }
            "inline" => {
                let parent = stack.last().map(|(_, t)| *t);
                project_inline_links(n, content, &definitions, parent, strings, sink);
            }
            _ => to_visit.extend(children),
        }
    }
}

/// A link reference definition's destination and title, keyed by the label
/// normalized the CommonMark way: trimmed, inner whitespace collapsed, lowercased.
type LinkDefinitions = std::collections::HashMap<String, (String, Option<String>)>;

fn normalize_label(label: &str) -> String {
    label
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn link_reference_definitions(root: tree_sitter::Node, content: &[u8]) -> LinkDefinitions {
    let mut definitions = LinkDefinitions::new();
    let mut to_visit: Vec<tree_sitter::Node> = vec![root];
    while let Some(n) = to_visit.pop() {
        let mut cursor = n.walk();
        let children: Vec<tree_sitter::Node> = n.children(&mut cursor).collect();
        if n.kind() == "link_reference_definition" {
            let label = children
                .iter()
                .find(|c| c.kind() == "link_label")
                .map(|c| bracket_inner(&text_of(*c, content)));
            let destination = children
                .iter()
                .find(|c| c.kind() == "link_destination")
                .map(|c| destination_text(&text_of(*c, content)));
            let title = children
                .iter()
                .find(|c| c.kind() == "link_title")
                .map(|c| title_text(&text_of(*c, content)));
            if let (Some(label), Some(destination)) = (label, destination) {
                definitions
                    .entry(normalize_label(&label))
                    .or_insert((destination, title));
            }
            continue;
        }
        to_visit.extend(children);
    }
    definitions
}

/// The first `inline` node below a heading (`atx_heading` carries it directly,
/// `setext_heading` under a `paragraph`).
fn find_inline(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut to_visit = vec![node];
    while let Some(n) = to_visit.pop() {
        if n.kind() == "inline" {
            return Some(n);
        }
        let mut cursor = n.walk();
        to_visit.extend(n.children(&mut cursor));
    }
    None
}

/// One row per link or image inside a block `inline` node, spans made
/// file-relative; an image nested in link text gets its own row.
fn project_inline_links(
    inline: tree_sitter::Node,
    content: &[u8],
    definitions: &LinkDefinitions,
    parent: Option<NameId>,
    strings: &mut Strings,
    sink: &mut FamilyBundle<TypeF>,
) {
    let start = inline.start_byte();
    let end = inline.end_byte();
    let language = tree_sitter::Language::new(tree_sitter_md::INLINE_LANGUAGE);
    let Some(tree) = parse(&content[start..end], language) else {
        return;
    };
    let inline_content = &content[start..end];
    let mut to_visit: Vec<tree_sitter::Node> = vec![tree.root_node()];
    while let Some(n) = to_visit.pop() {
        let mut cursor = n.walk();
        let mut children: Vec<tree_sitter::Node> = n.children(&mut cursor).collect();
        children.reverse();
        let child_text = |kind: &str| {
            children
                .iter()
                .find(|c| c.kind() == kind)
                .map(|c| text_of(*c, inline_content))
        };
        let row = match n.kind() {
            "inline_link" => Some((
                DocNodeKind::Link,
                child_text("link_text")
                    .map(|t| bracket_inner(&t))
                    .unwrap_or_default(),
                child_text("link_destination").map(|d| destination_text(&d)),
                child_text("link_title").map(|t| title_text(&t)),
            )),
            "full_reference_link" | "collapsed_reference_link" | "shortcut_link" => {
                let text = child_text("link_text")
                    .map(|t| bracket_inner(&t))
                    .unwrap_or_default();
                let label = child_text("link_label")
                    .map(|l| bracket_inner(&l))
                    .unwrap_or_else(|| text.clone());
                definitions
                    .get(&normalize_label(&label))
                    .map(|(destination, title)| {
                        (
                            DocNodeKind::Link,
                            text,
                            Some(destination.clone()),
                            title.clone(),
                        )
                    })
            }
            "uri_autolink" | "email_autolink" => {
                let inner = bracket_inner(&text_of(n, inline_content));
                Some((DocNodeKind::Link, inner.clone(), Some(inner), None))
            }
            "image" => {
                let description = child_text("image_description")
                    .map(|t| bracket_inner(&t))
                    .unwrap_or_default();
                let written = child_text("link_destination").map(|d| destination_text(&d));
                if let Some(destination) = written {
                    Some((
                        DocNodeKind::Image,
                        description,
                        Some(destination),
                        child_text("link_title").map(|t| title_text(&t)),
                    ))
                } else {
                    let label = child_text("link_label")
                        .map(|l| bracket_inner(&l))
                        .unwrap_or_else(|| description.clone());
                    definitions
                        .get(&normalize_label(&label))
                        .map(|(destination, title)| {
                            (
                                DocNodeKind::Image,
                                description,
                                Some(destination.clone()),
                                title.clone(),
                            )
                        })
                }
            }
            _ => None,
        };
        if let Some((kind, name, target, title)) = row {
            sink.aux.doc_nodes.push(DocNode {
                span: span(n, start as u32),
                kind,
                name: strings.intern(&name),
                parent,
                target: target.map(|t| strings.intern(&t)),
                title: title.map(|t| strings.intern(&t)),
                body: None,
            });
        }
        to_visit.extend(children);
    }
}

/// The text between one pair of enclosing brackets (`[..]`, `<..>` or `![..]`)
/// when the node span carries them, else the text unchanged.
fn bracket_inner(text: &str) -> String {
    let text = text.strip_prefix('!').unwrap_or(text);
    let inner = match (text.chars().next(), text.chars().last()) {
        (Some('['), Some(']')) | (Some('<'), Some('>')) if text.len() >= 2 => {
            &text[1..text.len() - 1]
        }
        _ => text,
    };
    inner.to_string()
}

/// A destination as written, minus the optional `<..>` wrapper.
fn destination_text(text: &str) -> String {
    let text = text.trim();
    match (text.chars().next(), text.chars().last()) {
        (Some('<'), Some('>')) if text.len() >= 2 => text[1..text.len() - 1].to_string(),
        _ => text.to_string(),
    }
}

/// A title as written, minus its `"..."`, `'...'` or `(...)` delimiters.
fn title_text(text: &str) -> String {
    let text = text.trim();
    match (text.chars().next(), text.chars().last()) {
        (Some('"'), Some('"')) | (Some('\''), Some('\'')) | (Some('('), Some(')'))
            if text.len() >= 2 =>
        {
            text[1..text.len() - 1].to_string()
        }
        _ => text.to_string(),
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
                    ResolutionOrigin::CorpusUnique,
                ));
            }
        }
        edges
    }
}
