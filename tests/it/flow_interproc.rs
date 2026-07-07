//! Cross-function, SCIP-resolved value flow (`examples/flow-interproc.dl`).
//!
//! The df_* lift is intra-procedural and the scip_*/call_* graph walks symbols,
//! not values; neither alone connects a value in one function to a value in
//! another. `flow_edge` unions df_edge with an interprocedural hop (an argument
//! feeding a call result flows into the resolved callee's parameters), and
//! `closure(flow_edge)` then walks value flow ACROSS the call boundary.
//!
//! THE GATE (run per language): a caller's parameter `tainted` is passed as the
//! argument to a callee whose parameter is `p`. The pair (tainted, p) is NOT a
//! direct flow_edge — it spans two functions — yet flow_reach contains it. That
//! asymmetry, with the two nodes living in different `fn`s, is the proof the
//! interprocedural hop fired and the closure walked it.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");
const PROG: &str = include_str!("../../examples/flow-interproc.dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flow_interproc_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn run(dir: &Path) -> (i32, String, String) {
    // The example also queries df_node; append it so the test can map ids.
    let prog = format!("{PROG}\n? df_node(id, kind, var, fn, file, line).\n");
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Split dl stdout into one section per `?` query (header + its data rows).
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

fn rows(sec: &str) -> Vec<Vec<String>> {
    sec.lines()
        .filter(|l| l.starts_with("  ") && l.contains('\t') && !l.contains("(0 rows)"))
        .map(|l| l.trim_start().split('\t').map(|s| s.trim_end().to_string()).collect())
        .collect()
}

/// The single df_node id matching (kind, var). Asserts uniqueness so the proof
/// pins one node, not a set.
fn node_id(nodes: &[Vec<String>], kind: &str, var: &str, out: &str, lang: &str) -> (String, String) {
    let hits: Vec<&Vec<String>> = nodes
        .iter()
        .filter(|r| r.len() >= 4 && r[1] == kind && r[2] == var)
        .collect();
    assert_eq!(hits.len(), 1, "[{lang}] expected one {kind}/{var} node, got {}:\n{out}", hits.len());
    (hits[0][0].clone(), hits[0][3].clone()) // (id, fn)
}

#[test]
fn value_flows_across_the_call_boundary_per_language() {
    // tainted (param of caller) -> arg of callee(...) -> p (param of callee).
    let rust = "fn callee(p: i32) -> i32 { let z = p; z }\n\
                fn caller(tainted: i32) {\n    let r = callee(tainted);\n    let _ = r;\n}\n";
    let ts = "function callee(p: number): number { const z = p; return z; }\n\
              function caller(tainted: number): void {\n    const r = callee(tainted);\n    const _x = r;\n}\n";
    let kotlin = "fun callee(p: Int): Int { val z = p; return z }\n\
                  fun caller(tainted: Int) {\n    val r = callee(tainted)\n    val u = r\n}\n";

    for (lang, ext, srcfile) in [("rust", "rs", rust), ("ts", "ts", ts), ("kotlin", "kt", kotlin)] {
        let d = sandbox(lang);
        fs::write(d.join(format!("src/lib.{ext}")), srcfile).unwrap();
        let (code, out, err) = run(&d);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");

        let secs = sections(&out);
        // order: flow_edge, flow_reach, flow_param_type, flow_node_type, df_node
        assert!(secs.len() >= 5, "[{lang}] expected 5 query sections:\n{out}");
        let mut edges: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[0]) {
            edges.insert((r[0].clone(), r[1].clone()));
        }
        let mut reach: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[1]) {
            reach.insert((r[0].clone(), r[1].clone()));
        }
        let nodes = rows(&secs[4]);

        let (tainted, caller_fn) = node_id(&nodes, "param", "tainted", &out, lang);
        let (p, callee_fn) = node_id(&nodes, "param", "p", &out, lang);

        // The two params live in different functions: this is a cross-fn claim.
        assert_ne!(caller_fn, callee_fn, "[{lang}] params must be in different fns:\n{out}");

        // DECISIVE 1: not a direct edge (it spans the call boundary) ...
        assert!(
            !edges.contains(&(tainted.clone(), p.clone())),
            "[{lang}] tainted->p must NOT be a direct flow_edge:\n{out}"
        );
        // ... but the interprocedural hop + closure reaches it.
        assert!(
            reach.contains(&(tainted.clone(), p.clone())),
            "[{lang}] tainted must REACH p across the call boundary:\n{out}"
        );

        // The interprocedural edge itself exists: some node in the caller fn
        // flows directly into the callee's param p.
        let inter: Vec<&(String, String)> = edges
            .iter()
            .filter(|(_, to)| to == &p)
            .collect();
        assert!(
            !inter.is_empty(),
            "[{lang}] expected an interprocedural edge into p:\n{out}"
        );
    }
}

/// THE POSITION GATE. Two arguments, two params: arg 0 must reach param 0 and
/// NOT param 1 (and symmetrically for arg 1). Under the old positional-blind
/// hop every arg reached every param, so the negative halves of this test are
/// exactly what df_arg(call, pos, arg) ⨝ df_param(param, pos) buys. Run per
/// language.
#[test]
fn args_flow_to_matching_param_positions_only() {
    let rust = "fn callee(x: i32, y: i32) { let u = x; let w = y; }\n\
                fn caller(a: i32, b: i32) {\n    let r = callee(a, b);\n    let _ = r;\n}\n";
    let ts = "function callee(x: number, y: number): void { const u = x; const w = y; }\n\
              function caller(a: number, b: number): void {\n    const r = callee(a, b);\n    const _x = r;\n}\n";
    let kotlin = "fun callee(x: Int, y: Int) { val u = x; val w = y }\n\
                  fun caller(a: Int, b: Int) {\n    val r = callee(a, b)\n    val q = r\n}\n";

    for (lang, ext, srcfile) in [("rust", "rs", rust), ("ts", "ts", ts), ("kotlin", "kt", kotlin)] {
        let d = sandbox(&format!("pos_{lang}"));
        fs::write(d.join(format!("src/lib.{ext}")), srcfile).unwrap();
        let (code, out, err) = run(&d);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");

        let secs = sections(&out);
        let mut reach: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[1]) {
            reach.insert((r[0].clone(), r[1].clone()));
        }
        let nodes = rows(&secs[4]);
        let (a, _) = node_id(&nodes, "param", "a", &out, lang);
        let (b, _) = node_id(&nodes, "param", "b", &out, lang);
        let (x, _) = node_id(&nodes, "param", "x", &out, lang);
        let (y, _) = node_id(&nodes, "param", "y", &out, lang);

        // Positive halves: each arg crosses into its own slot.
        assert!(reach.contains(&(a.clone(), x.clone())), "[{lang}] a must reach x (slot 0):\n{out}");
        assert!(reach.contains(&(b.clone(), y.clone())), "[{lang}] b must reach y (slot 1):\n{out}");
        // DECISIVE negative halves: no cross-slot leakage.
        assert!(!reach.contains(&(a.clone(), y.clone())), "[{lang}] a must NOT reach y (blind hop leak):\n{out}");
        assert!(!reach.contains(&(b.clone(), x.clone())), "[{lang}] b must NOT reach x (blind hop leak):\n{out}");
    }
}

/// THE RETURN GATE. A value passed INTO a callee, transformed, and RETURNED must
/// reach a sink the call result feeds. `driver`'s `secret` is the argument to
/// `ident`, whose param flows to its `ret` node; the backward hop carries that
/// ret to `driver`'s call result, which is then passed to `sink`. So `secret`
/// reaches `sink`'s param `v` — across a forward hop, the callee body, a backward
/// hop, and a second forward hop. Without the `ret` node + backward rule the
/// chain breaks at the return. Run per language.
#[test]
fn value_flows_back_out_of_a_callee_per_language() {
    let rust = "fn ident(p: i32) -> i32 { let q = p; q }\n\
                fn sink(v: i32) { let _ = v; }\n\
                fn driver(secret: i32) {\n    let r = ident(secret);\n    sink(r);\n}\n";
    let ts = "function ident(p: number): number { return p; }\n\
              function sink(v: number): void { const _x = v; }\n\
              function driver(secret: number): void {\n    const r = ident(secret);\n    sink(r);\n}\n";
    let kotlin = "fun ident(p: Int): Int { return p }\n\
                  fun sink(v: Int) { val _u = v }\n\
                  fun driver(secret: Int) {\n    val r = ident(secret)\n    sink(r)\n}\n";

    for (lang, ext, srcfile) in [("rust", "rs", rust), ("ts", "ts", ts), ("kotlin", "kt", kotlin)] {
        let d = sandbox(&format!("ret_{lang}"));
        fs::write(d.join(format!("src/lib.{ext}")), srcfile).unwrap();
        let (code, out, err) = run(&d);
        assert_eq!(code, 0, "[{lang}] must not error:\n{err}");

        let secs = sections(&out);
        let mut reach: HashSet<(String, String)> = HashSet::new();
        for r in rows(&secs[1]) {
            reach.insert((r[0].clone(), r[1].clone()));
        }
        let nodes = rows(&secs[4]);

        // The callee minted a `ret` node at all.
        assert!(
            nodes.iter().any(|r| r.len() >= 2 && r[1] == "ret"),
            "[{lang}] expected a ret node:\n{out}"
        );

        let (secret, _) = node_id(&nodes, "param", "secret", &out, lang);
        let (v, _) = node_id(&nodes, "param", "v", &out, lang);
        // DECISIVE: the source crosses INTO ident, back OUT via its return, and
        // into sink — reaching sink's param.
        assert!(
            reach.contains(&(secret.clone(), v.clone())),
            "[{lang}] secret must reach sink's v through ident's return:\n{out}"
        );
    }
}

/// A const-bound arrow (`const ident = (p) => p`) is lifted as its own fn scope —
/// params seeded, the expression body treated as an implicit return — so it joins
/// the interprocedural graph like a top-level function. `secret` must reach
/// `sink`'s `v` through the arrow's return.
#[test]
fn ts_arrow_const_is_lifted_as_a_function() {
    let d = sandbox("ts_arrow");
    fs::write(
        d.join("src/lib.ts"),
        "const ident = (p: number): number => p;\n\
         function sink(v: number): void { const _x = v; }\n\
         function driver(secret: number): void {\n    const r = ident(secret);\n    sink(r);\n}\n",
    )
    .unwrap();
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "must not error:\n{err}");

    let secs = sections(&out);
    let nodes = rows(&secs[4]);
    // The arrow was lifted: it has a param `p` under the `ident` fn sym.
    assert!(
        nodes.iter().any(|r| r.len() >= 4 && r[1] == "param" && r[2] == "p" && r[3].contains("::ident")),
        "arrow param p not lifted under ident:\n{out}"
    );
    let mut reach: HashSet<(String, String)> = HashSet::new();
    for r in rows(&secs[1]) {
        reach.insert((r[0].clone(), r[1].clone()));
    }
    let (secret, _) = node_id(&nodes, "param", "secret", &out, "ts");
    let (v, _) = node_id(&nodes, "param", "v", &out, "ts");
    assert!(
        reach.contains(&(secret, v)),
        "secret must reach v through the arrow's return:\n{out}"
    );
}

/// The typed view binds a callee's parameter slot to its declared type, resolved
/// to the type's sym (the "type-carrying" axis). Uses a user-defined type so the
/// slot survives the builtin-type filter.
#[test]
fn typed_view_carries_resolved_param_types() {
    let d = sandbox("typed");
    fs::write(
        d.join("src/lib.rs"),
        "struct Secret { v: i32 }\n\
         fn sink(p: Secret) -> i32 { let z = p; 0 }\n\
         fn caller(tainted: Secret) {\n    let r = sink(tainted);\n    let _ = r;\n}\n",
    )
    .unwrap();
    let (code, out, err) = run(&d);
    assert_eq!(code, 0, "must not error:\n{err}");

    let secs = sections(&out);
    // flow_param_type is the third query in the example.
    let typed: HashMap<(String, String), String> = rows(&secs[2])
        .into_iter()
        .filter(|r| r.len() >= 3)
        .map(|r| ((r[0].clone(), r[1].clone()), r[2].clone()))
        .collect();

    let hit = typed
        .iter()
        .find(|((sym, pos), ty)| sym.contains("::sink") && pos == "0" && ty.contains("::struct::Secret"));
    assert!(
        hit.is_some(),
        "expected sink param 0 typed as Secret:\n{out}\ntyped={typed:?}"
    );

    // Node-level view (flow_node_type, the fourth query): the specific `p` param
    // df_node of sink is bound to Secret, via df_param's positional index.
    let node_typed = rows(&secs[3]);
    assert!(
        node_typed.iter().any(|r| {
            r.len() >= 3 && r[0].ends_with(":param") && r[1].contains("::sink") && r[2].contains("::struct::Secret")
        }),
        "expected sink's param NODE bound to Secret at node granularity:\n{out}\nrows={node_typed:?}"
    );
}
