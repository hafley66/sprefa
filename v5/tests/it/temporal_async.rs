//! `@async` effect rules: the body binds request args over the converged tick,
//! one `pending_effect` row lands per solution, the daemon runs an executor
//! OFF-tick (`drain_effects`), and the response surfaces in the head relation for
//! a later tick to read. The first slice with real IO — the executors here are
//! in-process mocks, but the shape is the `gh`/`git`/http shell call. See
//! docs/research-reactive-effectful-datalog.md §8.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Result;
use serde_json::{Map, Value as Json};

use sprefa_v5::db;
use sprefa_v5::engine::{async_effect_arity, EffectExec, Engine, ShellEffectExec};
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_async_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn rows(db_path: &Path, sql: &str) -> Vec<Vec<String>> {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let mut s = conn.prepare(sql).unwrap();
    let ncol = s.column_count();
    let out = s
        .query_map([], |r| {
            let mut row = Vec::new();
            for i in 0..ncol {
                let v: rusqlite::types::Value = r.get(i)?;
                row.push(match v {
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    rusqlite::types::Value::Text(t) => t,
                    rusqlite::types::Value::Null => String::new(),
                    other => format!("{other:?}"),
                });
            }
            Ok(row)
        })
        .unwrap()
        .filter_map(|x| x.ok())
        .collect();
    out
}

/// A fixed map from a request arg (`url`) to canned response outputs. Records
/// every `(kind,url)` it was asked for, so a test can assert the executor ran
/// exactly once per distinct request (idempotence).
struct MockExec {
    table: HashMap<String, Vec<String>>,
    calls: Mutex<Vec<String>>,
}

impl EffectExec for MockExec {
    fn run(&self, kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        self.calls.lock().unwrap().push(format!("{kind}:{url}"));
        self.table
            .get(&url)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("mock has no canned response for {url}"))
    }
}

/// End to end: a `@async` rule emits a request, an off-tick drain runs the
/// executor and lands the response, and the next tick reads it. The unbound head
/// terms (`status`, `body`) are filled by the executor; the bound term (`key`)
/// echoes from the request.
#[test]
fn async_request_drains_and_lands_response() {
    let d = sandbox("e2e");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(key: str, url: str).\n\
         want(\"home\", \"https://api/home\").\n\
         want(\"about\", \"https://api/about\").\n\
         rel resp(key: str, status: int, body: str).\n\
         resp(key, status, body) <- @async want(key, url).\n\
         rel ok(key: str).\n\
         ok(key) <- resp(key, 200, _).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();

    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Tick 1: want is a fact; @async emits two pending_effect rows; resp empty.
    eng.tick(&prog, true).unwrap();
    let pend = rows(&dbp, "SELECT kind, args_json, done FROM pending_effect ORDER BY args_json");
    assert_eq!(pend.len(), 2, "one pending_effect per want row");
    assert!(pend.iter().all(|r| r[0] == "resp" && r[2] == "0"), "queued, not done: {pend:?}");
    assert_eq!(rows(&dbp, "SELECT * FROM rel_resp").len(), 0, "no response before drain");

    // Off-tick: the daemon drains. status comes back as a string; the int column
    // takes it by affinity.
    let exec = MockExec {
        table: HashMap::from([
            ("https://api/home".to_string(), vec!["200".to_string(), "HOME".to_string()]),
            ("https://api/about".to_string(), vec!["404".to_string(), "NOPE".to_string()]),
        ]),
        calls: Mutex::new(Vec::new()),
    };
    let n = eng.drain_effects(&prog, &exec).unwrap();
    assert_eq!(n, 2, "both requests drained");
    assert!(pend.iter().all(|_| true));

    // Tick 2: resp now populated; ok derives from the 200 row only.
    eng.tick(&prog, true).unwrap();
    let mut got = rows(&dbp, "SELECT key, status, body FROM rel_resp ORDER BY key");
    got.sort();
    assert_eq!(
        got,
        vec![
            vec!["about".to_string(), "404".to_string(), "NOPE".to_string()],
            vec!["home".to_string(), "200".to_string(), "HOME".to_string()],
        ]
    );
    assert_eq!(rows(&dbp, "SELECT key FROM rel_ok"), vec![vec!["home".to_string()]]);

    // All pending rows are now done; a second drain is a no-op (no double-fire).
    let n2 = eng.drain_effects(&prog, &exec).unwrap();
    assert_eq!(n2, 0, "drained requests do not re-run");
    assert_eq!(exec.calls.lock().unwrap().len(), 2, "executor ran once per request");
}

/// Re-emitting the same request on a later tick before it drains does not queue a
/// duplicate: the digest id collides and INSERT OR IGNORE holds.
#[test]
fn async_request_is_idempotent_across_ticks() {
    let d = sandbox("idem");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(url: str).\n\
         want(\"u1\").\n\
         rel resp(url: str, body: str).\n\
         resp(url, body) <- @async want(url).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        rows(&dbp, "SELECT COUNT(*) FROM pending_effect"),
        vec![vec!["1".to_string()]],
        "the same request emitted on three ticks queues exactly once"
    );
}

/// The real-IO path: `ShellEffectExec` runs an actual subprocess per request,
/// fills `{var}` from the args, and splits stdout into the two output slots
/// (`status`, `body`). The status column takes the string by int affinity.
#[test]
fn shell_effect_exec_runs_real_subprocess() {
    let d = sandbox("shell");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(key: str, url: str).\n\
         want(\"home\", \"api/home\").\n\
         rel resp(key: str, status: int, body: str).\n\
         resp(key, status, body) <- @async want(key, url).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    let arity = async_effect_arity(&prog);
    assert_eq!(arity.get("resp"), Some(&2), "status + body are the two unbound slots");
    let exec = ShellEffectExec {
        templates: HashMap::from([(
            "resp".to_string(),
            "printf '200\\n%s-body' '{url}'".to_string(),
        )]),
        n_out: arity,
    };
    let n = eng.drain_effects(&prog, &exec).unwrap();
    assert_eq!(n, 1);

    eng.tick(&prog, true).unwrap();
    assert_eq!(
        rows(&dbp, "SELECT key, status, body FROM rel_resp"),
        vec![vec!["home".to_string(), "200".to_string(), "api/home-body".to_string()]]
    );
}

/// Records peak in-flight count: each `run` bumps a counter, sleeps so the
/// overlap window is wide, then decrements. If the drain ran serially the peak
/// would be 1; the parallel drain over rayon should overlap requests.
struct ConcExec {
    in_flight: AtomicUsize,
    peak: AtomicUsize,
}

impl EffectExec for ConcExec {
    fn run(&self, _kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(40));
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();
        Ok(vec![format!("{key}-out")])
    }
}

/// The drain runs executors across the rayon pool, not one-at-a-time: with 8
/// pending requests the peak in-flight count exceeds 1. (The slow part of a real
/// effect is the shell spawn / network round-trip; nothing serializes them.)
#[test]
fn drain_runs_executors_in_parallel() {
    let d = sandbox("parallel");
    let dbp = d.join("db");
    let mut prog_src = String::from(
        "rel want(key: str).\n\
         rel resp(key: str, out: str).\n\
         resp(key, out) <- @async want(key).\n",
    );
    for i in 0..8 {
        prog_src.push_str(&format!("want(\"k{i}\").\n"));
    }
    fs::write(d.join("p.dl"), prog_src).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    assert_eq!(rows(&dbp, "SELECT COUNT(*) FROM pending_effect"), vec![vec!["8".to_string()]]);

    let exec = ConcExec { in_flight: AtomicUsize::new(0), peak: AtomicUsize::new(0) };
    let n = eng.drain_effects(&prog, &exec).unwrap();
    assert_eq!(n, 8, "all eight drained");
    assert!(
        exec.peak.load(Ordering::SeqCst) >= 2,
        "expected overlapping requests, peak in-flight was {}",
        exec.peak.load(Ordering::SeqCst)
    );

    eng.tick(&prog, true).unwrap();
    assert_eq!(rows(&dbp, "SELECT COUNT(*) FROM rel_resp"), vec![vec!["8".to_string()]]);
    assert_eq!(rows(&dbp, "SELECT done FROM pending_effect WHERE done = 0").len(), 0);
}

/// A response relation may not be headed by a source/derived rule too: it is
/// written only by the effect drain.
#[test]
fn async_response_rel_must_be_drain_only() {
    let d = sandbox("conflict");
    fs::write(
        d.join("p.dl"),
        "rel want(url: str).\n\
         want(\"u1\").\n\
         rel resp(url: str, body: str).\n\
         resp(url, body) <- @async want(url).\n\
         resp(url, \"x\") <- want(url).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let err = eng.tick(&prog, true).unwrap_err();
    assert!(
        err.to_string().contains("written only by the effect drain"),
        "expected a response-conflict bail: {err}"
    );
}
