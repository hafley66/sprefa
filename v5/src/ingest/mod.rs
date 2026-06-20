//! Document ingestion: a registry of grammars that emit structural relations
//! (headings, code blocks, sections) from non-source text. The first customer
//! is markdown; comment regions and other tree-sitter grammars follow the same
//! shape. Mirrors `typegraph::TypeLang`'s registry pattern but for documents:
//! an `IngestLang` declares the extensions it owns and returns `DocFacts`, which
//! the engine projects into the `doc_node` relation.
//!
//! `MarkdownDoc` parses via tree-sitter-md's block grammar (`tree_sitter_md::
//! LANGUAGE`), which gives us ATX + setext headings, fenced + indented code
//! blocks, and the structural skeleton (lists, blockquotes, paragraphs) for a
//! later widening. The block grammar alone is enough: the inline grammar
//! (`INLINE_LANGUAGE`) would add link/code/emphasis nodes, none of which the
//! `doc_node` relation needs today. Once a second concrete customer exists,
//! `IngestLang` and `TypeLang` fold into a single trait whose methods all
//! default empty -- the two shapes already agree on (file, line, kind, name,
//! parent).

use tree_sitter::{Node, Parser};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocNode {
    pub file: String,
    pub line: u32,          // 1-based
    pub kind: &'static str, // "heading" | "code_block" | ...
    pub name: String,       // heading title / code-fence language
    pub parent: String,     // enclosing heading text; "" at top level
    // Body text covered by the node. For `code_block` this is the lines between
    // the opening + closing fence (no fence markers, no language tag); for
    // `heading` it is empty. Used by the `doc_ref` bridge to name-match symbols
    // inside a code block. Not projected into the `doc_node` relation.
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocFacts {
    pub nodes: Vec<DocNode>,
}

/// A document grammar: declares the file extensions it owns and extracts
/// structural nodes. `: Sync` for rayon, matching `typegraph::TypeLang`.
pub trait IngestLang: Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn extract_docs(&self, file: &str, content: &str) -> DocFacts;
}

/// The registry. Extension overlap is resolved by order, as with `type_langs`.
pub fn ingest_langs() -> &'static [&'static dyn IngestLang] {
    &[&MarkdownDoc]
}

// ---- markdown ----------------------------------------------------------------

struct MarkdownDoc;

impl IngestLang for MarkdownDoc {
    fn name(&self) -> &'static str { "markdown" }

    fn matches(&self, p: &str) -> bool {
        p.ends_with(".md") || p.ends_with(".markdown")
    }

    fn extract_docs(&self, file: &str, content: &str) -> DocFacts {
        let mut parser = Parser::new();
        if parser.set_language(&tree_sitter_md::LANGUAGE.into()).is_err() {
            return DocFacts::default();
        }
        let Some(tree) = parser.parse(content, None) else { return DocFacts::default(); };

        let mut nodes: Vec<DocNode> = Vec::new();
        // Heading stack of (level, title). A new heading at level L pops any
        // entry with level >= L before pushing, so `parent` is the nearest
        // enclosing heading of a strictly lower level (sibling headings don't
        // nest under each other). Same algorithm as the hand-rolled extractor.
        let mut stack: Vec<(u32, String)> = Vec::new();

        let root = tree.root_node();
        // Depth-first preorder. We push children in reverse so the LIFO pop
        // restores source order, which the heading-stack algorithm relies on.
        let mut to_visit: Vec<Node> = vec![root];
        let mut cursor = root.walk();
        while let Some(n) = to_visit.pop() {
            cursor.reset(n);
            let mut children: Vec<Node> = n.children(&mut cursor).collect();
            // Reverse so the LIFO pop visits the first child first.
            children.reverse();
            // For non-leaf cases we extend the worklist; for headings/code
            // blocks we emit a node and DON'T descend (their children are
            // structural fragments the helpers below handle inline).
            match n.kind() {
                "atx_heading" | "setext_heading" => {
                    if let Some((level, title)) = heading_text(n, content) {
                        while stack.last().map_or(false, |(l, _)| *l >= level) { stack.pop(); }
                        let parent = stack.last().map(|(_, t)| t.clone()).unwrap_or_default();
                        nodes.push(DocNode {
                            file: file.to_string(),
                            line: n.start_position().row as u32 + 1,
                            kind: "heading",
                            name: title.clone(),
                            parent,
                            text: String::new(),
                        });
                        stack.push((level, title));
                    }
                }
                "fenced_code_block" => {
                    let (lang, body) = fenced_code_block_parts(n, content);
                    let parent = stack.last().map(|(_, t)| t.clone()).unwrap_or_default();
                    nodes.push(DocNode {
                        file: file.to_string(),
                        line: n.start_position().row as u32 + 1,
                        kind: "code_block",
                        name: lang,
                        parent,
                        text: body,
                    });
                }
                "indented_code_block" => {
                    let parent = stack.last().map(|(_, t)| t.clone()).unwrap_or_default();
                    nodes.push(DocNode {
                        file: file.to_string(),
                        line: n.start_position().row as u32 + 1,
                        kind: "code_block",
                        name: String::new(),
                        parent,
                        text: text_of(n, content).trim_end_matches('\n').to_string(),
                    });
                }
                _ => {
                    // Descend into everything else (section, paragraph, list,
                    // block_quote, document) so headings/code blocks nested in
                    // structural containers are still caught.
                    to_visit.extend(children);
                }
            }
        }
        // `nodes` is in source order: to_visit pops in reverse-child order, so
        // each subtree visits its first child first; emit order == source order.
        DocFacts { nodes }
    }
}

/// Pull `(level, title)` out of an `atx_heading` or `setext_heading` node. Level
/// comes from the marker/underline kind (`atx_h1_marker` / `setext_h1_underline`
/// / ...); title is the inline text with the marker stripped and whitespace
/// trimmed. Slicing the source by the inline child's byte range keeps the
/// original text verbatim (no markdown re-encoding).
fn heading_text(n: Node, src: &str) -> Option<(u32, String)> {
    let mut cursor = n.walk();
    let children: Vec<Node> = n.children(&mut cursor).collect();
    let level = children.iter().find_map(|c| match c.kind() {
        "atx_h1_marker" | "setext_h1_underline" => Some(1u32),
        "atx_h2_marker" | "setext_h2_underline" => Some(2),
        "atx_h3_marker" | "setext_h3_underline" => Some(3),
        "atx_h4_marker" | "setext_h4_underline" => Some(4),
        "atx_h5_marker" | "setext_h5_underline" => Some(5),
        "atx_h6_marker" | "setext_h6_underline" => Some(6),
        _ => None,
    })?;
    // ATX headings carry their title in an `inline` child; setext headings use a
    // `paragraph` child (the heading_content field differs by kind).
    let title = children.iter()
        .find(|c| c.kind() == "inline" || c.kind() == "paragraph")
        .map(|c| text_of(*c, src))
        .unwrap_or_default()
        .trim()
        .trim_end_matches('#')
        .trim()
        .to_string();
    Some((level, title))
}

/// `(language, body)` from a fenced_code_block. `language` is empty when no info
/// string is given. `body` is the code_fence_content text verbatim (no fence
/// markers, no language tag), trailing newline trimmed.
fn fenced_code_block_parts(n: Node, src: &str) -> (String, String) {
    let mut cursor = n.walk();
    let children: Vec<Node> = n.children(&mut cursor).collect();
    let mut lang = String::new();
    let mut body = String::new();
    for c in &children {
        match c.kind() {
            // info_string -> its `language` child if any.
            "info_string" => {
                let mut sub = c.walk();
                let info_kids: Vec<Node> = c.children(&mut sub).collect();
                if let Some(lc) = info_kids.iter().find(|x| x.kind() == "language") {
                    lang = text_of(*lc, src);
                }
            }
            "code_fence_content" => {
                body = text_of(*c, src).trim_end_matches('\n').to_string();
            }
            _ => {}
        }
    }
    (lang, body)
}

/// Slice the source by a node's byte range. tree-sitter's `text()` would do
/// this for us when given the bytes, but we already have `&str` and want to
/// avoid the round trip through `Cow<[u8]>`.
fn text_of(n: Node, src: &str) -> String {
    let s = n.start_byte();
    let e = n.end_byte();
    if s <= e && e <= src.len() { src[s..e].to_string() } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_headings_and_code_blocks_nest_by_level() {
        let md = "# Title\nsome text\n## Sub\n```rs\nfn x() {}\n```\n### Deep\n";
        let f = MarkdownDoc.extract_docs("a.md", md);
        let kinds: Vec<&str> = f.nodes.iter().map(|n| n.kind).collect();
        assert_eq!(kinds, vec!["heading", "heading", "code_block", "heading"]);
        assert_eq!(f.nodes[0].name, "Title");
        assert_eq!(f.nodes[0].parent, "");
        assert_eq!(f.nodes[1].name, "Sub");
        assert_eq!(f.nodes[1].parent, "Title");
        assert_eq!(f.nodes[2].name, "rs");
        assert_eq!(f.nodes[2].parent, "Sub");
        assert_eq!(f.nodes[2].text, "fn x() {}", "code-block body must land in .text");
        assert_eq!(f.nodes[3].name, "Deep");
        assert_eq!(f.nodes[3].parent, "Sub");
    }

    #[test]
    fn code_block_text_captures_body_until_close_fence() {
        // Multiple lines, blank line inside, closing fence not at column 0.
        let md = "```rs\nfn a() {}\n\n  let b = 1;\n   ```\n";
        let f = MarkdownDoc.extract_docs("a.md", md);
        assert_eq!(f.nodes.len(), 1);
        assert_eq!(f.nodes[0].kind, "code_block");
        assert_eq!(f.nodes[0].text, "fn a() {}\n\n  let b = 1;");
    }

    #[test]
    fn sibling_headings_do_not_nest() {
        let md = "# A\n# B\n## C\n";
        let f = MarkdownDoc.extract_docs("a.md", md);
        assert_eq!(f.nodes[1].name, "B");
        assert_eq!(f.nodes[1].parent, "", "B is a sibling of A, not a child");
        assert_eq!(f.nodes[2].name, "C");
        assert_eq!(f.nodes[2].parent, "B");
    }

    #[test]
    fn hash_run_without_space_is_not_a_heading() {
        let f = MarkdownDoc.extract_docs("a.md", "#tag\n###### ok\n");
        assert_eq!(f.nodes.len(), 1);
        assert_eq!(f.nodes[0].name, "ok");
    }

    #[test]
    fn setext_headings_parse_at_their_level() {
        // Setext: === -> h1, --- -> h2. Tree-sitter recognizes these; the
        // hand-rolled extractor missed them.
        let md = "Title One\n=========\n\nSubtitle\n--------\n";
        let f = MarkdownDoc.extract_docs("a.md", md);
        let names: Vec<&str> = f.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["Title One", "Subtitle"]);
        assert_eq!(f.nodes[1].parent, "Title One", "setext H2 nests under H1");
    }

    #[test]
    fn indented_code_block_emits_empty_language() {
        // Four-space indent is an indented code block (no info string).
        let md = "para\n\n    fn indented() {}\n    let x = 2;\n";
        let f = MarkdownDoc.extract_docs("a.md", md);
        let cb = f.nodes.iter().find(|n| n.kind == "code_block")
            .expect("indented code block");
        assert_eq!(cb.name, "", "indented code block has no language");
        assert!(cb.text.contains("fn indented()"), "body missing: {}", cb.text);
        assert!(cb.text.contains("let x = 2"), "second line missing: {}", cb.text);
    }

    #[test]
    fn nested_heading_inside_blockquote_still_extracts() {
        // Block-quote-wrapped markdown: the heading is inside a block_quote ->
        // paragraph path; the walk descends so it still gets caught. Verifies
        // the DFS recursion, which the hand-rolled line scanner didn't need.
        let md = "> # Quoted\n> body\n## After\n";
        let f = MarkdownDoc.extract_docs("a.md", md);
        let names: Vec<&str> = f.nodes.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"Quoted"), "quoted heading missing: {:?}", names);
        assert!(names.contains(&"After"), "post-quote heading missing: {:?}", names);
    }
}
