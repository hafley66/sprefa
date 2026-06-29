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

fn run(dir: &Path) -> (i32, String, String) {
    let prog = r#"
rel edge(src: text, dst: text).
edge(src, dst) <- scan("WORK", "edges.txt", p, rev),
                  match(p, rev, /(?P<src>[a-z][0-9]) (?P<dst>[a-z][0-9])/, l).

rel node_sim(a: text, b: text, score: int).
node_sim(a, b, score) <- node2vec(edge).

? node_sim(a, b, score).
"#;
    fs::write(dir.join("edges.txt"), edges_txt()).unwrap();
    fs::write(dir.join("p.dl"), prog).unwrap();
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
