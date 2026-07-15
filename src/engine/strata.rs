use super::*;

pub(crate) fn closure_map(rules: &[&Rule]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for r in rules {
        if let Some(edge) = r.closure_edge() {
            m.insert(r.head.rel.clone(), edge.to_string());
        }
    }
    m
}

/// Unique edge relations across all closure heads (one condensation per graph).
/// Unique edge relations across closure heads AND scc heads. `refresh_cond_cache`
/// condenses every edge either operator needs; closure-view rebuild
/// (`first_empty_closure_edge`/`rebuild_closures`) stays closure-only (the
/// scc_node SQL tables exist only for closure edges — scc reads the in-memory
/// cond).
pub(crate) fn cond_edges_for<'a>(
    closure_edges: &[&'a str],
    scc_rules: &[&'a Rule],
) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = closure_edges.to_vec();
    for r in scc_rules {
        if let Some(e) = r.scc_edge() {
            if !out.contains(&e) {
                out.push(e);
            }
        }
    }
    out
}

pub(crate) fn dedup_edges(closures: &HashMap<String, String>) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for e in closures.values() {
        if !out.contains(&e.as_str()) {
            out.push(e.as_str());
        }
    }
    out
}

/// One digest over the whole derived layer (derived rules, closure-seed rules,
/// closure edges). Derived tables rebuild atomically, so a single moved bit
/// forces the rebuild; without this an edited derived rule or ground fact keeps
/// serving rows from a warm db (the derived twin of `source_rule_digests`).
pub(crate) fn derived_program_digest(
    derived_rules: &[&Rule],
    seed_rules: &[(&Rule, ClosureSeed)],
    edges: &[&str],
) -> [u8; 32] {
    let mut acc = [0u8; 32];
    let xor = |acc: &mut [u8; 32], s: String| {
        let h = blake3::hash(s.as_bytes());
        for (a, b) in acc.iter_mut().zip(h.as_bytes()) {
            *a ^= b;
        }
    };
    for r in derived_rules {
        xor(&mut acc, format!("{r:?}"));
    }
    for (r, _) in seed_rules {
        xor(&mut acc, format!("seed:{r:?}"));
    }
    for e in edges {
        xor(&mut acc, format!("edge:{e}"));
    }
    acc
}

/// The literal a query pins head position `pos` to, via a literal head term.
/// None if that position is a free variable. A pinned src/dst on a closure head
/// seeds the transitive walk (the find-refs / blast-radius point query).
pub(crate) fn pinned_value(q: &Query, pos: usize) -> Option<String> {
    match &q.head.terms[pos] {
        Term::Str(s) => Some(s.clone()),
        _ => None,
    }
}

/// A derived rule that reads a closure head in a *seedable* shape: one endpoint
/// of the 2-ary closure atom is pinned to a literal, the other is free. We answer
/// it as a seeded reachability walk over the condensation (the same BFS the
/// closure point query uses), not by materializing the Theta(V^2) closure.
pub(crate) struct ClosureSeed {
    /// The edge relation the closure head is over (`closures[head]`).
    pub(crate) edge: String,
    /// The pinned literal (the seed node).
    pub(crate) seed: String,
    /// true = src pinned (walk out / callees); false = dst pinned (walk in).
    pub(crate) forward: bool,
    /// Variable name of the free endpoint, as it appears in the closure atom.
    pub(crate) free_var: String,
}

/// Classify a derived rule as closure-seedable. Returns `Some(seed)` iff:
///   - exactly one positive body atom references a closure head, that head is
///     2-ary, one column is pinned (a `Term::Str` there, or a `Term::Var` bound
///     by a body `Cmp { var = "lit", Eq }`), the other column is a free
///     `Term::Var`, and
///   - no other positive body atom joins on the free var (the closure is a leaf:
///     the free var occurs only in the closure atom and the head).
/// Otherwise `None` (caller decides: not-a-closure-read = fine; closure-read but
/// not seedable = a hard error in `check_stratification`).
pub(crate) fn closure_seed_of(
    rule: &Rule,
    closures: &HashMap<String, String>,
) -> Option<ClosureSeed> {
    // The single positive closure atom, if exactly one exists.
    let mut closure_atoms = rule.body.iter().filter_map(|it| match it {
        BodyItem::Pos(a) if closures.contains_key(&a.rel) => Some(a),
        _ => None,
    });
    let atom = closure_atoms.next()?;
    if closure_atoms.next().is_some() {
        return None;
    } // >1 closure read: not seedable
    if atom.terms.len() != 2 {
        return None;
    }
    let edge = closures.get(&atom.rel)?.clone();

    // An Eq body constraint `v = "lit"` (either operand order) pins var `v`.
    let lit_for = |v: &str| -> Option<String> {
        rule.body.iter().find_map(|it| match it {
            BodyItem::Cmp(c) if c.op == CmpOp::Eq => match (&c.lhs, &c.rhs) {
                (Term::Var(lv), Term::Str(s)) | (Term::Str(s), Term::Var(lv)) if lv == v => {
                    Some(s.clone())
                }
                _ => None,
            },
            _ => None,
        })
    };
    // Resolve each endpoint to either a literal seed or a free var name.
    enum End {
        Seed(String),
        Free(String),
    }
    let classify = |t: &Term| -> Option<End> {
        match t {
            Term::Str(s) => Some(End::Seed(s.clone())),
            Term::Var(v) => Some(match lit_for(v) {
                Some(s) => End::Seed(s),
                None => End::Free(v.clone()),
            }),
            _ => None,
        }
    };
    let (e0, e1) = (classify(&atom.terms[0])?, classify(&atom.terms[1])?);
    let (seed, forward, free_var) = match (e0, e1) {
        (End::Seed(s), End::Free(v)) => (s, true, v),
        (End::Free(v), End::Seed(s)) => (s, false, v),
        _ => return None, // both pinned or both free: not the seedable shape
    };
    // The free var must be a leaf: it may appear only in this closure atom and the
    // head, never in another positive body atom (that would be a real join we
    // cannot answer from the walk alone).
    let other_join = rule.body.iter().any(|it| match it {
        BodyItem::Pos(a) if !std::ptr::eq(a, atom) => a
            .terms
            .iter()
            .any(|t| matches!(t, Term::Var(v) if *v == free_var)),
        _ => false,
    });
    if other_join {
        return None;
    }
    Some(ClosureSeed {
        edge,
        seed,
        forward,
        free_var,
    })
}

/// Reject a derived rule body that reads a closure head in a non-seedable shape.
/// Seedable reads (one endpoint pinned to a literal) ARE allowed — they evaluate
/// as a seeded reachability walk after the condensation is built (see
/// `eval_closure_seed_rule`). An unpinned read would require materializing the
/// full closure, which the SCC condensation exists to avoid; keep that out.
pub(crate) fn check_stratification(
    derived_rules: &[&Rule],
    closures: &HashMap<String, String>,
) -> Result<()> {
    for r in derived_rules {
        let reads_closure = r.body.iter().any(|it| {
            matches!(it,
            BodyItem::Pos(a) | BodyItem::Neg(a) if closures.contains_key(&a.rel))
        });
        if !reads_closure {
            continue;
        }
        // Negated closure reads are never seedable.
        let neg_closure = r.body.iter().any(|it| {
            matches!(it,
            BodyItem::Neg(a) if closures.contains_key(&a.rel))
        });
        if !neg_closure && closure_seed_of(r, closures).is_some() {
            continue;
        }
        let name = r
            .body
            .iter()
            .find_map(|it| match it {
                BodyItem::Pos(a) | BodyItem::Neg(a) if closures.contains_key(&a.rel) => {
                    Some(a.rel.clone())
                }
                _ => None,
            })
            .unwrap_or_default();
        bail!(
            "rule '{}' reads closure relation '{}' in its body in an unpinned shape; \
               reading a closure from a rule body is only supported when one endpoint is \
               pinned to a literal (seeded reachability), e.g. \
               `h(b) <- {}(a, b), a = \"X\".`. An unpinned read would materialize the \
               full closure; query '{}' directly instead.",
            r.head.rel,
            name,
            name,
            name
        );
    }
    Ok(())
}

/// Split the non-closure derived rules into seeded-closure rules (evaluated by
/// seeded BFS in the query phase) and ordinary derived rules (lowered to SQL),
/// then reject any ordinary derived rule that reads a seeded-closure head: that
/// head is filled only in the query phase, so a tier-0 rule reading it would
/// see it empty. Hoisted out of `tick` and `tick_paths`, which carried
/// identical copies of this split + validation loop (surfaced by the
/// repeated `seed_rules` / `derived_rules` locals).
pub(crate) fn split_seed_and_derived<'a>(
    all_derived: &[&'a Rule],
    closures: &HashMap<String, String>,
) -> Result<(Vec<(&'a Rule, ClosureSeed)>, Vec<&'a Rule>)> {
    let seed_rules: Vec<(&Rule, ClosureSeed)> = all_derived
        .iter()
        .copied()
        .filter_map(|r| closure_seed_of(r, closures).map(|cs| (r, cs)))
        .collect();
    let derived_rules: Vec<&Rule> = all_derived
        .iter()
        .copied()
        .filter(|r| closure_seed_of(r, closures).is_none())
        .collect();
    let seed_heads: HashSet<&str> = seed_rules
        .iter()
        .map(|(r, _)| r.head.rel.as_str())
        .collect();
    for r in &derived_rules {
        for it in &r.body {
            if let BodyItem::Pos(a) | BodyItem::Neg(a) = it {
                if seed_heads.contains(a.rel.as_str()) {
                    bail!(
                        "relation '{}' is seeded from a closure and cannot feed another \
                           derived rule ('{}') in the same tick; query it directly.",
                        a.rel,
                        r.head.rel
                    );
                }
            }
        }
    }
    Ok((seed_rules, derived_rules))
}

// Auto-index. A derived rule joins relations on a shared variable; that variable
// is the join key. Without an index the join is a nested scan, with one it is a
// lookup:
//
//   calls(c1,c2) <- fndef(c1, p, s, e), callsite(c2, p, l), ...
//                              \_______________/
//                          shared var p  =  the join key (path)
//
//   NO index                          index on the probe column
//   ────────                          ────────────────────────────
//   for each fndef row (F):           for each fndef row (F):
//     scan ALL callsite rows (C)        seek callsite WHERE path = p
//   cost  O(F * C)                     cost  O(F * log C)
//
// On the kernel/ subtree F=16k, C=96k, so the scan version is ~1.5e9 row touches
// (the 30s run). Indexing the path column collapses it to seeks.
//
// Heuristic: index every column a variable reaches across >= 2 body atoms (an
// equality join key), plus a negated atom's correlation column. This is the
// cheap form of Souffle's automatic index selection (Subotic, Jordan, Scholz,
// "Automatic index selection for large-scale datalog", which computes the
// minimal set of composite, ordered indexes via a min-chain-cover / Dilworth on
// the lattice of search orders). One single-column index per join key captures
// most of the win and stays trivial.
pub(crate) fn auto_indexes(rules: &[&Rule], rels: &Rels) -> Vec<(String, String)> {
    let mut occ: HashMap<String, Vec<(String, usize)>> = HashMap::new();
    for r in rules {
        for item in &r.body {
            let atom = match item {
                BodyItem::Pos(a) | BodyItem::Neg(a) => a,
                _ => continue,
            };
            for (pos, t) in atom.terms.iter().enumerate() {
                if let Term::Var(v) = t {
                    occ.entry(v.clone())
                        .or_default()
                        .push((atom.rel.clone(), pos));
                }
            }
        }
    }
    let mut out: BTreeSet<(String, String)> = BTreeSet::new();
    for places in occ.values() {
        if places.len() < 2 {
            continue;
        } // a variable in one atom is not a join key
        for (rel, pos) in places {
            if let Some(meta) = rels.get(rel) {
                if *pos < meta.cols.len() {
                    out.insert((rel.clone(), meta.cols[*pos].name.clone()));
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Derived relations transitively reachable from a set of changed relations in
/// the rule dependency graph. A derived rel is a pure function of its body rels,
/// so a rel whose body never (transitively) touches a changed source CANNOT have
/// changed and need not be rebuilt. Returns the affected derived heads only.
pub(crate) fn affected_derived(
    derived_rules: &[&Rule],
    changed: &HashSet<String>,
) -> HashSet<String> {
    let mut affected: HashSet<String> = changed.clone(); // seed with changed sources
    loop {
        let mut grew = false;
        for r in derived_rules {
            if affected.contains(&r.head.rel) {
                continue;
            }
            let touches = r.body.iter().any(|it| match it {
                BodyItem::Pos(a) | BodyItem::Neg(a) => affected.contains(&a.rel),
                _ => false,
            });
            if touches {
                affected.insert(r.head.rel.clone());
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    derived_rules
        .iter()
        .map(|r| r.head.rel.clone())
        .filter(|h| affected.contains(h))
        .collect()
}

/// The two operator heads (scc + node2vec) fill in the QUERY phase, after the
/// derived fixpoint (`eval_scc_rule` / `eval_node2vec_rule` read an edge relation
/// the fixpoint already materialized). So a derived rule that reads one of those
/// heads cannot be lowered into the same fixpoint — it would read an empty table
/// and silently emit wrong rows. The fix: split the derived layer into a
/// `pre`-stratum (no transitive dependency on an operator head) and a
/// `post`-stratum (depends on one). The pre-stratum runs in the main fixpoint
/// (before the operator evals); the post-stratum runs AFTER the heads fill.
pub(crate) struct DerivedStrata<'a> {
    pub(crate) pre_rules: Vec<&'a Rule>,
    pub(crate) post_rules: Vec<&'a Rule>,
    pub(crate) pre_rels: Vec<String>,
    pub(crate) post_rels: Vec<String>,
}

/// Partition the derived rules/rels around the operator boundary. A rel is in the
/// post-stratum iff it transitively reads an scc/node2vec head (reuses the
/// `affected_derived` dependency walk, seeded with the operator head names).
/// Bails if an operator's own input edge depends on another operator head:
/// chaining one graph operator into another within a single tick is a cycle
/// through the query phase, which this two-stratum split cannot order.
pub(crate) fn partition_derived_strata<'a>(
    derived_rules: &[&'a Rule],
    derived_rels: &[String],
    scc_rules: &[&'a Rule],
    node2vec_rules: &[&'a Rule],
) -> Result<DerivedStrata<'a>> {
    let mut op_heads: HashSet<String> = HashSet::new();
    for r in scc_rules {
        op_heads.insert(r.head.rel.clone());
    }
    for r in node2vec_rules {
        op_heads.insert(r.head.rel.clone());
    }
    let post_heads = affected_derived(derived_rules, &op_heads);
    for r in scc_rules.iter().chain(node2vec_rules.iter()) {
        let edge = r
            .scc_edge()
            .or_else(|| r.node2vec_edge())
            .expect("operator rule has an scc/node2vec edge");
        if post_heads.contains(edge) {
            bail!(
                "operator rule '{}' reads edge relation '{}', which transitively \
                   depends on another operator head (scc/node2vec); chaining one graph \
                   operator into another within a single tick is not supported — \
                   materialize '{}' in a separate program/tick.",
                r.head.rel,
                edge,
                edge
            );
        }
    }
    let pre_rules = derived_rules
        .iter()
        .copied()
        .filter(|r| !post_heads.contains(&r.head.rel))
        .collect();
    let post_rules = derived_rules
        .iter()
        .copied()
        .filter(|r| post_heads.contains(&r.head.rel))
        .collect();
    let pre_rels = derived_rels
        .iter()
        .filter(|r| !post_heads.contains(*r))
        .cloned()
        .collect();
    let post_rels = derived_rels
        .iter()
        .filter(|r| post_heads.contains(*r))
        .cloned()
        .collect();
    Ok(DerivedStrata {
        pre_rules,
        post_rules,
        pre_rels,
        post_rels,
    })
}

pub(crate) fn intern_rel(s: &str, id: &mut HashMap<String, u32>, name: &mut Vec<String>) -> u32 {
    if let Some(&i) = id.get(s) {
        return i;
    }
    let i = name.len() as u32;
    id.insert(s.to_string(), i);
    name.push(s.to_string());
    i
}

/// stratum(C) = max over edges C->D of (stratum(D) + 1 if that edge is negative).
/// The condensed graph is a DAG, so this memoized recursion terminates.
pub(crate) fn comp_stratum(c: usize, succ: &[Vec<(u32, u32)>], memo: &mut [u32]) -> u32 {
    if memo[c] != u32::MAX {
        return memo[c];
    }
    let mut s = 0u32;
    for &(d, w) in &succ[c] {
        s = s.max(comp_stratum(d as usize, succ, memo) + w);
    }
    memo[c] = s;
    s
}

/// Stratify derived rules: a rule that negates relation R lands in a stratum
/// strictly above every rule defining R, so `!R` reads a finished relation.
/// Returns rule indices grouped by stratum, ascending. Errors if a negation
/// sits inside a recursive cycle (unstratifiable; positive recursion is fine).
pub(crate) fn stratify(rules: &[&Rule]) -> Result<Vec<Vec<usize>>> {
    let mut id: HashMap<String, u32> = HashMap::new();
    let mut name: Vec<String> = Vec::new();
    // (head, body, stratum-forcing). A negation OR an aggregation over `body` forces
    // the head into a strictly higher stratum (it must read a finished relation).
    let mut edges: Vec<(u32, u32, bool)> = Vec::new();
    for r in rules {
        let h = intern_rel(&r.head.rel, &mut id, &mut name);
        let agg = r.has_agg();
        for item in &r.body {
            let (b, force) = match item {
                BodyItem::Pos(a) => (intern_rel(&a.rel, &mut id, &mut name), agg),
                BodyItem::Neg(a) => (intern_rel(&a.rel, &mut id, &mut name), true),
                _ => continue,
            };
            edges.push((h, b, force));
        }
    }
    let n = name.len();
    let mut adj = vec![Vec::new(); n];
    for &(h, b, _) in &edges {
        adj[h as usize].push(b);
    }
    let (comp, ncomp) = scc::tarjan(&adj);

    // A negation OR aggregation inside a recursive cycle has no stratified meaning.
    // (The typecheck path reports this as a `not-stratified` TypeDiag first; this
    // bail is defense so eval never runs an ill-defined fixpoint.)
    for &(h, b, force) in &edges {
        if force && comp[h as usize] == comp[b as usize] {
            bail!(
                "unstratifiable: relation '{}' is aggregated or negated inside a recursive cycle",
                name[b as usize]
            );
        }
    }
    // condensed edge weight: 1 if any stratum-forcing edge (negation or aggregation)
    // crosses these components
    let mut cw: HashMap<(u32, u32), u32> = HashMap::new();
    for &(h, b, force) in &edges {
        let (cu, cv) = (comp[h as usize], comp[b as usize]);
        if cu != cv {
            let e = cw.entry((cu, cv)).or_insert(0);
            *e = (*e).max(if force { 1 } else { 0 });
        }
    }
    let mut succ = vec![Vec::new(); ncomp];
    for (&(cu, cv), &w) in &cw {
        succ[cu as usize].push((cv, w));
    }

    let mut memo = vec![u32::MAX; ncomp];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (ri, r) in rules.iter().enumerate() {
        let c = comp[id[&r.head.rel] as usize] as usize;
        let s = comp_stratum(c, &succ, &mut memo) as usize;
        if s >= groups.len() {
            groups.resize(s + 1, Vec::new());
        }
        groups[s].push(ri);
    }
    Ok(groups)
}

/// Split one stratum's rules into rel-level dependency components, dependencies
/// first. Rules sharing a head rel form one node; a component is recursive when
/// its rels are mutually reachable (multi-rel SCC) or a rule reads its own head.
/// `stratify` groups by negation depth, so a stratum can hold long acyclic
/// chains — each such component needs exactly one execution pass, not a fixpoint.
pub(crate) fn rel_components(group: &[usize], rules: &[&Rule]) -> Vec<(Vec<usize>, bool)> {
    let mut id: HashMap<&str, u32> = HashMap::new();
    let mut nheads = 0u32;
    for &ri in group {
        id.entry(rules[ri].head.rel.as_str()).or_insert_with(|| {
            nheads += 1;
            nheads - 1
        });
    }
    let mut adj = vec![Vec::new(); nheads as usize];
    let mut self_edge = vec![false; nheads as usize];
    for &ri in group {
        let h = id[rules[ri].head.rel.as_str()];
        for item in &rules[ri].body {
            let atom = match item {
                BodyItem::Pos(a) | BodyItem::Neg(a) => a,
                _ => continue,
            };
            match id.get(atom.rel.as_str()) {
                Some(&b) if b == h => self_edge[h as usize] = true, // tarjan skips self-loops
                Some(&b) => adj[h as usize].push(b),
                None => {} // reads a lower stratum / source rel: already final
            }
        }
    }
    let (comp, ncomp) = scc::tarjan(&adj);
    // adj points head -> body dependency, and tarjan completes every SCC
    // reachable from a node before the node's own, so ascending comp id is
    // dependencies-first evaluation order.
    let mut out: Vec<(Vec<usize>, bool)> = vec![(Vec::new(), false); ncomp];
    let mut comp_size = vec![0usize; ncomp];
    for (n, &c) in comp.iter().enumerate() {
        comp_size[c as usize] += 1;
        if self_edge[n] {
            out[c as usize].1 = true;
        }
    }
    for (c, size) in comp_size.iter().enumerate() {
        if *size > 1 {
            out[c].1 = true;
        }
    }
    for &ri in group {
        out[comp[id[rules[ri].head.rel.as_str()] as usize] as usize]
            .0
            .push(ri);
    }
    out
}
