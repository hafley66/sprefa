//! Structural node embeddings (DeepWalk / node2vec) — the graph sibling of the
//! text `Embedder`. Where `stub`/fastembed embed a *string* by its content, this
//! embeds a *node* by where it sits in a graph: random walks turn the graph into
//! "sentences" of node ids, then a skip-gram pass (word2vec, negative sampling)
//! learns one vector per node so nodes that co-occur in walks land near each
//! other. Names are irrelevant; topology is everything.
//!
//! Pure Rust, zero new deps: a SplitMix64 RNG seeded deterministically (so a run
//! reproduces like the `stub` text backend), an in-CSR adjacency, and an f32 SGD.
//! The output is the same `Vec<(node_id, vec)>` "pool" the text KNN consumes, so
//! `node_sim` reuses `refresh_similar_rel`'s brute-force cosine verbatim.
//!
//! v1 is DeepWalk (uniform neighbor steps). The node2vec `p`/`q` second-order
//! bias is a config knob left at 1.0 (= uniform) for now; the walk seam takes the
//! previous node so adding the bias later is local.

use super::l2_normalize;

/// Hyper-parameters. Defaults are the common node2vec starting point; every
/// field is overridable via `SPREFA_N2V_*` env in `from_env`.
#[derive(Clone, Copy, Debug)]
pub struct N2vConfig {
    pub dim: usize,        // coordinates per node
    pub walk_len: usize,   // steps per walk
    pub num_walks: usize,  // walks started per node
    pub window: usize,     // skip-gram context radius
    pub neg: usize,        // negative samples per positive
    pub epochs: usize,     // passes over the walk corpus
    pub lr: f32,           // SGD learning rate
    pub seed: u64,         // RNG seed (determinism)
}

impl Default for N2vConfig {
    fn default() -> Self {
        N2vConfig { dim: 128, walk_len: 40, num_walks: 10, window: 5,
                    neg: 5, epochs: 1, lr: 0.025, seed: 0x5eed_1337 }
    }
}

impl N2vConfig {
    /// Override any field from `SPREFA_N2V_<FIELD>` (DIM, WALKLEN, NUMWALKS,
    /// WINDOW, NEG, EPOCHS, LR, SEED). Unset fields keep the default.
    pub fn from_env() -> Self {
        let mut c = N2vConfig::default();
        let u = |k: &str, d: usize| std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(d);
        c.dim = u("SPREFA_N2V_DIM", c.dim);
        c.walk_len = u("SPREFA_N2V_WALKLEN", c.walk_len);
        c.num_walks = u("SPREFA_N2V_NUMWALKS", c.num_walks);
        c.window = u("SPREFA_N2V_WINDOW", c.window);
        c.neg = u("SPREFA_N2V_NEG", c.neg);
        c.epochs = u("SPREFA_N2V_EPOCHS", c.epochs);
        if let Some(lr) = std::env::var("SPREFA_N2V_LR").ok().and_then(|s| s.parse().ok()) { c.lr = lr; }
        if let Some(sd) = std::env::var("SPREFA_N2V_SEED").ok().and_then(|s| s.parse().ok()) { c.seed = sd; }
        c
    }
}

/// SplitMix64: tiny, dep-free, deterministic. One u64 of state per generator.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in [0, n).
    fn below(&mut self, n: usize) -> usize { (self.next_u64() % n as u64) as usize }
    /// Uniform f32 in [0, 1).
    fn unit(&mut self) -> f32 { (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32 }
}

/// Directed edges -> (node id list, CSR-ish adjacency by node index). Edges are
/// treated as directed as given; add both directions in the caller (or the dl
/// rule) for an undirected walk. Isolated endpoints that only ever appear as a
/// target still get a node id (and an empty neighbor list).
fn build_adj(edges: &[(String, String)]) -> (Vec<String>, Vec<Vec<u32>>) {
    use std::collections::HashMap;
    let mut idx: HashMap<String, u32> = HashMap::new();
    let mut ids: Vec<String> = Vec::new();
    let intern = |s: &str, ids: &mut Vec<String>, idx: &mut HashMap<String, u32>| -> u32 {
        if let Some(&i) = idx.get(s) { return i; }
        let i = ids.len() as u32;
        ids.push(s.to_string());
        idx.insert(s.to_string(), i);
        i
    };
    for (a, b) in edges {
        intern(a, &mut ids, &mut idx);
        intern(b, &mut ids, &mut idx);
    }
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); ids.len()];
    for (a, b) in edges {
        adj[idx[a] as usize].push(idx[b]);
    }
    (ids, adj)
}

/// Generate `num_walks` walks of length `walk_len` from every node. A walk stuck
/// at a sink (no out-neighbors) stops early. Uniform neighbor choice = DeepWalk.
fn walks(adj: &[Vec<u32>], cfg: &N2vConfig, rng: &mut Rng) -> Vec<Vec<u32>> {
    let n = adj.len();
    let mut out = Vec::with_capacity(n * cfg.num_walks);
    for _ in 0..cfg.num_walks {
        for start in 0..n {
            let mut w = Vec::with_capacity(cfg.walk_len);
            let mut cur = start as u32;
            w.push(cur);
            for _ in 1..cfg.walk_len {
                let nbrs = &adj[cur as usize];
                if nbrs.is_empty() { break; }
                cur = nbrs[rng.below(nbrs.len())];
                w.push(cur);
            }
            out.push(w);
        }
    }
    out
}

#[inline]
fn sigmoid(x: f32) -> f32 { 1.0 / (1.0 + (-x).exp()) }

/// Skip-gram with negative sampling. `inp` is the embedding we keep (one row per
/// node); `out` is the throwaway context matrix. For each (center, context) pair
/// inside the window, pull center toward context and push it away from `neg`
/// random nodes. Standard word2vec SGD, single-threaded over the f32 matrices.
fn skipgram(walks: &[Vec<u32>], n_nodes: usize, cfg: &N2vConfig, rng: &mut Rng) -> Vec<Vec<f32>> {
    let d = cfg.dim;
    // Small symmetric init in [-0.5/d, 0.5/d), the usual word2vec scheme.
    let mut inp: Vec<f32> = (0..n_nodes * d).map(|_| (rng.unit() - 0.5) / d as f32).collect();
    let mut out: Vec<f32> = vec![0.0; n_nodes * d];

    for _ in 0..cfg.epochs {
        for w in walks {
            for (i, &center) in w.iter().enumerate() {
                let lo = i.saturating_sub(cfg.window);
                let hi = (i + cfg.window + 1).min(w.len());
                let c = center as usize * d;
                for j in lo..hi {
                    if j == i { continue; }
                    let ctx = w[j] as usize;
                    // one positive (label 1) + neg negatives (label 0)
                    for s in 0..=cfg.neg {
                        let (target, label) = if s == 0 { (ctx, 1.0f32) }
                                              else { (rng.below(n_nodes), 0.0f32) };
                        let t = target * d;
                        let dot: f32 = (0..d).map(|k| inp[c + k] * out[t + k]).sum();
                        let g = (label - sigmoid(dot)) * cfg.lr;
                        // gradient step on both rows (read both before writing)
                        for k in 0..d {
                            let ci = inp[c + k];
                            let ti = out[t + k];
                            inp[c + k] = ci + g * ti;
                            out[t + k] = ti + g * ci;
                        }
                    }
                }
            }
        }
    }

    (0..n_nodes).map(|i| {
        let mut v = inp[i * d..(i + 1) * d].to_vec();
        l2_normalize(&mut v);
        v
    }).collect()
}

/// End to end: directed edge list -> `(node_id, L2-normalized vector)` pool,
/// ready for the same cosine KNN the text `similar` rel uses.
pub fn embed_graph(edges: &[(String, String)], cfg: &N2vConfig) -> Vec<(String, Vec<f32>)> {
    let (ids, adj) = build_adj(edges);
    if ids.is_empty() { return Vec::new(); }
    let mut rng = Rng(cfg.seed);
    let ws = walks(&adj, cfg, &mut rng);
    let vecs = skipgram(&ws, ids.len(), cfg, &mut rng);
    ids.into_iter().zip(vecs).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cos(a: &[f32], b: &[f32]) -> f32 { a.iter().zip(b).map(|(x, y)| x * y).sum() }

    /// Two 4-cliques joined by a single bridge edge. After training, two nodes in
    /// the same clique must be closer than two nodes across the bridge — the whole
    /// point of a structural embedding.
    #[test]
    fn two_clusters_separate() {
        // cluster A: a0..a3 fully connected; cluster B: b0..b3 fully connected;
        // one bridge a0-b0. Undirected => add both directions.
        let mut edges: Vec<(String, String)> = Vec::new();
        let mut clique = |p: &str, edges: &mut Vec<(String, String)>| {
            for i in 0..4 { for j in 0..4 { if i != j {
                edges.push((format!("{p}{i}"), format!("{p}{j}")));
            }}}
        };
        clique("a", &mut edges);
        clique("b", &mut edges);
        edges.push(("a0".into(), "b0".into()));
        edges.push(("b0".into(), "a0".into()));

        let cfg = N2vConfig { dim: 32, walk_len: 20, num_walks: 40, window: 4,
                              neg: 5, epochs: 3, lr: 0.05, seed: 1 };
        // @recompute unguarded: unit test of the primitive, not a reactive rule
        let pool = embed_graph(&edges, &cfg);
        let get = |name: &str| pool.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap();

        let within = cos(&get("a1"), &get("a2"));   // same clique
        let across = cos(&get("a1"), &get("b1"));   // different clique
        eprintln!("[node2vec] within-cluster cos={within:.3}  across-cluster cos={across:.3}");
        assert!(within > across,
            "within-cluster cos {within:.3} should exceed across-cluster cos {across:.3}");
    }
}
