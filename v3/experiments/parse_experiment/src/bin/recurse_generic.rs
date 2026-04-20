//! Tier 1.7: generic injection driver.
//!
//! Replaces `recurse.rs`'s hand-rolled `find_injections` branches with a
//! query-driven loop. Each grammar registers an `injections.scm` string,
//! compiled once to a `tree_sitter::Query`. The driver loop matches that
//! query against every parsed tree and queues `@injection.content` sites,
//! honoring `#set! injection.language` (static) and dynamic
//! `@injection.language` captures, plus `#match?` regex predicates.
//!
//! This is the shape the real v3 runtime will use: per-grammar
//! injections.scm + a registry, sprf ops stay out of injection logic.
//!
//! Run: `cargo run --bin recurse_generic`

use std::collections::HashMap;

use regex::Regex;
use tree_sitter::{
    Language, Node, Parser, Point, Query, QueryCursor, Range, StreamingIterator, Tree,
};

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

// -- injections.scm per grammar ----------------------------------------------

const MD_INJECTIONS: &str = r#"
(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content)
"#;

// Rust: any string literal whose content opens with `{` or `[` → json.
const RUST_INJECTIONS: &str = r#"
((string_literal) @injection.content
  (#match? @injection.content "^\"\\s*[\\{\\[]")
  (#set! injection.language "json"))
"#;

// JSON: any string content that looks like Rust (vec!/fn/println!) → rust.
const JSON_INJECTIONS: &str = r#"
((string_content) @injection.content
  (#match? @injection.content "(vec!|fn |println!)")
  (#set! injection.language "rust"))
"#;

// -- driver ------------------------------------------------------------------

struct GrammarSpec {
    lang:       Language,
    /// Compiled injections.scm for this grammar. None = no children possible.
    injections: Option<Query>,
}

#[derive(Clone, Copy)]
struct InjectionSite {
    parent_node_id: usize,
    range:          Range,
    lang_key:       &'static str,
    depth:          usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let registry: HashMap<&'static str, GrammarSpec> = [
        ("sprefa-md-stand-in", GrammarSpec {
            lang:       tree_sitter_md::LANGUAGE.into(),
            injections: Some(Query::new(&tree_sitter_md::LANGUAGE.into(), MD_INJECTIONS)?),
        }),
        ("rust", GrammarSpec {
            lang:       tree_sitter_rust::LANGUAGE.into(),
            injections: Some(Query::new(&tree_sitter_rust::LANGUAGE.into(), RUST_INJECTIONS)?),
        }),
        ("json", GrammarSpec {
            lang:       tree_sitter_json::LANGUAGE.into(),
            injections: Some(Query::new(&tree_sitter_json::LANGUAGE.into(), JSON_INJECTIONS)?),
        }),
    ].into_iter().collect();

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
        let spec = registry.get(site.lang_key).expect("lang in registry");
        parser.set_language(&spec.lang)?;
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

        if let Some(query) = spec.injections.as_ref() {
            run_injection_query(query, &tree, site.depth, &registry, &mut next_id, |child| {
                let pad = "  ".repeat(child.depth);
                println!("{pad}  -> queueing depth {} {} bytes {}..{}",
                    child.depth, child.lang_key,
                    child.range.start_byte, child.range.end_byte);
                queue.push(child);
            });
        }

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

/// Run `query` against `tree`, emit one InjectionSite per valid match.
///
/// Supports:
///   - `@injection.content`             — required capture, defines range
///   - `@injection.language`            — dynamic lang from captured text
///   - `(#set! injection.language "x")` — static lang via query properties
///   - `(#match? @cap "regex")`         — filter matches whose capture fails regex
fn run_injection_query(
    query:    &Query,
    tree:     &Tree,
    depth:    usize,
    registry: &HashMap<&'static str, GrammarSpec>,
    next_id:  &mut usize,
    mut emit: impl FnMut(InjectionSite),
) {
    let content_idx  = query.capture_index_for_name("injection.content");
    let language_idx = query.capture_index_for_name("injection.language");
    let Some(content_idx) = content_idx else { return };

    let src = SRC.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut it = cursor.matches(query, tree.root_node(), src);
    while let Some(m) = it.next() {
        if !check_predicates(query, m.pattern_index, m.captures, src) {
            continue;
        }

        let Some(content_node) = m
            .captures
            .iter()
            .find(|c| c.index == content_idx)
            .map(|c| c.node)
        else { continue };

        // Dynamic language takes precedence; fall back to #set!.
        let dyn_lang = language_idx.and_then(|idx| {
            m.captures.iter().find(|c| c.index == idx).map(|c| node_text(c.node))
        });
        let set_lang = query.property_settings(m.pattern_index)
            .iter()
            .find(|p| &*p.key == "injection.language")
            .and_then(|p| p.value.as_deref());

        let lang_str: &str = match (dyn_lang.as_deref(), set_lang) {
            (Some(s), _) => s.trim(),
            (None, Some(s)) => s,
            (None, None) => continue,
        };

        let Some(lang_key) = resolve_lang_key(lang_str, registry) else { continue };

        let range = if is_quoted_node(content_node) {
            inner_string_range(content_node)
        } else {
            content_node.range()
        };

        emit(InjectionSite {
            parent_node_id: { *next_id += 1; *next_id },
            range,
            lang_key,
            depth: depth + 1,
        });
    }
}

/// Evaluate `#match? @cap "regex"` predicates. Other predicates pass through.
fn check_predicates(
    query:     &Query,
    pat_idx:   usize,
    captures:  &[tree_sitter::QueryCapture],
    src:       &[u8],
) -> bool {
    for p in query.general_predicates(pat_idx) {
        if &*p.operator == "match?" {
            // args: [Capture(idx), String(regex)]
            use tree_sitter::QueryPredicateArg;
            let (Some(QueryPredicateArg::Capture(cap_idx)), Some(QueryPredicateArg::String(rx)))
                = (p.args.get(0), p.args.get(1)) else { continue };
            let Ok(re) = Regex::new(rx) else { continue };
            let Some(node) = captures.iter().find(|c| c.index == *cap_idx).map(|c| c.node)
                else { return false };
            let text = std::str::from_utf8(&src[node.byte_range()]).unwrap_or("");
            if !re.is_match(text) { return false; }
        }
    }
    true
}

fn resolve_lang_key(name: &str, registry: &HashMap<&'static str, GrammarSpec>) -> Option<&'static str> {
    registry.keys().find(|k| **k == name).copied()
}

fn is_quoted_node(n: Node) -> bool {
    matches!(n.kind(), "string_literal" | "string")
}

fn inner_string_range(n: Node) -> Range {
    let r = n.range();
    Range {
        start_byte:  r.start_byte + 1,
        end_byte:    r.end_byte.saturating_sub(1),
        start_point: Point { row: r.start_point.row, column: r.start_point.column + 1 },
        end_point:   Point { row: r.end_point.row,   column: r.end_point.column.saturating_sub(1) },
    }
}

// -- helpers (same as recurse.rs) --------------------------------------------

fn node_text(n: Node) -> String {
    SRC[n.byte_range()].to_owned()
}

fn count_nodes(n: Node) -> usize {
    let mut count = 0;
    walk(n, &mut |_| count += 1);
    count
}

fn walk<'t>(n: Node<'t>, f: &mut dyn FnMut(Node<'t>)) {
    f(n);
    let mut c = n.walk();
    for child in n.children(&mut c) { walk(child, f); }
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
