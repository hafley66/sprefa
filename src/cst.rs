//! CST-as-relation (christmas #3): the whole-tree enumeration that backs the
//! `node`/`child` built-in query relations. Mirrors `run_ts`'s parser setup
//! (engine.rs) but runs NO query — it enumerates every NAMED node of a file's
//! tree-sitter tree. The engine derives each node's spine id (a kind-salted
//! `_where_bytes` id) and the `child(parent, child)` edges from this Vec.
//!
//! Named children only (coordinator decision 2): anonymous punctuation tokens
//! (`(`, `,`, `;`) are dropped — codemod anchors are named nodes, and the row
//! count stays sane.

/// One tree-sitter node: its kind and byte span, plus the index (into the
/// returned Vec) of its parent. The root has `parent_ix == None`. `parent_ix`
/// references an earlier element (pre-order push), so `ids[parent_ix]` is always
/// available when the engine derives edges in a second pass.
#[derive(Clone, Debug)]
pub struct CstNode {
    pub kind: String,
    pub lo: usize,
    pub hi: usize,
    pub parent_ix: Option<usize>,
}

/// Enumerate every named node of `content` parsed with `lang`, pre-order. The
/// returned Vec's order guarantees a node's parent appears before it. Anonymous
/// (unnamed/punctuation) nodes are skipped, but their NAMED descendants are
/// reparented to the nearest named ancestor so the tree stays connected.
pub fn walk_cst(content: &str, lang: &tree_sitter::Language) -> anyhow::Result<Vec<CstNode>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(lang)?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow::anyhow!("cst parse failed"))?;

    let mut out: Vec<CstNode> = Vec::new();
    // Iterative pre-order DFS over a TreeCursor (no recursion blowup on deep
    // files). Each stack entry is (node, nearest-named-ancestor-ix). The root
    // node is always named; if a node is unnamed we keep the inherited parent
    // for its children (reparent to nearest named ancestor).
    let mut stack: Vec<(tree_sitter::Node, Option<usize>)> = vec![(tree.root_node(), None)];
    while let Some((node, parent_named_ix)) = stack.pop() {
        let my_ix = if node.is_named() {
            let ix = out.len();
            out.push(CstNode {
                kind: node.kind().to_string(),
                lo: node.start_byte(),
                hi: node.end_byte(),
                parent_ix: parent_named_ix,
            });
            Some(ix)
        } else {
            // Unnamed node: don't emit a row, but let its named descendants
            // attach to this node's nearest named ancestor.
            parent_named_ix
        };
        // Push children in reverse so they pop in source order. Order does not
        // matter for a set relation, but stable order keeps tests legible.
        let mut cursor = node.walk();
        let children: Vec<tree_sitter::Node> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push((child, my_ix));
        }
    }
    Ok(out)
}

// ── comment_node: every comment as a grammar-backed fact ─────────────────────

/// One comment span: its 1-based start/end line, 0-based byte column, and the
/// RAW text (delimiters included). `comment_node` records the raw span so an
/// `edit` can key off it; `classify_comment` derives the stripped `text` and the
/// `kind` from `raw`. Shared by the tree-sitter walk (`walk_comments`, this
/// module) and the oxc TS arm (`typegraph::ts_comments`).
#[derive(Clone, Debug)]
pub struct RawComment {
    pub start_row: u32,
    pub start_col: u32,
    pub end_row: u32,
    pub end_col: u32,
    pub raw: String,
}

/// Enumerate every comment of `content` parsed with `lang`. Grammar-backed:
/// tree-sitter names comment productions `comment` / `line_comment` /
/// `block_comment` / `multiline_comment` (grammar-dependent), so any node whose
/// kind CONTAINS "comment" is one. A `//` (or `#`, `/*`) inside a string literal
/// is lexed as part of that string, never a comment node — the string-literal
/// safety that a naive text scan can't give. Positions come straight from
/// tree-sitter (0-based row/col), normalized here to 1-based line / 0-based
/// column to match `sg`/`diag`.
pub fn walk_comments(content: &str, lang: &tree_sitter::Language) -> anyhow::Result<Vec<RawComment>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(lang)?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow::anyhow!("cst parse failed"))?;

    let mut out: Vec<RawComment> = Vec::new();
    // Full DFS over every node (comments are tree-sitter "extras", attached as
    // named siblings anywhere in the tree). A comment node is a leaf for our
    // purposes, so we don't descend into it.
    let mut stack: Vec<tree_sitter::Node> = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind().contains("comment") {
            let sp = node.start_position();
            let ep = node.end_position();
            let raw = content.get(node.start_byte()..node.end_byte()).unwrap_or("").to_string();
            out.push(RawComment {
                start_row: sp.row as u32 + 1,
                start_col: sp.column as u32,
                end_row: ep.row as u32 + 1,
                end_col: ep.column as u32,
                raw,
            });
            continue;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Ok(out)
}

/// Enumerate HTML-comment (`<!-- ... -->`) spans in a markdown file via
/// tree-sitter-md's BLOCK grammar (the same grammar `ingest::MarkdownDoc` uses
/// for headings/code blocks). Markdown has no dedicated comment node kind —
/// an HTML comment lexes as `html_block` alongside every other raw-HTML
/// island — so this walk filters `html_block` nodes to ones whose trimmed
/// text starts `<!--` and ends `-->`, instead of the generic `kind().
/// contains("comment")` test `walk_comments` uses for a real grammar comment
/// production. `todo(category): text` (the plan-doc convention) lives inside
/// this raw span; `classify_comment` strips the `<!--`/`-->` delimiters.
pub fn walk_md_comments(content: &str) -> anyhow::Result<Vec<RawComment>> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_md::LANGUAGE.into())?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| anyhow::anyhow!("markdown parse failed"))?;

    let mut out: Vec<RawComment> = Vec::new();
    let mut stack: Vec<tree_sitter::Node> = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "html_block" {
            let raw = content.get(node.start_byte()..node.end_byte()).unwrap_or("");
            let trimmed = raw.trim();
            if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
                let sp = node.start_position();
                let ep = node.end_position();
                out.push(RawComment {
                    start_row: sp.row as u32 + 1,
                    start_col: sp.column as u32,
                    end_row: ep.row as u32 + 1,
                    end_col: ep.column as u32,
                    raw: raw.to_string(),
                });
            }
            continue;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    Ok(out)
}

/// Classify a comment's RAW text into `(kind, stripped_text)`. `kind` is
/// `"doc"` for `///` / `//!` / `/**` / `/*!` markers, else `"line"` for `//` /
/// `#` line comments and `"block"` for `/* */`. `stripped_text` drops the
/// comment tokens and per-line `*`/leading whitespace (the suppress grammar
/// parses this text). Language-agnostic: keyed on the marker prefix, not the
/// tree-sitter node name, so a Rust `///` and a TS `/** */` both read as `doc`.
pub fn classify_comment(raw: &str) -> (&'static str, String) {
    let t = raw.trim();
    // `/**/` is an empty block, not a doc block.
    if (t.starts_with("///") || t.starts_with("//!"))
        // `////` (a line of slashes) is a plain line comment, not rustdoc.
        && !t.starts_with("////")
    {
        let body = t.trim_start_matches('/').trim_start_matches('!');
        return ("doc", body.trim().to_string());
    }
    if (t.starts_with("/**") || t.starts_with("/*!")) && t != "/**/" {
        return ("doc", crate::typegraph::clean_block_comment(t).trim_end().to_string());
    }
    if t.starts_with("/*") {
        return ("block", crate::typegraph::clean_block_comment(t).trim_end().to_string());
    }
    // Markdown HTML comment (`walk_md_comments`'s only shape): no doc/line
    // variant, always a block.
    if let Some(body) = t.strip_prefix("<!--").and_then(|s| s.strip_suffix("-->")) {
        return ("block", body.trim().to_string());
    }
    if let Some(body) = t.strip_prefix("//") {
        return ("line", body.trim().to_string());
    }
    if t.starts_with('#') {
        return ("line", t.trim_start_matches('#').trim().to_string());
    }
    ("line", t.to_string())
}

// LANG-JUNCTION(comment-cst-extensions): the extension -> grammar-label map feeding `comment_node` and CST node/child extraction; a label here must exist in the ast-grammars table (`ts_lang` resolves it)
/// Map a file path's extension to the `ts_lang` label, or `None` for files with
/// no compiled CST grammar (skip them). Covers the 11 languages `ts_lang`
/// supports. The engine resolves the label via `ts_lang` so this module never
/// touches the grammar registry.
pub fn lang_label_for_path(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    // Dockerfile is name-based, not extension-based.
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    if base == "dockerfile" || base.ends_with(".dockerfile") {
        return Some("dockerfile");
    }
    let ext = lower.rsplit('.').next().unwrap_or("");
    Some(match ext {
        "rs" => "rust",
        "c" | "h" => "c",
        "kt" | "kts" => "kotlin",
        "py" => "python",
        "sh" | "bash" => "bash",
        "go" => "go",
        "hcl" | "tf" => "hcl",
        "bzl" | "bazel" => "starlark",
        "jsonnet" | "libsonnet" => "jsonnet",
        // `comment_node` powers README narrative notes in the .dl example corpus.
        "dl" => "dl",
        "tmpl" | "gotmpl" | "gohtml" => "gotmpl",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_a_small_rust_file() {
        let lang = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
        let nodes = walk_cst("fn alpha() {}\n", &lang).unwrap();
        // root source_file + function_item + identifier + parameters + block, etc.
        assert!(nodes.len() >= 3, "expected several named nodes, got {nodes:?}");
        // Root is the source_file, parent None.
        assert_eq!(nodes[0].kind, "source_file");
        assert_eq!(nodes[0].parent_ix, None);
        assert_eq!(nodes[0].lo, 0);
        // Exactly one node has no parent (the root).
        assert_eq!(nodes.iter().filter(|n| n.parent_ix.is_none()).count(), 1);
        // A function_item exists and its parent is the root (named ancestor).
        let fi = nodes.iter().position(|n| n.kind == "function_item").expect("function_item");
        assert_eq!(nodes[fi].parent_ix, Some(0), "function_item's named parent is the root");
        // An identifier `alpha` is present.
        assert!(nodes.iter().any(|n| n.kind == "identifier" && &"fn alpha() {}\n"[n.lo..n.hi] == "alpha"));
    }

    #[test]
    fn walks_a_small_python_file() {
        let lang = tree_sitter::Language::new(tree_sitter_python::LANGUAGE);
        let nodes = walk_cst("def beta():\n    pass\n", &lang).unwrap();
        assert!(nodes.iter().any(|n| n.kind == "function_definition"), "python fn def: {nodes:?}");
        assert_eq!(nodes.iter().filter(|n| n.parent_ix.is_none()).count(), 1);
    }

    #[test]
    fn walk_comments_grammar_backed_string_safety() {
        let lang = tree_sitter::Language::new(tree_sitter_rust::LANGUAGE);
        // `//` inside a string literal must NOT be a comment node.
        let src = "// real\nfn f() { let s = \"// fake\"; }\n/* block */\n";
        let cs = walk_comments(src, &lang).unwrap();
        let texts: Vec<String> = cs.iter().map(|c| c.raw.clone()).collect();
        assert!(texts.iter().any(|r| r.contains("// real")), "line comment: {texts:?}");
        assert!(texts.iter().any(|r| r.contains("/* block */")), "block comment: {texts:?}");
        assert!(!texts.iter().any(|r| r.contains("fake")), "string leaked: {texts:?}");
        // 1-based line, 0-based col.
        let first = cs.iter().find(|c| c.raw.contains("// real")).unwrap();
        assert_eq!((first.start_row, first.start_col), (1, 0));
    }

    #[test]
    fn walk_md_comments_finds_html_comments_only() {
        // An HTML comment (incl. multi-line) is a row; other html_blocks and
        // markdown structure are not.
        let src = "# Title\n\n<!-- todo(perf): fix the thing\nspans lines -->\n\n<div>raw html island</div>\n\nBody `<!-- not a comment, inline code -->` text.\n";
        let cs = walk_md_comments(src).unwrap();
        assert_eq!(cs.len(), 1, "exactly the block comment: {cs:?}");
        assert_eq!((cs[0].start_row, cs[0].end_row), (3, 5), "1-based span incl. trailing newline row");
        assert!(cs[0].raw.contains("todo(perf)"));
        // classify strips the delimiters and lands kind=block.
        let (kind, text) = classify_comment(&cs[0].raw);
        assert_eq!(kind, "block");
        assert!(text.starts_with("todo(perf): fix the thing"), "{text}");
    }

    #[test]
    fn classify_comment_kinds_and_strip() {
        assert_eq!(classify_comment("// hi"), ("line", "hi".to_string()));
        assert_eq!(classify_comment("/// doc"), ("doc", "doc".to_string()));
        assert_eq!(classify_comment("//! inner doc"), ("doc", "inner doc".to_string()));
        assert_eq!(classify_comment("/* b */"), ("block", "b".to_string()));
        assert_eq!(classify_comment("/** d */"), ("doc", "d".to_string()));
        assert_eq!(classify_comment("# py"), ("line", "py".to_string()));
        // `////` and `/**/` are not doc/block-doc (the empty-block `/**/`
        // classifies as a plain block, not a doc block).
        assert_eq!(classify_comment("//// sep").0, "line");
        assert_eq!(classify_comment("/**/").0, "block");
    }

    #[test]
    fn lang_label_covers_extensions() {
        assert_eq!(lang_label_for_path("src/a.rs"), Some("rust"));
        assert_eq!(lang_label_for_path("x/foo.py"), Some("python"));
        assert_eq!(lang_label_for_path("Dockerfile"), Some("dockerfile"));
        assert_eq!(lang_label_for_path("deep/Dockerfile"), Some("dockerfile"));
        assert_eq!(lang_label_for_path("a.unknown"), None);
    }
}
