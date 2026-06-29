//! ghcacher parity: each named test here mirrors a behavioral test in
//! ~/projects/ghcacher (src/db.rs, src/sync/*, src/output.rs) and asserts the
//! SAME observable outcome from the dl port, with a no-network mock executor.
//! The point is "we can repeat ghcacher's tests in dl": the conditional cache
//! (poll_state), the append-only change_log, idempotent dedup, entity
//! normalization, the reactive resync trigger, and ndjson output all reproduce.
//! Two behaviors are `#[ignore]` with the reason: the upsert UPDATE family needs
//! latest-wins (gap B, argmax by req_tx) and the live tail needs a query
//! subscription. Surrogate integer ids (get_repo_id) do not map — dl is
//! content-addressed by design.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

use anyhow::Result;
use serde_json::{Map, Value as Json};

use sprefa_v5::db;
use sprefa_v5::engine::{EffectExec, Engine};
use sprefa_v5::prepare_paths;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_ghparity_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn rows(db_path: &Path, sql: &str) -> Vec<Vec<String>> {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let mut s = conn.prepare(sql).unwrap();
    let ncol = s.column_count();
    s.query_map([], |r| {
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
    .collect()
}

/// Write `src` to the sandbox and open an engine over it. Callers re-`prepare_paths`
/// to get the parsed `Program` they drive ticks with.
fn run(d: &Path, src: &str) -> Engine {
    fs::write(d.join("p.dl"), src).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    Engine::new(conn, d.to_path_buf())
}

/// Drive `cycles` of tick+drain with `exec`, then a final settle tick.
fn drive(eng: &mut Engine, prog: &sprefa_v5::ast::Program, exec: &dyn EffectExec, cycles: usize) {
    for _ in 0..cycles {
        eng.tick(prog, true).unwrap();
        eng.drain_effects(prog, exec).unwrap();
    }
    eng.tick(prog, true).unwrap();
}

/// A conditional-GET mock: empty `prev` etag -> a 200 carrying `out200`; any
/// carried etag -> a 304 with no body. `out200` is the executor's output slots
/// (status, then the head's remaining columns in order).
struct CondGet {
    out200: Vec<String>,
}
impl EffectExec for CondGet {
    fn run(&self, _kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        let prev = args.get("prev").and_then(|v| v.as_str()).unwrap_or("");
        Ok(if prev.is_empty() {
            self.out200.clone()
        } else {
            // 304: status + empty remaining slots (one per non-status output).
            let mut v = vec!["304".to_string()];
            v.extend(std::iter::repeat(String::new()).take(self.out200.len() - 1));
            v
        })
    }
}

/// ghcacher db.rs `poll_state_roundtrip` + `poll_state_preserves_interval_on_304`:
/// the etag AND the poll_interval are stored per resource; a 304 (no fresh values)
/// keeps the prior ones. Here `poll_state` is a two-field `@next` carry; the 200
/// installs (etag, interval), the 304 carries both forward unchanged.
#[test]
fn parity_poll_state_roundtrip_and_304_preserves_fields() {
    let d = sandbox("pollstate");
    let dbp = d.join("db");
    let src = "rel watch(ep: text).\n\
        watch(\"repos/o/n/events\").\n\
        rel ps(ep: text, tag: text, interval: text).\n\
        rel ps_next(ep: text, tag: text, interval: text).\n\
        rel poll(ep: text, prev: text).\n\
        poll(ep, prev) <- watch(ep), ps(ep, prev, _).\n\
        poll(ep, \"\")  <- watch(ep), !ps(ep, _, _).\n\
        rel resp(ep: text, status: int, tag: text, interval: text).\n\
        resp(ep, status, tag, iv) <- @async poll(ep, prev).\n\
        ps_next(ep, tag, iv)  <- resp(ep, 200, tag, iv).\n\
        ps_next(ep, old, oiv) <- resp(ep, 304, _, _), ps(ep, old, oiv).\n\
        ps(ep, t, iv) <- @next ps_next(ep, t, iv).\n";
    let mut eng = run(&d, src);
    let (prog, _d, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let exec = CondGet { out200: vec!["200".into(), "\"abc\"".into(), "60".into()] };
    drive(&mut eng, &prog, &exec, 5);

    let ps = rows(&dbp, "SELECT tag, interval FROM rel_ps");
    assert_eq!(ps, vec![vec!["\"abc\"".to_string(), "60".to_string()]],
        "etag and interval roundtrip, and the 304 preserved both");
}

/// ghcacher db.rs `change_log_insert` + the `upsert_*_idempotent` family: distinct
/// entities each append one row; re-deriving the same entity is a no-op (structural
/// set dedup = INSERT-OR-IGNORE). Driven for many cycles to prove the count is
/// stable (a cache-hit 304 adds nothing).
#[test]
fn parity_change_log_append_is_idempotent() {
    let d = sandbox("changelog");
    let dbp = d.join("db");
    let src = "rel watch(ep: text).\n\
        watch(\"o/n\").\n\
        rel etag(ep: text, tag: text).\n\
        rel etag_next(ep: text, tag: text).\n\
        rel poll(ep: text, prev: text).\n\
        poll(ep, prev) <- watch(ep), etag(ep, prev).\n\
        poll(ep, \"\")  <- watch(ep), !etag(ep, _).\n\
        rel resp(ep: text, status: int, tag: text, body: text).\n\
        resp(ep, status, tag, body) <- @async poll(ep, prev).\n\
        etag_next(ep, tag) <- resp(ep, 200, tag, _).\n\
        etag_next(ep, old) <- resp(ep, 304, _, _), etag(ep, old).\n\
        etag(ep, tag) <- @next etag_next(ep, tag).\n\
        rel stars(ep: text, n: text).\n\
        stars(ep, n) <- resp(ep, 200, _, body), jsonp(body, \"stargazers_count\", n).\n\
        rel name(ep: text, v: text).\n\
        name(ep, v) <- resp(ep, 200, _, body), jsonp(body, \"full_name\", v).\n\
        rel change_log(ep: text, kind: text, val: text).\n\
        rel change_log_next(ep: text, kind: text, val: text).\n\
        change_log_next(ep, kind, val) <- change_log(ep, kind, val).\n\
        change_log_next(ep, \"stars\", n) <- stars(ep, n).\n\
        change_log_next(ep, \"full_name\", v) <- name(ep, v).\n\
        change_log(ep, kind, val) <- @next change_log_next(ep, kind, val).\n";
    let mut eng = run(&d, src);
    let (prog, _d, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let exec = CondGet {
        out200: vec!["200".into(), "etagA".into(),
            r#"{"stargazers_count": 42, "full_name": "o/n"}"#.into()],
    };
    drive(&mut eng, &prog, &exec, 12);

    let mut log = rows(&dbp, "SELECT kind, val FROM rel_change_log");
    log.sort();
    assert_eq!(log, vec![
        vec!["full_name".to_string(), "o/n".to_string()],
        vec!["stars".to_string(), "42".to_string()],
    ], "two distinct entities, each once, stable across 12 cycles");
}

/// ghcacher db.rs `change_log_with_payload`: a change carries a JSON payload and a
/// field of it (`number`) is read back. Here `jsonp` lifts the field into the
/// change row's `val` (no effect — the body is a bound column).
#[test]
fn parity_change_log_with_payload() {
    let d = sandbox("payload");
    let dbp = d.join("db");
    let body = r#"{"number": 42, "title": "My PR"}"#;
    let src = format!(
        "rel resp(ep: text, body: text).\n\
        resp(\"o/n\", {body:?}).\n\
        rel pr_number(ep: text, n: text).\n\
        pr_number(ep, n) <- resp(ep, body), jsonp(body, \"number\", n).\n\
        rel change_log(ep: text, kind: text, val: text).\n\
        rel change_log_next(ep: text, kind: text, val: text).\n\
        change_log_next(ep, kind, val) <- change_log(ep, kind, val).\n\
        change_log_next(ep, \"pull_request\", n) <- pr_number(ep, n).\n\
        change_log(ep, kind, val) <- @next change_log_next(ep, kind, val).\n");
    let mut eng = run(&d, &src);
    let (prog, _d, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    for _ in 0..4 { eng.tick(&prog, true).unwrap(); }

    let log = rows(&dbp, "SELECT kind, val FROM rel_change_log");
    assert_eq!(log, vec![vec!["pull_request".to_string(), "42".to_string()]],
        "the change row carries the payload's number field");
}

/// ghcacher sync/events.rs `status_event_triggers_pr_resync_via_sha`: an observed
/// event row causes the affected PR/repo to be re-synced. In dl the trigger is a
/// rule, not imperative code: an `event` row derives a `resync` target, which a
/// downstream poll rule would consume. The reactive derivation IS the trigger.
#[test]
fn parity_event_triggers_resync() {
    let d = sandbox("resync");
    let dbp = d.join("db");
    let src = "rel event(repo: text, kind: text, sha: text).\n\
        event(\"o/n\", \"status\", \"deadbeef\").\n\
        event(\"o/n\", \"push\", \"cafef00d\").\n\
        rel resync(repo: text).\n\
        resync(repo) <- event(repo, _, _).\n\
        ? resync(repo).\n";
    let mut eng = run(&d, src);
    let (prog, _d, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    eng.tick(&prog, true).unwrap();

    let got = rows(&dbp, "SELECT repo FROM rel_resync");
    assert_eq!(got, vec![vec!["o/n".to_string()]],
        "an event row triggers a single resync of its repo (deduped)");
}

/// ghcacher output.rs `json_format_produces_ndjson`: the cache emits newline-
/// delimited JSON. The dl port's `--query-json` emits one JSON object per `?`
/// query (`{query, columns, rows, count}`), the parity surface for a tail/SSE
/// consumer.
#[test]
fn parity_query_json_is_ndjson() {
    let d = sandbox("ndjson");
    let src = "rel pr(num: int, state: text).\n\
        pr(1, \"open\").\n\
        pr(2, \"closed\").\n\
        ? pr(num, state).\n";
    fs::write(d.join("p.dl"), src).unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--root", d.to_str().unwrap(), "--db", d.join("db").to_str().unwrap(), "--query-json"])
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().find(|l| l.trim_start().starts_with('{'))
        .expect("a JSON line");
    let v: Json = serde_json::from_str(line).expect("ndjson line parses");
    assert_eq!(v["count"], Json::from(2), "two pr rows: {line}");
    assert_eq!(v["query"].as_str().unwrap_or(""), "pr", "query name echoed: {line}");
}

/// ghcacher sync/prs.rs `upsert_pr_insert_and_update`: insert PR#1 "First", then a
/// re-sync of PR#1 "Updated" leaves COUNT=1 with title="Updated" (latest-wins,
/// keyed on number). A naive dl accumulator keeps BOTH titles (set-valued), so
/// this needs gap B (a `latest(key)` reduction = argmax by req_tx). Un-ignore when
/// latest-wins lands.
#[test]
#[ignore = "needs gap B: latest-wins/upsert (argmax by req_tx)"]
fn parity_upsert_pr_update_latest_wins() {
    let d = sandbox("upsert");
    let dbp = d.join("db");
    // Two observations of the same PR number with different titles, tagged by tick.
    let src = "rel obs(num: text, title: text, tx: int).\n\
        obs(\"1\", \"First\", 1).\n\
        obs(\"1\", \"Updated\", 2).\n\
        rel pull_request(num: text, title: text).\n\
        # DESIRED (gap B): keep only the latest tx per num.\n\
        pull_request(num, title) <- latest(obs, num, tx), obs(num, title, tx).\n\
        ? pull_request(num, title).\n";
    let mut eng = run(&d, src);
    let (prog, _d, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    eng.tick(&prog, true).unwrap();

    let pr = rows(&dbp, "SELECT num, title FROM rel_pull_request");
    assert_eq!(pr, vec![vec!["1".to_string(), "Updated".to_string()]],
        "latest-wins: one row per number, the newest title");
}
