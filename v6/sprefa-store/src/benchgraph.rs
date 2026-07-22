//! One deterministic DAG generator, shared by both sides of the head-to-head so
//! their INPUTS are byte-identical by construction. Nodes 0 and 1 are roots
//! (no parents); every other node has mixed support so retracting root 0 leaves
//! a non-trivial subset alive.

/// `parents[node]` = the parent node ids. Nodes 0 and 1 are roots.
pub fn gen(layers: usize, width: usize) -> Vec<Vec<i64>> {
    let n = 2 + layers * width;
    let mut parents: Vec<Vec<i64>> = vec![Vec::new(); n];
    for l in 0..layers {
        for w in 0..width {
            let id = 2 + l * width + w;
            if l == 0 {
                parents[id].push(0);
                if w % 3 == 0 {
                    parents[id].push(1);
                }
            } else {
                let prev = 2 + (l - 1) * width;
                parents[id].push((prev + w) as i64);
                parents[id].push((prev + (w + 1) % width) as i64);
            }
        }
    }
    parents
}

/// Flatten to `(parent, child)` edges.
pub fn edges(parents: &[Vec<i64>]) -> Vec<(i64, i64)> {
    let mut e = Vec::new();
    for (id, ps) in parents.iter().enumerate() {
        for &p in ps {
            e.push((p, id as i64));
        }
    }
    e
}

/// A multi-relation reference graph: THREE logical relations so the polymorphic
/// `(tag, id)` key is load-bearing. Local ids deliberately COLLIDE across
/// relations (module 5, fn 5, type 5 are three distinct rows), so `id` alone
/// cannot address a row — only `(tag, id)` can. Edges cross relations
/// (module -> fn -> type), so retracting a module cascades through all three.
///
/// tag 0 = modules  (roots, no parents, weight 1)
/// tag 1 = functions (each depends on 1-2 modules; weight = # module parents)
/// tag 2 = types     (each depends on 1-2 functions; weight = # fn parents)
///
/// Fan-in of 2 on the derived tiers is the point: a function supported by two
/// modules SURVIVES the loss of one (weight 2 -> 1), so this is real Z-set
/// retraction, not naive reachability.
pub struct MultiGraph {
    /// (tag, id, weight)
    pub rows: Vec<(u32, i64, i64)>,
    /// (parent_tag, parent_id, child_tag, child_id)
    pub edges: Vec<(u32, i64, u32, i64)>,
    /// The retract target (a root in relation 0).
    pub seed: (u32, i64),
    /// rows per relation, index = tag.
    pub per_tag: [usize; 3],
}

/// The proven layered DAG, but tiered into THREE relations so `(tag, id)` is
/// load-bearing and one retraction cascades across all three. Tier of a node =
/// its dependency depth; `tag = tier % 3`. Roots (tier 0) are relation 0.
/// Consecutive tiers always differ mod 3, so EVERY edge crosses relations.
/// Local ids restart per relation, so they collide across relations (only
/// `(tag,id)` is unique). Two roots (0 and 1) with mixed support means
/// retracting root 0 kills the 0-lineage while the 1-lineage survives — real
/// Z-set retraction with a non-trivial cross-relation cascade.
pub fn gen_multi(layers: usize, width: usize) -> MultiGraph {
    let parents = gen(layers, width); // parents[g] = global parent ids
    let n = parents.len();

    // tier(g): roots (g<2) = 0; node 2+l*width+w = tier l+1.
    let tier = |g: usize| -> usize {
        if g < 2 { 0 } else { 1 + (g - 2) / width }
    };
    let tag_of = |g: usize| -> u32 { (tier(g) % 3) as u32 };

    // Assign a per-relation local id to every global node, in global order.
    let mut local = vec![0i64; n];
    let mut per_tag = [0usize; 3];
    for g in 0..n {
        let t = tag_of(g) as usize;
        local[g] = per_tag[t] as i64;
        per_tag[t] += 1;
    }

    let mut rows = Vec::with_capacity(n);
    let mut edges = Vec::new();
    for g in 0..n {
        let w = if parents[g].is_empty() { 1 } else { parents[g].len() as i64 };
        rows.push((tag_of(g), local[g], w));
        for &p in &parents[g] {
            let pg = p as usize;
            edges.push((tag_of(pg), local[pg], tag_of(g), local[g]));
        }
    }

    MultiGraph {
        rows,
        edges,
        seed: (tag_of(0), local[0]), // global root 0
        per_tag,
    }
}

/// Encode `(tag, id)` into one dense integer so the resident engines (dd, dbsp)
/// — which only do reachability over opaque node keys — see byte-identical
/// inputs/outputs to the tagged SQLite side. Stride must exceed any local id.
pub const TAG_STRIDE: i64 = 1_000_000_000;

#[inline]
pub fn encode(tag: u32, id: i64) -> i64 {
    tag as i64 * TAG_STRIDE + id
}

/// The proven layered graph, but with CYCLES injected so the counting cascade is
/// provably WRONG and DRed/dd are provably right at scale. Back-edges point from a
/// node to its own layer-`l-1` parent, forming a 2-cycle (parent supports child AND
/// child supports parent). `back_stride` selects which nodes get a back-edge: every
/// node where `(global_id) % back_stride == 0`, so `back_stride=1` makes every
/// derived node cyclic and a large stride makes it sparse. `back_stride=0` = no
/// back-edges (identical to `gen_multi`). Each back-edge adds a support, so the
/// ancestor's weight rises by one — real Z-set weight, not a boolean.
///
/// Correctness consequence: a cycle whose only outside anchor is root 0 dies when
/// root 0 is cut (no surviving anchor). Counting keeps it alive (phantom — the
/// members mutually support each other, weight never reaches 0). DRed and dd kill
/// it. `oracle_survivors` is the independent referee.
pub fn gen_multi_cyclic(layers: usize, width: usize, back_stride: usize) -> MultiGraph {
    let mut g = gen_multi(layers, width);
    if back_stride == 0 {
        return g;
    }
    // Rebuild global structure to find each node's layer-(l-1) parent to point back at.
    let parents = gen(layers, width); // parents[global] = global parent ids
    let n = parents.len();
    let tier = |gid: usize| -> usize { if gid < 2 { 0 } else { 1 + (gid - 2) / width } };
    let tag_of = |gid: usize| -> u32 { (tier(gid) % 3) as u32 };
    // recover the same per-relation local ids gen_multi assigned (global order).
    let mut local = vec![0i64; n];
    let mut per_tag = [0usize; 3];
    for gid in 0..n {
        let t = tag_of(gid) as usize;
        local[gid] = per_tag[t] as i64;
        per_tag[t] += 1;
    }
    // add back-support edges child -> first-parent, and bump the parent's weight.
    let mut extra_weight = std::collections::HashMap::<(u32, i64), i64>::new();
    for gid in 2..n {
        if gid % back_stride != 0 {
            continue;
        }
        let Some(&p) = parents[gid].first() else { continue };
        // Never draw a back-edge INTO a root (global id < 2): a root must stay a
        // true source (in-degree 0), and an edge into the cut node would make "cut"
        // mean node-deleted to the oracle but root-re-derivable to dd/DRed. The
        // interesting cycle is between two DERIVED nodes, anchored to a root only
        // through a forward path — cut the root and the whole cycle must die.
        if (p as usize) < 2 {
            continue;
        }
        let (pt, pi) = (tag_of(p as usize), local[p as usize]);
        let (ct, ci) = (tag_of(gid), local[gid]);
        // child supports parent (the back-edge that closes the cycle).
        g.edges.push((ct, ci, pt, pi));
        *extra_weight.entry((pt, pi)).or_insert(0) += 1;
    }
    for row in g.rows.iter_mut() {
        if let Some(add) = extra_weight.get(&(row.0, row.1)) {
            row.2 += add;
        }
    }
    g
}

/// Independent ground truth: after cutting `cut`, which rows are still supported?
/// A row survives iff it is forward-reachable (over support edges) from a SURVIVING
/// root — a root being any row with no incoming support edge (in-degree 0). This is
/// a dead-simple in-Rust BFS owing nothing to counting, DRed, dd, or SQLite, so it
/// is the referee all three are checked against. Returns encoded survivor keys.
pub fn oracle_survivors(g: &MultiGraph, cut: (u32, i64)) -> std::collections::BTreeSet<i64> {
    use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
    let cut_key = encode(cut.0, cut.1);
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut has_parent: HashSet<i64> = HashSet::new();
    for (pt, pi, ct, ci) in &g.edges {
        let (pk, ck) = (encode(*pt, *pi), encode(*ct, *ci));
        adj.entry(pk).or_default().push(ck);
        has_parent.insert(ck);
    }
    // roots = rows with no incoming support edge, minus the cut row.
    let mut frontier: VecDeque<i64> = VecDeque::new();
    let mut seen: BTreeSet<i64> = BTreeSet::new();
    for (t, i, _w) in &g.rows {
        let k = encode(*t, *i);
        if k != cut_key && !has_parent.contains(&k) {
            seen.insert(k);
            frontier.push_back(k);
        }
    }
    while let Some(k) = frontier.pop_front() {
        if let Some(children) = adj.get(&k) {
            for &c in children {
                if c != cut_key && seen.insert(c) {
                    frontier.push_back(c);
                }
            }
        }
    }
    seen
}
