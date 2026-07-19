//! Sequence-keyed duplicate detectors: symbol-occurrence shapes, call
//! sequences, and token n-gram similarity proposals (relocated from the
//! monolithic `propose.rs`; decomposition plan step 11, old
//! refactor/file-splits shape).

use super::*;
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
/// Ngram-stat (fuzzy similarity) kernel. The only kernel using non-equality
/// similarity. Each statement is reduced to a set of token n-grams (sliding
/// window of size N over the normalized leaf-kind stream — same leaf
/// normalization as ast-shape: identifiers→ID, literals→LIT). Two statements
/// are similar iff their n-gram sets have Jaccard overlap >= threshold. The
/// `similarity_runs` matcher then finds maximal runs of consecutive
/// position-wise-similar statement pairs.
///
/// Catches "near-duplicate" blocks where the structure is mostly shared but
/// small differences (extra arg, reordered clauses, different branch count)
/// prevent exact-match kernels from seeing them. The Jaccard threshold
/// controls precision/recall: 0.7 catches 70%-overlap blocks that
/// ast/tree/cfg kernels miss entirely.
const NGRAM_N: usize = 3;
const NGRAM_THRESHOLD: f64 = 0.7;
const NGRAM_SEED: usize = 3;
pub fn ngram_stat_proposals(content: &str) -> Vec<Proposal> {
    let mut parser = Parser::new();
    if parser.set_language(&lang()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };
    let stmts = statement_ranges(&tree);
    let gram_sets: Vec<HashSet<u64>> = stmts
        .iter()
        .map(|(n, _, _)| {
            let kinds = leaf_kinds(*n, content);
            ngram_set(&kinds, NGRAM_N)
        })
        .collect();
    let line_start = line_start_bytes(content);
    let root = tree.root_node();
    let mut out = Vec::new();
    for (a, n, occ) in similarity_runs(&gram_sets, NGRAM_THRESHOLD, NGRAM_SEED) {
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

/// Maximal matching runs over a sequence of sets using Jaccard similarity.
/// Two windows match iff every position-wise pair has Jaccard >= threshold.
/// Returns `(start, len, occurrences)` for each maximal run, dropping subsumed
/// runs. Cardinality pre-filter: if `min(|A|,|B|) / max(|A|,|B|) < threshold`
/// the pair is skipped without computing intersection.
fn similarity_runs(
    items: &[HashSet<u64>],
    threshold: f64,
    seed: usize,
) -> Vec<(usize, usize, usize)> {
    let len = items.len();
    if len < seed + 1 {
        return Vec::new();
    }
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut blocks: Vec<(usize, usize, usize)> = Vec::new();
    for i in 0..=len - seed {
        for j in (i + 1)..=len - seed {
            let lo = items[i].len().min(items[j].len());
            let hi = items[i].len().max(items[j].len());
            if hi == 0 || (lo as f64 / hi as f64) < threshold {
                continue;
            }
            if jaccard(&items[i], &items[j]) < threshold {
                continue;
            }
            if !seen.insert((i, j)) {
                continue;
            }
            let mut ok = true;
            for k in 1..seed {
                if jaccard(&items[i + k], &items[j + k]) < threshold {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            let mut n = seed;
            while i + n < len && j + n < len && jaccard(&items[i + n], &items[j + n]) >= threshold
            {
                n += 1;
            }
            let mut occ = 1usize;
            for m in 0..=len.saturating_sub(n) {
                if m == i || m + n > len {
                    continue;
                }
                let mut all = true;
                for k in 0..n {
                    if jaccard(&items[i + k], &items[m + k]) < threshold {
                        all = false;
                        break;
                    }
                }
                if all {
                    occ += 1;
                }
            }
            blocks.push((i, n, occ));
        }
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
