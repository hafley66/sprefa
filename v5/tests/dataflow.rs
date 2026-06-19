//! The lift-to-node dataflow family (`df_node` / `df_edge`), wired as a lazy
//! indexer like CALL_RELS. The Rust extractor (syn) lifts each value-bearing
//! position to a node and emits local value-flow edges; the engine stores them
//! and `df_reaches(a,b) <- closure(df_edge)` walks the lifted graph transitively
//! on the SAME SCC engine the call/type/module graphs already use.
//!
//! The gate test proves the model end to end on a macro-free chain:
//!   fn f(name) { let s = greet(name); let u = echo(&s); use_up(&u); }
//! `name` (param) must REACH the read of `u` several hops later — but the pair
//! is NOT a direct `df_edge`. That asymmetry (absent from base edges, present in
//! the closure) is the proof: the lift produced a real graph and the shared
//! closure engine is doing the transitive walk, not just echoing base rows.
//!
//! Sprefa `?` queries are single-relation (no inline joins), and a closure head
//! may only be read in a rule with a literal-pinned endpoint. So the proof runs
//! three direct queries and joins their result tables in Rust.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dataflow_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args([
            "--root",
            dir.to_str().unwrap(),
            "--db",
            dir.join("db").to_str().unwrap(),
        ])
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Split dl stdout into one String per `?` query, in order. Each section is the
/// header line plus its indented data rows.
fn sections(out: &str) -> Vec<String> {
    let mut secs: Vec<String> = Vec::new();
    for line in out.lines() {
        if line.starts_with("? ") {
            secs.push(String::new());
        }
        if let Some(cur) = secs.last_mut() {
            if !cur.is_empty() {
                cur.push('\n');
            }
            cur.push_str(line);
        }
    }
    secs
}

/// Parse a section's indented tab-separated rows into Vec<Vec<String>>.
fn rows(sec: &str) -> Vec<Vec<String>> {
    sec.lines()
        .filter(|l| l.starts_with("  ") && l.contains('\t') && !l.contains("(0 rows)"))
        .map(|l| {
            l.trim_start()
                .split('\t')
                .map(|s| s.trim_end().to_string())
                .collect()
        })
        .collect()
}

/// The two DATAFLOW_RELS are reserved, matching CALL_RELS / TYPE_RELS.
#[test]
fn dataflow_rels_are_reserved() {
    let d = sandbox("reserved");
    for rel in ["df_node", "df_edge"] {
        let prog = format!("rel {rel}(a: text).\n? {rel}(\"x\").\n");
        let (code, _out, err) = run(&d, &prog);
        assert_ne!(code, 0, "{rel} must be reserved (expected error):\n{err}");
        assert!(
            err.contains("built-in dataflow relation"),
            "{rel} reservation message missing:\n{err}"
        );
    }
}

/// A language whose extract_dataflow is still the empty default keeps the wiring
/// live: the lazy indexer runs, relations are queryable and empty, and
/// closure(df_edge) is a legal (empty) edge rel. (Rust/Kotlin/TS all override
/// now; this is the reservation + lazy-trigger smoke test for any not-yet-
/// implemented front-end that returns the empty default.)
#[test]
fn dataflow_lazy_gate_smoke() {
    let d = sandbox("empty");
    fs::create_dir_all(d.join("src")).unwrap();
    // No .rs/.kt/.ts file scanned -> df_node/df_edge stay empty, closure is legal.
    fs::write(d.join("src/notes.txt"), "not source\n").unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.txt\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_reaches(a, b).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "no-source scan must not error:\n{err}");
    assert!(out.contains("(0 rows)"), "expected zero-row footers:\n{out}");
}

/// THE GATE. The lift produces a node for every value-bearing position; local
/// flow edges link them; and `closure(df_edge)` walks the chain so the `name`
/// param REACHES the read of `u` several hops later. That pair is absent from
/// `df_edge` (not a base edge) yet present in `df_reaches` (transitive) — the
/// asymmetry that proves the closure engine is doing real work on the lifted graph.
#[test]
fn rust_lift_closes_transitively() {
    let d = sandbox("rust");
    fs::create_dir_all(d.join("src")).unwrap();
    // Macro-free so syn sees the real Expr children (macros are token streams,
    // which the lift mints a node for but cannot chase into).
    //   chain in f: name(param) -> greet(name) -> s -> echo(&s) -> u -> use_up(&u)
    fs::write(
        d.join("src/lib.rs"),
        "fn greet(g: &str) -> String { String::new() }\n\
         fn echo(x: &str) -> String { String::new() }\n\
         fn use_up(v: &str) {}\n\
         fn f(name: &str) {\n    \
             let s = greet(name);\n    \
             let u = echo(&s);\n    \
             use_up(&u);\n\
         }\n",
    )
    .unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /./, line).\n",
        "rel df_reaches(from: text, to: text).\n",
        "df_reaches(a, b) <- closure(df_edge).\n",
        "? df_node(id, kind, var, fn, file, line).\n",
        "? df_edge(from, to).\n",
        "? df_reaches(from, to).\n",
    );
    let (code, out, err) = run(&d, prog);
    assert_eq!(code, 0, "Rust lift must not error:\n{err}");

    let secs = sections(&out);
    assert!(secs.len() >= 3, "expected 3 query sections:\n{out}");

    // Section 0: df_node -> map id -> (kind, var).
    let mut nodes: HashMap<String, (String, String)> = HashMap::new();
    for r in rows(&secs[0]) {
        assert!(r.len() >= 3, "df_node row too short: {r:?}");
        nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone()));
    }
    // Section 1: df_edge -> (from, to) set.
    let mut edges: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[1]) {
        assert!(r.len() >= 2, "df_edge row too short: {r:?}");
        edges.insert((r[0].clone(), r[1].clone()));
    }
    // Section 2: df_reaches -> (from, to) set.
    let mut reaches: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[2]) {
        assert!(r.len() >= 2, "df_reaches row too short: {r:?}");
        reaches.insert((r[0].clone(), r[1].clone()));
    }

    // The lift produced the source node: exactly one param named `name`.
    let name_params: Vec<&String> = nodes
        .iter()
        .filter(|(_, (k, v))| k == "param" && v == "name")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(name_params.len(), 1, "expected one param `name`:\n{out}");
    let name_id = name_params[0].clone();

    // And the sink node: exactly one var_read of `u`.
    let u_reads: Vec<&String> = nodes
        .iter()
        .filter(|(_, (k, v))| k == "var_read" && v == "u")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(u_reads.len(), 1, "expected one var_read `u`:\n{out}");
    let u_id = u_reads[0].clone();

    // There is a real graph to walk, not an empty one.
    assert!(!edges.is_empty(), "df_edge is empty — lift produced no edges:\n{out}");

    // DECISIVE: the (name, u_read) pair is NOT a base edge ...
    assert!(
        !edges.contains(&(name_id.clone(), u_id.clone())),
        "name->u must NOT be a direct df_edge (closure earns its keep):\n{out}"
    );
    // ... yet closure reaches it: name flows to u across the chain.
    assert!(
        reaches.contains(&(name_id.clone(), u_id.clone())),
        "name must REACH u transitively via closure(df_edge):\n{out}"
    );
}

/// THE TAINT GATE. Exact dataflow says `m = a + 1` is NOT `a` (a new value), so
/// `a` would not reach `m`. Taint tracking propagates `a` THROUGH the operation
/// into `m`, so the source param `q` reaches the sink argument `m`. That reach,
/// routed via a `binop` node, is the proof the lift is doing taint-style
/// propagation through operations — the defining feature of taint vs dataflow.
/// Run on Rust, Kotlin, and TS to show the model generalizes across front-ends.
#[test]
fn taint_propagates_through_operations_per_language() {
    let rust = "fn sink(v: i32) {}\nfn go(q: i32) {\n    let a = q;\n    let m = a + 1;\n    sink(m);\n}\n";
    let kotlin = "fun sink(v: Int) {}\nfun go(q: Int) {\n    val a = q\n    val m = a + 1\n    sink(m)\n}\n";
    let ts = "function sink(v: number): void {}\nfunction go(q: number): void {\n    const a = q;\n    const m = a + 1;\n    sink(m);\n}\n";

    for (lang, ext, src) in [("rust", "rs", rust), ("kotlin", "kt", kotlin), ("ts", "ts", ts)] {
        let d = sandbox(&format!("taint_{lang}"));
        fs::create_dir_all(d.join("src")).unwrap();
        fs::write(d.join(format!("src/lib.{ext}")), src).unwrap();
        let prog = format!(
            "rel seen(path: file).\n\
             seen(path) <- scan(\"WORK\", \"src/**/*.{ext}\", path, rev), match(path, rev, /./, line).\n\
             rel df_reaches(from: text, to: text).\n\
             df_reaches(a, b) <- closure(df_edge).\n\
             ? df_node(id, kind, var, fn, file, line).\n\
             ? df_edge(from, to).\n\
             ? df_reaches(from, to).\n",
        );
        let (code, out, err) = run(&d, &prog);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");

        let secs = sections(&out);
        assert!(secs.len() >= 3, "[{lang}] expected 3 query sections:\n{out}");
        let mut nodes: HashMap<String, (String, String)> = HashMap::new();
        for r in rows(&secs[0]) {
            nodes.insert(r[0].clone(), (r[1].clone(), r[2].clone()));
        }
        let mut edges: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[1]) {
            edges.insert((r[0].clone(), r[1].clone()));
        }
        let mut reaches: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[2]) {
            reaches.insert((r[0].clone(), r[1].clone()));
        }

        // the source: param `q`. the sink: the read of `m`. the operation: a binop.
        let q_id = single(&nodes, "param", "q", &out, lang);
        let m_id = single(&nodes, "var_read", "m", &out, lang);
        let binop = single(&nodes, "binop", "", &out, lang);

        // q is NOT directly bound to m — the only route between them is the binop.
        assert!(
            !edges.contains(&(q_id.clone(), m_id.clone())),
            "[{lang}] q->m must NOT be a direct edge:\n{out}"
        );
        // q reaches the binop, the binop reaches m, and so q reaches m. The path
        // traverses the operation: exact dataflow would stop at `a + 1`.
        assert!(reaches.contains(&(q_id.clone(), binop.clone())), "[{lang}] q must reach the binop:\n{out}");
        assert!(reaches.contains(&(binop.clone(), m_id.clone())), "[{lang}] binop must reach m:\n{out}");
        assert!(reaches.contains(&(q_id.clone(), m_id.clone())), "[{lang}] q must reach m THROUGH the binop (taint):\n{out}");
    }
}

fn single(nodes: &HashMap<String, (String, String)>, kind: &str, var: &str, out: &str, lang: &str) -> String {
    let hits: Vec<&String> = nodes
        .iter()
        .filter(|(_, (k, v))| k == kind && v == var)
        .map(|(id, _)| id)
        .collect();
    assert_eq!(hits.len(), 1, "[{lang}] expected one {kind}/{var:?} node, got {}:\n{out}", hits.len());
    hits[0].clone()
}
