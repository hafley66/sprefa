//! Structure-keyed duplicate detectors: AST shape, statement-tree, CFG
//! skeleton, data-dependence, and callgraph shape proposals (relocated from
//! the monolithic `propose.rs`; decomposition plan step 11, old
//! refactor/file-splits shape).

use super::*;
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
            lo: lo_line,
            hi: hi_line,
            occurrences: occ,
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
    let hashes: Vec<u64> = stmts
        .iter()
        .map(|(n, _, _)| subtree_hash(*n, content))
        .collect();
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
pub(crate) fn statement_ranges<'a>(tree: &'a tree_sitter::Tree) -> Vec<(Node<'a>, usize, usize)> {
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
pub(crate) fn subtree_hash(node: Node, src: &str) -> u64 {
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
    let hashes: Vec<u64> = stmts
        .iter()
        .map(|(n, _, _)| cfg_skeleton_hash(*n))
        .collect();
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
            "if_expression"
            | "for_expression"
            | "while_expression"
            | "loop_expression"
            | "match_expression"
            | "match_arm"
            | "break_expression"
            | "continue_expression"
            | "return_expression" => {
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
    let hashes: Vec<u64> = stmts
        .iter()
        .map(|(n, _, _)| ddg_hash(*n, content))
        .collect();
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
