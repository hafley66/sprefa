//! Document ingestion: a registry of grammars that emit structural relations
//! (headings, code blocks, sections) from non-source text. The first customer
//! is markdown; comment regions and other tree-sitter grammars follow the same
//! shape. Mirrors `typegraph::TypeLang`'s registry pattern but for documents:
//! a `DocLang` declares the extensions it owns and returns `DocFacts`, which the
//! engine projects into the `doc_node` relation.
//!
//! v1 markdown is hand-rolled (line-prefix headings, fenced code blocks) so the
//! first customer needs no new grammar dependency; a tree-sitter-markdown
//! grammar can replace it for richer structure (inline links, lists) later.
//! Once a second concrete customer exists, `DocLang` and `TypeLang` fold into a
//! single `IngestLang` whose methods all default empty -- the two shapes already
//! agree on (file, line, kind, name, parent).

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocNode {
    pub file: String,
    pub line: u32,          // 1-based
    pub kind: &'static str, // "heading" | "code_block" | ...
    pub name: String,       // heading title / code-fence language
    pub parent: String,     // enclosing heading text; "" at top level
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocFacts {
    pub nodes: Vec<DocNode>,
}

/// A document grammar: declares the file extensions it owns and extracts
/// structural nodes. `: Sync` for rayon, matching `typegraph::TypeLang`.
pub trait DocLang: Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    fn extract_docs(&self, file: &str, content: &str) -> DocFacts;
}

/// The registry. Extension overlap is resolved by order, as with `type_langs`.
pub fn doc_langs() -> &'static [&'static dyn DocLang] {
    &[&MarkdownDoc]
}

// ---- markdown ----------------------------------------------------------------

struct MarkdownDoc;

impl DocLang for MarkdownDoc {
    fn name(&self) -> &'static str { "markdown" }

    fn matches(&self, p: &str) -> bool {
        p.ends_with(".md") || p.ends_with(".markdown")
    }

    fn extract_docs(&self, file: &str, content: &str) -> DocFacts {
        let mut nodes: Vec<DocNode> = Vec::new();
        // Heading stack of (level, title). A new heading at level L pops any
        // entry with level >= L before pushing, so `parent` is the nearest
        // enclosing heading of a strictly lower level (sibling headings don't
        // nest under each other).
        let mut stack: Vec<(u32, String)> = Vec::new();
        let mut in_fence = false;
        for (i, raw) in content.lines().enumerate() {
            let lineno = (i + 1) as u32;
            let trimmed = raw.trim_start();
            // Fenced code block: a line of >=3 backticks or ~ chars toggles. The
            // language tag follows the opening fence (```rs); the close fence
            // carries none. We record the opening as one code_block node.
            if fence_marker(trimmed).is_some() {
                in_fence = !in_fence;
                if in_fence {
                    let lang = trimmed[3..].trim();
                    nodes.push(DocNode {
                        file: file.to_string(), line: lineno,
                        kind: "code_block", name: lang.to_string(),
                        parent: stack.last().map(|(_, t)| t.clone()).unwrap_or_default(),
                    });
                }
                continue;
            }
            if in_fence { continue; }
            // ATX heading: 1-6 `#` then whitespace then title. A `#` run not
            // followed by whitespace (e.g. `#tag`) is not a heading.
            let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
            if (1..=6).contains(&hashes) {
                let rest = &trimmed[hashes..];
                if rest.starts_with(char::is_whitespace) {
                    let title = rest.trim().trim_end_matches('#').trim().to_string();
                    let level = hashes as u32;
                    while stack.last().map_or(false, |(l, _)| *l >= level) { stack.pop(); }
                    let parent = stack.last().map(|(_, t)| t.clone()).unwrap_or_default();
                    nodes.push(DocNode {
                        file: file.to_string(), line: lineno,
                        kind: "heading", name: title.clone(), parent,
                    });
                    stack.push((level, title));
                }
            }
        }
        DocFacts { nodes }
    }
}

/// A fenced-code marker line: >=3 backticks or tildides and nothing else of
/// substance (whitespace + an optional language tag after the opening fence).
fn fence_marker(line: &str) -> Option<char> {
    let c = line.chars().next()?;
    if c != '`' && c != '~' { return None; }
    if line.chars().take(3).all(|x| x == c) { Some(c) } else { None }
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
        assert_eq!(f.nodes[3].name, "Deep");
        assert_eq!(f.nodes[3].parent, "Sub");
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
}
