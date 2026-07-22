//! THE covering-set agreement anchor: every one of v5's 7 graph functions, run
//! on-disk over `cx_dep`, must agree byte-for-byte with the resident pure-Rust
//! ORACLE (partition-canonical for SCC). If any shape disagrees the test names it.
//!
//! The oracle is VENDORED VERBATIM from the repo root (the store is built in
//! isolation from the v5 tree by design — `Cargo.toml`). Keep `mod oracle` a byte
//! copy of `../../src/graph/{scc,walk}.rs`; a drift there is a lying test.
//! Drift check: `tests/covering-oracle-check.sh` diffs the two.

use sea_orm::{ConnectOptions, Database};
use sprefa_store::{reach, relstore::RelStore};
use std::collections::{BTreeMap, BTreeSet};

// ============================ VENDORED ORACLE ================================
// Verbatim from src/graph/scc.rs + src/graph/walk.rs (repo root). DO NOT edit
// the logic; only the module wrapper is local.
mod oracle {
    use std::collections::{BTreeSet, VecDeque};

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
            if index[start] != u32::MAX { continue; }
            let mut work: Vec<(u32, usize)> = vec![(start as u32, 0)];
            while let Some(&(v, ci)) = work.last() {
                let vu = v as usize;
                if ci == 0 {
                    index[vu] = idx; low[vu] = idx; idx += 1;
                    stack.push(v); on_stack[vu] = true;
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
                        if low[vu] < low[pu] { low[pu] = low[vu]; }
                    }
                    if low[vu] == index[vu] {
                        loop {
                            let w = stack.pop().unwrap();
                            on_stack[w as usize] = false;
                            comp[w as usize] = ncomp;
                            if w == v { break; }
                        }
                        ncomp += 1;
                    }
                }
            }
        }
        (comp, ncomp as usize)
    }

    pub struct Cond {
        pub comp: Vec<u32>,
        pub ncomp: usize,
        pub size: Vec<u64>,
        pub cyclic: Vec<bool>,
        pub cadj: Vec<Vec<u32>>,
        pub cadj_rev: Vec<Vec<u32>>,
        pub members: Vec<Vec<u32>>,
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
        for c in 0..ncomp { if size[c] > 1 { cyclic[c] = true; } }
        for (u, succ) in adj.iter().enumerate() {
            for &w in succ { if w as usize == u { cyclic[comp[u] as usize] = true; } }
        }
        let mut sets: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); ncomp];
        for (u, succ) in adj.iter().enumerate() {
            let cu = comp[u];
            for &w in succ {
                let cw = comp[w as usize];
                if cu != cw { sets[cu as usize].insert(cw); }
            }
        }
        let cadj: Vec<Vec<u32>> = sets.into_iter().map(|s| s.into_iter().collect()).collect();
        let mut cadj_rev = vec![Vec::new(); ncomp];
        for (u, succ) in cadj.iter().enumerate() {
            for &v in succ { cadj_rev[v as usize].push(u as u32); }
        }
        Cond { comp, ncomp, size, cyclic, cadj, cadj_rev, members }
    }

    pub fn count_pairs(c: &Cond) -> u128 {
        let ncomp = c.ncomp;
        let mut indeg = vec![0u32; ncomp];
        for cc in 0..ncomp { for &s in &c.cadj[cc] { indeg[s as usize] += 1; } }
        let mut topo: Vec<u32> = (0..ncomp as u32).filter(|&x| indeg[x as usize] == 0).collect();
        let mut qi = 0;
        while qi < topo.len() {
            let x = topo[qi]; qi += 1;
            for &s in &c.cadj[x as usize] {
                indeg[s as usize] -= 1;
                if indeg[s as usize] == 0 { topo.push(s); }
            }
        }
        let words = (ncomp + 63) / 64;
        let mut reach = vec![0u64; ncomp * words];
        for &cc in topo.iter().rev() {
            let cu = cc as usize;
            for s in c.cadj[cu].clone() {
                let su = s as usize;
                reach[cu * words + su / 64] |= 1u64 << (su % 64);
                for w in 0..words { reach[cu * words + w] |= reach[su * words + w]; }
            }
        }
        let mut total: u128 = 0;
        for cc in 0..ncomp {
            if c.cyclic[cc] { total += (c.size[cc] as u128) * (c.size[cc] as u128); }
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

    pub fn reaches_from(c: &Cond, start: u32) -> Vec<u32> {
        let c0 = c.comp[start as usize];
        let mut seen_comp = vec![false; c.ncomp];
        let mut q: VecDeque<u32> = c.cadj[c0 as usize].iter().copied().collect();
        while let Some(cc) = q.pop_front() {
            if seen_comp[cc as usize] { continue; }
            seen_comp[cc as usize] = true;
            for &s in &c.cadj[cc as usize] { if !seen_comp[s as usize] { q.push_back(s); } }
        }
        if c.cyclic[c0 as usize] { seen_comp[c0 as usize] = true; }
        let mut out = Vec::new();
        for cc in 0..c.ncomp {
            if seen_comp[cc] { out.extend_from_slice(&c.members[cc]); }
        }
        out
    }

    pub fn reached_by(c: &Cond, target: u32) -> Vec<u32> {
        let c0 = c.comp[target as usize];
        let mut seen_comp = vec![false; c.ncomp];
        let mut q: VecDeque<u32> = c.cadj_rev[c0 as usize].iter().copied().collect();
        while let Some(cc) = q.pop_front() {
            if seen_comp[cc as usize] { continue; }
            seen_comp[cc as usize] = true;
            for &s in &c.cadj_rev[cc as usize] { if !seen_comp[s as usize] { q.push_back(s); } }
        }
        if c.cyclic[c0 as usize] { seen_comp[c0 as usize] = true; }
        let mut out = Vec::new();
        for cc in 0..c.ncomp {
            if seen_comp[cc] { out.extend_from_slice(&c.members[cc]); }
        }
        out
    }

    pub fn multi_source_walk(
        adj: &[Vec<u32>],
        starts: &[(u32, u32, i64)],
        halt: Option<&[bool]>,
        depth_cap: Option<i64>,
    ) -> Vec<(u32, u32, i64)> {
        let n = adj.len();
        let mut out: Vec<(u32, u32, i64)> = Vec::new();
        if starts.is_empty() { return out; }
        let mut by_tag = starts.to_vec();
        by_tag.sort_unstable();
        by_tag.dedup();
        let mut seen = vec![0u32; n];
        let mut gen: u32 = 0;
        let mut queue: std::collections::VecDeque<(u32, i64)> = std::collections::VecDeque::new();
        let mut i = 0;
        while i < by_tag.len() {
            let tag = by_tag[i].0;
            gen += 1;
            queue.clear();
            while i < by_tag.len() && by_tag[i].0 == tag {
                let (_, node, depth) = by_tag[i];
                if (node as usize) < n && seen[node as usize] != gen {
                    seen[node as usize] = gen;
                    out.push((tag, node, depth));
                    queue.push_back((node, depth));
                }
                i += 1;
            }
            while let Some((mid, d)) = queue.pop_front() {
                if let Some(h) = halt { if h[mid as usize] { continue; } }
                if let Some(cap) = depth_cap { if d >= cap { continue; } }
                for &node in &adj[mid as usize] {
                    if seen[node as usize] != gen {
                        seen[node as usize] = gen;
                        out.push((tag, node, d + 1));
                        queue.push_back((node, d + 1));
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    pub fn multi_source_halt_bfs(adj: &[Vec<u32>], starts: &[(u32, u32)], halt: &[bool]) -> Vec<(u32, u32)> {
        let starts3: Vec<(u32, u32, i64)> = starts.iter().map(|&(tag, node)| (tag, node, 0)).collect();
        multi_source_walk(adj, &starts3, Some(halt), None)
            .into_iter().map(|(tag, node, _)| (tag, node)).collect()
    }
}
// ========================== END VENDORED ORACLE =============================

async fn open_store() -> (RelStore, std::path::PathBuf) {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("cover_{}_{uniq}.sqlite", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
    opt.max_connections(1).min_connections(1);
    let store = RelStore::attach(Database::connect(opt).await.unwrap()).await.unwrap();
    (store, path)
}
fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_file(p);
    let _ = std::fs::remove_file(format!("{}-wal", p.display()));
    let _ = std::fs::remove_file(format!("{}-shm", p.display()));
}

/// Dense-index an explicit edge list: node universe = sorted unique endpoints,
/// remapped to 0..k. Returns (adj over 0..k, idx->key). Keys ARE the dense idx
/// as i64, so the store's cx_dep uses those same ints and results map by identity.
fn dense(edges: &[(u32, u32)]) -> (Vec<Vec<u32>>, usize) {
    let mut nodes: BTreeSet<u32> = BTreeSet::new();
    for &(a, b) in edges { nodes.insert(a); nodes.insert(b); }
    let remap: BTreeMap<u32, u32> = nodes.iter().enumerate().map(|(i, &n)| (n, i as u32)).collect();
    let k = nodes.len();
    let mut adj = vec![Vec::new(); k];
    for &(a, b) in edges { adj[remap[&a] as usize].push(remap[&b]); }
    for v in adj.iter_mut() { v.sort_unstable(); }
    (adj, k)
}

async fn load_dense(adj: &[Vec<u32>]) -> (RelStore, std::path::PathBuf) {
    let (store, path) = open_store().await;
    // rows: every node so keys exist (weight irrelevant to reach; keeps universe explicit).
    let rows: Vec<(i64, i64, i64)> = (0..adj.len() as i64).map(|k| (0, k, 1)).collect();
    // NOTE store key = rel*STRIDE + row; we use rel 0, row = dense idx, so key == idx.
    store.add_rows(&rows).await.unwrap();
    let mut deps = Vec::new();
    for (u, succ) in adj.iter().enumerate() {
        for &w in succ { deps.push((0i64, u as i64, 0i64, w as i64)); }
    }
    store.add_deps(&deps).await.unwrap();
    (store, path)
}

fn sorted<T: Ord>(mut v: Vec<T>) -> Vec<T> { v.sort(); v }

/// Partition (set of member-groups, each sorted) from oracle comp[] over dense idx.
fn oracle_partition(cond: &oracle::Cond) -> BTreeSet<Vec<i64>> {
    let mut groups: BTreeMap<u32, Vec<i64>> = BTreeMap::new();
    for (node, &c) in cond.comp.iter().enumerate() { groups.entry(c).or_default().push(node as i64); }
    groups.into_values().map(sorted).collect()
}
/// Partition from store (node_key, comp_repr) — group by repr, drop the repr label.
fn store_partition(labels: &[(i64, i64)]) -> BTreeSet<Vec<i64>> {
    let mut groups: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for &(node, comp) in labels { groups.entry(comp).or_default().push(node); }
    groups.into_values().map(sorted).collect()
}

// ---- the shape matrix -------------------------------------------------------
fn shapes() -> Vec<(&'static str, Vec<(u32, u32)>)> {
    vec![
        ("chain5", vec![(0, 1), (1, 2), (2, 3), (3, 4)]),
        ("diamond", vec![(0, 1), (0, 2), (1, 3), (2, 3)]),
        ("cycle3", vec![(1, 2), (2, 3), (3, 1)]),
        ("cycle3+tail", vec![(0, 1), (1, 2), (2, 3), (3, 1), (2, 4)]),
        ("two_cycles", vec![(0, 1), (1, 0), (2, 3), (3, 2), (1, 2)]),
        ("self_loop", vec![(0, 0), (0, 1), (1, 2)]),
        ("figure8", vec![(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]),
        ("dag_wide", vec![(0, 2), (0, 3), (1, 3), (1, 4), (2, 5), (3, 5), (4, 5)]),
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scc_condensed_and_count_pairs_agree() {
    for (name, edges) in shapes() {
        let (adj, _k) = dense(&edges);
        let cond = oracle::build_condensed(&adj);
        let (store, path) = load_dense(&adj).await;

        // --- SCC partition ---
        let labels = reach::scc_labels(store.conn()).await.unwrap();
        assert_eq!(store_partition(&labels), oracle_partition(&cond),
            "{name}: SCC partition disagreed");

        // --- count_pairs (byte-exact) ---
        let want = oracle::count_pairs(&cond) as i128;
        let got = reach::count_pairs(store.conn()).await.unwrap();
        assert_eq!(got, want, "{name}: count_pairs disagreed");

        // --- condensation: size multiset + cyclic flags (repr space) ---
        let cd = reach::build_condensed(store.conn()).await.unwrap();
        // oracle repr = min member of each comp
        let mut o_size: BTreeMap<i64, i64> = BTreeMap::new();
        let mut o_cyclic: BTreeMap<i64, bool> = BTreeMap::new();
        for c in 0..cond.ncomp {
            let repr = *cond.members[c].iter().min().unwrap() as i64;
            o_size.insert(repr, cond.size[c] as i64);
            o_cyclic.insert(repr, cond.cyclic[c]);
        }
        let s_size: BTreeMap<i64, i64> = cd.size.iter().copied().collect();
        let s_cyclic: BTreeMap<i64, bool> = cd.cyclic.iter().copied().collect();
        assert_eq!(s_size, o_size, "{name}: component sizes disagreed");
        assert_eq!(s_cyclic, o_cyclic, "{name}: cyclic flags disagreed");
        // cadj as repr-pair set
        let mut o_cadj: BTreeSet<(i64, i64)> = BTreeSet::new();
        for cu in 0..cond.ncomp {
            let ru = *cond.members[cu].iter().min().unwrap() as i64;
            for &cw in &cond.cadj[cu] {
                let rw = *cond.members[cw as usize].iter().min().unwrap() as i64;
                o_cadj.insert((ru, rw));
            }
        }
        let s_cadj: BTreeSet<(i64, i64)> = cd.cadj.iter().copied().collect();
        assert_eq!(s_cadj, o_cadj, "{name}: condensed edges disagreed");

        cleanup(&path);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reaches_from_and_reached_by_agree() {
    for (name, edges) in shapes() {
        let (adj, k) = dense(&edges);
        let cond = oracle::build_condensed(&adj);
        let (store, path) = load_dense(&adj).await;
        for start in 0..k as u32 {
            let want = sorted(oracle::reaches_from(&cond, start).into_iter().map(|n| n as i64).collect());
            let got = sorted(reach::reaches_from(store.conn(), start as i64).await.unwrap());
            assert_eq!(got, want, "{name}: reaches_from({start}) disagreed");

            let want_r = sorted(oracle::reached_by(&cond, start).into_iter().map(|n| n as i64).collect());
            let got_r = sorted(reach::reached_by(store.conn(), start as i64).await.unwrap());
            assert_eq!(got_r, want_r, "{name}: reached_by({start}) disagreed");
        }
        cleanup(&path);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multi_source_walk_and_halt_bfs_agree() {
    // (name, edges, starts(tag,node,depth), halt nodes, cap)
    let cases: Vec<(&str, Vec<(u32, u32)>, Vec<(u32, u32, i64)>, Vec<u32>, Option<i64>)> = vec![
        ("chain", vec![(0, 1), (1, 2), (2, 3)], vec![(0, 0, 0)], vec![], None),
        ("chain_cap2", vec![(0, 1), (1, 2), (2, 3)], vec![(0, 0, 0)], vec![], Some(2)),
        ("chain_halt2", vec![(0, 1), (1, 2), (2, 3)], vec![(0, 0, 0)], vec![2], None),
        ("min_depth", vec![(0, 1), (1, 4), (0, 2), (2, 3), (3, 4)], vec![(0, 0, 0)], vec![], None),
        ("two_tags", vec![(0, 2), (1, 2), (2, 3)], vec![(10, 0, 0), (20, 1, 0)], vec![], None),
        ("cycle", vec![(0, 1), (1, 2), (2, 0)], vec![(0, 0, 0)], vec![], Some(64)),
        // no-cap cycle: the depth-carrying-CTE infinite-loop hazard. Must terminate
        // via the per-(tag,node) visited dedup, recording each once at min depth.
        ("cycle_nocap", vec![(0, 1), (1, 2), (2, 0)], vec![(0, 0, 0)], vec![], None),
        ("cycle_nocap_halt", vec![(0, 1), (1, 2), (2, 0), (2, 3)], vec![(0, 0, 0)], vec![1], None),
    ];
    for (name, edges, starts, halt_nodes, cap) in cases {
        // build adj sized to explicit max node (walk vectors are dense 0..n already)
        let n = edges.iter().flat_map(|&(a, b)| [a, b]).chain(starts.iter().map(|s| s.1)).max().unwrap() as usize + 1;
        let mut adj = vec![Vec::new(); n];
        for &(a, b) in &edges { adj[a as usize].push(b); }
        let mut halt_mask = vec![false; n];
        for &h in &halt_nodes { halt_mask[h as usize] = true; }

        let want = oracle::multi_source_walk(&adj, &starts, if halt_nodes.is_empty() { None } else { Some(&halt_mask) }, cap);

        // load these exact edges (keys == node idx) into the store
        let (store, path) = open_store().await;
        let rows: Vec<(i64, i64, i64)> = (0..n as i64).map(|k| (0, k, 1)).collect();
        store.add_rows(&rows).await.unwrap();
        let deps: Vec<(i64, i64, i64, i64)> = edges.iter().map(|&(a, b)| (0, a as i64, 0, b as i64)).collect();
        store.add_deps(&deps).await.unwrap();

        let starts_i: Vec<(i64, i64, i64)> = starts.iter().map(|&(t, nd, d)| (t as i64, nd as i64, d)).collect();
        let halt_i: Vec<i64> = halt_nodes.iter().map(|&h| h as i64).collect();
        let got = reach::multi_source_walk(store.conn(), &starts_i,
            if halt_i.is_empty() { None } else { Some(&halt_i) }, cap).await.unwrap();
        let want_i: Vec<(i64, i64, i64)> = want.iter().map(|&(t, nd, d)| (t as i64, nd as i64, d)).collect();
        assert_eq!(got, want_i, "{name}: multi_source_walk disagreed");

        // halt_bfs special case (no cap)
        if cap.is_none() {
            let starts2: Vec<(u32, u32)> = starts.iter().map(|&(t, nd, _)| (t, nd)).collect();
            let want_h = oracle::multi_source_halt_bfs(&adj, &starts2, &halt_mask);
            let starts2_i: Vec<(i64, i64)> = starts2.iter().map(|&(t, nd)| (t as i64, nd as i64)).collect();
            let got_h = reach::multi_source_halt_bfs(store.conn(), &starts2_i, &halt_i).await.unwrap();
            let want_h_i: Vec<(i64, i64)> = want_h.iter().map(|&(t, nd)| (t as i64, nd as i64)).collect();
            assert_eq!(got_h, want_h_i, "{name}: multi_source_halt_bfs disagreed");
        }
        cleanup(&path);
    }
}
