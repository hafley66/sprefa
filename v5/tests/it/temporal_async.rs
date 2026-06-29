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
        cwd: std::path::PathBuf::new(),
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

/// A typed `sh` decl supplies the effect template (replacing an `effect_cmd`
/// row): `sh resp(url) -> (status, body) = `...`.` is keyed by the `@async`
/// head rel name, so the head-response drain runs the declared command. The
/// backtick body keeps `\n` literal (printf splits it into the two out slots).
#[test]
fn sh_decl_supplies_effect_template() {
    let d = sandbox("shdecl");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(key: str, url: str).\n\
         want(\"home\", \"api/home\").\n\
         sh resp(url) -> (status: int, body: str) = `printf '200\\n%s-body' '{url}'`.\n\
         rel resp(key: str, status: int, body: str).\n\
         resp(key, status, body) <- @async want(key, url).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    // The template comes from the `sh` registry, not an effect_cmd relation.
    let templates = sprefa_v5::engine::shell_templates(&prog);
    assert_eq!(templates.get("resp").map(String::as_str), Some("printf '200\\n%s-body' '{url}'"));
    let exec = ShellEffectExec { templates, n_out: async_effect_arity(&prog), cwd: PathBuf::new() };
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 1);

    eng.tick(&prog, true).unwrap();
    assert_eq!(
        rows(&dbp, "SELECT key, status, body FROM rel_resp"),
        vec![vec!["home".to_string(), "200".to_string(), "api/home-body".to_string()]]
    );
}

/// The explicit body-effect call site: `gh(repo, path) -> (status, body)` in the
/// rule body resolves to a `sh gh(r, p)` decl. Two things slice 2 adds are
/// exercised here: (1) the `sh` decl's params (`r`, `p`) name the template holes
/// POSITIONALLY, decoupled from the call-arg var names (`repo`, `path`); (2) a
/// body var (`tag`) that is NOT an effect arg is carried through `full_json` and
/// reconstructed into the head alongside the response outs. Same one drain model
/// the head-response form desugars to.
#[test]
fn body_effect_call_site_drains_with_full_env() {
    let d = sandbox("bodyeffect");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(repo: str, path: str, tag: str).\n\
         want(\"octo\", \"README\", \"v1\").\n\
         sh gh(r, p) -> (status: int, body: str) = `printf '200\\n%s/%s' '{r}' '{p}'`.\n\
         rel resp(repo: str, path: str, tag: str, status: int, body: str).\n\
         resp(repo, path, tag, status, body) <- @async\n\
             want(repo, path, tag), gh(repo, path) -> (status, body).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    // One pending request; kind is the `sh` name (`gh`), head_rel is the response
    // rel (`resp`), and the hole map is param-keyed (`r`/`p`, not `repo`/`path`).
    let pend = rows(&dbp, "SELECT kind, head_rel, args_json FROM pending_effect");
    assert_eq!(pend.len(), 1, "one request for the single want row");
    assert_eq!(pend[0][0], "gh", "kind = the sh decl name");
    assert_eq!(pend[0][1], "resp", "head_rel = the response rel");
    assert!(pend[0][2].contains("\"r\":\"octo\"") && pend[0][2].contains("\"p\":\"README\""),
        "hole map keyed by sh params r/p: {}", pend[0][2]);

    // arity is keyed by the sh name, not the head rel.
    assert_eq!(async_effect_arity(&prog).get("gh"), Some(&2));
    let exec = ShellEffectExec {
        templates: sprefa_v5::engine::shell_templates(&prog),
        n_out: async_effect_arity(&prog),
        cwd: PathBuf::new(),
    };
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 1);

    eng.tick(&prog, true).unwrap();
    // `tag` (a body var, not an effect arg) reconstructs from full_json; status +
    // body come from the executor; the template filled r=repo, p=path.
    assert_eq!(
        rows(&dbp, "SELECT repo, path, tag, status, body FROM rel_resp"),
        vec![vec!["octo".to_string(), "README".to_string(), "v1".to_string(),
                  "200".to_string(), "octo/README".to_string()]]
    );
}

/// A `sh*` stream executor: `run_stream` yields MANY rows per drain (one per
/// output line), unlike `run` (one response). Models a subscription returning a
/// batch of events.
struct StreamMock {
    out_rows: Vec<Vec<String>>,
}

impl EffectExec for StreamMock {
    fn run(&self, _kind: &str, _args: &Map<String, Json>) -> Result<Vec<String>> {
        Ok(self.out_rows.first().cloned().unwrap_or_default())
    }
    fn run_stream(&self, _kind: &str, _args: &Map<String, Json>) -> Result<Vec<Vec<String>>> {
        Ok(self.out_rows.clone())
    }
}

/// Phase 4: a `@stream` (`sh*`) subscription drains through `drain_streams`,
/// fanning each output line into its own head row, and the job STAYS 'running'
/// (a stream is long-lived). The bound body var (`repo`) echoes into every row;
/// the out vars (`kind`,`at`) come from the line.
#[test]
fn stream_subscription_fans_lines_and_stays_running() {
    let d = sandbox("stream");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel watch(repo: text).\n\
         watch(\"octo\").\n\
         sh* events(repo) -> (kind: text, at: text) = `printf '%s' '{repo}'`.\n\
         rel event(repo: text, kind: text, at: text).\n\
         event(repo, kind, at) <- @stream watch(repo), events(repo) -> (kind, at).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(dbp.to_str()).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    // The stream queued one pending request (one watch row).
    assert_eq!(rows(&dbp, "SELECT COUNT(*) FROM pending_effect"), vec![vec!["1".to_string()]]);

    let exec = StreamMock {
        out_rows: vec![
            vec!["push".to_string(), "2026-01".to_string()],
            vec!["pr".to_string(), "2026-02".to_string()],
        ],
    };
    let n = eng.drain_streams(&prog, &exec).unwrap();
    assert_eq!(n, 2, "two event lines fanned into two head rows");
    eng.tick(&prog, true).unwrap();
    let mut got = rows(&dbp, "SELECT repo, kind, at FROM rel_event ORDER BY kind");
    got.sort();
    assert_eq!(got, vec![
        vec!["octo".to_string(), "pr".to_string(), "2026-02".to_string()],
        vec!["octo".to_string(), "push".to_string(), "2026-01".to_string()],
    ]);
    // The subscription persists: the job is still 'running', never flipped 'done'.
    assert_eq!(rows(&dbp, "SELECT state FROM pending_effect"), vec![vec!["running".to_string()]]);
    // A second drain re-runs the live stream; identical lines OR IGNORE-dedup, so
    // the head rel does not grow, and the job stays running.
    assert_eq!(eng.drain_streams(&prog, &exec).unwrap(), 2);
    eng.tick(&prog, true).unwrap();
    assert_eq!(rows(&dbp, "SELECT COUNT(*) FROM rel_event"), vec![vec!["2".to_string()]]);
    assert_eq!(rows(&dbp, "SELECT state FROM pending_effect"), vec![vec!["running".to_string()]]);
}

/// Phase 4 real-IO: `ShellEffectExec::run_stream` splits a subprocess's stdout
/// into LINES x tab-separated slots (the `@tsv` convention, D-7). A two-line
/// printf yields two head rows.
#[test]
fn shell_stream_splits_tsv_lines() {
    let d = sandbox("stream_sh");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel watch(repo: text).\n\
         watch(\"octo\").\n\
         sh* events(repo) -> (kind: text, at: text) = `printf 'push\\t2026-01\\npr\\t2026-02\\n'`.\n\
         rel event(repo: text, kind: text, at: text).\n\
         event(repo, kind, at) <- @stream watch(repo), events(repo) -> (kind, at).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(dbp.to_str()).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    let exec = ShellEffectExec {
        templates: sprefa_v5::engine::shell_templates(&prog),
        n_out: async_effect_arity(&prog),
        cwd: PathBuf::new(),
    };
    assert_eq!(eng.drain_streams(&prog, &exec).unwrap(), 2);
    eng.tick(&prog, true).unwrap();
    let mut got = rows(&dbp, "SELECT repo, kind, at FROM rel_event ORDER BY kind");
    got.sort();
    assert_eq!(got, vec![
        vec!["octo".to_string(), "pr".to_string(), "2026-02".to_string()],
        vec!["octo".to_string(), "push".to_string(), "2026-01".to_string()],
    ]);
}

/// `check_effect`: an explicit body-effect call whose out-arity disagrees with
/// the `sh` decl is a `effect-arity` error, and a `{hole}` the decl never
/// references is `unused-hole`. Both surface as `TypeDiag`s from `prepare_paths`.
#[test]
fn effect_arity_and_hole_are_checked() {
    let d = sandbox("eff_arity");
    fs::write(
        d.join("p.dl"),
        "rel want(repo: str).\n\
         want(\"octo\").\n\
         sh gh(r, p) -> (status: int, body: str) = `printf '200' '{r}'`.\n\
         rel resp(repo: str, status: int).\n\
         resp(repo, status) <- @async want(repo), gh(repo) -> (status).\n",
    )
    .unwrap();
    let (_prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let codes: Vec<&str> = diags.iter().map(|x| x.code.as_str()).collect();
    // gh takes 2 params, called with 1; returns 2, bound to 1; param `p` has no {p}.
    assert!(codes.contains(&"effect-arity"), "want effect-arity, got {codes:?}");
    assert!(codes.contains(&"unused-hole"), "want unused-hole, got {codes:?}");
}

/// `check_effect`: an explicit call to no declared `sh` is `unknown-sh`.
#[test]
fn effect_unknown_sh_is_flagged() {
    let d = sandbox("eff_unknown");
    fs::write(
        d.join("p.dl"),
        "rel want(repo: str).\n\
         want(\"octo\").\n\
         rel resp(repo: str, body: str).\n\
         resp(repo, body) <- @async want(repo), nope(repo) -> (body).\n",
    )
    .unwrap();
    let (_prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    assert!(
        diags.iter().any(|x| x.code == "unknown-sh"),
        "want unknown-sh, got {:?}",
        diags.iter().map(|x| x.code.as_str()).collect::<Vec<_>>()
    );
}

/// `check_effect`: `@async` calling a `sh*` (Stream) decl crosses the temporal
/// axis and is `temporal-kind-mismatch`.
#[test]
fn effect_temporal_kind_must_agree() {
    let d = sandbox("eff_cross");
    fs::write(
        d.join("p.dl"),
        "rel want(repo: str).\n\
         want(\"octo\").\n\
         sh* feed(r) -> (line: str) = `printf '%s' '{r}'`.\n\
         rel resp(repo: str, line: str).\n\
         resp(repo, line) <- @async want(repo), feed(repo) -> (line).\n",
    )
    .unwrap();
    let (_prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    assert!(
        diags.iter().any(|x| x.code == "temporal-kind-mismatch"),
        "want temporal-kind-mismatch, got {:?}",
        diags.iter().map(|x| x.code.as_str()).collect::<Vec<_>>()
    );
}

/// Counts every `run` and returns a fixed single-column output. Used to assert
/// how many times the executor actually fired across drains.
struct CountExec {
    calls: AtomicUsize,
    out: Vec<String>,
}

impl EffectExec for CountExec {
    fn run(&self, _kind: &str, _args: &Map<String, Json>) -> Result<Vec<String>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.out.clone())
    }
}

/// Phase 3 exactly-once: a `sh!` (Mutate) effect that a crash left in `running`
/// (claimed but not committed) is NOT re-fired on the next drain — a mutating
/// effect must never double-POST. The drain claims `queued -> running` atomically
/// and only runs the row it claimed; a row already `running` is quarantined.
#[test]
fn mutating_effect_fires_exactly_once_across_a_crash() {
    let d = sandbox("mut_once");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(key: text).\n\
         want(\"k1\").\n\
         sh! post(k) -> (ok: text) = `printf 'done' '{k}'`.\n\
         rel resp(key: text, ok: text).\n\
         resp(key, ok) <- @async want(key), post(key) -> (ok).\n",
    )
    .unwrap();
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    // sh! with @async must NOT cross the temporal axis (Mutate <-> @async is fine).
    assert!(!diags.iter().any(|x| x.code == "temporal-kind-mismatch"),
        "sh! is callable from @async: {diags:?}");
    let conn = db::open(dbp.to_str()).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    let exec = CountExec { calls: AtomicUsize::new(0), out: vec!["done".into()] };
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 1, "first drain runs it");
    assert_eq!(exec.calls.load(Ordering::SeqCst), 1);
    assert_eq!(rows(&dbp, "SELECT state FROM pending_effect"), vec![vec!["done".to_string()]]);

    // Simulate a crash mid-flight: the claim landed (state='running') but the run
    // never committed (done=0). The reconcile must leave it alone.
    {
        let c = db::open(dbp.to_str()).unwrap();
        c.conn().execute("UPDATE pending_effect SET state = 'running', done = 0", []).unwrap();
    }
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 0, "a running sh! is not re-fired");
    assert_eq!(exec.calls.load(Ordering::SeqCst), 1, "exactly once across the crash");
}

/// Phase 3 contrast: a `sh` (Read) effect IS re-runnable. A crash-orphaned
/// `running` read row is fair game on the next drain (re-firing a cached read is
/// harmless), so the executor runs again.
#[test]
fn read_effect_reruns_after_a_crash() {
    let d = sandbox("read_rerun");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(key: text).\n\
         want(\"k1\").\n\
         sh get(k) -> (ok: text) = `printf 'v' '{k}'`.\n\
         rel resp(key: text, ok: text).\n\
         resp(key, ok) <- @async want(key), get(key) -> (ok).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(dbp.to_str()).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    let exec = CountExec { calls: AtomicUsize::new(0), out: vec!["v".into()] };
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 1);
    {
        let c = db::open(dbp.to_str()).unwrap();
        c.conn().execute("UPDATE pending_effect SET state = 'running', done = 0", []).unwrap();
    }
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 1, "a running sh read re-runs");
    assert_eq!(exec.calls.load(Ordering::SeqCst), 2, "read fired twice");
}

/// Phase 1b.2 `collect(x)`: an effect arg wrapped in `collect` gathers `x` across
/// ALL body solutions and fires ONE request (the provider batch-by-id); the
/// response fans back out, one head row per output line. Three `want` ids => one
/// pending request with `ids="a,b,c"` => three `star` rows.
#[test]
fn collect_batches_one_request_and_fans_response() {
    let d = sandbox("collect");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(repo: text, id: text).\n\
         want(\"octo\", \"a\").\n\
         want(\"octo\", \"b\").\n\
         want(\"octo\", \"c\").\n\
         sh nodes(ids) -> (key: text, val: text) = `printf '%s' '{ids}'`.\n\
         rel star(key: text, val: text).\n\
         star(key, val) <- @async want(repo, id), nodes(collect(id)) -> (key, val).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(dbp.to_str()).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    // ONE batch request for all three ids; the collected list is sorted+deduped.
    assert_eq!(rows(&dbp, "SELECT COUNT(*) FROM pending_effect"), vec![vec!["1".to_string()]]);
    assert_eq!(
        rows(&dbp, "SELECT args_json, batch FROM pending_effect"),
        vec![vec!["{\"ids\":\"a,b,c\"}".to_string(), "1".to_string()]]
    );

    // The batch response is three lines; each fans into a star row.
    let exec = StreamMock {
        out_rows: vec![
            vec!["a".into(), "A".into()],
            vec!["b".into(), "B".into()],
            vec!["c".into(), "C".into()],
        ],
    };
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 1, "one batch request drained");
    eng.tick(&prog, true).unwrap();
    let mut got = rows(&dbp, "SELECT key, val FROM rel_star ORDER BY key");
    got.sort();
    assert_eq!(got, vec![
        vec!["a".to_string(), "A".to_string()],
        vec!["b".to_string(), "B".to_string()],
        vec!["c".to_string(), "C".to_string()],
    ]);
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

/// The `effect_log` built-in projects `pending_effect` into a queryable rel: the
/// drain queue, live. A rule reading it sees each request's `state` march
/// queued -> done across a drain, so a `.dl` program can observe (and rail on)
/// its own effect queue — the dl-native call log, the parity surface against an
/// external cache's call_log.
#[test]
fn effect_log_mirrors_the_drain_queue() {
    let d = sandbox("efflog");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel want(key: str, url: str).\n\
         want(\"home\", \"https://api/home\").\n\
         rel resp(key: str, status: int, body: str).\n\
         resp(key, status, body) <- @async want(key, url).\n\
         rel queued(id: str).\n\
         queued(id) <- effect_log(id, _, _, \"queued\", _, _).\n\
         rel landed(id: str).\n\
         landed(id) <- effect_log(id, _, _, \"done\", _, _).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Tick 1 queues the request; tick 2 projects effect_log (the row queued at
    // tick-1 end is visible at tick-2 start) and the `queued` rail fires.
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();
    let log = rows(&dbp, "SELECT kind, head, state FROM rel_effect_log");
    assert_eq!(log.len(), 1, "one effect_log row for the one request: {log:?}");
    assert_eq!(log[0], vec!["resp".to_string(), "resp".to_string(), "queued".to_string()]);
    assert_eq!(rows(&dbp, "SELECT id FROM rel_queued").len(), 1, "queued rail fired");
    assert_eq!(rows(&dbp, "SELECT id FROM rel_landed").len(), 0, "nothing done yet");

    // Drain off-tick, then re-tick: effect_log now reads the row as done.
    let exec = MockExec {
        table: HashMap::from([(
            "https://api/home".to_string(),
            vec!["200".to_string(), "HOME".to_string()],
        )]),
        calls: Mutex::new(Vec::new()),
    };
    eng.drain_effects(&prog, &exec).unwrap();
    eng.tick(&prog, true).unwrap();
    let state: Vec<Vec<String>> = rows(&dbp, "SELECT state FROM rel_effect_log");
    assert_eq!(state, vec![vec!["done".to_string()]], "drain flipped the state");
    assert_eq!(rows(&dbp, "SELECT id FROM rel_queued").len(), 0, "no longer queued");
    assert_eq!(rows(&dbp, "SELECT id FROM rel_landed").len(), 1, "landed rail fires on done");
}
