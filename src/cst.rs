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
    fn lang_label_covers_extensions() {
        assert_eq!(lang_label_for_path("src/a.rs"), Some("rust"));
        assert_eq!(lang_label_for_path("x/foo.py"), Some("python"));
        assert_eq!(lang_label_for_path("Dockerfile"), Some("dockerfile"));
        assert_eq!(lang_label_for_path("deep/Dockerfile"), Some("dockerfile"));
        assert_eq!(lang_label_for_path("a.unknown"), None);
    }
}
