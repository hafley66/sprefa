//! Tier 1.5: arbitrary-depth recursive injection.
//!
//! Source layout:
//!   sprf op_call    body opaque to host (stand-in: brace_after string scan)
//!     md body       parsed by tree-sitter-md
//!       fenced_code_block           tree-sitter-md node
//!         info_string ("rust")       picks the inner language
//!         code_fence_content         byte range parsed by tree-sitter-rust
//!
//! Three Languages, two injection levels, all addressed in original-source
//! coordinates. Fixed-point recursion would extend this to N levels with the
//! same loop body.
//!
//! Run: `cargo run --bin nest`

use tree_sitter::{Language, Node, Parser, Point, Range, Tree};

const SRC: &str = "\
md(article) {
# heading

paragraph mentioning `code` inline.

```rust
fn answer() -> i32 { 42 }
```

trailing prose.
}
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let body = brace_after(SRC, "md(");
    println!("== level 0 (sprf, stand-in) ==");
    println!("body bytes {}..{}\n", body.start, body.end);

    let mut parser = Parser::new();

    // Level 1: parse the md body with tree-sitter-md.
    parser.set_language(&tree_sitter_md::LANGUAGE.into())?;
    parser.set_included_ranges(&[range_of(SRC, body.clone())])?;
    let md_tree = parser.parse(SRC, None).expect("md parse");
    println!("== level 1 (markdown) ==");
    println!("root.range:  {:?}", md_tree.root_node().range());
    println!("root.sexp:   {}", md_tree.root_node().to_sexp());
    println!("has_error:   {}\n", md_tree.root_node().has_error());

    // Level 2: walk the md tree for fenced code blocks, inject each per
    // info_string. Sprf's real injection table will map info string → Language;
    // here we hard-code "rust" → tree-sitter-rust.
    let mut depth_counter = 0usize;
    walk(md_tree.root_node(), &mut |n| {
        if n.kind() == "fenced_code_block" {
            depth_counter += 1;
            inject_code_fence(&mut parser, n).unwrap();
        }
    });
    println!("\n== summary ==");
    println!("level-2 injections performed: {depth_counter}");
    Ok(())
}

fn inject_code_fence(parser: &mut Parser, fence: Node) -> Result<(), Box<dyn std::error::Error>> {
    let info = child_by_kind(fence, "info_string").and_then(|n| {
        // info_string wraps a `language` child. Tree-sitter-md exposes the
        // raw text via the node range.
        child_by_kind(n, "language").or(Some(n))
    });
    let lang_text = info.map(|n| node_text(n).trim().to_owned()).unwrap_or_default();
    let content = match child_by_kind(fence, "code_fence_content") {
        Some(n) => n,
        None => return Ok(()),
    };
    let lang: Option<Language> = match lang_text.as_str() {
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "json" => Some(tree_sitter_json::LANGUAGE.into()),
        _      => None,
    };
    let Some(lang) = lang else {
        println!("(skipping fenced code block, no Language for info {:?})", lang_text);
        return Ok(());
    };

    parser.set_language(&lang)?;
    parser.set_included_ranges(&[content.range()])?;
    let inner = parser.parse(SRC, None).expect("inner parse");
    println!("\n  -> level 2 ({}) at {:?}", lang_text, content.range());
    print_tree_summary(&inner, "     ");
    Ok(())
}

fn print_tree_summary(t: &Tree, indent: &str) {
    println!("{indent}root.range: {:?}", t.root_node().range());
    println!("{indent}root.sexp:  {}", t.root_node().to_sexp());
    println!("{indent}has_error:  {}", t.root_node().has_error());
}

// -- traversal helpers -------------------------------------------------------

fn walk<'t>(n: Node<'t>, f: &mut dyn FnMut(Node<'t>)) {
    f(n);
    let mut c = n.walk();
    for child in n.children(&mut c) { walk(child, f); }
}

fn child_by_kind<'t>(n: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut c = n.walk();
    let hit = n.children(&mut c).find(|ch| ch.kind() == kind);
    hit
}

fn node_text(n: Node) -> String {
    SRC[n.byte_range()].to_owned()
}

fn brace_after(src: &str, marker: &str) -> std::ops::Range<usize> {
    let m = src.find(marker).expect("marker");
    let lbrace = src[m..].find('{').expect("lbrace") + m;
    let bytes  = src.as_bytes();
    let mut depth = 0i32;
    let mut i = lbrace;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => { depth -= 1; if depth == 0 { return (lbrace + 1)..i; } }
            _ => {}
        }
        i += 1;
    }
    panic!("unbalanced");
}

fn range_of(src: &str, r: std::ops::Range<usize>) -> Range {
    Range {
        start_byte:  r.start,
        end_byte:    r.end,
        start_point: byte_to_point(src, r.start),
        end_point:   byte_to_point(src, r.end),
    }
}

fn byte_to_point(src: &str, byte: usize) -> Point {
    let mut row = 0usize; let mut col = 0usize;
    for (i, ch) in src.char_indices() {
        if i == byte { return Point { row, column: col }; }
        if ch == '\n' { row += 1; col = 0; } else { col += ch.len_utf8(); }
    }
    Point { row, column: col }
}
