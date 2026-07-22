//! Reachability workload — the REAL product query. Blast radius = transitive closure
//! of the call graph (node = function, edge = call). This is the query the engine
//! exists to answer incrementally, so its scaling info is the one that transfers.
//!
//! Rational setup, stated so we can argue about it:
//! - graph shape: a layered sparse DAG. Node u calls OUT_DEGREE functions with higher
//!   id in a bounded window. Real call graphs are sparse, mostly-forward, with bounded
//!   fan-out; a DAG keeps the closure finite and the oracle exact (cycles would only
//!   enlarge it). base = node count; edges ≈ base·OUT_DEGREE.
//! - edits are LOCALIZED, not uniform: each tick picks ONE function and rewrites a
//!   couple of its out-edges ("you changed what f calls"). This is the true change
//!   shape and the reason incrementality matters — one edit perturbs a small slice of
//!   a large closure. Uniform-random edits would misrepresent the product.
//! - the measured answer is the FULL reachable-pair relation digest (all engines must
//!   agree), maintained/recomputed each tick. That is a materialized blast-radius view.

use crate::{mix, Stream};
use std::collections::{HashMap, HashSet};

pub const OUT_DEGREE: usize = 3;
pub const WINDOW: i64 = 12; // a call targets a function within the next WINDOW ids
pub const MUL: i64 = 100_000_000; // edge (u,v) encoded as u*MUL + v

pub fn enc(u: i64, v: i64) -> i64 {
    u * MUL + v
}
pub fn dec(k: i64) -> (i64, i64) {
    (k / MUL, k % MUL)
}

fn h(a: i64, b: i64) -> u64 {
    let mut z = ((a as u64) << 20 ^ b as u64).wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z ^ (z >> 31)
}

/// The deterministic initial call graph at `n_nodes`. Shared by every reach engine's
/// setup AND the oracle, so they agree. Encoded edge keys.
pub fn initial_edges(n_nodes: usize) -> Vec<i64> {
    let n = n_nodes as i64;
    // dedup: a call-graph edge exists or not — no multiplicity. Two i values can
    // land on the same v; without dedup an engine counts weight 2 while a HashMap
    // oracle counts 1, and they diverge on the first removal.
    let mut set: HashSet<i64> = HashSet::with_capacity(n_nodes * OUT_DEGREE);
    for u in 0..n {
        for i in 0..OUT_DEGREE as i64 {
            // modulo in u64 THEN cast — casting u64->i64 first can go negative and
            // Rust's % keeps the sign, which would make span<=0 and v<u or negative.
            let span = (h(u, i) % WINDOW as u64) as i64 + 1; // 1..=WINDOW, always > 0
            let v = u + span;
            if v > u && v < n {
                set.insert(enc(u, v)); // forward DAG only
            }
        }
    }
    set.into_iter().collect()
}

/// All-pairs reachable set of a live edge multiset (keys with weight>0), as a digest.
/// BFS from each node over the adjacency of the live edges. O(V·(V+E)).
pub fn reach_digest(live_edges: &HashSet<i64>) -> (i64, u64) {
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut nodes: HashSet<i64> = HashSet::new();
    for &k in live_edges {
        let (u, v) = dec(k);
        adj.entry(u).or_default().push(v);
        nodes.insert(u);
        nodes.insert(v);
    }
    let mut digest = 0i64;
    let mut count = 0u64;
    let mut stack: Vec<i64> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    for &src in &nodes {
        seen.clear();
        stack.clear();
        if let Some(vs) = adj.get(&src) {
            stack.extend(vs);
        }
        while let Some(x) = stack.pop() {
            if seen.insert(x) {
                digest ^= mix(enc(src, x)); // reachable pair (src -> x)
                count += 1;
                if let Some(vs) = adj.get(&x) {
                    stack.extend(vs);
                }
            }
        }
    }
    (digest, count)
}

/// The reach workload's stream + oracle. Localized edits: each tick rewrites one
/// function's out-edges. Oracle = reach_digest of the final live edge set.
pub fn reach_stream(base: usize, seed: u64, ticks: usize, edits: usize) -> Stream {
    let n = base as i64;
    let mut live: HashSet<i64> = initial_edges(base).into_iter().collect();
    let mut rng = seed ^ (base as u64).wrapping_mul(0x9E3779B97F4A7C15);
    let mut next = || {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        rng >> 16
    };
    let mut out = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        let (mut adds, mut removes) = (Vec::new(), Vec::new());
        for _ in 0..edits.max(1) {
            let u = (next() % n as u64) as i64; // u64 modulo then cast — never negative
            // remove one existing out-edge of u (if any), add one new out-edge. Pick the
            // MIN matching key, not `iter().find()` — HashSet order is per-process random,
            // which would make the stream (and the oracle digest) irreproducible run-to-run.
            if let Some(k) = live.iter().copied().filter(|&k| dec(k).0 == u).min() {
                removes.push(k);
                live.remove(&k);
            }
            let span = (next() % WINDOW as u64) as i64 + 1; // 1..=WINDOW, always > 0
            let v = (u + span).min(n - 1);
            if v > u {
                let k = enc(u, v);
                if live.insert(k) {
                    adds.push(k);
                }
            }
        }
        out.push((adds, removes));
    }
    // Define the oracle by REPLAYING the emitted stream (weighted), exactly as an
    // engine does — not from the generator's parallel `live` set, which can drift.
    // This makes expected == a correct engine by construction.
    let mut replay: HashMap<i64, i64> =
        initial_edges(base).into_iter().map(|k| (k, 1)).collect();
    for (adds, rems) in &out {
        for &k in adds {
            *replay.entry(k).or_insert(0) += 1;
        }
        for &k in rems {
            if let Some(w) = replay.get_mut(&k) {
                *w -= 1;
                if *w <= 0 {
                    replay.remove(&k);
                }
            }
        }
    }
    let live_set: HashSet<i64> = replay.into_keys().collect();
    let (expected_digest, expected_live) = reach_digest(&live_set);
    Stream { ticks: out, expected_digest, expected_live }
}
