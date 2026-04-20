//! Tier 1.6: fixed-point recursive injection at arbitrary depth.
//!
//! Demonstrates that the injection algorithm has no special-case for depth.
//! One queue, one loop, one `Parser` swapped between Languages.
//!
//! Source layout (four parser languages, three injection levels):
//!
//!   sprf op_call body              (stand-in via brace_after)
//!   └ md body                      tree-sitter-md
//!       └ rust code fence          tree-sitter-rust
//!           └ string literal       tree-sitter-json (treats string content as JSON)
//!               └ string in JSON   tree-sitter-rust (treats JSON string as Rust expr)
//!
//! The registry is the *only* thing that decides "what next". Adding a new
//! injection rule is a one-line change.
//!
//! Run: `cargo run --bin recurse`

use std::collections::HashMap;

use tree_sitter::{Language, Node, Parser, Point, Range, Tree};

const SRC: &str = r##"
md(article) {
# top heading

```rust
fn main() {
    let cfg = "{\"port\": 8080, \"hint\": \"vec![1,2]\"}";
    let n   = "42";
    println!("{}", cfg);
}
```
}
"##;

#[derive(Clone, Copy)]
struct InjectionSite {
    parent_node_id: usize,
    range:          Range,
    lang_key:       &'static str,
    depth:          usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Registry: lang_key → tree-sitter Language. The injection-decider closure
    // below picks a key from inspecting an inner Tree's nodes.
    let registry = HashMap::from([
        ("sprefa-md-stand-in", tree_sitter_md::LANGUAGE.into()),
        ("rust",               tree_sitter_rust::LANGUAGE.into()),
        ("json",               tree_sitter_json::LANGUAGE.into()),
    ]);

    let body = brace_after(SRC, "md(");
    let mut queue: Vec<InjectionSite> = vec![InjectionSite {
        parent_node_id: 0,
        range:          range_of(SRC, body),
        lang_key:       "sprefa-md-stand-in",
        depth:          0,
    }];

    let mut all_trees: Vec<(usize, &'static str, usize, Tree)> = Vec::new();
    let mut parser = Parser::new();
    let mut next_id = 1usize;

    while let Some(site) = queue.pop() {
        let lang: &Language = registry.get(site.lang_key).expect("lang in registry");
        parser.set_language(lang)?;
        parser.set_included_ranges(&[site.range])?;
        let tree = parser.parse(SRC, None).expect("parse");

        let pad = "  ".repeat(site.depth);
        println!("{pad}[depth {}] lang {:>20} bytes {}..{} -> nodes={}, has_error={}",
            site.depth,
            site.lang_key,
            site.range.start_byte,
            site.range.end_byte,
            count_nodes(tree.root_node()),
            tree.root_node().has_error());

        // Decide what (if anything) inside THIS tree to inject next.
        find_injections(&tree, site.depth, &mut next_id, |child_site| {
            let pad = "  ".repeat(child_site.depth);
            println!("{pad}  -> queueing depth {} {} bytes {}..{}",
                child_site.depth, child_site.lang_key,
                child_site.range.start_byte, child_site.range.end_byte);
            queue.push(child_site);
        });

        all_trees.push((site.parent_node_id, site.lang_key, site.depth, tree));
    }

    println!("\n== summary ==");
    println!("trees produced: {}", all_trees.len());
    println!("max depth:      {}", all_trees.iter().map(|t| t.2).max().unwrap_or(0));
    let mut by_depth: HashMap<usize, usize> = HashMap::new();
    for t in &all_trees { *by_depth.entry(t.2).or_default() += 1; }
    let mut levels: Vec<_> = by_depth.into_iter().collect(); levels.sort();
    for (d, n) in levels { println!("  depth {}: {} tree(s)", d, n); }
    Ok(())
}

/// Single decision point. Inspect a parsed tree and emit injection sites for
/// the next pass. This is where sprf's per-op `body_grammar()` plus a
/// per-grammar injections.scm would live in the real runtime.
fn find_injections(
    tree:     &Tree,
    depth:    usize,
    next_id:  &mut usize,
    mut emit: impl FnMut(InjectionSite),
) {
    walk(tree.root_node(), &mut |n| {
        // Markdown: fenced code blocks with info_string drive the next lang.
        if n.kind() == "fenced_code_block" {
            let info  = child_by_kind(n, "info_string")
                .and_then(|i| child_by_kind(i, "language"))
                .map(|l| node_text(l).trim().to_owned())
                .unwrap_or_default();
            let key: &'static str = match info.as_str() {
                "rust" => "rust",
                "json" => "json",
                _      => return,
            };
            if let Some(content) = child_by_kind(n, "code_fence_content") {
                emit(InjectionSite {
                    parent_node_id: { *next_id += 1; *next_id },
                    range:          content.range(),
                    lang_key:       key,
                    depth:          depth + 1,
                });
            }
        }
        // Rust: every string_literal whose content looks like JSON gets injected.
        if n.kind() == "string_literal" {
            // Strip the surrounding quote_chars; inject only the inner range.
            let inner = inner_string_range(n);
            let text  = &SRC[inner.start_byte..inner.end_byte];
            if looks_like_json(text) {
                emit(InjectionSite {
                    parent_node_id: { *next_id += 1; *next_id },
                    range:          inner,
                    lang_key:       "json",
                    depth:          depth + 1,
                });
            }
        }
        // JSON: every string whose content parses as a Rust expr gets injected.
        // (Demonstrates that the same algorithm flips direction at depth.)
        if n.kind() == "string_content" {
            let text = node_text(n);
            if looks_like_rust(&text) {
                emit(InjectionSite {
                    parent_node_id: { *next_id += 1; *next_id },
                    range:          n.range(),
                    lang_key:       "rust",
                    depth:          depth + 1,
                });
            }
        }
    });
}

fn looks_like_json(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with('{') && t.ends_with('}')) || (t.starts_with('[') && t.ends_with(']'))
}

fn looks_like_rust(s: &str) -> bool {
    s.contains("vec![") || s.contains("fn ") || s.contains("println!")
}

fn inner_string_range(n: Node) -> Range {
    // Skip first and last bytes (the quote chars). Naive but enough here.
    let r = n.range();
    Range {
        start_byte:  r.start_byte + 1,
        end_byte:    r.end_byte.saturating_sub(1),
        start_point: Point { row: r.start_point.row, column: r.start_point.column + 1 },
        end_point:   Point { row: r.end_point.row,   column: r.end_point.column.saturating_sub(1) },
    }
}

// -- traversal helpers (same as nest.rs) -------------------------------------

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

fn count_nodes(n: Node) -> usize {
    let mut count = 0;
    walk(n, &mut |_| count += 1);
    count
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
