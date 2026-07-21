//! Enforced cross-engine equivalence: the on-disk SQLite cascade and the
//! resident differential-dataflow engine must produce byte-identical survivor
//! sets after the same retraction. Previously this was eyeballed; here it is a
//! test that fails if they diverge.
//!
//! Both sides are example binaries (dd can't live in the lib — it's a dev-dep),
//! so the test builds them once and runs each as a subprocess, then diffs stdout.
//! Tiny scales only; each example self-caps its heap.

use std::path::PathBuf;
use std::process::Command;

use sprefa_store::benchgraph;

/// Independent oracle: naive forward BFS from the surviving root (node 1) over
/// the raw DAG. Shares NO code with the sqlite cascade or the dd dataflow — if
/// both engines match this, they aren't wrong in the same way. The semantics are
/// provably equal: a node survives the retraction of root 0 iff it still has a
/// support chain to a live root, which is exactly reachability from node 1.
fn oracle_survivors(layers: u32, width: u32) -> String {
    use std::collections::{HashMap, HashSet};
    let g = benchgraph::gen_multi(layers as usize, width as usize);
    // children adjacency over ENCODED keys (what the engines emit).
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for (pt, pid, ct, cid) in &g.edges {
        children
            .entry(benchgraph::encode(*pt, *pid))
            .or_default()
            .push(benchgraph::encode(*ct, *cid));
    }
    // roots encode to 0 (retracted) and 1 (survives). BFS from the surviving root.
    let mut alive: HashSet<i64> = HashSet::new();
    let mut stack = vec![1i64];
    alive.insert(1);
    while let Some(u) = stack.pop() {
        if let Some(cs) = children.get(&u) {
            for &c in cs {
                if alive.insert(c) {
                    stack.push(c);
                }
            }
        }
    }
    let mut ids: Vec<i64> = alive.into_iter().collect();
    ids.sort_unstable();
    let mut s = String::new();
    for id in ids {
        s.push_str(&id.to_string());
        s.push('\n');
    }
    s
}

fn examples_dir() -> PathBuf {
    // target dir is a sibling of CARGO_MANIFEST_DIR (or overridden by CARGO_TARGET_DIR).
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("target"));
    target.join("release").join("examples")
}

/// Engines to compare. dbsp is included only when built with `--features
/// with-dbsp` (it needs the stable toolchain; the default nightly ICEs on it).
fn engines() -> Vec<&'static str> {
    let mut e = vec!["sqlite_reach", "dd_reach"];
    if cfg!(feature = "with-dbsp") {
        e.push("dbsp_reach");
    }
    e
}

fn build_examples() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut args = vec!["build", "--release"];
    for e in engines() {
        args.push("--example");
        args.push(e);
    }
    if cfg!(feature = "with-dbsp") {
        args.push("--features");
        args.push("with-dbsp");
    }
    let status = Command::new(cargo)
        .args(&args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("spawn cargo build");
    assert!(status.success(), "building the head-to-head examples failed");
}

fn survivors(bin: &str, layers: u32, width: u32) -> String {
    let path = examples_dir().join(bin);
    let out = Command::new(&path)
        .args([layers.to_string(), width.to_string()])
        .output()
        .unwrap_or_else(|e| panic!("run {}: {e}", path.display()));
    assert!(
        out.status.success(),
        "{bin} exited {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 stdout")
}

fn assert_equiv(layers: u32, width: u32) {
    let oracle = oracle_survivors(layers, width);
    assert!(!oracle.trim().is_empty(), "oracle produced no survivors at {layers}x{width}");
    // Every engine must match the INDEPENDENT oracle, not merely each other.
    for engine in engines() {
        let got = survivors(engine, layers, width);
        assert_eq!(
            got,
            oracle,
            "{engine} disagrees with the BFS oracle at {layers}x{width}: {} vs oracle {}",
            got.lines().count(),
            oracle.lines().count()
        );
    }
}

#[test]
fn engines_match_independent_oracle_byte_for_byte() {
    build_examples();
    // Minimal, a wide fan-in shape, and a deeper branching shape.
    assert_equiv(1, 2);
    assert_equiv(2, 200);
    assert_equiv(4, 500);
    assert_equiv(6, 1000);
}
