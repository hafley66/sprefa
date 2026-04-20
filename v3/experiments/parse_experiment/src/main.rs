//! v3 parsing approach validation: tree-sitter parser-with-set_included_ranges
//! to compose grammars at parse time. No grammar authoring required for this
//! tier; uses tree-sitter-json and tree-sitter-rust as the per-op languages.
//!
//! What this proves:
//!   - one Parser swapping between Languages
//!   - set_included_ranges with absolute byte ranges from the original source
//!   - byte/row/col positions in inner trees are real source positions
//!   - error tolerance survives across the injection boundary
//!
//! What this defers (Tier 2):
//!   - writing tree-sitter-sprefa host grammar
//!   - recursive host-into-host injection (DefaultFork brace bodies)
//!   - incremental reparse via Tree::edit
//!
//! Run: `cargo run --bin inject`

use tree_sitter::{Language, Parser, Point, Range};

// Sprf-shaped surface: each op_call has a single pair of outer { ... }; the
// body itself can contain balanced sub-braces from the inner language.
const SRC: &str = "\
op1(json) { {\"key\": \"value\", \"n\": 42} }
op2(rust) { fn foo() -> i32 { 0 } }
op3(json) { not_valid_json: at all }
";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Stand-in for the host parse + walk pass: hand-pick (lang, brace-body)
    // pairs by string search. Replace with real host-grammar walk in Tier 2.
    let injections: Vec<(&str, std::ops::Range<usize>)> = vec![
        ("json", brace_after(SRC, "op1")),
        ("rust", brace_after(SRC, "op2")),
        ("json", brace_after(SRC, "op3")),
    ];

    let mut parser = Parser::new();

    for (lang_name, byte_range) in injections {
        let lang: Language = match lang_name {
            "json" => tree_sitter_json::LANGUAGE.into(),
            "rust" => tree_sitter_rust::LANGUAGE.into(),
            _ => unreachable!(),
        };
        parser.set_language(&lang)?;

        let included = Range {
            start_byte:  byte_range.start,
            end_byte:    byte_range.end,
            start_point: byte_to_point(SRC, byte_range.start),
            end_point:   byte_to_point(SRC, byte_range.end),
        };
        parser.set_included_ranges(&[included])?;

        let tree = parser.parse(SRC, None).expect("parse");
        let root = tree.root_node();

        println!("---- [{}] body bytes {}..{} ----",
            lang_name, byte_range.start, byte_range.end);
        println!("body text: {:?}", &SRC[byte_range.clone()]);
        println!("root.range: {:?}", root.range());
        println!("root.sexp:  {}", root.to_sexp());
        println!("has_error:  {}", root.has_error());
        if root.has_error() {
            walk_errors(root, 0);
        }
        println!();
    }

    // Always clear included_ranges before reusing the parser for an unbounded
    // parse, otherwise the previous restriction silently persists.
    parser.set_included_ranges(&[])?;

    Ok(())
}

// -- helpers -----------------------------------------------------------------

fn brace_after(src: &str, marker: &str) -> std::ops::Range<usize> {
    let m = src.find(marker).expect("marker");
    let lbrace = src[m..].find('{').expect("lbrace") + m;
    let bytes  = src.as_bytes();
    let mut depth = 0i32;
    let mut i = lbrace;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => { depth -= 1; if depth == 0 {
                // Body excludes the outer braces themselves; matches how a
                // host grammar would expose `body` as the child of brace_slot.
                return (lbrace + 1)..i;
            }}
            _ => {}
        }
        i += 1;
    }
    panic!("unbalanced braces after {:?}", marker);
}

fn byte_to_point(src: &str, byte: usize) -> Point {
    let mut row    = 0u32;
    let mut col    = 0u32;
    for (i, ch) in src.char_indices() {
        if i == byte { return Point { row: row as usize, column: col as usize } }
        if ch == '\n' { row += 1; col = 0; } else { col += ch.len_utf8() as u32; }
    }
    Point { row: row as usize, column: col as usize }
}

fn walk_errors(n: tree_sitter::Node, depth: usize) {
    let pad = "  ".repeat(depth);
    if n.is_error() || n.is_missing() {
        println!("{}{} at {:?}", pad,
            if n.is_missing() { "MISSING" } else { "ERROR" }, n.range());
    }
    let mut c = n.walk();
    for child in n.children(&mut c) { walk_errors(child, depth + 1); }
}
