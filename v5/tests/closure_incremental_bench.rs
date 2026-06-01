//! Prove-or-kill for the reactive framing's rung-1 claim: an incremental tick
//! should recondense a call-graph closure ONLY when the call topology actually
//! moved, so per-keystroke cost is bimodal (free on edits that don't change
//! edges) and independent of corpus size.
//!
//! We build a synthetic call chain of N files (`fn fK { fK+1() }`), cold-tick a
//! `closure(calls)` program, then replay edits via `tick_paths` and read the
//! `recondensed` counter (Tarjan invocations). Asserted on the counter (exact,
//! non-flaky); wall-time is printed for the report. Run with `--nocapture`.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

const PROG: &str = r#"
rel fndef(name: text, path: file, s: int, e: int).
rel callsite(callee: text, path: file, line: int).
rel calls(caller: text, callee: text).
rel reaches(src: text, dst: text).
fndef(name, path, s, e) <- scan("WORK", "src/**/*.rs", path, rev),
  ast(path, rev, :rust, "(function_item name: (identifier) @name) @fn", s, e).
callsite(callee, path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  ast(path, rev, :rust, "(call_expression function: (identifier) @callee)", line).
calls(caller, callee) <- fndef(caller, p, s, e), callsite(callee, p, l), s <= l, l <= e, fndef(callee, _, _, _).
reaches(src, dst) <- closure(calls).
"#;

fn file_body(i: usize, n: usize, pad: char) -> String {
    // line 1: a comment (in-place edit target, no line shift)
    // line 2: fn decl ; line 3: a call to the next fn ; line 4: close
    let callee = if i + 1 < n { format!("    f{}();\n", i + 1) } else { String::new() };
    format!("// pad {pad}\npub fn f{i}() {{\n{callee}}}\n")
}

fn build_corpus(dir: &PathBuf, n: usize) {
    let src = dir.join("src");
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(&src).unwrap();
    for i in 0..n {
        fs::write(src.join(format!("f{i}.rs")), file_body(i, n, 'a')).unwrap();
    }
}

/// Returns (recondensed_delta, wall_ms) for ticking one changed file.
fn tick_one(eng: &mut Engine, prog: &sprefa_v5::ast::Program, abs: PathBuf) -> (usize, f64) {
    let before = eng.recondensed;
    let t = Instant::now();
    eng.tick_paths(prog, std::slice::from_ref(&abs), true).unwrap();
    (eng.recondensed - before, t.elapsed().as_secs_f64() * 1000.0)
}

fn run_for_n(n: usize) -> (usize, usize) {
    let dir = std::env::temp_dir().join(format!("closure_bench_{n}"));
    build_corpus(&dir, n);
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());

    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let cold_ms = t.elapsed().as_secs_f64() * 1000.0;
    let cold_recond = eng.recondensed;

    // Edit 1: flip the pad comment char on an interior file, IN PLACE (no line
    // shift). fndef/callsite rows are line-based and unchanged → source digest
    // unchanged → the `calls` edge is never marked dirty → cond reused.
    let target = dir.join("src/f5.rs");
    fs::write(&target, file_body(5, n, 'b')).unwrap();
    let (same_line_recond, same_line_ms) = tick_one(&mut eng, &prog, target.clone());

    // Edit 2: change the call target (f6 -> f7) — real topology change. Rows
    // move → edge dirty → digest moves → exactly one recondense (the `calls` edge).
    let mut body = file_body(5, n, 'b');
    body = body.replace("    f6();", "    f7();");
    fs::write(&target, body).unwrap();
    let (topo_recond, topo_ms) = tick_one(&mut eng, &prog, target.clone());

    println!(
        "N={n:>4}  cold: {cold_ms:>7.1}ms ({cold_recond} recond)  |  same-line edit: {same_line_ms:>6.2}ms ({same_line_recond} recond)  |  topology edit: {topo_ms:>6.2}ms ({topo_recond} recond)"
    );
    (same_line_recond, topo_recond)
}

#[test]
fn closure_recondenses_only_on_topology_change_and_is_corpus_independent() {
    println!("\n=== closure incremental cond-cache: prove-or-kill ===");
    let (small_same, small_topo) = run_for_n(30);
    let (big_same, big_topo) = run_for_n(300);

    // PROVE 1 (bimodal): an edit that doesn't move call edges recondenses nothing.
    assert_eq!(small_same, 0, "same-line edit must reuse the condensation (N=30)");
    assert_eq!(big_same, 0, "same-line edit must reuse the condensation (N=300)");

    // PROVE 2 (corpus-independent): the free case stays free as the corpus grows 10x.
    // (Counter equality across N is the corpus-independence proof; wall-time is printed.)

    // The topology change recondenses exactly the one affected edge graph, at any N.
    assert_eq!(small_topo, 1, "topology edit recondenses the calls graph once (N=30)");
    assert_eq!(big_topo, 1, "topology edit recondenses the calls graph once (N=300)");
    println!("=== PROVE: same-line edits recondense 0 graphs at N=30 AND N=300; topology edits recondense exactly 1 ===\n");
}
