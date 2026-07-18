//! Aggregation heads in `?` QUERY items (2026-07-10): a query head carrying an
//! aggregate call (count/sum/min/max/json_group_array, plus the two-column
//! json_group_object) switches from SELECT DISTINCT to GROUP BY — the rule-head
//! aggregate shape, positional. Var terms are the group set (and the deterministic
//! ORDER BY), literals stay WHERE filters, wildcards collapse. The rule-head twin
//! is tests/it/json_agg.rs. Same sandbox harness.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("query_agg_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The rows printed under `? rel =>`, each row a Vec of tab-separated cells.
fn query_rows(out: &str, rel: &str) -> Vec<Vec<String>> {
    let block = out.split(&format!("? {rel} =>")).nth(1).unwrap_or("");
    let mut rows = Vec::new();
    for line in block.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l.starts_with('(') { continue; }
        if l.starts_with('?') { break; }
        rows.push(l.split('\t').map(|c| c.to_string()).collect());
    }
    rows
}

const SALES: &str = r#"
rel sale(dept: text, item: text, price: int).
sale("food", "apple", 3).
sale("food", "pear", 4).
sale("toy", "ball", 9).
"#;

/// `? rel(group_col, json_group_array(member), _)` returns valid JSON arrays,
/// grouped by the surviving var, deterministic across two runs.
#[test]
fn query_json_group_array_grouped_and_valid_json() {
    let dir = sandbox("jga");
    let prog = format!("{SALES}? sale(dept, json_group_array(items), _).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let rows = query_rows(&out, "sale");
    assert_eq!(rows.len(), 2, "one row per dept: {out}");
    for row in &rows {
        let arr: serde_json::Value = serde_json::from_str(&row[1])
            .unwrap_or_else(|e| panic!("payload `{}` is not json: {e}", row[1]));
        assert!(arr.is_array(), "json_group_array yields an array: {}", row[1]);
        if row[0] == "food" {
            let mut names: Vec<String> = arr.as_array().unwrap().iter()
                .map(|x| x.as_str().unwrap().to_string()).collect();
            names.sort();
            assert_eq!(names, vec!["apple".to_string(), "pear".to_string()]);
        }
    }
    // Determinism: a second run over the same db is byte-identical.
    let (_, out2, _) = run(&dir, &prog);
    assert_eq!(query_rows(&out, "sale"), query_rows(&out2, "sale"), "deterministic aggregate output");
}

/// count / sum in a query head, grouped, exact expected rows.
#[test]
fn query_count_and_sum_grouped() {
    let dir = sandbox("countsum");
    let prog = format!("{SALES}? sale(dept, count(n), _).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let mut counts: Vec<Vec<String>> = query_rows(&out, "sale");
    counts.sort();
    assert_eq!(counts, vec![
        vec!["food".to_string(), "2".to_string()],
        vec!["toy".to_string(), "1".to_string()],
    ], "count per dept: {out}");

    let prog = format!("{SALES}? sale(dept, _, sum(revenue)).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let mut sums = query_rows(&out, "sale");
    sums.sort();
    assert_eq!(sums, vec![
        vec!["food".to_string(), "7".to_string()],
        vec!["toy".to_string(), "9".to_string()],
    ], "sum of price per dept: {out}");
}

/// Whole-rel aggregate: all wildcards + one aggregate returns exactly one row.
#[test]
fn query_whole_rel_aggregate_one_row() {
    let dir = sandbox("wholerel");
    let prog = format!("{SALES}? sale(_, _, sum(revenue)).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let rows = query_rows(&out, "sale");
    assert_eq!(rows, vec![vec!["16".to_string()]], "one row, total price: {out}");
}

/// A literal filter and an aggregate together = WHERE + GROUP BY.
#[test]
fn query_literal_filter_plus_aggregate() {
    let dir = sandbox("filter");
    let prog = format!("{SALES}? sale(\"food\", _, sum(revenue)).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let rows = query_rows(&out, "sale");
    // The dept var is pinned to "food" (a literal) so it drops out of SELECT and
    // GROUP BY; the whole filtered rel aggregates to one row.
    assert_eq!(rows, vec![vec!["7".to_string()]], "food-only sum: {out}");
}

/// json_group_object two-column shape end to end: key = col i, value = col i+1.
#[test]
fn query_json_group_object_two_column() {
    let dir = sandbox("jgo");
    let prog = format!("{SALES}? sale(dept, json_group_object(items, prices), _).\n");
    let (code, out, err) = run(&dir, &prog);
    assert_eq!(code, 0, "run failed:\nstdout={out}\nstderr={err}");
    let rows = query_rows(&out, "sale");
    assert_eq!(rows.len(), 2, "one object per dept: {out}");
    for row in &rows {
        let obj: serde_json::Value = serde_json::from_str(&row[1])
            .unwrap_or_else(|e| panic!("payload `{}` is not json: {e}", row[1]));
        assert!(obj.is_object(), "json_group_object yields an object: {}", row[1]);
        if row[0] == "food" {
            assert_eq!(obj["apple"], serde_json::json!(3));
            assert_eq!(obj["pear"], serde_json::json!(4));
        }
    }
}

/// The malformed two-arg shape (no trailing `_` at the value column) is the
/// shaped two-column error.
#[test]
fn query_json_group_object_missing_wildcard_errs() {
    let dir = sandbox("jgo_bad");
    // sale has 3 cols; json_group_object counts as one term, so this is a 2-term
    // query against a 3-col rel — but the shaped error fires at lowering first
    // only when arity matches. Use a 2-col rel so arity is exact and the missing
    // trailing `_` is the sole defect.
    let prog = r#"
rel pair(name: text, value: int).
pair("a", 1).
? pair(name, json_group_object(k, v)).
"#;
    let (_code, out, err) = run(&dir, prog);
    // The engine logs the query failure to stderr (`[dl] query ... failed`) and
    // exits 0 for a bad `?`; assert on the shaped message, not the exit code.
    let combined = format!("{out}{err}");
    assert!(combined.contains("query `pair` failed"), "query reported as failed: {combined}");
    assert!(combined.contains("consumes two columns") && combined.contains("put _ at the value column"),
        "shaped two-column error: {combined}");
}

/// A non-aggregate function call in a query head keeps the existing bail.
#[test]
fn query_non_agg_call_still_bails() {
    let dir = sandbox("nonagg");
    let prog = r#"
rel raw(name: text).
raw("hi").
? raw(replace(name, "h", "H")).
"#;
    let (_code, out, err) = run(&dir, prog);
    let combined = format!("{out}{err}");
    assert!(combined.contains("query `raw` failed"), "query reported as failed: {combined}");
    assert!(combined.contains("function call not supported in a query head"), "existing message: {combined}");
}

// ---- daemon path ------------------------------------------------------------

/// Spawn a foreground singleton whose home is `<dir>/home` (hermetic:
/// XDG_STATE_HOME sandbox + no ambient ~/.config/sprefa repos), serving `dir`.
fn run_daemon(dir: &PathBuf) -> DaemonGuard {
    let home = dir.join("home");
    fs::create_dir_all(&home).unwrap();
    DaemonGuard(Command::new(DL)
        .args(["daemon", "start", "--foreground"])
        .arg(dir.join("p.dl"))
        .current_dir(dir)
        .env("DL_DAEMON_ROOT", dir)
        .env("XDG_STATE_HOME", &home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn dl daemon"))
}


/// An aggregated `?` answered over the daemon `query` RPC: the daemon runs its
/// loaded program's queries through `run_queries_capture` -> `lower_query`, the
/// same aggregate lowering as the one-shot path.
#[test]
fn query_aggregate_over_daemon() {
    let dir = sandbox("daemon");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join("p.dl"), format!("{SALES}? sale(dept, count(n), _).\n")).unwrap();

    let mut child = run_daemon(&dir);
    let sock = sprefa_v5::daemon::socket_path_for(&dir.join("home").join("sprefa"));
    let mut ready = false;
    for _ in 0..200 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { ready = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "daemon socket not ready");

    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 2, "method": "query",
        "params": {"root": dir.to_string_lossy()},
    }).to_string();
    let resp = crate::util::uds_rpc(&sock, &body).expect("query response");
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let results = v["result"]["results"].as_array().expect("results array");
    let sale = results.iter().find(|r| r["rel"] == "sale").expect("sale query result");
    let cols: Vec<String> = sale["columns"].as_array().unwrap().iter()
        .map(|c| c.as_str().unwrap().to_string()).collect();
    assert_eq!(cols, vec!["dept".to_string(), "n".to_string()], "aggregate header: {resp}");
    let mut got: Vec<(String, i64)> = sale["rows"].as_array().unwrap().iter()
        .map(|row| (row[0].as_str().unwrap().to_string(), row[1].as_i64().unwrap()))
        .collect();
    got.sort();
    assert_eq!(got, vec![("food".to_string(), 2), ("toy".to_string(), 1)], "grouped counts over daemon: {resp}");

    // Clean shutdown.
    let _ = crate::util::uds_rpc(&sock,
        r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#);
    let _ = child.wait();
}
