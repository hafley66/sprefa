//! SCC-condensed transitive closure. Pure graph: no SQL, no engine types.
//!
//!   reaches(x,y) iff comp(x)==comp(y) and that SCC is cyclic
//!                 or comp(x) reaches comp(y) in the condensed DAG
//!
//! The condensation answers reaches(x,*) and counts the full closure without
//! ever building the Theta(V^2) pair table. See `bin/scc_reach.rs` for the
//! standalone proof and `engine.rs` for the wired-in path.

use std::collections::{BTreeSet, VecDeque};

/// Iterative Tarjan SCC. Returns (component-id per node, number of components).
pub fn tarjan(adj: &[Vec<u32>]) -> (Vec<u32>, usize) {
    let n = adj.len();
    let mut index = vec![u32::MAX; n];
    let mut low = vec![0u32; n];
    let mut on_stack = vec![false; n];
    let mut comp = vec![u32::MAX; n];
    let mut stack: Vec<u32> = Vec::new();
    let mut idx = 0u32;
    let mut ncomp = 0u32;
    for start in 0..n {
        if index[start] != u32::MAX {
            continue;
        }
        let mut work: Vec<(u32, usize)> = vec![(start as u32, 0)];
        while let Some(&(v, ci)) = work.last() {
            let vu = v as usize;
            if ci == 0 {
                index[vu] = idx;
                low[vu] = idx;
                idx += 1;
                stack.push(v);
                on_stack[vu] = true;
            }
            if ci < adj[vu].len() {
                work.last_mut().unwrap().1 += 1;
                let w = adj[vu][ci];
                let wu = w as usize;
                if index[wu] == u32::MAX {
                    work.push((w, 0));
                } else if on_stack[wu] && index[wu] < low[vu] {
                    low[vu] = index[wu];
                }
            } else {
                work.pop();
                if let Some(&(p, _)) = work.last() {
                    let pu = p as usize;
                    if low[vu] < low[pu] {
                        low[pu] = low[vu];
                    }
                }
                if low[vu] == index[vu] {
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w as usize] = false;
                        comp[w as usize] = ncomp;
                        if w == v {
                            break;
                        }
                    }
                    ncomp += 1;
                }
            }
        }
    }
    (comp, ncomp as usize)
}

/// The condensation: everything needed to count the closure and answer queries,
/// none of which stores the closure itself.
pub struct Cond {
    pub comp: Vec<u32>,
    pub ncomp: usize,
    pub size: Vec<u64>,
    pub cyclic: Vec<bool>,
    pub cadj: Vec<Vec<u32>>,     // condensed DAG, deduped (out-edges)
    pub cadj_rev: Vec<Vec<u32>>, // condensed DAG reversed (in-edges), for reverse reach
    pub members: Vec<Vec<u32>>,  // comp -> member node ids
}

pub fn build_condensed(adj: &[Vec<u32>]) -> Cond {
    let (comp, ncomp) = tarjan(adj);
    let mut size = vec![0u64; ncomp];
    let mut members = vec![Vec::new(); ncomp];
    for (node, &c) in comp.iter().enumerate() {
        size[c as usize] += 1;
        members[c as usize].push(node as u32);
    }
    let mut cyclic = vec![false; ncomp];
    for c in 0..ncomp {
        if size[c] > 1 {
            cyclic[c] = true;
        }
    }
    for (u, succ) in adj.iter().enumerate() {
        for &w in succ {
            if w as usize == u {
                cyclic[comp[u] as usize] = true;
            }
        }
    }
    let mut sets: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); ncomp];
    for (u, succ) in adj.iter().enumerate() {
        let cu = comp[u];
        for &w in succ {
            let cw = comp[w as usize];
            if cu != cw {
                sets[cu as usize].insert(cw);
            }
        }
    }
    let cadj: Vec<Vec<u32>> = sets.into_iter().map(|s| s.into_iter().collect()).collect();
    let mut cadj_rev = vec![Vec::new(); ncomp];
    for (u, succ) in cadj.iter().enumerate() {
        for &v in succ {
            cadj_rev[v as usize].push(u as u32);
        }
    }
    Cond {
        comp,
        ncomp,
        size,
        cyclic,
        cadj,
        cadj_rev,
        members,
    }
}

/// Total reaches pairs (incl cyclic self-reach), counted from the condensation.
pub fn count_pairs(c: &Cond) -> u128 {
    let ncomp = c.ncomp;
    let mut indeg = vec![0u32; ncomp];
    for cc in 0..ncomp {
        for &s in &c.cadj[cc] {
            indeg[s as usize] += 1;
        }
    }
    let mut topo: Vec<u32> = (0..ncomp as u32)
        .filter(|&x| indeg[x as usize] == 0)
        .collect();
    let mut qi = 0;
    while qi < topo.len() {
        let x = topo[qi];
        qi += 1;
        for &s in &c.cadj[x as usize] {
            indeg[s as usize] -= 1;
            if indeg[s as usize] == 0 {
                topo.push(s);
            }
        }
    }
    let words = (ncomp + 63) / 64;
    let mut reach = vec![0u64; ncomp * words];
    for &cc in topo.iter().rev() {
        let cu = cc as usize;
        for s in c.cadj[cu].clone() {
            let su = s as usize;
            reach[cu * words + su / 64] |= 1u64 << (su % 64);
            for w in 0..words {
                reach[cu * words + w] |= reach[su * words + w];
            }
        }
    }
    let mut total: u128 = 0;
    for cc in 0..ncomp {
        if c.cyclic[cc] {
            total += (c.size[cc] as u128) * (c.size[cc] as u128);
        }
        let mut wsum: u128 = 0;
        for w in 0..words {
            let mut bits = reach[cc * words + w];
            while bits != 0 {
                let b = w * 64 + bits.trailing_zeros() as usize;
                wsum += c.size[b] as u128;
                bits &= bits - 1;
            }
        }
        total += (c.size[cc] as u128) * wsum;
    }
    total
}

/// Everything `start` reaches, reconstructed on demand from the condensation:
/// a small BFS over the condensed DAG, then expand the reached components to
/// their member nodes. Never touches a materialized closure.
pub fn reaches_from(c: &Cond, start: u32) -> Vec<u32> {
    let c0 = c.comp[start as usize];
    let mut seen_comp = vec![false; c.ncomp];
    let mut q: VecDeque<u32> = c.cadj[c0 as usize].iter().copied().collect();
    while let Some(cc) = q.pop_front() {
        if seen_comp[cc as usize] {
            continue;
        }
        seen_comp[cc as usize] = true;
        for &s in &c.cadj[cc as usize] {
            if !seen_comp[s as usize] {
                q.push_back(s);
            }
        }
    }
    if c.cyclic[c0 as usize] {
        seen_comp[c0 as usize] = true;
    } // self-reach via the cycle
    let mut out = Vec::new();
    for cc in 0..c.ncomp {
        if seen_comp[cc] {
            out.extend_from_slice(&c.members[cc]);
        }
    }
    out
}

/// Everything that reaches `target`: the mirror of `reaches_from`, walking the
/// reversed condensed DAG. Same algorithm, in-edges instead of out-edges.
pub fn reached_by(c: &Cond, target: u32) -> Vec<u32> {
    let c0 = c.comp[target as usize];
    let mut seen_comp = vec![false; c.ncomp];
    let mut q: VecDeque<u32> = c.cadj_rev[c0 as usize].iter().copied().collect();
    while let Some(cc) = q.pop_front() {
        if seen_comp[cc as usize] {
            continue;
        }
        seen_comp[cc as usize] = true;
        for &s in &c.cadj_rev[cc as usize] {
            if !seen_comp[s as usize] {
                q.push_back(s);
            }
        }
    }
    if c.cyclic[c0 as usize] {
        seen_comp[c0 as usize] = true;
    }
    let mut out = Vec::new();
    for cc in 0..c.ncomp {
        if seen_comp[cc] {
            out.extend_from_slice(&c.members[cc]);
        }
    }
    out
}
