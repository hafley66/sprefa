//! Semi-naive `rebuild_derived`: proves the delta-evaluation rewrite produces
//! the EXACT same row set the naive re-run-to-delta-0 loop would (row-set
//! equality, not "some plausible output"), across the two recursive shapes
//! the arc targets: (1) a `reach`-style single-rule self-recursion seeded by
//! a separate base rule, and (2) `entry_reach_node_raw`'s shape — a
//! depth-bounded rule (`d0 < N` guard) plus a min-depth dedup via negation in
//! a LATER stratum, mirroring std/entry.dl. Expected sets are hand-computed
//! from the graph, not copied from a prior run.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_seminaive_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn run(dir: &PathBuf, prog: &str) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl")).current_dir(dir)
        .arg("--db").arg(dir.join("db").to_str().unwrap())
        .env("DL_NO_DAEMON", "1")
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .output().expect("run dl");
    assert!(out.status.success(),
        "dl failed; stderr={}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn rows_of(stdout: &str, head: &str) -> Vec<String> {
    let mut out: Vec<String> = stdout.lines()
        .skip_while(|l| !l.starts_with(&format!("? {head} =>")))
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .filter(|l| !l.trim().starts_with('('))
        .map(|l| l.trim().to_string())
        .collect();
    out.sort();
    out
}

/// Mutual recursion across TWO rels (p/q), each rule reading the OTHER rel's
/// recursive occurrence — exercises the per-occurrence variant differentiation
/// (not just a single self-recursive rule).
#[test]
fn mutual_recursion_across_two_rels_matches_hand_computed_set() {
    let dir = sandbox("mutual");
    let stdout = run(&dir, r#"
rel edge(a: text, b: text).
edge("s1","a"). edge("a","b"). edge("b","c"). edge("c","d").
edge("s2","x"). edge("x","y").
rel seed_p(n: text).
seed_p("s1").
rel seed_q(n: text).
seed_q("s2").
rel p(n: text).
rel q(n: text).
p(n) <- seed_p(n).
p(n) <- q(m), edge(m, n).
q(n) <- seed_q(n).
q(n) <- p(m), edge(m, n).
? p(n).
? q(n).
"#);
    // p starts at s1, alternates hops through q via edge; q starts at s2.
    // Level 0: p={s1}, q={s2}
    // Level 1: p += edge(q) = edge(s2)->x => p={s1,x}; q += edge(p)=edge(s1)->a => q={s2,a}
    // Level 2: p += edge(q new: a)->b => p={s1,x,b}; q += edge(p new: x)->y => q={s2,a,y}
    // Level 3: p += edge(q new: y)-> (none, y has no outgoing edge) => p={s1,x,b}
    //          q += edge(p new: b)->c => q={s2,a,y,c}
    // Level 4: p += edge(q new: c)->d => p={s1,x,b,d}; q += edge(p new: none new) => q unchanged
    // Level 5: p unchanged (d has no outgoing edge); q unchanged. Converged.
    let mut p = rows_of(&stdout, "p");
    let mut q = rows_of(&stdout, "q");
    p.sort();
    q.sort();
    assert_eq!(p, vec!["b", "d", "s1", "x"], "p mismatch:\n{stdout}");
    assert_eq!(q, vec!["a", "c", "s2", "y"], "q mismatch:\n{stdout}");
}

/// The `entry_reach_node_raw` shape from std/entry.dl: a depth-bounded
/// self-recursive rule (`d0 < N` guard, base rule seeds depth 0) feeding a
/// LATER-stratum min-depth dedup via negation. Proves depth-bounded
/// self-recursion AND a negation-consuming downstream rule both see the
/// exact same rows the naive loop would produce.
#[test]
fn depth_bounded_self_recursion_with_downstream_negation_matches_hand_computed_set() {
    let dir = sandbox("depth");
    // Diamond: s -> a -> t, s -> b -> t. t is reachable at depth 2 via both
    // arms; min-depth dedup must keep exactly one (t, 2) row, no (t, 3+).
    // A dangling extra edge (t -> u -> v -> ... ) checks the depth cap does
    // not silently truncate short paths while it bounds long ones.
    let stdout = run(&dir, r#"
rel edge(a: text, b: text).
edge("s","a"). edge("s","b"). edge("a","t"). edge("b","t").
edge("t","u"). edge("u","v").
rel seed(n: text).
seed("s").
rel reach_raw(n: text, d: int).
reach_raw(n, 0) <- seed(n).
reach_raw(m, d0 + 1) <- reach_raw(n, d0), edge(n, m), d0 < 64.
rel not_min(n: text, d: int).
not_min(n, d) <- reach_raw(n, d), reach_raw(n, d2), d2 < d.
rel reach(n: text, d: int).
reach(n, d) <- reach_raw(n, d), !not_min(n, d).
? reach(n, d).
"#);
    // s@0; a@1, b@1 (from s); t@2 (from a or b, min wins); u@3; v@4.
    let mut reach = rows_of(&stdout, "reach");
    reach.sort();
    assert_eq!(reach, vec![
        "a\t1", "b\t1", "s\t0", "t\t2", "u\t3", "v\t4",
    ], "min-depth reach mismatch:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Counter-based red/green: `Engine::fixpoint_full_reruns` counts full-input
// rule re-executions inside a recursive fixpoint (every statement execution
// after the first pass of the naive loop — the exact waste semi-naive
// removes). In-process Engine (the extract_cache.rs idiom) so the counter
// and the `force_naive_fixpoint` A/B lever are directly observable without
// process-global env mutation.
// ---------------------------------------------------------------------------

/// A seed + a self-recursive hop over an `n`-edge chain: reach("x0") ..
/// reach("x{n}"). Naive pass p adds exactly one node past depth p, so the
/// loop runs n+1 passes over 2 statements — full re-runs = 2 * n, exactly.
fn chain_program(n: usize) -> String {
    let mut prog = String::from("rel edge(a: text, b: text).\n");
    for i in 0..n {
        prog.push_str(&format!("edge(\"x{i}\", \"x{}\").\n", i + 1));
    }
    prog.push_str("rel seed(x: text).\nseed(\"x0\").\n");
    prog.push_str("rel reach(x: text).\n");
    prog.push_str("reach(x) <- seed(x).\n");
    prog.push_str("reach(to) <- reach(from), edge(from, to).\n");
    prog
}

fn tick_chain(dir: &PathBuf, n: usize, naive: bool) -> (usize, Vec<String>) {
    let prog = parse::parse(lex::lex(&chain_program(n)).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.force_naive_fixpoint.set(naive);
    eng.tick(&prog, true).unwrap();
    let rows: Vec<String> = eng
        .query_sql("SELECT x FROM rel_reach ORDER BY x", &[]).unwrap()
        .into_iter()
        .map(|r| r[0].as_str().unwrap_or_default().to_string())
        .collect();
    (eng.fixpoint_full_reruns.get(), rows)
}

/// The counter is deterministic: a fixed fixture yields the exact expected
/// count, twice (fresh db each run).
#[test]
fn naive_rerun_counter_is_deterministic() {
    // n = 4: naive loop = 5 passes x 2 stmts, passes 2..5 counted = 8.
    let (count1, rows1) = tick_chain(&sandbox("cnt_det_a"), 4, true);
    let (count2, rows2) = tick_chain(&sandbox("cnt_det_b"), 4, true);
    assert_eq!(count1, 8, "naive full re-runs for a 4-chain: 2 stmts x 4 extra passes");
    assert_eq!(count2, count1, "the counter is deterministic across runs");
    assert_eq!(rows1, rows2);
    assert_eq!(rows1.len(), 5, "reach = x0..x4");
}

/// Scaling red/green: widen the chain so the NAIVE evaluator's counter
/// blows past a stated budget (this assertion would fail red under naive
/// evaluation), while the semi-naive evaluator lands at a SPECIFIC count:
/// zero — seeds run once, every iteration statement reads a `_delta_*`
/// snapshot, never the full input. Row sets must be identical.
#[test]
fn seminaive_eliminates_full_reruns_at_scale() {
    let n = 40;
    let (naive_count, naive_rows) = tick_chain(&sandbox("cnt_scale_naive"), n, true);
    let (semi_count, semi_rows) = tick_chain(&sandbox("cnt_scale_semi"), n, false);
    assert_eq!(naive_count, 2 * n, "naive re-runs scale with fixpoint depth");
    let budget = 20;
    assert!(naive_count > budget,
        "the naive evaluator exceeds the re-run budget ({naive_count} > {budget}) — \
         this is the red arm the semi-naive rewrite exists to fix");
    assert_eq!(semi_count, 0,
        "semi-naive issues ZERO full-input re-runs (deltas only)");
    assert_eq!(naive_rows, semi_rows, "row sets are identical across evaluators");
    assert_eq!(naive_rows.len(), n + 1);
}
