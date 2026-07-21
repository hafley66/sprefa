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
