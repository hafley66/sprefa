//! Extract-function proposer (Track 2): over a file's source, find verbatim
//! duplicated line-blocks and infer the extract-fn signature for each by
//! lexical-scope analysis of the tree-sitter tree. The free variables (read in
//! the block, not bound inside it) become the proposed fn's params.
//!
//! This is the Rust port of the validated python prototype
//! (`/tmp/infer_sig.py` + `/tmp/propose.py`), reusing sprefa's own
//! `tree_sitter_rust` grammar. The engine exposes it as the lazy built-in
//! relation `propose_extract(path, lo, hi, param)` — one row per (block, param)
//! — so a dl rule can query refactor recommendations.
//!
//! Two stages, mirroring the prototype:
//!   1. `dup_blocks`: maximal verbatim duplicated consecutive-line runs (lexical).
//!   2. `free_vars`: scope-aware walk. A read is free iff no enclosing scope
//!      binds it; the RHS/value of a binding is read BEFORE the pattern enters
//!      scope; blocks/for/while/if-let/match-arms/closures each push a frame so
//!      a binding in a sub-scope never masks a param for reads outside it.
//!      Only bindings and reads inside the block's byte window count, so an
//!      enclosing `let` correctly stays a free var (param) for the block.

use std::collections::{HashMap, HashSet};

use tree_sitter::{Node, Parser};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub lo: usize,
    pub hi: usize,
    pub occurrences: usize,
    pub gain: usize,
    pub params: Vec<String>,
}

/// One extract-fn proposal per verbatim-duplicated block in `content`.
/// `lo`/`hi` are 1-based line numbers of the block's first occurrence.
/// `occurrences` is how many times the maximal run appears; `gain` is the
/// predicted dup-removal reward (`lines × (occurrences − 1)` — the structural
/// signal that validated 97% vs raw LOC's 50%). Sorted by `gain` desc.
pub fn extract_proposals(content: &str) -> Vec<Proposal> {
    let lines: Vec<&str> = content.lines().collect();
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(content, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let root = tree.root_node();
    let line_start = line_start_bytes(content);

    let mut out = Vec::new();
    for (a, n, occ) in dup_blocks(&lines, 6) {
        let lo_byte = line_start[a];
        let hi_byte = line_start.get(a + n).copied().unwrap_or(content.len());
        let params = free_vars(root, content, lo_byte, hi_byte);
        out.push(Proposal {
            lo: a + 1, hi: a + n, occurrences: occ,
            gain: n * occ.saturating_sub(1),
            params,
        });
    }
    out.sort_by(|x, y| y.gain.cmp(&x.gain));
    out
}

/// Render a proposal as Rust source: the inferred signature (params untyped —
/// a sketch to read, not yet compile) wrapping the verbatim block. The body is
/// the duplicated text as-is; a human (or a later typed codegen pass) applies it.
pub fn render_proposal(p: &Proposal, content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let body: String = lines[p.lo - 1..p.hi].iter()
        .map(|l| format!("    {l}")).collect::<Vec<_>>().join("\n");
    let dedup_note = if p.occurrences > 2 {
        format!("\n  // removes {} duplicated copies", p.occurrences - 1)
    } else {
        String::new()
    };
    format!(
        "// gain: {} ({} lines x {} occurrences)\nfn extracted_{}({}) {{\n{}\n}}{}",
        p.gain, p.hi - p.lo + 1, p.occurrences,
        p.lo, p.params.join(", "), body,
        dedup_note,
    )
}

fn lang() -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)
}

/// Byte offset of the start of each line (line 0 == offset 0). Used to turn a
/// 1-based line range into the byte window the scope walk filters on.
fn line_start_bytes(content: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in content.bytes().enumerate() {
        if b == b'\n' {
            v.push(i + 1);
        }
    }
    v.push(content.len());
    v
}

/// Maximal matching runs over ANY equatable sequence: used by the verbatim
/// detector (over raw lines) and the AST-shape detector (over normalized token
/// kinds). Returns `(start, len, occurrences)` for each maximal run, dropping
/// runs subsumed by a longer one sharing the same start. This is the
/// kernel-agnostic matcher; the feature extractor is the only thing that
/// differs between detectors.
fn matching_runs<T>(items: &[T], seed: usize) -> Vec<(usize, usize, usize)>
where
    T: Eq + std::hash::Hash,
{
    let mut buckets: HashMap<&[T], Vec<usize>> = HashMap::new();
    if items.len() < seed {
        return Vec::new();
    }
    for i in 0..=items.len() - seed {
        buckets.entry(&items[i..i + seed]).or_default().push(i);
    }
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut blocks: Vec<(usize, usize, usize)> = Vec::new();
    for idxs in buckets.values() {
        if idxs.len() < 2 {
            continue;
        }
        let (a, b) = (idxs[0], idxs[1]);
        if !seen.insert((a, b)) {
            continue;
        }
        let mut n = seed;
        while a + n < items.len() && b + n < items.len() && items[a + n] == items[b + n] {
            n += 1;
        }
        let distinct: HashSet<&T> = items[a..a + n].iter().collect();
        if distinct.len() < 3 {
            continue;
        }
        let occ = idxs.iter().filter(|&&i| {
            i + n <= items.len() && items[i..i + n] == items[a..a + n]
        }).count();
        blocks.push((a, n, occ));
    }
    blocks.sort_by(|x, y| y.1.cmp(&x.1));
    let mut kept: Vec<(usize, usize, usize)> = Vec::new();
    for (a, n, occ) in blocks {
        let subsumed = kept.iter().any(|(ka, kn, _)| *ka <= a && a + n <= *ka + *kn);
        if !subsumed {
            kept.push((a, n, occ));
        }
    }
    kept
}

/// Maximal verbatim duplicated consecutive-line runs of length >= `seed`
/// (the verbatim detector's feature extractor + the generic matcher).
/// `(a, n, occ)`: first start index (0-based), run length, occurrence count.
fn dup_blocks(lines: &[&str], seed: usize) -> Vec<(usize, usize, usize)> {
    matching_runs(lines, seed)
}

/// AST-shape (Type-2 clone) detector. Normalizes the CST leaf stream — every
/// identifier becomes `ID`, every literal `LIT`, punctuation/keywords kept — so
/// two regions with the same structure but different names match. Catches what
/// the verbatim detector misses (`for x in y` vs `for a in b`, `ext.insert` vs
/// `map.insert`). Same matcher (`matching_runs`), different feature extractor.
const AST_SEED: usize = 12;

pub fn ast_shape_proposals(content: &str) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let toks = ast_shape_tokens(&tree, content);
    let kinds: Vec<&str> = toks.iter().map(|(k, _)| k.as_str()).collect();
    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in matching_runs(&kinds, AST_SEED) {
        let lo_line = toks[a].1;
        let hi_line = toks[a + n - 1].1;
        if hi_line <= lo_line {
            continue; // require a multi-line shape, not a one-liner idiom
        }
        let lo_byte = line_start[lo_line - 1];
        let hi_byte = line_start.get(hi_line).copied().unwrap_or(content.len());
        let params = free_vars(root, content, lo_byte, hi_byte);
        out.push(Proposal {
            lo: lo_line, hi: hi_line, occurrences: occ,
            gain: n * occ.saturating_sub(1),
            params,
        });
    }
    out.sort_by(|x, y| y.gain.cmp(&x.gain));
    out
}

/// Pre-order leaf stream of the CST as `(normalized_kind, 1-based_line)`. Leaf
/// tokens only (child_count 0): identifiers -> `ID`, literals -> `LIT`, all
/// punctuation/keyword leaves keep their kind (the structural skeleton).
/// Comments are dropped by source-text check (robust to the grammar's comment
/// node naming) — they repeat shape but aren't code structure.
fn ast_shape_tokens(tree: &tree_sitter::Tree, content: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.child_count() == 0 {
            let txt = &content[n.start_byte()..n.end_byte()];
            if txt.starts_with("//") || txt.starts_with("/*") {
                continue;
            }
            let k = match n.kind() {
                "identifier" | "metavariable" => "ID".to_string(),
                kk if kk.ends_with("_literal") => "LIT".to_string(),
                kk => kk.to_string(),
            };
            out.push((k, n.start_position().row + 1));
        } else {
            let mut cur = n.walk();
            let ch: Vec<Node> = n.children(&mut cur).collect();
            for c in ch.into_iter().rev() {
                stack.push(c);
            }
        }
    }
    out
}

fn is_keyword(s: &str) -> bool {
    matches!(s,
        "self" | "Self" | "super" | "crate" | "return" | "if" | "else" | "for"
        | "while" | "loop" | "match" | "let" | "mut" | "ref" | "move" | "in"
        | "as" | "fn" | "use" | "pub" | "struct" | "enum" | "impl" | "trait"
        | "mod" | "where" | "dyn" | "async" | "await" | "break" | "continue"
        | "const" | "static" | "unsafe" | "extern" | "type" | "true" | "false"
        | "Some" | "None" | "Ok" | "Err"
    )
}

/// Field name of a `.field`, any name-part of a `path::Name`, or any token
/// inside a macro's `token_tree`/`macro_invocation` (macro bodies are opaque
/// tokens, not scoped references).
fn should_skip(n: Node) -> bool {
    let p = match n.parent() {
        Some(p) => p,
        None => return false,
    };
    match p.kind() {
        "field_expression" => p.child_by_field_name("field")
            .map_or(false, |f| f.start_byte() == n.start_byte() && f.end_byte() == n.end_byte()),
        "scoped_identifier" | "scoped_type_identifier" | "token_tree" | "macro_invocation" => true,
        _ => false,
    }
}

fn text<'a>(n: Node, src: &'a str) -> &'a str {
    &src[n.start_byte()..n.end_byte()]
}

/// Add pattern-bound names whose binding site is in `[lo, hi)` to the current
/// scope frame. Out-of-window bindings are skipped so an enclosing `let` stays
/// a free var (param) for an extracted block.
fn bind_in_win(pat: Node, src: &str, scopes: &mut Vec<HashSet<String>>, lo: usize, hi: usize) {
    let mut stack = vec![pat];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" {
            let nm = text(n, src);
            if nm != "_" && lo <= n.start_byte() && n.start_byte() < hi {
                scopes.last_mut().unwrap().insert(nm.to_string());
            }
        }
        let mut cur = n.walk();
        for c in n.children(&mut cur) {
            stack.push(c);
        }
    }
}

fn free_vars(root: Node, src: &str, lo: usize, hi: usize) -> Vec<String> {
    let mut scopes: Vec<HashSet<String>> = vec![HashSet::new()];
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    walk(root, src, &mut scopes, lo, hi, &mut out, &mut seen);
    out
}

fn walk(
    node: Node,
    src: &str,
    scopes: &mut Vec<HashSet<String>>,
    lo: usize,
    hi: usize,
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    let in_win = |n: Node| lo <= n.start_byte() && n.start_byte() < hi;
    match node.kind() {
        "identifier" => {
            if !in_win(node) {
                return;
            }
            let name = text(node, src);
            if is_keyword(name) || should_skip(node) {
                return;
            }
            if !scopes.iter().any(|s| s.contains(name)) {
                if seen.insert(name.to_string()) {
                    out.push(name.to_string());
                }
            }
            return;
        }
        "let_declaration" | "let_condition" => {
            if let Some(v) = node.child_by_field_name("value") {
                walk(v, src, scopes, lo, hi, out, seen);
            }
            if let Some(p) = node.child_by_field_name("pattern") {
                bind_in_win(p, src, scopes, lo, hi);
            }
            return;
        }
        "for_expression" => {
            if let Some(v) = node.child_by_field_name("value") {
                walk(v, src, scopes, lo, hi, out, seen);
            }
            scopes.push(HashSet::new());
            if let Some(p) = node.child_by_field_name("pattern") {
                bind_in_win(p, src, scopes, lo, hi);
            }
            if let Some(b) = node.child_by_field_name("body") {
                walk(b, src, scopes, lo, hi, out, seen);
            }
            scopes.pop();
            return;
        }
        "while_expression" => {
            scopes.push(HashSet::new());
            if let Some(c) = node.child_by_field_name("condition") {
                walk(c, src, scopes, lo, hi, out, seen);
            }
            if let Some(b) = node.child_by_field_name("body") {
                walk(b, src, scopes, lo, hi, out, seen);
            }
            scopes.pop();
            return;
        }
        "if_expression" => {
            let cond = node.child_by_field_name("condition");
            let cons = node.child_by_field_name("consequence");
            let alt = node.child_by_field_name("alternative");
            if let Some(c) = cond {
                scopes.push(HashSet::new());
                walk(c, src, scopes, lo, hi, out, seen);
                if let Some(cs) = cons {
                    walk(cs, src, scopes, lo, hi, out, seen);
                }
                scopes.pop();
            } else if let Some(cs) = cons {
                walk(cs, src, scopes, lo, hi, out, seen);
            }
            if let Some(a) = alt {
                walk(a, src, scopes, lo, hi, out, seen);
            }
            return;
        }
        "closure_expression" => {
            scopes.push(HashSet::new());
            if let Some(p) = node.child_by_field_name("parameters") {
                bind_in_win(p, src, scopes, lo, hi);
            }
            if let Some(b) = node.child_by_field_name("body") {
                walk(b, src, scopes, lo, hi, out, seen);
            }
            scopes.pop();
            return;
        }
        "match_arm" => {
            scopes.push(HashSet::new());
            let mut past = false;
            let mut cur = node.walk();
            for cc in node.children(&mut cur) {
                if cc.kind() == "=>" {
                    past = true;
                    continue;
                }
                if !past {
                    bind_in_win(cc, src, scopes, lo, hi);
                } else {
                    walk(cc, src, scopes, lo, hi, out, seen);
                }
            }
            scopes.pop();
            return;
        }
        "block" => {
            scopes.push(HashSet::new());
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                walk(c, src, scopes, lo, hi, out, seen);
            }
            scopes.pop();
            return;
        }
        _ => {
            let mut cur = node.walk();
            for c in node.children(&mut cur) {
                walk(c, src, scopes, lo, hi, out, seen);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the two hand-extracted fns' free vars must equal their params.
    /// `bind_whole_match_span`'s body reads exactly its 8 params; `bind_match_op`'s
    /// body reads exactly its 10. Regression for the proposer's inference core.
    #[test]
    fn oracle_extracted_fn_signatures() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/engine.rs"),
        )
        .unwrap();
        let mut parser = Parser::new();
        parser.set_language(&lang()).unwrap();
        let tree = parser.parse(&src, None).unwrap();
        let cases: &[(&str, &[&str])] = &[
            ("bind_whole_match_span",
             &["ext", "idv", "caps", "content", "where_file", "repo", "path", "where_bytes"]),
            ("bind_match_op",
             &["binds", "regex", "mlv", "idv", "content", "where_file",
              "re_cache", "where_bytes", "repo", "path"]),
        ];
        for (name, expected) in cases {
            let mut cur = tree.root_node().walk();
            let body = tree
                .root_node()
                .children(&mut cur)
                .find(|n| n.kind() == "function_item" && {
                    n.child_by_field_name("name")
                        .map_or(false, |nm| text(nm, &src) == *name)
                })
                .and_then(|n| n.child_by_field_name("body"))
                .unwrap_or_else(|| panic!("fn {name} not found"));
            let lo = body.start_byte();
            let hi = body.end_byte();
            let got: HashSet<String> = free_vars(tree.root_node(), &src, lo, hi).into_iter().collect();
            let exp: HashSet<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(got, exp, "fn {name}: got {:?} expected {:?}", got, exp);
        }
    }
}
