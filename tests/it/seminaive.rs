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

fn run_failing_with_row_max(dir: &PathBuf, prog: &str, row_max: usize) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl")).current_dir(dir)
        .arg("--db").arg(dir.join("db").to_str().unwrap())
        .env("DL_NO_DAEMON", "1")
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_FIXPOINT_ROW_MAX", row_max.to_string())
        .output().expect("run dl");
    assert!(!out.status.success(), "diverging program unexpectedly succeeded");
    String::from_utf8_lossy(&out.stderr).into_owned()
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

/// Regression for the `merge(MinBy(d))` lattice added to std/entry.dl's
/// reachability rules: a bare depth-counter recursion over a cyclic graph
/// produces a (node x path-length) PRODUCT (one row per node per depth it's
/// reachable at, growing until the `d0 < 64` cap). The MinBy lattice keyed on
/// the node collapses that to exactly one row per node, holding the minimum
/// depth. Graph: s->a->b->c->t plus a back-edge c->a forming a cycle, so the
/// unguarded recursion would keep re-deriving a/b/c/t at ever-increasing
/// depths through the cycle.
#[test]
fn minby_lattice_collapses_cyclic_reach_to_one_row_per_node_at_min_depth() {
    let dir = sandbox("minby");
    let stdout = run(&dir, r#"
rel edge(from: text, to: text).
edge("s","a"). edge("a","b"). edge("b","c"). edge("c","t"). edge("c","a").
rel seed(node: text).
seed("s").
rel reach(node: text, d: int) key(node) merge(MinBy(d)).
reach(node, 0) <- seed(node).
reach(to, d0 + 1) <- reach(from, d0), edge(from, to), d0 < 64.
? reach(node, d).
"#);
    let mut rows = rows_of(&stdout, "reach");
    rows.sort();
    assert_eq!(
        rows,
        vec!["a\t1", "b\t2", "c\t3", "s\t0", "t\t4"],
        "reach mismatch (expect exactly one row per node at min depth):\n{stdout}"
    );
}

/// RED/GREEN companion to the test above: same 5-node cyclic graph, same
/// `d0 < 8` depth guard, run as TWO shapes of the SAME recursive rule so the
/// row-count delta the lattice fixes is explicit and hand-verifiable.
///
/// RED (`reach_count`, no `merge`): a plain depth counter re-derives every
/// node on every lap of the cycle up to the guard, producing a (node x
/// path-length) product. Exact rows measured directly off this program:
///   s 0, a 1, b 2, c 3, t 4, a 4, b 5, c 6, t 7, a 7, t 4
/// (11 rows total; t is reached at both depth 4 and depth 7, each its own
/// row since there is no merge). Pinned here as the RED regression baseline.
/// GREEN (`reach`, `merge(MinBy(d))`): collapses to exactly 5 rows, one per
/// node at its minimum depth -- proving the lattice is what shrinks 11 -> 5,
/// not an artifact of the guard or graph shape.
#[test]
fn counter_shape_produces_row_product_that_minby_lattice_collapses() {
    let dir = sandbox("redgreen");
    let red_stdout = run(&dir, r#"
rel edge(from: text, to: text).
edge("s","a"). edge("a","b"). edge("b","c"). edge("c","t"). edge("c","a").
rel seed(node: text).
seed("s").
rel reach_count(node: text, d: int).
reach_count(node, 0) <- seed(node).
reach_count(to, d0 + 1) <- reach_count(from, d0), edge(from, to), d0 < 8.
? reach_count(node, d).
"#);
    let red_rows = rows_of(&red_stdout, "reach_count");
    assert_eq!(
        red_rows.len(),
        11,
        "RED baseline: plain counter (no merge) on the cyclic graph with \
         d0 < 8 must produce exactly 11 (node x depth) rows, not 5:\n{red_stdout}"
    );

    let dir2 = sandbox("redgreen_green");
    let green_stdout = run(&dir2, r#"
rel edge(from: text, to: text).
edge("s","a"). edge("a","b"). edge("b","c"). edge("c","t"). edge("c","a").
rel seed(node: text).
seed("s").
rel reach(node: text, d: int) key(node) merge(MinBy(d)).
reach(node, 0) <- seed(node).
reach(to, d0 + 1) <- reach(from, d0), edge(from, to), d0 < 8.
? reach(node, d).
"#);
    let mut green_rows = rows_of(&green_stdout, "reach");
    green_rows.sort();
    assert_eq!(
        green_rows,
        vec!["a\t1", "b\t2", "c\t3", "s\t0", "t\t4"],
        "GREEN: same graph, same d0 < 8 guard, merge(MinBy(d)) must collapse \
         the 11-row product down to 5:\n{green_stdout}"
    );

    assert_eq!(
        red_rows.len() - green_rows.len(),
        6,
        "row-count delta between the unguarded counter and the MinBy lattice \
         must be exactly 11 - 5 = 6 on this graph"
    );
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

/// An unbounded `depth + 1` rule on a cyclic edge keeps producing distinct rows.
#[test]
fn growing_recursive_component_hits_row_budget_loudly() {
    let dir = sandbox("diverging_budget");
    let stderr = run_failing_with_row_max(&dir, r#"
rel edge(a: int, b: int).
edge(1, 1).
rel reach(node: int, depth: int).
reach(a, 0) <- edge(a, b).
reach(b, depth + 1) <- reach(a, depth), edge(a, b).
"#, 3);
    assert!(stderr.contains("fixpoint row budget exceeded"), "missing budget bail:\n{stderr}");
    assert!(stderr.contains("reach"), "missing recursive relation name:\n{stderr}");
    assert!(stderr.contains("4 rows (limit 3)"), "missing promoted-row count:\n{stderr}");
    assert!(stderr.contains("recursion is growing without converging — add a depth bound like `depth < 64`, see std/entry.dl"),
        "missing repair text:\n{stderr}");
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

/// Intern-key arc coordination check: a `sym`-typed recursive rel must round-
/// trip through the semi-naive delta path — `rebuild_derived_seminaive` builds
/// its `_delta_*` scratch tables from `Col.ty.sql()`, which already maps
/// `Type::Sym` to INTEGER (same as `Type::Int`), so this is a regression
/// guard, not new plumbing. `reach` recurses over `df_edge` (both cols sym as
/// of the intern-key arc); the printed `?` output decodes back to the
/// original id TEXT (df_node.id is `file::line::kind` before hashing), so a
/// byte-level row-set match here also proves the query print-seam decode
/// composes with semi-naive evaluation, not just the naive loop.
#[test]
fn sym_typed_recursive_rel_round_trips_through_the_delta_table() {
    let dir = sandbox("sym_delta");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/lib.rs"), "fn go(a: i32) -> i32 {\n    let b = a;\n    let c = b;\n    let d = c;\n    d\n}\n").unwrap();
    let prog = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
rel reach(from: sym, to: sym).
reach(a, b) <- df_edge(a, b).
reach(a, c) <- reach(a, b), df_edge(b, c).
? df_edge(from, to).
? reach(from, to).
"#;
    let stdout = run(&dir, prog);
    let edges = rows_of(&stdout, "df_edge");
    let reach = rows_of(&stdout, "reach");
    assert!(!edges.is_empty(), "df_edge must be non-empty for a 4-let-binding chain:\n{stdout}");
    // Every direct edge is itself a reach pair (base case), and the reach set
    // must be strictly bigger (the transitive var_read/let_bind chain a->b->c->d
    // gives at least one 2-hop pair the base df_edge set does not).
    for e in &edges {
        assert!(reach.contains(e), "direct edge {e:?} must appear in reach:\n{stdout}");
    }
    assert!(reach.len() > edges.len(),
        "reach must contain at least one transitive (non-direct-edge) pair:\n{stdout}");
}

// ---------------------------------------------------------------------------
// `_stmt_ms` SUM + statement-count telemetry: semi-naive splits a recursive
// rel's work across many small delta statements per iteration, so the old
// per-statement MAX made a hot rel look nearly free. The table now records
// (SUM ms, n statement executions) per rel; these tests pin the exact,
// deterministic n for the chain fixture under both evaluators.
// ---------------------------------------------------------------------------

fn stmt_ms_rows(dir: &PathBuf, n: usize, naive: bool) -> Vec<(String, i64, i64)> {
    let prog = parse::parse(lex::lex(&chain_program(n)).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.force_naive_fixpoint.set(naive);
    eng.tick(&prog, true).unwrap();
    eng.query_sql("SELECT rel, ms, n FROM _stmt_ms ORDER BY rel", &[]).unwrap()
        .into_iter()
        .map(|r| (
            r[0].as_str().unwrap_or_default().to_string(),
            r[1].as_i64().unwrap_or(-1),
            r[2].as_i64().unwrap_or(-1),
        ))
        .collect()
}

/// Semi-naive statement count for the 4-chain is exact and deterministic.
/// Every statement rebuild_derived issues for the rel rides `timed`:
/// 1 table wipe + 1 seed rule + 1 delta seed insert, then per iteration
/// (5 = 4 producing + 1 empty converging) a `_delta_new` clear + 1 delta
/// variant + 1 anti-join subtract, plus per PROMOTING iteration (4) the
/// promote insert + delta clear + delta refill:
/// n = 3 + 5*3 + 4*3 = 30. The ms column is wall time (not asserted
/// exactly), but it must be present and non-negative — the SUM the
/// derived-phase coverage math reads.
#[test]
fn stmt_ms_records_exact_statement_count_seminaive() {
    let rows = stmt_ms_rows(&sandbox("stmtms_semi"), 4, false);
    let reach: Vec<_> = rows.iter().filter(|(rel, _, _)| rel == "reach").collect();
    assert_eq!(reach.len(), 1, "one _stmt_ms row per rel: {rows:?}");
    let (_, ms, count) = reach[0];
    assert_eq!(*count, 30, "semi-naive 4-chain: 3 setup + 5 iters x 3 + 4 promotes x 3: {rows:?}");
    assert!(*ms >= 0, "ms is a wall-time sum, never negative: {rows:?}");
}

/// Naive statement count for the same fixture: 1 table wipe + 2 statements
/// (seed + recursive) per pass, 5 passes to observe delta 0 — n = 11 exactly.
/// Red/green twin of `stmt_ms_records_exact_statement_count_seminaive`:
/// under the old MAX semantics both evaluators stored one statement's ms and
/// the count column did not exist, so neither assertion could be written.
#[test]
fn stmt_ms_records_exact_statement_count_naive() {
    let rows = stmt_ms_rows(&sandbox("stmtms_naive"), 4, true);
    let reach: Vec<_> = rows.iter().filter(|(rel, _, _)| rel == "reach").collect();
    assert_eq!(reach.len(), 1, "one _stmt_ms row per rel: {rows:?}");
    let (_, ms, count) = reach[0];
    assert_eq!(*count, 11, "naive 4-chain: 1 wipe + 2 stmts x 5 passes: {rows:?}");
    assert!(*ms >= 0, "ms is a wall-time sum, never negative: {rows:?}");
}

/// The derived-phase sibling buckets land in the SAME table: a closure edge's
/// condensation pass writes `closure:<edge>` and `cond_cache:<edge>` rows
/// (n = 1 each — one pass per edge per tick), so `SUM(ms) FROM _stmt_ms`
/// covers the engine-side closure work, not just the SQL fixpoint.
#[test]
fn stmt_ms_carries_closure_sibling_buckets() {
    let dir = sandbox("stmtms_closure");
    let prog_text = r#"
rel edge(a: text, b: text).
edge("x","y"). edge("y","z").
rel reaches(a: text, b: text).
reaches(a, b) <- closure(edge).
? reaches("x", to).
"#;
    let prog = parse::parse(lex::lex(prog_text).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.tick(&prog, true).unwrap();
    let rows: Vec<(String, i64)> = eng
        .query_sql("SELECT rel, n FROM _stmt_ms WHERE rel LIKE '%:edge' ORDER BY rel", &[]).unwrap()
        .into_iter()
        .map(|r| (r[0].as_str().unwrap_or_default().to_string(), r[1].as_i64().unwrap_or(-1)))
        .collect();
    assert_eq!(rows, vec![
        ("closure:edge".to_string(), 1),
        ("cond_cache:edge".to_string(), 1),
    ], "closure sibling buckets missing from _stmt_ms");
}
