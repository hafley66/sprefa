//! Native halt-gated BFS fast path (`try_native_halt_bfs`) vs the SQL semi-naive
//! fixpoint. The engine recognizes `port_reach`'s contraction shape —
//!   head(tag, node) <- seed(tag, start), edge(start, node).
//!   head(tag, node) <- head(tag, mid), !halt(mid, _), edge(mid, node).
//! — and runs it as an in-memory BFS. These tests prove (1) the native path
//! produces the EXACT hand-computed contraction, and (2) it is row-for-row
//! identical to the SQL fixpoint (forced via DL_NO_HALT_BFS), including the
//! cycle and halt-node cases.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_haltbfs_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// Run `prog`, optionally forcing the SQL fixpoint (bypass the native path).
fn run(dir: &PathBuf, prog: &str, force_sql: bool) -> String {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl")).current_dir(dir)
        .arg("--db").arg(dir.join(if force_sql { "sql.db" } else { "nat.db" }).to_str().unwrap())
        .env("DL_NO_DAEMON", "1")
        .env("SPREFA_CONFIG", "/nonexistent/x.toml");
    if force_sql { cmd.env("DL_NO_HALT_BFS", "1"); }
    let out = cmd.output().expect("run dl");
    assert!(out.status.success(), "dl failed; stderr={}", String::from_utf8_lossy(&out.stderr));
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

// A contraction walk with a halt node and a cycle, all in plain `text` node
// space (no `sym` decode needed). Graph:
//   s -> a -> b -> t ,  b -> a (cycle) ,  a is a HALT node (a port pin).
// Seed: port p starts at s. Walk records everything reachable, but never
// expands OUT of the halt node `a`.
//   frontier from s: a (recorded; a is halt -> stop)
//   so p reaches only {a}. b, t are behind the halt.
// A second port q seeded at b (not halt): b -> t, b -> a(halt, recorded).
//   q reaches {t, a}. From a: halt, stop. From t: no out-edge.
const PROG: &str = r#"
rel edge(from: text, to: text).
edge("s","a"). edge("a","b"). edge("b","t"). edge("b","a").
rel halt(node: text, kind: text).
halt("a", "pin").
rel seed(port: text, start: text).
seed("p","s"). seed("q","b").
rel reach(port: text, node: text).
reach(port, node) <- seed(port, start), edge(start, node).
reach(port, node) <- reach(port, mid), !halt(mid, _), edge(mid, node).
? reach(port, node).
"#;

#[test]
fn native_halt_bfs_matches_hand_computed_contraction() {
    let dir = sandbox("hand");
    let got = rows_of(&run(&dir, PROG, false), "reach");
    // p: s->a (halt, stop) => {a}. q: b->t, b->a (halt, stop) => {a, t}.
    assert_eq!(got, vec!["p\ta", "q\ta", "q\tt"], "native contraction mismatch");
}

#[test]
fn native_halt_bfs_row_identical_to_sql_fixpoint() {
    let dir_n = sandbox("nat");
    let dir_s = sandbox("sql");
    let native = rows_of(&run(&dir_n, PROG, false), "reach");
    let sql = rows_of(&run(&dir_s, PROG, true), "reach");
    assert_eq!(native, sql, "native BFS diverged from SQL fixpoint\nnative={native:?}\nsql={sql:?}");
}
