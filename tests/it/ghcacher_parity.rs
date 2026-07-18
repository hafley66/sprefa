//! ghcacher parity: each named test here mirrors a behavioral test in
//! ~/projects/ghcacher (src/db.rs, src/sync/*, src/output.rs) and asserts the
//! SAME observable outcome from the dl port, with a no-network mock executor.
//! The point is "we can repeat ghcacher's tests in dl": the conditional cache
//! (poll_state), the append-only change_log, idempotent dedup, entity
//! normalization, the reactive resync trigger, and ndjson output all reproduce.
//! The upsert UPDATE family (latest-wins, once thought to need a new argmax
//! builtin = "gap B") is just the relational argmax — `max(tx)` per key joined
//! back to the winning row, no engine change (parity_upsert_pr_update_latest_wins).
//! Surrogate integer ids (get_repo_id) do not map — dl is content-addressed by
//! design.

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

fn rows(db_path: &Path, rel: &str, sql: &str) -> Vec<Vec<String>> {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    conn.query_values(rel, sql, &[])
        .unwrap()
        .into_iter()
        .map(|row| row.into_iter().map(|cell| cell.to_lossy_string()).collect())
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

    let ps = rows(&dbp, "rel_ps_txt", "SELECT tag, interval FROM rel_ps_txt");
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

    let mut log = rows(&dbp, "rel_change_log_txt", "SELECT kind, val FROM rel_change_log_txt");
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

    let log = rows(&dbp, "rel_change_log_txt", "SELECT kind, val FROM rel_change_log_txt");
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

    let got = rows(&dbp, "rel_resync_txt", "SELECT repo FROM rel_resync_txt");
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
        .args(["--db", d.join("db").to_str().unwrap(), "--query-json"])
        .current_dir(d)
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
/// keyed on number). A naive accumulator keeps BOTH titles, so this looked like it
/// needed a new `latest`/argmax builtin (gap B). It does not: latest-wins IS the
/// textbook relational argmax — `max(tx)` per key, then join back to recover the
/// winning row. Two existing pieces (the head-only `max` aggregate + an ordinary
/// join), no engine change. PR#2 (single observation) rides through untouched.
#[test]
fn parity_upsert_pr_update_latest_wins() {
    let d = sandbox("upsert");
    let dbp = d.join("db");
    // Observations of each PR number tagged by tick; #1 is re-observed (an update).
    let src = "rel obs(num: text, title: text, tx: int).\n\
        obs(\"1\", \"First\", 1).\n\
        obs(\"1\", \"Updated\", 2).\n\
        obs(\"2\", \"Solo\", 1).\n\
        rel latest_tx(num: text, tx: int).\n\
        latest_tx(num, max(tx)) <- obs(num, _, tx).\n\
        rel pull_request(num: text, title: text).\n\
        pull_request(num, title) <- latest_tx(num, tx), obs(num, title, tx).\n\
        ? pull_request(num, title).\n";
    let mut eng = run(&d, src);
    let (prog, _d, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    eng.tick(&prog, true).unwrap();

    let mut pr = rows(&dbp, "rel_pull_request_txt", "SELECT num, title FROM rel_pull_request_txt");
    pr.sort();
    assert_eq!(pr, vec![
        vec!["1".to_string(), "Updated".to_string()],
        vec!["2".to_string(), "Solo".to_string()],
    ], "latest-wins: one row per number, the newest title (argmax via max+join)");
}

/// A two-kind mock for the FULL port (examples/gh-cache-full.dl). The effect kind
/// is the `sh` fn NAME: `fetch` (the conditional GET, 5 output slots: status, etag,
/// remaining, reset, body — body last) and `list_fetch` (the paginated list, 1
/// slot: the merged array body). Proves the whole loop wires through `@async`: rate
/// capture + gate, the 200/304 etag cache, `--paginate`-shaped array normalize, and
/// the change feed — no network, no jq.
struct FullMock;
impl EffectExec for FullMock {
    fn run(&self, kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        match kind {
            "fetch" => {
                let prev = args.get("prev").and_then(|v| v.as_str()).unwrap_or("");
                Ok(if prev.is_empty() {
                    // 200: status, etag, x-ratelimit-remaining, -reset, body.
                    vec!["200".into(), "etagA".into(), "4998".into(), "1700000000".into(),
                         r#"{"stargazers_count": 42, "full_name": "cli/cli"}"#.into()]
                } else {
                    // 304: no body, but the rate headers still arrive (remaining ticks down).
                    vec!["304".into(), "".into(), "4997".into(), "1700000000".into(), "".into()]
                })
            }
            // The merged multi-page array (what `gh api --paginate | jq -s add` yields).
            "list_fetch" => Ok(vec![
                r#"[{"number":1,"title":"fix","state":"open","user":{"login":"alice"}},
                    {"number":2,"title":"feat","state":"closed","user":{"login":"bob"}}]"#.into(),
            ]),
            _ => Ok(Vec::new()),
        }
    }
}

/// The feature-complete port runs end to end: drive examples/gh-cache-full.dl with
/// FullMock and assert every feature surfaced — stars normalized from a 200 body,
/// the rate reading captured + carried, both PRs normalized from the paginated
/// array (latest-wins argmax), and the change feed accumulated.
#[test]
fn parity_full_port_end_to_end() {
    let d = sandbox("fullport");
    let dbp = d.join("db");
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/gh-cache-full.dl");
    let src = fs::read_to_string(&example).expect("read gh-cache-full.dl");
    let mut eng = run(&d, &src);
    let (prog, _d, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    drive(&mut eng, &prog, &FullMock, 4);

    let stars = rows(&dbp, "rel_stars_txt", "SELECT n FROM rel_stars_txt");
    assert_eq!(stars, vec![vec!["42".to_string()]], "stars normalized from the 200 body");

    let reading = rows(&dbp, "rel_reading_txt", "SELECT remaining FROM rel_reading_txt");
    assert!(!reading.is_empty(), "the rate reading was captured + carried: {reading:?}");

    let mut prs = rows(&dbp, "rel_pull_request_txt", "SELECT num, title, state, author FROM rel_pull_request_txt");
    prs.sort();
    assert_eq!(prs, vec![
        vec!["1".to_string(), "fix".to_string(), "open".to_string(), "alice".to_string()],
        vec!["2".to_string(), "feat".to_string(), "closed".to_string(), "bob".to_string()],
    ], "both PRs normalized from the paginated array (latest-wins): {prs:?}");

    let log = rows(&dbp, "rel_change_log_txt", "SELECT DISTINCT kind FROM rel_change_log_txt ORDER BY kind");
    assert_eq!(log, vec![vec!["pull_request".to_string()], vec!["stars".to_string()]],
        "the change feed accumulated both entity kinds: {log:?}");
}
