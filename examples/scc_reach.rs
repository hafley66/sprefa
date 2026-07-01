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
use sprefa_v5::scc::{build_condensed, count_pairs, reaches_from};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

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
