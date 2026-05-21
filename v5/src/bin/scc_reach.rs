//! E4 proof: SCC-condensed transitive closure over `rel_calls` in a SQLite db.
//! The naive recursive fixpoint took ~197s to materialize this closure (E1).
//! This computes the SAME closure structurally, and answers reaches(x,*) queries
//! from the condensed form, WITHOUT ever building the Theta(V^2) pair table.
//!
//!   reaches(x,y) iff comp(x)==comp(y) and that SCC is cyclic
//!                 or comp(x) reaches comp(y) in the condensed DAG
//!
//! Usage:
//!   scc_reach <db>                 stats + total closure size + timing
//!   scc_reach <db> reaches <fn>    list everything <fn> reaches (from condensed form)

use rusqlite::Connection;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::time::Instant;

/// Iterative Tarjan SCC. Returns (component-id per node, number of components).
fn tarjan(adj: &[Vec<u32>]) -> (Vec<u32>, usize) {
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

/// The condensation: everything needed to count the closure and answer queries,
/// none of which stores the closure itself.
struct Cond {
    comp: Vec<u32>,
    ncomp: usize,
    size: Vec<u64>,
    cyclic: Vec<bool>,
    cadj: Vec<Vec<u32>>,    // condensed DAG, deduped
    members: Vec<Vec<u32>>, // comp -> member node ids
}

fn build_condensed(adj: &[Vec<u32>]) -> Cond {
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
    Cond { comp, ncomp, size, cyclic, cadj, members }
}

/// Total reaches pairs (incl cyclic self-reach), counted from the condensation.
fn count_pairs(c: &Cond) -> u128 {
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

/// Everything `start` reaches, reconstructed on demand from the condensation:
/// a small BFS over the condensed DAG, then expand the reached components to
/// their member nodes. Never touches a materialized closure.
fn reaches_from(c: &Cond, start: u32) -> Vec<u32> {
    let c0 = c.comp[start as usize];
    let mut seen_comp = vec![false; c.ncomp];
    let mut q: VecDeque<u32> = c.cadj[c0 as usize].iter().copied().collect();
    while let Some(cc) = q.pop_front() {
        if seen_comp[cc as usize] { continue; }
        seen_comp[cc as usize] = true;
        for &s in &c.cadj[cc as usize] { if !seen_comp[s as usize] { q.push_back(s); } }
    }
    if c.cyclic[c0 as usize] { seen_comp[c0 as usize] = true; } // self-reach via the cycle
    let mut out = Vec::new();
    for cc in 0..c.ncomp {
        if seen_comp[cc] { out.extend_from_slice(&c.members[cc]); }
    }
    out
}

fn naive_reach_count(adj: &[Vec<u32>]) -> u128 {
    let n = adj.len();
    let mut total: u128 = 0;
    for s in 0..n {
        let mut seen = vec![false; n];
        let mut q: VecDeque<u32> = adj[s].iter().copied().collect();
        while let Some(v) = q.pop_front() {
            if seen[v as usize] { continue; }
            seen[v as usize] = true;
            for &w in &adj[v as usize] { if !seen[w as usize] { q.push_back(w); } }
        }
        total += seen.iter().filter(|&&b| b).count() as u128;
    }
    total
}

fn verify_small() {
    let names = ["main", "run", "parse", "lex", "log", "helper"];
    let id = |s: &str| names.iter().position(|&x| x == s).unwrap() as u32;
    let mut adj = vec![Vec::new(); names.len()];
    for (a, b) in [("main","run"),("run","parse"),("run","log"),("parse","lex"),("lex","run")] {
        adj[id(a) as usize].push(id(b));
    }
    let c = build_condensed(&adj);
    assert_eq!(count_pairs(&c), naive_reach_count(&adj), "SCC == naive on example");
    assert_eq!(count_pairs(&c), 16, "running example has 16 reaches pairs");
    assert_eq!(reaches_from(&c, id("main")).len(), 4, "main reaches run,parse,lex,log");
    assert!(reaches_from(&c, id("run")).contains(&id("run")), "run reaches itself (on the cycle)");
    assert_eq!(reaches_from(&c, id("log")).len(), 0, "log is a sink");
    println!("verify: example closure=16, reaches(main)=4, run self-reaches, log sink -> OK");
}

fn main() {
    verify_small();
    let args: Vec<String> = std::env::args().collect();
    let db = args.get(1).cloned().unwrap_or_else(|| { eprintln!("usage: scc_reach <db> [reaches <fn>]"); std::process::exit(1); });

    let conn = Connection::open(&db).unwrap();
    let mut intern: HashMap<String, u32> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    let mut edges: Vec<(u32, u32)> = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT caller, callee FROM rel_calls").unwrap();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).unwrap();
        for row in rows.flatten() {
            let mut id = |s: String| -> u32 {
                if let Some(&i) = intern.get(&s) { return i; }
                let i = names.len() as u32; intern.insert(s.clone(), i); names.push(s); i
            };
            let a = id(row.0); let b = id(row.1);
            edges.push((a, b));
        }
    }
    let n = names.len();
    let mut adj = vec![Vec::new(); n];
    for &(a, b) in &edges { adj[a as usize].push(b); }

    let t = Instant::now();
    let cond = build_condensed(&adj);
    let total = count_pairs(&cond);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("nodes={n} edges={} SCCs={}", edges.len(), cond.ncomp);
    println!("reaches pairs (the full closure) = {total}");
    println!("SCC-condensed build+count in {ms:.1} ms   (naive fixpoint for the same closure: ~197000 ms)");

    if args.get(2).map(|s| s.as_str()) == Some("reaches") {
        if let Some(fname) = args.get(3) {
            match intern.get(fname) {
                Some(&fid) => {
                    let tq = Instant::now();
                    let mut hit: Vec<&str> = reaches_from(&cond, fid).iter().map(|&i| names[i as usize].as_str()).collect();
                    let qus = tq.elapsed().as_secs_f64() * 1_000_000.0;
                    hit.sort();
                    println!("\nreaches({fname}) = {} functions, answered from condensed form in {qus:.0} us:", hit.len());
                    let preview: Vec<&str> = hit.iter().take(20).copied().collect();
                    println!("  {}{}", preview.join(", "), if hit.len() > 20 { ", ..." } else { "" });
                }
                None => println!("\n'{fname}' is not a caller in the graph"),
            }
        }
    }
}
