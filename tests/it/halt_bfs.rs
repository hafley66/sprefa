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
    run_with_trace(dir, prog, force_sql, false).0
}

fn run_with_trace(dir: &PathBuf, prog: &str, force_sql: bool, trace: bool) -> (String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl")).current_dir(dir)
        .arg("--db").arg(dir.join(if force_sql { "sql.db" } else { "nat.db" }).to_str().unwrap())
        .env("DL_NO_DAEMON", "1")
        .env("SPREFA_CONFIG", "/nonexistent/x.toml");
    if trace { cmd.env("DL_BFS_TRACE", "1"); }
    if force_sql { cmd.env("DL_NO_HALT_BFS", "1"); }
    let out = cmd.output().expect("run dl");
    assert!(out.status.success(), "dl failed; stderr={}", String::from_utf8_lossy(&out.stderr));
    (String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

fn run_discovered(dir: &PathBuf, force_sql: bool) -> (String, String) {
    let db_path = dir.join(if force_sql { "sql.db" } else { "nat.db" });
    let mut command = Command::new(DL);
    command.current_dir(dir)
        .arg("--db").arg(db_path.to_str().unwrap())
        .env("DL_NO_DAEMON", "1")
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_BFS_TRACE", "1");
    if force_sql { command.env("DL_NO_HALT_BFS", "1"); }
    let output = command.output().expect("run discovered dl");
    assert!(output.status.success(), "discovered dl failed; stderr={}", String::from_utf8_lossy(&output.stderr));
    (String::from_utf8_lossy(&output.stdout).into_owned(), String::from_utf8_lossy(&output.stderr).into_owned())
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

fn raw_depth_rows(db: &PathBuf, table: &str, columns: &str) -> Vec<String> {
    let connection = rusqlite::Connection::open(db).unwrap();
    let mut statement = connection.prepare(&format!("SELECT {columns} FROM rel_{table}")).unwrap();
    let mut rows = statement.query([]).unwrap();
    let mut output = Vec::new();
    while let Some(row) = rows.next().unwrap() {
        let mut cells = Vec::new();
        for column_index in 0..row.as_ref().column_count() {
            cells.push(match row.get_ref(column_index).unwrap() {
                rusqlite::types::ValueRef::Integer(value) => value.to_string(),
                rusqlite::types::ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned(),
                rusqlite::types::ValueRef::Null => String::new(),
                other => format!("{other:?}"),
            });
        }
        output.push(cells.join("\t"));
    }
    output.sort();
    output
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

const DEPTH_PROG: &str = r#"
rel flow_edge(from: text, to: text).
flow_edge("a", "b"). flow_edge("b", "c"). flow_edge("a", "c"). flow_edge("c", "a").
rel entry_seed(node: text).
entry_seed("a").
rel entry_reach_node(node: text, d: int) key(node) merge(MinBy(d)).
entry_reach_node(node, 0) <- entry_seed(node).
entry_reach_node(to, d0 + 1) <- entry_reach_node(from, d0), flow_edge(from, to), d0 < 2.
rel op_reach_seed(op: text, node: text).
op_reach_seed("load", "a").
rel op_reach_node(op: text, n: text, d: int) key(op, n) merge(MinBy(d)).
op_reach_node(op, n, 0) <- op_reach_seed(op, n).
op_reach_node(op, to, d0 + 1) <- op_reach_node(op, from, d0), flow_edge(from, to), d0 < 2.
? entry_reach_node(node, d).
? op_reach_node(op, n, d).
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

#[test]
fn native_depth_walk_rows_match_sql_including_depth() {
    let native_dir = sandbox("depth_native");
    let sql_dir = sandbox("depth_sql");
    let (native_entry_stdout, native_trace) = run_with_trace(&native_dir, DEPTH_PROG, false, true);
    let sql_entry_stdout = run(&sql_dir, DEPTH_PROG, true);
    let native_entry = rows_of(&native_entry_stdout, "entry_reach_node");
    let sql_entry = rows_of(&sql_entry_stdout, "entry_reach_node");
    assert_eq!(native_entry, sql_entry, "entry depth rows diverged\nnative={native_entry:?}\nsql={sql_entry:?}\nstdout={native_entry_stdout}");

    let native_op = rows_of(&run(&native_dir, DEPTH_PROG, false), "op_reach_node");
    let sql_op = rows_of(&run(&sql_dir, DEPTH_PROG, true), "op_reach_node");
    assert_eq!(native_op, sql_op, "op depth rows diverged\nnative={native_op:?}\nsql={sql_op:?}");
    let native_entry_raw = raw_depth_rows(&native_dir.join("nat.db"), "entry_reach_node", "node, d");
    let sql_entry_raw = raw_depth_rows(&sql_dir.join("sql.db"), "entry_reach_node", "node, d");
    let native_op_raw = raw_depth_rows(&native_dir.join("nat.db"), "op_reach_node", "op, n, d");
    let sql_op_raw = raw_depth_rows(&sql_dir.join("sql.db"), "op_reach_node", "op, n, d");
    assert_eq!(native_entry_raw, sql_entry_raw, "entry raw rows diverged, including depth");
    assert_eq!(native_op_raw, sql_op_raw, "op raw rows diverged, including depth");
    assert!(native_entry_raw.iter().any(|row| row.ends_with("\t0")), "entry rows must include depth 0: {native_entry_raw:?}");
    assert!(native_op_raw.iter().any(|row| row.ends_with("\t0")), "op rows must include depth 0: {native_op_raw:?}");
    assert_eq!(native_trace.matches("[bfs-cache] load edge=flow_edge").count(), 1, "flow_edge should load once per tick:\n{native_trace}");
    assert!(native_trace.contains("[bfs-cache] reuse edge=flow_edge"), "second reach walk should reuse flow_edge:\n{native_trace}");
}

#[test]
fn discovered_duplicate_origins_still_fire_native_port_walk() {
    let directory = sandbox("duplicate_origins");
    fs::create_dir_all(directory.join(".dl")).unwrap();
    fs::write(directory.join(".dl/10-root-a.dl"), r#"
rel edge(from: text, to: text).
edge("s", "a"). edge("a", "b").
rel halt(node: text, kind: text).
halt("a", "pin").
rel seed(port: text, start: text).
seed("p", "s").
rel reach(port: text, node: text).
reach(port, node) <- seed(port, start), edge(start, node).
? reach(port, node).
"#).unwrap();
    fs::write(directory.join(".dl/20-root-b.dl"), r#"
reach(port, node) <- seed(port, start), edge(start, node).
reach(port, node) <- reach(port, mid), !halt(mid, _), edge(mid, node).
"#).unwrap();
    let (native_stdout, native_stderr) = run_discovered(&directory, false);
    let (sql_stdout, _) = run_discovered(&directory, true);
    assert_eq!(rows_of(&native_stdout, "reach"), rows_of(&sql_stdout, "reach"));
    assert!(native_stderr.contains("[bfs] reach:"), "native recognizer did not fire:\n{native_stderr}");
}
