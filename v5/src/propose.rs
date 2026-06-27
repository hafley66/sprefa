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

/// Symbol-shape (Type-2 + semantic) detector. Like AST-shape but each
/// identifier is normalized to its RESOLVED SCIP symbol (not a uniform `ID`),
/// so two regions match only if they reference the same entities in the same
/// structure — a CST⨝symbol join. The "type-shape" kernel's feasible form: RA
/// resolves types/fns/methods to stable monikers (locals stay opaque `local N`,
/// folded back to `ID`). `occ_spans` is `(line0, col0, symbol)` for ONE file.
const SYM_SEED: usize = 12;

pub fn symbol_shape_proposals(content: &str, occ_spans: &[(i32, i32, &str)]) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let sym_at: HashMap<(i32, i32), &str> =
        occ_spans.iter().map(|(l, c, s)| ((*l, *c), *s)).collect();
    let toks = symbol_shape_tokens(&tree, content, &sym_at);
    let kinds: Vec<&str> = toks.iter().map(|(k, _)| k.as_str()).collect();
    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in matching_runs(&kinds, SYM_SEED) {
        let lo_line = toks[a].1;
        let hi_line = toks[a + n - 1].1;
        if hi_line <= lo_line {
            continue;
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

fn symbol_shape_tokens(
    tree: &tree_sitter::Tree,
    content: &str,
    sym_at: &HashMap<(i32, i32), &str>,
) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.child_count() == 0 {
            let txt = &content[n.start_byte()..n.end_byte()];
            if txt.starts_with("//") || txt.starts_with("/*") {
                continue;
            }
            let pos = n.start_position();
            let k = match n.kind() {
                "identifier" | "metavariable" => {
                    match sym_at.get(&(pos.row as i32, pos.column as i32)) {
                        // opaque local monikers carry no entity info -> fold to ID
                        Some(s) if !s.starts_with("local ") => s.to_string(),
                        _ => "ID".to_string(),
                    }
                }
                kk if kk.ends_with("_literal") => "LIT".to_string(),
                kk => kk.to_string(),
            };
            out.push((k, pos.row + 1));
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

/// Tree-iso (graph-isomorphism) kernel. Each statement's full CST subtree is
/// Merkle-hashed: `hash(kind, [child_hashes…])`. Identifiers and literals are
/// anonymous holes (constant hash) — they are EDGES in the dependency graph,
/// not nodes; graph isomorphism matches node types and edge structure, not
/// edge labels. Two statements match iff their expression shape is identical
/// (same call arity, same field-access depth, same branch structure), regardless
/// of the names involved. Finds duplicated statement-sequences the leaf-stream
/// kernels miss (the leaf stream flattens tree depth, losing grouping).
const TREE_SEED: usize = 3;

pub fn tree_shape_proposals(content: &str) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let stmts = statement_ranges(&tree);
    let hashes: Vec<u64> = stmts.iter().map(|(n, _, _)| subtree_hash(*n, content)).collect();
    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in matching_runs(&hashes, TREE_SEED) {
        let lo_line = stmts[a].1;
        let hi_line = stmts[a + n - 1].2;
        if hi_line <= lo_line {
            continue;
        }
        let lo_byte = line_start[lo_line - 1];
        let hi_byte = line_start.get(hi_line).copied().unwrap_or(content.len());
        let params = free_vars(root, content, lo_byte, hi_byte);
        out.push(Proposal {
            lo: lo_line,
            hi: hi_line,
            occurrences: occ,
            gain: (hi_line - lo_line + 1) * occ.saturating_sub(1),
            params,
        });
    }
    out.sort_by(|x, y| y.gain.cmp(&x.gain));
    out
}

/// Per-statement Merkle hash for every direct named child of any `block` node.
/// `(hash, start_line, end_line)` all 1-based. Collected from ALL blocks
/// (including nested) and sorted into source order so matching_runs sees a
/// faithful statement sequence. Braces are excluded (named_children skips
/// anonymous tokens); comments are excluded by kind.
fn statement_ranges<'a>(tree: &'a tree_sitter::Tree) -> Vec<(Node<'a>, usize, usize)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        let children: Vec<Node> = n.named_children(&mut cur).collect();
        if n.kind() == "block" {
            for child in &children {
                let k = child.kind();
                if k == "line_comment" || k == "block_comment" {
                    continue;
                }
                let lo = child.start_position().row + 1;
                let hi = child.end_position().row + 1;
                out.push((*child, lo, hi));
            }
        }
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }
    out.sort_by_key(|(_, lo, _)| *lo);
    out
}

/// Recursive Merkle hash of a CST subtree. Leaf identifiers → anonymous hole
/// (1), leaf literals → anonymous hole (2); all other leaves hash their kind;
/// interior nodes hash `kind ⨁ child_hashes`. Comments are skipped entirely
/// (do not contribute to the parent hash) so two statements differing only in
/// trailing comments hash identically.
fn subtree_hash(node: Node, src: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    match node.kind() {
        "identifier" | "metavariable" => {
            1u64.hash(&mut h);
        }
        k if k.ends_with("_literal") => {
            2u64.hash(&mut h);
        }
        k => {
            k.hash(&mut h);
            let mut cur = node.walk();
            for child in node.children(&mut cur) {
                let ck = child.kind();
                if ck != "line_comment" && ck != "block_comment" {
                    subtree_hash(child, src).hash(&mut h);
                }
            }
        }
    }
    h.finish()
}

/// Call-seq (dataflow fingerprint) kernel. Each statement is hashed by the
/// SORTED SET of resolved non-local symbols it references (fns, methods, types,
/// globals — everything SCIP resolves, minus opaque locals). Two statements
/// match iff they reference the same entities; matching_runs then finds blocks
/// whose consecutive statements touch the same symbols in the same order.
///
/// This is the coarsest semantic kernel: it ignores structure entirely (a
/// `foo(bar)` and `if bar { foo() }` hash the same if both reference {foo, bar}).
/// Catches repeated dataflow patterns — same API surface used in the same
/// sequence — that syntactic kernels miss when the surrounding code differs.
const CALL_SEED: usize = 3;

pub fn call_seq_proposals(content: &str, occ_spans: &[(i32, i32, &str)]) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let stmts = statement_ranges(&tree);

    let mut syms_by_line: HashMap<i32, Vec<&str>> = HashMap::new();
    for &(l, _c, s) in occ_spans {
        if !s.starts_with("local ") {
            syms_by_line.entry(l).or_default().push(s);
        }
    }

    let hashes: Vec<u64> = stmts
        .iter()
        .map(|(_, lo, hi)| {
            let mut syms: Vec<&str> = Vec::new();
            for line in (*lo as i32 - 1)..=(*hi as i32 - 1) {
                if let Some(ss) = syms_by_line.get(&line) {
                    syms.extend_from_slice(ss);
                }
            }
            syms.sort();
            syms.dedup();
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for s in &syms {
                s.hash(&mut h);
            }
            h.finish()
        })
        .collect();

    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in matching_runs(&hashes, CALL_SEED) {
        let lo_line = stmts[a].1;
        let hi_line = stmts[a + n - 1].2;
        if hi_line <= lo_line {
            continue;
        }
        let lo_byte = line_start[lo_line - 1];
        let hi_byte = line_start.get(hi_line).copied().unwrap_or(content.len());
        let params = free_vars(root, content, lo_byte, hi_byte);
        out.push(Proposal {
            lo: lo_line,
            hi: hi_line,
            occurrences: occ,
            gain: (hi_line - lo_line + 1) * occ.saturating_sub(1),
            params,
        });
    }
    out.sort_by(|x, y| y.gain.cmp(&x.gain));
    out
}

/// CFG-shape (control-flow topology) kernel. Each statement is hashed by its
/// control-flow skeleton: the pre-order sequence of branch/loop node kinds
/// (if/for/while/loop/match/match_arm/break/continue/return) found in its
/// subtree. Two statements match iff they have the same control-flow rhythm.
/// matching_runs then finds blocks whose consecutive statements share the same
/// branching topology.
///
/// Unlike tree-iso (which hashes the FULL subtree including expressions), this
/// collapses body content to anonymous operations. `if cond { foo(bar); }` and
/// `if guard { baz(qux); }` hash the same — same branch shape, different ops.
/// Catches "same algorithm structure" (iterate-then-conditional-then-iterate)
/// that expression-aware kernels miss when the operations differ.
const CFG_SEED: usize = 3;

pub fn cfg_shape_proposals(content: &str) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let stmts = statement_ranges(&tree);
    let hashes: Vec<u64> = stmts.iter().map(|(n, _, _)| cfg_skeleton_hash(*n)).collect();
    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in matching_runs(&hashes, CFG_SEED) {
        let lo_line = stmts[a].1;
        let hi_line = stmts[a + n - 1].2;
        if hi_line <= lo_line {
            continue;
        }
        let lo_byte = line_start[lo_line - 1];
        let hi_byte = line_start.get(hi_line).copied().unwrap_or(content.len());
        let params = free_vars(root, content, lo_byte, hi_byte);
        out.push(Proposal {
            lo: lo_line,
            hi: hi_line,
            occurrences: occ,
            gain: (hi_line - lo_line + 1) * occ.saturating_sub(1),
            params,
        });
    }
    out.sort_by(|x, y| y.gain.cmp(&x.gain));
    out
}

/// Hash of a statement's control-flow skeleton. Walks the subtree and collects
/// control-flow node kinds in source order (pre-order DFS), then hashes the
/// sequence. `match_arm` is included so different arm counts produce different
/// hashes.
fn cfg_skeleton_hash(node: Node) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "if_expression" | "for_expression" | "while_expression" | "loop_expression"
            | "match_expression" | "match_arm" | "break_expression"
            | "continue_expression" | "return_expression" => {
                n.kind().hash(&mut h);
            }
            _ => {}
        }
        let mut cur = n.walk();
        let children: Vec<Node> = n.children(&mut cur).collect();
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }
    h.finish()
}

/// DDG-shape (data-dependency topology) kernel. Each statement is hashed by
/// its def/use footprint: how many new names it BINDS (let-patterns, for-loop
/// vars, closure params, match-arm patterns) and how many names it READS
/// (identifier references not in binding position, not keywords, not field
/// names). Two statements match iff they have the same (defs, uses) counts —
/// the same local dataflow rhythm. matching_runs then finds blocks whose
/// consecutive statements share the same def/use shape.
///
/// Distinct from call-seq (which fingerprints external symbol sets): ddg-shape
/// is purely LOCAL — it ignores which functions are called, only counting how
/// many names flow in and out. `let x = foo(a, b);` and `let y = a + b;` hash
/// the same (1 def, 2 uses). Catches "same variable-threading structure"
/// regardless of the operations involved.
const DDG_SEED: usize = 3;

pub fn ddg_shape_proposals(content: &str) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let stmts = statement_ranges(&tree);
    let hashes: Vec<u64> = stmts.iter().map(|(n, _, _)| ddg_hash(*n, content)).collect();
    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in matching_runs(&hashes, DDG_SEED) {
        let lo_line = stmts[a].1;
        let hi_line = stmts[a + n - 1].2;
        if hi_line <= lo_line {
            continue;
        }
        let lo_byte = line_start[lo_line - 1];
        let hi_byte = line_start.get(hi_line).copied().unwrap_or(content.len());
        let params = free_vars(root, content, lo_byte, hi_byte);
        out.push(Proposal {
            lo: lo_line,
            hi: hi_line,
            occurrences: occ,
            gain: (hi_line - lo_line + 1) * occ.saturating_sub(1),
            params,
        });
    }
    out.sort_by(|x, y| y.gain.cmp(&x.gain));
    out
}

/// Per-statement dataflow fingerprint. Counts names DEFINED (binding-position
/// identifiers) and names USED (reference-position identifiers outside any
/// binding pattern subtree, excluding keywords and field-path segments). The
/// pair `(defs, uses)` is hashed: two statements with identical def/use
/// counts share a dataflow shape.
fn ddg_hash(node: Node, src: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut bind_ranges: Vec<(usize, usize)> = Vec::new();
    let mut defs = 0u64;
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let pat = match n.kind() {
            "let_declaration" | "let_condition" => n.child_by_field_name("pattern"),
            "for_expression" => n.child_by_field_name("pattern"),
            "closure_expression" => n.child_by_field_name("parameters"),
            _ => None,
        };
        if let Some(p) = pat {
            bind_ranges.push((p.start_byte(), p.end_byte()));
            defs += count_idents_in(p);
        }
        if n.kind() == "match_arm" {
            let mut cur = n.walk();
            let children: Vec<Node> = n.children(&mut cur).collect();
            if let Some(idx) = children.iter().position(|c| c.kind() == "=>") {
                for c in &children[..idx] {
                    bind_ranges.push((c.start_byte(), c.end_byte()));
                    defs += count_idents_in(*c);
                }
            }
        }
        let mut cur = n.walk();
        let children: Vec<Node> = n.children(&mut cur).collect();
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }
    let mut uses = 0u64;
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.child_count() == 0 && n.kind() == "identifier" {
            let in_bind = bind_ranges
                .iter()
                .any(|(lo, hi)| *lo <= n.start_byte() && n.start_byte() < *hi);
            let txt = text(n, src);
            if !in_bind && !is_keyword(txt) && !should_skip(n) {
                uses += 1;
            }
        }
        let mut cur = n.walk();
        let children: Vec<Node> = n.children(&mut cur).collect();
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    defs.hash(&mut h);
    uses.hash(&mut h);
    h.finish()
}

fn count_idents_in(n: Node) -> u64 {
    let mut count = 0u64;
    let mut stack = vec![n];
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" {
            count += 1;
        }
        let mut cur = n.walk();
        for c in n.children(&mut cur) {
            stack.push(c);
        }
    }
    count
}

/// Callgraph-iso (call topology) kernel. Each statement is hashed by the
/// multiset of calls in its subtree: for each `call_expression`, record
/// `(callee_type, arity)` where callee_type distinguishes function calls
/// (callee is an identifier) from method calls (callee is a field_expression).
/// Two statements match iff they make the same calls with the same arities.
///
/// Call nesting is captured indirectly: `foo(bar())` produces entries
/// `[(fn,1), (fn,0)]` (foo has 1 arg, bar has 0), while `foo(); bar()`
/// produces `[(fn,0), (fn,0)]`. Distinct from tree-iso: ignores all non-call
/// structure (control flow, assignments, literals) so that two statements
/// wrapping the same calls in different syntactic scaffolding still match.
const CALLGRAPH_SEED: usize = 3;

pub fn callgraph_shape_proposals(content: &str) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let stmts = statement_ranges(&tree);
    let hashes: Vec<u64> = stmts.iter().map(|(n, _, _)| callgraph_hash(*n)).collect();
    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in matching_runs(&hashes, CALLGRAPH_SEED) {
        let lo_line = stmts[a].1;
        let hi_line = stmts[a + n - 1].2;
        if hi_line <= lo_line {
            continue;
        }
        let lo_byte = line_start[lo_line - 1];
        let hi_byte = line_start.get(hi_line).copied().unwrap_or(content.len());
        let params = free_vars(root, content, lo_byte, hi_byte);
        out.push(Proposal {
            lo: lo_line,
            hi: hi_line,
            occurrences: occ,
            gain: (hi_line - lo_line + 1) * occ.saturating_sub(1),
            params,
        });
    }
    out.sort_by(|x, y| y.gain.cmp(&x.gain));
    out
}

/// Per-statement call topology fingerprint. Walks the subtree collecting
/// `(callee_type, arity)` for every `call_expression`: type 1 = function call
/// (callee is an identifier or path), type 2 = method call (callee is a
/// field_expression). The sorted multiset is hashed.
fn callgraph_hash(node: Node) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut entries: Vec<(u8, usize)> = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression" {
            let callee_type = match n.child_by_field_name("function") {
                Some(f) if f.kind() == "field_expression" => 2u8,
                Some(_) => 1u8,
                None => 0u8,
            };
            let arity = n
                .child_by_field_name("arguments")
                .map(|a| a.named_child_count())
                .unwrap_or(0);
            entries.push((callee_type, arity));
        }
        let mut cur = n.walk();
        let children: Vec<Node> = n.children(&mut cur).collect();
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }
    entries.sort();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    entries.hash(&mut h);
    h.finish()
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

    // --- matching_runs (kernel-agnostic matcher) ---

    #[test]
    fn matching_runs_finds_exact_dup() {
        let items = ["a", "b", "c", "x", "a", "b", "c"];
        let runs = matching_runs(&items, 2);
        assert!(
            runs.iter().any(|&(s, n, occ)| s == 0 && n == 3 && occ == 2),
            "expected (0,3,2) in {:?}", runs
        );
    }

    #[test]
    fn matching_runs_drops_subsumed() {
        let items = ["a", "b", "c", "d", "a", "b", "c", "d"];
        let runs = matching_runs(&items, 2);
        assert_eq!(runs.len(), 1, "one maximal run, got {:?}", runs);
        assert_eq!(runs[0], (0, 4, 2));
    }

    #[test]
    fn matching_runs_requires_min_distinct() {
        let items = ["x", "x", "x", "x", "x", "x"];
        let runs = matching_runs(&items, 2);
        assert!(runs.is_empty(), "trivial repetition filtered: {:?}", runs);
    }

    // --- subtree_hash (graph-isomorphism hasher) ---

    fn first_node_of_kind<'a>(tree: &'a tree_sitter::Tree, kind: &str) -> Node<'a> {
        let mut stack = vec![tree.root_node()];
        while let Some(n) = stack.pop() {
            if n.kind() == kind {
                return n;
            }
            let mut cur = n.walk();
            for c in n.children(&mut cur) {
                stack.push(c);
            }
        }
        panic!("no {kind} found in tree");
    }

    #[test]
    fn subtree_hash_erases_identifier_names() {
        let mut parser = Parser::new();
        parser.set_language(&lang()).unwrap();
        let s1 = "fn f() { foo(bar); }";
        let s2 = "fn f() { baz(qux); }";
        let t1 = parser.parse(s1, None).unwrap();
        let t2 = parser.parse(s2, None).unwrap();
        let h1 = subtree_hash(first_node_of_kind(&t1, "call_expression"), s1);
        let h2 = subtree_hash(first_node_of_kind(&t2, "call_expression"), s2);
        assert_eq!(h1, h2, "same structure, different names must hash equal");
    }

    #[test]
    fn subtree_hash_distinguishes_structure() {
        let mut parser = Parser::new();
        parser.set_language(&lang()).unwrap();
        let s1 = "fn f() { foo(bar); }";
        let s2 = "fn f() { foo.bar; }";
        let t1 = parser.parse(s1, None).unwrap();
        let t2 = parser.parse(s2, None).unwrap();
        let h1 = subtree_hash(first_node_of_kind(&t1, "call_expression"), s1);
        let h2 = subtree_hash(first_node_of_kind(&t2, "field_expression"), s2);
        assert_ne!(h1, h2, "call vs field-access must hash differently");
    }

    // --- verbatim kernel ---

    #[test]
    fn verbatim_finds_exact_dup_block() {
        let src = "fn a() {\n    let x = one();\n    let y = two();\n    let z = three();\n    foo(x);\n    bar(y);\n    baz(z);\n}\nfn b() {\n    let x = one();\n    let y = two();\n    let z = three();\n    foo(x);\n    bar(y);\n    baz(z);\n}\n";
        let p = extract_proposals(src);
        assert!(p.iter().any(|p| p.occurrences >= 2), "exact dup found: {:?}", p);
    }

    // --- ast-shape kernel ---

    #[test]
    fn ast_shape_finds_renamed_var_dup() {
        let src = "fn alpha() {\n    let result = compute(input);\n    map.insert(key, result);\n    return verify(result);\n}\nfn beta() {\n    let value = compute(input);\n    map.insert(key, value);\n    return verify(value);\n}\n";
        let p = ast_shape_proposals(src);
        assert!(!p.is_empty(), "renamed-var dup caught by AST shape");
    }

    // --- tree-iso kernel ---

    #[test]
    fn tree_shape_finds_renamed_var_dup() {
        let src = "fn alpha() {\n    let result = compute(input);\n    map.insert(key, result);\n    return verify(result);\n}\nfn beta() {\n    let value = compute(input);\n    map.insert(key, value);\n    return verify(value);\n}\n";
        let p = tree_shape_proposals(src);
        assert!(!p.is_empty(), "tree-iso catches renamed-var structural dup");
        assert!(p.iter().any(|p| p.occurrences >= 2));
    }

    #[test]
    fn tree_shape_rejects_different_structure() {
        let src = "fn alpha() {\n    let result = compute(input);\n    map.insert(key, result);\n    return verify(result);\n}\nfn beta() {\n    if input.is_valid() {\n        handle(input);\n    }\n}\n";
        let p = tree_shape_proposals(src);
        assert!(p.is_empty(), "structurally different blocks do not match: {:?}", p);
    }

    // --- call-seq kernel ---

    #[test]
    fn call_seq_finds_matching_symbol_pattern() {
        let src = "fn a() {\n    x();\n    y();\n    z();\n}\nfn b() {\n    x();\n    y();\n    z();\n}\n";
        let spans: Vec<(i32, i32, &str)> = vec![
            (1, 4, "pkg/x()."), (2, 4, "pkg/y()."), (3, 4, "pkg/z()."),
            (6, 4, "pkg/x()."), (7, 4, "pkg/y()."), (8, 4, "pkg/z()."),
        ];
        let p = call_seq_proposals(src, &spans);
        assert_eq!(p.len(), 1, "one matching symbol-set block: {:?}", p);
        assert_eq!(p[0].occurrences, 2);
    }

    #[test]
    fn call_seq_rejects_different_symbols() {
        let src = "fn a() {\n    x();\n    y();\n    z();\n}\nfn b() {\n    p();\n    q();\n    r();\n}\n";
        let spans: Vec<(i32, i32, &str)> = vec![
            (1, 4, "pkg/x()."), (2, 4, "pkg/y()."), (3, 4, "pkg/z()."),
            (6, 4, "pkg/p()."), (7, 4, "pkg/q()."), (8, 4, "pkg/r()."),
        ];
        let p = call_seq_proposals(src, &spans);
        assert!(p.is_empty(), "different symbols do not match: {:?}", p);
    }

    // --- ddg-shape kernel ---

    #[test]
    fn ddg_shape_finds_matching_def_use_rhythm() {
        // Two blocks with the same per-statement def/use rhythm but different
        // operations and names. Each window has 3 distinct hash values so the
        // trivial-repetition filter passes.
        //   H(1,1) H(1,2) H(0,2)  — 1 def+1 use, 1 def+2 uses, 0 def+2 uses
        let src = "fn alpha(a: i32) {\n    let x = a;\n    let y = combine(x);\n    result(y);\n}\nfn beta(p: i32) {\n    let s = p;\n    let t = merge(s);\n    output(t);\n}\n";
        let p = ddg_shape_proposals(src);
        assert!(!p.is_empty(), "same def/use rhythm must match: {:?}", p);
        assert!(p.iter().any(|p| p.occurrences >= 2));
    }

    #[test]
    fn ddg_shape_rejects_different_def_use_counts() {
        // alpha: every statement defines 1 var and reads 3 names (H(1,3)).
        // beta: every statement reads 1 name, defines nothing (H(0,1)).
        // No window matches across the two groups.
        let src = "fn alpha(a: i32, b: i32) {\n    let x = combine(a, b);\n    let y = combine(x, a);\n    let z = combine(y, x);\n}\nfn beta() {\n    do_thing();\n    do_other();\n    do_more();\n}\n";
        let p = ddg_shape_proposals(src);
        assert!(p.is_empty(), "different def/use counts must not match: {:?}", p);
    }

    // --- callgraph-iso kernel ---

    #[test]
    fn callgraph_finds_matching_call_topology() {
        // Two blocks: same call topology per statement, different names and
        // non-call structure. Each statement has a distinct call multiset so
        // the trivial-repetition filter passes.
        //   stmt 1: [(fn,2)]           — foo(a, b)     / bar(x, y)
        //   stmt 2: [(fn,1),(fn,0)]   — outer(inner()) / quux(corge())
        //   stmt 3: [(method,1)]      — obj.method(z)  / inst.send(w)
        let src = "fn alpha(a: i32, b: i32, obj: T) {\n    foo(a, b);\n    outer(inner());\n    obj.method(a);\n}\nfn beta(x: i32, y: i32, inst: T) {\n    bar(x, y);\n    quux(corge());\n    inst.send(x);\n}\n";
        let p = callgraph_shape_proposals(src);
        assert!(!p.is_empty(), "same call topology must match: {:?}", p);
        assert!(p.iter().any(|p| p.occurrences >= 2));
    }

    #[test]
    fn callgraph_rejects_different_arity() {
        // alpha: [(fn,2)] per stmt. beta: [(fn,0)] per stmt. Different topology.
        let src = "fn alpha(a: i32, b: i32) {\n    foo(a, b);\n    bar(a, b);\n    baz(a, b);\n}\nfn beta() {\n    ping();\n    pong();\n    pang();\n}\n";
        let p = callgraph_shape_proposals(src);
        assert!(p.is_empty(), "different arity must not match: {:?}", p);
    }
}
