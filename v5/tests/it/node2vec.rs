//! Structural node embeddings via the `node2vec(edge)` DSL operator.
//!
//! The unit test in `src/embed/node2vec.rs` proves the algorithm separates
//! clusters; this proves the engine WIRING: a dl program declares an edge rel,
//! extracts it from a scanned file, and `node_sim(a, b, score) <- node2vec(edge).`
//! fills the head with KNN pairs. THE GATE: in a graph of two 4-cliques joined by
//! one bridge edge, the nearest neighbor of an in-cluster node is another node of
//! the SAME cluster — topology, not names, drove the embedding.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("node2vec_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Two 4-cliques (a0..a3, b0..b3) fully connected within, plus one bridge a0<->b0.
/// Undirected => emit both directions so walks can traverse either way.
fn edges_txt() -> String {
    let mut lines = Vec::new();
    for p in ["a", "b"] {
        for i in 0..4 { for j in 0..4 { if i != j {
            lines.push(format!("{p}{i} {p}{j}"));
        }}}
    }
    lines.push("a0 b0".into());
    lines.push("b0 a0".into());
    lines.join("\n") + "\n"
}

const PROG: &str = r#"
rel edge(src: text, dst: text).
edge(src, dst) <- scan("WORK", "edges.txt", p, rev),
                  match(p, rev, /(?P<src>[a-z][0-9]) (?P<dst>[a-z][0-9])/, l).

rel node_sim(a: text, b: text, score: int).
node_sim(a, b, score) <- node2vec(edge).

? node_sim(a, b, score).
"#;

fn run(dir: &Path) -> (i32, String, String) {
    fs::write(dir.join("edges.txt"), edges_txt()).unwrap();
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap()])
        // Deterministic + enough signal for a clean split on a tiny graph.
        .env("SPREFA_N2V_DIM", "32")
        .env("SPREFA_N2V_NUMWALKS", "40")
        .env("SPREFA_N2V_WALKLEN", "20")
        .env("SPREFA_N2V_EPOCHS", "3")
        .env("SPREFA_N2V_SEED", "1")
        .env("SPREFA_NODE_SIM_K", "7")
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// (a, b, score) rows from the `? node_sim` block.
fn sim_rows(out: &str) -> Vec<(String, String, i64)> {
    let mut rows = Vec::new();
    let mut in_block = false;
    for line in out.lines() {
        if line.starts_with("? node_sim") { in_block = true; continue; }
        if in_block {
            if line.starts_with("? ") { break; }
            let t = line.trim();
            if t.is_empty() || t.starts_with('(') { continue; }
            let cols: Vec<&str> = t.split('\t').map(|s| s.trim()).collect();
            if cols.len() == 3 {
                if let Ok(sc) = cols[2].parse::<i64>() {
                    rows.push((cols[0].to_string(), cols[1].to_string(), sc));
                }
            }
        }
    }
    rows
}

#[test]
fn node2vec_separates_two_clusters() {
    let dir = sandbox("clusters");
    let (code, out, err) = run(&dir);
    assert_eq!(code, 0, "dl failed: {err}\n{out}");

    let rows = sim_rows(&out);
    assert!(!rows.is_empty(), "node_sim came back empty\nstdout:\n{out}\nstderr:\n{err}");

    // Every node should have neighbors, and each 'a' node's single best neighbor
    // (highest score) must be another 'a' node — same cluster.
    for src in ["a1", "a2", "a3"] {
        let mut nbrs: Vec<&(String, String, i64)> =
            rows.iter().filter(|(a, _, _)| a == src).collect();
        assert!(!nbrs.is_empty(), "{src} has no neighbors in node_sim");
        nbrs.sort_by_key(|(_, _, s)| -s);
        let best = &nbrs[0].1;
        assert!(best.starts_with('a'),
            "{src}'s nearest neighbor was {best} (cross-cluster); embedding did not capture structure\nrows: {nbrs:?}");
    }
}

/// One `dl` run over the given edge content, sharing a persistent `--db`, with
/// the W1/W2 digest trace on. Returns stderr (carries the `[node2vec] graph
/// 'edge': ...` marker: re-embed / cache hit / skip).
fn run_db_edges(dir: &Path, db: &Path, edges: &str) -> String {
    fs::write(dir.join("edges.txt"), edges).unwrap();
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--root", dir.to_str().unwrap(), "--db", db.to_str().unwrap(), "--no-daemon"])
        .env("SPREFA_N2V_DIM", "32")
        .env("SPREFA_N2V_NUMWALKS", "40")
        .env("SPREFA_N2V_WALKLEN", "20")
        .env("SPREFA_N2V_EPOCHS", "3")
        .env("SPREFA_N2V_SEED", "1")
        .env("SPREFA_NODE_SIM_K", "7")
        .env("SPREFA_N2V_TRACE", "1")
        .output()
        .expect("run dl");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn run_db(dir: &Path, db: &Path) -> String {
    run_db_edges(dir, db, &edges_txt())
}

/// W1 digest-skip: node2vec is a global op; an unchanged edge set must not
/// re-embed. First run over a fresh db re-embeds and records the edge digest in
/// `_reldigest`; a second run over the SAME db sees an identical graph, the
/// digest hits, and the embed is skipped. The `_reldigest` baseline persisting
/// across two processes is the same durable property `digest_skip` relies on.
#[test]
fn node2vec_digest_skip_on_unchanged_graph() {
    let dir = sandbox("digest_skip");
    fs::write(dir.join("edges.txt"), edges_txt()).unwrap();
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let db = dir.join("db");

    let first = run_db(&dir, &db);
    assert!(first.contains("graph 'edge': re-embed"),
        "first run over a fresh db must re-embed; stderr:\n{first}");
    assert!(!first.contains("graph 'edge': skip"),
        "first run cannot skip (no stored digest); stderr:\n{first}");

    let second = run_db(&dir, &db);
    assert!(second.contains("graph 'edge': skip (digest unchanged)"),
        "second run over an unchanged graph must skip the embed; stderr:\n{second}");
    assert!(!second.contains("graph 'edge': re-embed"),
        "second run must not re-embed an unchanged graph; stderr:\n{second}");
}

/// W2 recent-digest cache: bouncing between two graphs (A -> B -> A) must not
/// re-embed A the second time. W1 alone re-embeds on every switch (it remembers
/// only the last digest); W2 keeps the last N digests' vectors, so revisiting A
/// is a cache hit. A and B differ by one extra component so their edge digests
/// differ.
#[test]
fn node2vec_w2_cache_hit_on_revisited_graph() {
    let dir = sandbox("w2_cache");
    let db = dir.join("db");
    let graph_a = edges_txt();
    // B = A plus a disjoint 2-node component: a genuinely different edge set.
    let graph_b = format!("{graph_a}c0 c1\nc1 c0\n");

    let a1 = run_db_edges(&dir, &db, &graph_a);
    assert!(a1.contains("graph 'edge': re-embed"),
        "first sight of A must embed; stderr:\n{a1}");

    let b1 = run_db_edges(&dir, &db, &graph_b);
    assert!(b1.contains("graph 'edge': re-embed"),
        "B is a new graph, must embed; stderr:\n{b1}");

    let a2 = run_db_edges(&dir, &db, &graph_a);
    assert!(a2.contains("graph 'edge': cache hit (digest seen)"),
        "revisiting A must hit the W2 cache, not re-embed; stderr:\n{a2}");
    assert!(!a2.contains("graph 'edge': re-embed"),
        "revisiting A must not re-embed; stderr:\n{a2}");
}

/// The W2 cache is bounded: with SPREFA_N2V_CACHE=1 only the most recent digest
/// is retained, so A -> B -> A evicts A and the revisit re-embeds (proves the
/// LRU prune actually fires, not an unbounded store).
#[test]
fn node2vec_w2_cache_is_bounded() {
    let dir = sandbox("w2_bound");
    let db = dir.join("db");
    let graph_a = edges_txt();
    let graph_b = format!("{graph_a}c0 c1\nc1 c0\n");

    // Write the given edges, run with cap=1, return stderr.
    let run = |edges: &str| -> String {
        fs::write(dir.join("edges.txt"), edges).unwrap();
        fs::write(dir.join("p.dl"), PROG).unwrap();
        let out = Command::new(DL)
            .arg(dir.join("p.dl"))
            .args(["--root", dir.to_str().unwrap(), "--db", db.to_str().unwrap(), "--no-daemon"])
            .env("SPREFA_N2V_DIM", "32").env("SPREFA_N2V_NUMWALKS", "40")
            .env("SPREFA_N2V_WALKLEN", "20").env("SPREFA_N2V_EPOCHS", "3")
            .env("SPREFA_N2V_SEED", "1").env("SPREFA_NODE_SIM_K", "7")
            .env("SPREFA_N2V_TRACE", "1").env("SPREFA_N2V_CACHE", "1")
            .output().expect("run dl");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let _ = run(&graph_a);   // embed A, cache = {A}
    let _ = run(&graph_b);   // embed B, cap=1 evicts A, cache = {B}
    let a2 = run(&graph_a);  // A is gone -> must re-embed (not a cache hit)
    assert!(a2.contains("graph 'edge': re-embed"),
        "cap=1 must evict A after B, so the revisit re-embeds; stderr:\n{a2}");
    assert!(!a2.contains("cache hit"),
        "with cap=1 the revisit cannot be a cache hit; stderr:\n{a2}");
}
