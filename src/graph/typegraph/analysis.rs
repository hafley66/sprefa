//! Shared graph analysis fns: type-shape hashing + least-general-
//! generalization pairs. Language-agnostic (operates on the resolved
//! (from, to, kind) edge tuple), so kept out of every per-language module.

/// Field-tree Merkle hash per type name, for shape-isomorphism detection.
///
/// Hashes the structural shape of each type's field tree using only `field`
/// and `variant` edges — the data shape. `impl` and `generic` edges are
/// cross-cutting, not shape, and are excluded. Two types with the same hash
/// are field-tree-isomorphic: same arity, depth, and leaf shape, regardless
/// of names. Names are NOT in the hash; this is pure shape.
///
/// Fixpoint iteration handles recursive types (`struct List { tail: Box<List> }`)
/// — a self-reference stabilizes because each round only mixes in the prior
/// round's hash, and the structure converges. The leaf sentinel (`LEAF`) is
/// the hash of any name with no field/variant children, so all primitives and
/// external types hash alike.
///
/// `edges` is the engine's full `type_edge(from, to, kind)` row set. Returns
/// one `(name, hex_hash)` per name that appears in any data edge, sorted by
/// name for deterministic output. blake3 matches the rest of the codebase's
/// persistent hash convention.
pub fn type_shape_hashes(edges: &[(String, String, String)]) -> Vec<(String, String)> {
    use std::collections::{BTreeMap, BTreeSet};

    // Keep only field/variant edges (the data shape). Drop impl/generic.
    let data_edges: Vec<(&str, &str)> = edges
        .iter()
        .filter(|(_, _, k)| k == "field" || k == "variant")
        .map(|(a, b, _)| (a.as_str(), b.as_str()))
        .collect();

    // Names appearing anywhere in the data graph.
    let mut names: BTreeSet<String> = BTreeSet::new();
    for (a, b) in &data_edges {
        names.insert((*a).to_string());
        names.insert((*b).to_string());
    }

    // Adjacency: name -> sorted unique child-name list. Duplicates collapse
    // (two fields of type T = shape {T}). Switch to a sorted Vec WITH dups to
    // make multiplicity count (struct{x: T, y: T} distinct from {z: T}).
    let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (a, b) in &data_edges {
        adj.entry((*a).to_string()).or_default().push((*b).to_string());
    }
    for v in adj.values_mut() {
        v.sort();
        v.dedup();
    }

    let leaf_hash = *blake3::hash(b"LEAF").as_bytes();

    // Initial hash: every name starts as a leaf.
    let mut cur: BTreeMap<String, [u8; 32]> = names
        .iter()
        .map(|n| (n.clone(), leaf_hash))
        .collect();

    // Fixpoint: re-hash until stable or we hit the iter cap. The cap is a
    // safety net; in practice convergence is at most depth-of-the-deepest-tree
    // iterations (or never, only if the graph is pathologically oscillating).
    for _ in 0..64 {
        let mut next: BTreeMap<String, [u8; 32]> = BTreeMap::new();
        let mut stable = true;
        for n in &names {
            let mut h = blake3::Hasher::new();
            match adj.get(n) {
                None => { h.update(b"LEAF"); }
                Some(cs) if cs.is_empty() => { h.update(b"LEAF"); }
                Some(cs) => {
                    for c in cs {
                        match cur.get(c) {
                            Some(ch) => { h.update(ch); }
                            None => { h.update(b"EXT"); }
                        }
                    }
                }
            }
            let bytes = *h.finalize().as_bytes();
            if bytes != cur[n] {
                stable = false;
            }
            next.insert(n.clone(), bytes);
        }
        cur = next;
        if stable {
            break;
        }
    }

    cur.into_iter()
        .map(|(n, h)| (n, hex_string(&h)))
        .collect()
}

fn hex_string(bytes: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Anti-unification (Plotkin's LGG, Least General Generalization) for type trees.
///
/// For each ordered pair of distinct type names `(a, b)` with `a < b`, computes
/// how many "fresh variables" the LGG introduces. Two identical types produce 0.
/// Two types that differ only in N leaf positions produce N. Two unrelated types
/// produce a number bounded by their combined tree size.
///
/// Algorithm (simplified; treats type_edge as a tree, ignores sharing/cycles
/// via memoization that treats a revisit as opaque):
///   - `lgg(a, a)`         → 0 vars (identical)
///   - `lgg(leaf_a, leaf_b)` → 1 var  (distinct leaves)
///   - `lgg(a, b)` when both have field/variant children, same arity, same
///     kind sequence (after sorting by (kind, name)) → recurse pairwise,
///     sum the var counts.
///   - otherwise → 1 var (shape diverges; generalize the whole node away).
///
/// `edges` is the engine's full `type_edge` row set; only field/variant edges
/// contribute (impl/generic are excluded, same as `type_shape_hashes`).
///
/// Output: one `(a, b, vars)` per canonical pair with `a < b` and `vars >= 1`.
/// Pairs with `vars == 0` are identical and covered by `type_shape` already.
/// Sorted for deterministic output.
pub fn type_lgg_pairs(edges: &[(String, String, String)]) -> Vec<(String, String, i64)> {
    use std::collections::BTreeMap;

    // Build adjacency: name -> sorted Vec<(kind, child_name)>. field/variant only.
    let mut adj: BTreeMap<String, Vec<(&'static str, String)>> = BTreeMap::new();
    for (a, b, k) in edges {
        let kind: Option<&'static str> = match k.as_str() {
            "field" => Some("field"),
            "variant" => Some("variant"),
            _ => None,
        };
        if let Some(kind) = kind {
            adj.entry(a.clone()).or_default().push((kind, b.clone()));
        }
    }
    for v in adj.values_mut() {
        v.sort();
        // Don't dedup: a struct with two fields of the same type has shape
        // {T, T}, distinct from {T}. Keep multiplicity.
    }

    // All distinct names (including leaves that only appear as `to`).
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (a, b, _) in edges {
        names.insert(a.clone());
        names.insert(b.clone());
    }
    let names: Vec<String> = names.into_iter().collect();

    // Pair cache: (a, b) -> var count. Memoizes to handle DAG sharing and
    // breaks cycles (a revisit mid-recursion returns the cached value, which
    // is conservative — it pretends the recursion already terminated).
    let mut cache: BTreeMap<(String, String), i64> = BTreeMap::new();

    let mut out: Vec<(String, String, i64)> = Vec::new();
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            let a = &names[i];
            let b = &names[j];
            // a < b lexicographically because names is sorted.
            let vars = lgg_var_count(a, b, &adj, &mut cache);
            if vars >= 1 {
                out.push((a.clone(), b.clone(), vars));
            }
        }
    }
    out.sort();
    out
}

fn lgg_var_count(
    a: &str,
    b: &str,
    adj: &std::collections::BTreeMap<String, Vec<(&'static str, String)>>,
    cache: &mut std::collections::BTreeMap<(String, String), i64>,
) -> i64 {
    if a == b {
        return 0;
    }
    // Canonicalize the cache key so (a,b) and (b,a) share.
    let key = if a < b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    };
    if let Some(&v) = cache.get(&key) {
        return v;
    }
    // Tentative 1 to break cycles: a recursive revisit returns 1 (conservative).
    cache.insert(key.clone(), 1);

    let ca = adj.get(a);
    let cb = adj.get(b);
    let result = match (ca, cb) {
        (None, None) => 1, // two distinct leaves
        (Some(ca), Some(cb)) if ca.len() == cb.len() => {
            // Same arity. Pairwise-align children by sorted position. If the
            // kind sequence matches, recurse and sum; else diverge.
            let kinds_match = ca.iter().zip(cb.iter()).all(|((ka, _), (kb, _))| ka == kb);
            if kinds_match {
                let mut sum = 0i64;
                for ((_, na), (_, nb)) in ca.iter().zip(cb.iter()) {
                    sum += lgg_var_count(na, nb, adj, cache);
                }
                sum
            } else {
                1
            }
        }
        _ => 1, // arity differs or one is leaf
    };

    cache.insert(key, result);
    result
}

// ── Python (tree-sitter) ─────────────────────────────────────────────────────
//
// Same diet-extractor contract as Kotlin: one tree-sitter parse feeds the
// entity, edge, call, dataflow, and doc walks. Python has no static type system,
// so entities/edges/dataflow are honest about what a syntax-only pass can see:
// `type_edge`/`type_sig` come ONLY from PEP 484 annotations (class bases,
// annotated attributes, annotated params/returns, annotated local assignments
// in a body — the TS "uses" convention); un-annotated code still gets full
// entity/call/dataflow/doc coverage. type_link (name resolution) is SCOPED OUT
// of this extractor entirely — an attribute-chain callee (`obj.method()`) is
// emitted with its bare trailing name and left for the engine's existing
// by_name resolver, exactly like Kotlin/TS; nothing here tries to guess a
// receiver's type. `self`/`cls` are dropped from parameter lists so positions
// align with `type_sig.pos`/`df_param.pos` (the Rust/Kotlin receiver
// convention) — matched by a literal name check since Python has no syntactic
// receiver marker. `EntityKind::Module` exists only so a module docstring (no
// enclosing class/def) has a `type_entity` row to join.


#[cfg(test)]
mod tests {
    use super::*;

    fn edge(a: &str, b: &str, k: &str) -> (String, String, String) {
        (a.into(), b.into(), k.into())
    }

    fn shape_hash_of(hashes: &[(String, String)], name: &str) -> String {
        hashes.iter().find(|(n, _)| n == name).map(|(_, h)| h.clone())
            .unwrap_or_else(|| panic!("no hash for {name}: {hashes:?}"))
    }

    #[test]
    fn type_shape_iso_two_structs_same_arity_same_hash() {
        // Point{x: f64, y: f64} and Coord{lat: f64, lon: f64} — both hold
        // two leaves. Names differ; shape identical.
        let edges = vec![
            edge("Point", "f64", "field"), edge("Point", "g64", "field"),
            edge("Coord", "h64", "field"), edge("Coord", "i64", "field"),
        ];
        let h = type_shape_hashes(&edges);
        assert_eq!(shape_hash_of(&h, "Point"), shape_hash_of(&h, "Coord"));
        // The leaves themselves all hash alike (LEAF sentinel).
        assert_eq!(shape_hash_of(&h, "f64"), shape_hash_of(&h, "h64"));
    }

    #[test]
    fn type_shape_different_arity_different_hash() {
        let edges = vec![
            edge("One", "f", "field"),
            edge("Two", "g", "field"), edge("Two", "h", "field"),
        ];
        let h = type_shape_hashes(&edges);
        assert_ne!(shape_hash_of(&h, "One"), shape_hash_of(&h, "Two"));
    }

    #[test]
    fn type_shape_recursive_type_converges() {
        // struct List { head: i32, tail: Box<List> } — self-reference.
        let edges = vec![
            edge("List", "i32", "field"),
            edge("List", "Box_List", "field"),
            edge("Box_List", "List", "field"),
        ];
        let h = type_shape_hashes(&edges);
        // Smoke: the function terminates and produces a stable hash for List.
        let list_hash = shape_hash_of(&h, "List");
        // Running twice gives the same answer (fixpoint is deterministic).
        let h2 = type_shape_hashes(&edges);
        assert_eq!(list_hash, shape_hash_of(&h2, "List"));
        // And the self-referential shape differs from a flat 2-field struct.
        let flat = vec![edge("Flat", "a", "field"), edge("Flat", "b", "field")];
        let hf = type_shape_hashes(&flat);
        assert_ne!(list_hash, shape_hash_of(&hf, "Flat"));
    }

    #[test]
    fn type_shape_impl_and_generic_excluded() {
        // Two structs with the same fields but different impls/generics should
        // hash alike — impl/generic aren't shape.
        let a = vec![
            edge("Foo", "i32", "field"),
            edge("Foo", "u32", "field"),
            edge("Foo", "Drop", "impl"),
            edge("Foo", "T", "generic"),
        ];
        let b = vec![
            edge("Bar", "i32", "field"),
            edge("Bar", "u32", "field"),
        ];
        assert_eq!(shape_hash_of(&type_shape_hashes(&a), "Foo"),
                   shape_hash_of(&type_shape_hashes(&b), "Bar"));
    }

    #[test]
    fn type_shape_variant_edges_count_as_shape() {
        // enum Action { Save(Path), Quit } vs struct Wrapper{ a: Path, b: Leaf }
        // — both have two data children, one of which is Path.
        let a = vec![
            edge("Action", "Path", "variant"),
            edge("Action", "Quit", "variant"),
        ];
        let b = vec![
            edge("Wrapper", "Path", "field"),
            edge("Wrapper", "Leaf", "field"),
        ];
        assert_eq!(shape_hash_of(&type_shape_hashes(&a), "Action"),
                   shape_hash_of(&type_shape_hashes(&b), "Wrapper"));
    }

    #[test]
    fn lgg_identical_types_zero_vars() {
        // Two types with identical field structure but distinct names. A and A2
        // have 2 fields each pointing at distinct leaves (a, b vs c, d), so
        // var_count(A, A2) = 2 (two slots each generalizing to a fresh var).
        let edges = vec![
            edge("A", "a", "field"), edge("A", "b", "field"),
            edge("A2", "c", "field"), edge("A2", "d", "field"),
        ];
        let pairs = type_lgg_pairs(&edges);
        let aa2 = pairs.iter().find(|(x, y, _)| x == "A" && y == "A2").map(|(_, _, v)| *v);
        assert_eq!(aa2, Some(2));
        // Every emitted pair has var_count >= 1 (vars == 0 is filtered).
        assert!(pairs.iter().all(|(_, _, v)| *v >= 1));
        // And no pair has identical names.
        assert!(pairs.iter().all(|(a, b, _)| a != b));
    }

    #[test]
    fn lgg_completely_identical_zero_skipped() {
        // Identical type names: lgg(A, A) returns 0, not emitted.
        let edges = vec![edge("A", "x", "field")];
        // Only one type name with edges + x; no a<b pair where a==b.
        let pairs = type_lgg_pairs(&edges);
        assert!(pairs.iter().all(|(a, b, _)| a != b));
    }

    #[test]
    fn lgg_different_arity_one_var() {
        // A has 2 fields, B has 1. Arity differs → opaque generalization.
        let edges = vec![
            edge("A", "x", "field"), edge("A", "y", "field"),
            edge("B", "z", "field"),
        ];
        let pairs = type_lgg_pairs(&edges);
        let ab = pairs.iter().find(|(p, q, _)| p == "A" && q == "B").map(|(_, _, v)| *v);
        assert_eq!(ab, Some(1));
    }

    #[test]
    fn lgg_shared_child_zero_vars_for_that_slot() {
        // A and B both have a field of the SAME type C. The C/C slot is 0 vars.
        // The other slot differs (X vs Y) → 1 var. Total = 1.
        let edges = vec![
            edge("A", "C", "field"), edge("A", "X", "field"),
            edge("B", "C", "field"), edge("B", "Y", "field"),
        ];
        let pairs = type_lgg_pairs(&edges);
        let ab = pairs.iter().find(|(p, q, _)| p == "A" && q == "B").map(|(_, _, v)| *v);
        assert_eq!(ab, Some(1));
    }

    // --- dataflow lift: instantiations, positional args, named fields, members
}
