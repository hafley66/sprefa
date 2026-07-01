//! ghcacher, as a datalog program (examples/gh-cache.dl). The conditional-request
//! cache loop end to end with a mock executor: the FIRST poll (no etag) returns
//! 200 + a body, term-form `jsonp` extracts entities, the `@next` `change_log`
//! appends them, and the etag is carried. Every subsequent poll sends that etag
//! and the mock returns 304 — no body, no entity, so `change_log` is untouched.
//! That 304-skip (a cache hit costs nothing and emits no change) is the property
//! ghcacher exists for, here proven without a network. See the gh-cache example.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Result;
use serde_json::{Map, Value as Json};

use sprefa_v5::db;
use sprefa_v5::engine::{EffectExec, Engine};
use sprefa_v5::prepare_paths;

use crate::clock_lock::{clear_now, set_now, CLOCK_LOCK};

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_ghcache_{tag}_{}", std::process::id()));
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

/// The GitHub API as a conditional cache: the first request (empty `prev` etag)
/// returns 200 with a body and a fresh etag; any request that carries an etag is
/// a cache hit and returns 304 with no body. Records every (status) it served so
/// the test can assert a 304 actually happened (the resource was re-polled).
struct GhMock {
    body: String,
    served: Mutex<Vec<String>>,
}

impl EffectExec for GhMock {
    fn run(&self, _kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        let prev = args.get("prev").and_then(|v| v.as_str()).unwrap_or("");
        let out = if prev.is_empty() {
            // 200: fresh etag + the JSON body.
            vec!["200".to_string(), "etagA".to_string(), self.body.clone()]
        } else {
            // 304 Not Modified: no etag change, no body.
            vec!["304".to_string(), String::new(), String::new()]
        };
        self.served.lock().unwrap().push(out[0].clone());
        Ok(out)
    }
}

/// A resource that CHANGES once, then settles: the first poll (no etag) is a 200
/// with `etagA`/body1; a poll carrying `etagA` finds the resource changed and
/// returns a NEW 200 (`etagB`/body2); any later etag is a 304. Models a star
/// count ticking up once. Used to prove the latest-wins (`resp_current`) view.
struct GhChanging {
    body1: String,
    body2: String,
}
impl EffectExec for GhChanging {
    fn run(&self, _kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        let prev = args.get("prev").and_then(|v| v.as_str()).unwrap_or("");
        Ok(match prev {
            "" => vec!["200".into(), "etagA".into(), self.body1.clone()],
            "etagA" => vec!["200".into(), "etagB".into(), self.body2.clone()],
            _ => vec!["304".into(), String::new(), String::new()],
        })
    }
}

/// Latest-wins, and the bug it fixes. `resp` accumulates one row per response
/// (the history); a changing resource lands TWO 200s with different etags. The
/// naive carry `etag_next <- resp(200, tag, _)` then derives BOTH etags, so the
/// next poll fans out and the cache multiplies. The fix is a monotone `clock`
/// bucket threaded into `resp`: `resp_latest(ep, max(b))` picks the newest 200,
/// `resp_current` is that single body, and the etag carry reads `resp_current`
/// so it stays single-valued. A 304 lands no 200, so `resp_current` (hence the
/// etag) holds at the last good version. Entities derive from `resp_current`
/// (latest-wins); `change_log` still keeps every value that was ever current.
/// Pure datalog — the upsert reduction, no engine change.
#[test]
fn resp_current_is_the_latest_wins_view_over_accumulated_resp() {
    let _g = CLOCK_LOCK.lock().unwrap();
    let d = sandbox("latest");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel watch(ep: text).\n\
         watch(\"repos/cli/cli\").\n\
         rel etag(ep: text, tag: text).\n\
         rel etag_next(ep: text, tag: text).\n\
         rel poll(ep: text, prev: text, b: int).\n\
         poll(ep, prev, b) <- watch(ep), etag(ep, prev), clock(300, b).\n\
         poll(ep, \"\", b)  <- watch(ep), !etag(ep, _), clock(300, b).\n\
         rel resp(ep: text, b: int, status: int, tag: text, body: text).\n\
         resp(ep, b, status, tag, body) <- @async poll(ep, prev, b).\n\
         rel resp_latest(ep: text, b: int).\n\
         resp_latest(ep, max(b)) <- resp(ep, b, 200, _, _).\n\
         rel resp_current(ep: text, tag: text, body: text).\n\
         resp_current(ep, tag, body) <- resp(ep, b, 200, tag, body), resp_latest(ep, b).\n\
         etag_next(ep, tag) <- resp_current(ep, tag, _).\n\
         etag(ep, tag) <- @next etag_next(ep, tag).\n\
         rel stars(ep: text, n: text).\n\
         stars(ep, n) <- resp_current(ep, _, body), jsonp(body, \"stargazers_count\", n).\n\
         rel change_log(ep: text, kind: text, val: text).\n\
         rel change_log_next(ep: text, kind: text, val: text).\n\
         change_log_next(ep, kind, val) <- change_log(ep, kind, val).\n\
         change_log_next(ep, \"stars\", n) <- stars(ep, n).\n\
         change_log(ep, kind, val) <- @next change_log_next(ep, kind, val).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = GhChanging {
        body1: r#"{"stargazers_count": 1}"#.to_string(),
        body2: r#"{"stargazers_count": 2}"#.to_string(),
    };
    // Advance a clock bucket per cycle so the carried-etag poll re-fires:
    // bucket 0 -> 200 etagA(body1); bucket 1 (carry etagA) -> 200 etagB(body2);
    // bucket 2+ (carry etagB) -> 304. Settle.
    for i in 0..8 {
        set_now(1_000_000 + (i as i64) * 300);
        eng.tick(&prog, true).unwrap();
        eng.drain_effects(&prog, &exec).unwrap();
    }
    eng.tick(&prog, true).unwrap();

    // Raw resp accumulated BOTH 200 versions (plus the 304s) — the history. (A
    // version can repeat across buckets during the one-tick etag-carry lag; that
    // is harmless, `max(bucket)` still picks the newest, so assert on DISTINCT
    // versions seen.)
    let resp200 = rows(&dbp, "SELECT DISTINCT tag FROM rel_resp WHERE status = 200 ORDER BY tag");
    assert_eq!(resp200, vec![vec!["etagA".to_string()], vec!["etagB".to_string()]],
        "resp keeps every 200 version (history)");
    // The carry stayed single-valued (the bug would leave two etags).
    let etags = rows(&dbp, "SELECT tag FROM rel_etag");
    assert_eq!(etags, vec![vec!["etagB".to_string()]], "etag carry is single-valued (latest)");
    // resp_current is JUST the current version's body -> stars reflects the LATEST.
    let stars = rows(&dbp, "SELECT n FROM rel_stars");
    assert_eq!(stars, vec![vec!["2".to_string()]], "latest-wins: stars is the newest value only");
    // change_log still has the full history (both star values were once current).
    let mut log: Vec<String> = rows(&dbp, "SELECT val FROM rel_change_log").into_iter().flatten().collect();
    log.sort();
    assert_eq!(log, vec!["1".to_string(), "2".to_string()],
        "change_log keeps every value that was ever current (the feed)");
    clear_now();
}

#[test]
fn gh_cache_lands_entities_then_304_is_a_free_cache_hit() {
    let d = sandbox("loop");
    let dbp = d.join("db");
    // The cache loop WITHOUT `every()` (manual ticks drive it; the example file
    // carries every(300) for the daemon). `!etag` seeds the first poll with "".
    fs::write(
        d.join("p.dl"),
        "rel watch(ep: text).\n\
         watch(\"repos/cli/cli\").\n\
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
         rel full_name(ep: text, name: text).\n\
         full_name(ep, name) <- resp(ep, 200, _, body), jsonp(body, \"full_name\", name).\n\
         rel change_log(ep: text, kind: text, val: text).\n\
         rel change_log_next(ep: text, kind: text, val: text).\n\
         change_log_next(ep, kind, val) <- change_log(ep, kind, val).\n\
         change_log_next(ep, \"stars\", n) <- stars(ep, n).\n\
         change_log_next(ep, \"full_name\", v) <- full_name(ep, v).\n\
         change_log(ep, kind, val) <- @next change_log_next(ep, kind, val).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    let exec = GhMock {
        body: r#"{"stargazers_count": 42, "full_name": "cli/cli"}"#.to_string(),
        served: Mutex::new(Vec::new()),
    };

    // Drive the loop: tick emits/derives, drain runs the conditional request
    // off-tick. Five cycles lets the etag carry advance (first poll "" -> 200 ->
    // carry etagA -> re-poll with etagA -> 304) and settle.
    for _ in 0..5 {
        eng.tick(&prog, true).unwrap();
        eng.drain_effects(&prog, &exec).unwrap();
    }
    eng.tick(&prog, true).unwrap();

    // The 200 body's two fields are in the change feed, exactly once each.
    let mut log = rows(&dbp, "SELECT kind, val FROM rel_change_log ORDER BY kind, val");
    log.sort();
    assert_eq!(
        log,
        vec![
            vec!["full_name".to_string(), "cli/cli".to_string()],
            vec!["stars".to_string(), "42".to_string()],
        ],
        "change_log holds the 200's entities, deduped"
    );

    // A re-poll carrying the etag happened and the mock served a 304 — the cache
    // hit. The 304 added no entity and no change_log row (asserted above).
    let served = exec.served.lock().unwrap().clone();
    assert!(served.contains(&"200".to_string()), "the first poll was a 200: {served:?}");
    assert!(served.contains(&"304".to_string()), "a carried-etag re-poll got a 304: {served:?}");

    // The carried etag settled on the 200's value (the 304 kept it).
    let etag = rows(&dbp, "SELECT tag FROM rel_etag");
    assert_eq!(etag, vec![vec!["etagA".to_string()]], "etag carried from the 200");
}

/// The cadence cap, network-free. The poll args carry a `poll_bucket` counter
/// that `every(N)` advances once per N-second boundary, so a re-poll fires on
/// cadence and an unchanged resource between boundaries hashes to the SAME
/// request id (INSERT-OR-IGNORE) — no new call leaves the machine no matter how
/// often the daemon ticks. This is the "don't destroy GitHub" guarantee, proven
/// by counting executor calls: many ticks inside one bucket add zero calls, and a
/// single bucket advance adds exactly one conditional request. `every` is driven
/// deterministically by resetting its `_carry_meta` bucket between ticks (no real
/// time, no network).
struct CountingGh {
    calls: Mutex<Vec<(String, String)>>, // (prev etag, bucket) per request
}
impl EffectExec for CountingGh {
    fn run(&self, _kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        let prev = args.get("prev").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let bucket = args
            .get("bucket")
            .map(|v| v.to_string())
            .unwrap_or_default();
        self.calls.lock().unwrap().push((prev.clone(), bucket));
        // First poll (no etag) -> 200 + body + fresh etag; any carried etag is a
        // cache hit -> 304 (the resource never changes in this test).
        Ok(if prev.is_empty() {
            vec!["200".into(), "etagA".into(), r#"{"stargazers_count": 7}"#.into()]
        } else {
            vec!["304".into(), String::new(), String::new()]
        })
    }
}

#[test]
fn repolls_once_per_cadence_bucket_and_is_silent_between() {
    let _g = CLOCK_LOCK.lock().unwrap();
    let d = sandbox("cadence");
    let dbp = d.join("db");
    // The cadence is one join: `clock(300, b)` binds the current time bucket, which
    // varies the poll args (and thus the request id) once per 300s. No counter.
    fs::write(
        d.join("p.dl"),
        "rel watch(ep: text).\n\
         watch(\"repos/cli/cli\").\n\
         rel etag(ep: text, tag: text).\n\
         rel etag_next(ep: text, tag: text).\n\
         rel poll(ep: text, prev: text, bucket: int).\n\
         poll(ep, prev, b) <- watch(ep), etag(ep, prev), clock(300, b).\n\
         poll(ep, \"\",  b) <- watch(ep), !etag(ep, _),   clock(300, b).\n\
         rel resp(ep: text, status: int, tag: text, body: text).\n\
         resp(ep, status, tag, body) <- @async poll(ep, prev, bucket).\n\
         etag_next(ep, tag) <- resp(ep, 200, tag, _).\n\
         etag_next(ep, old) <- resp(ep, 304, _, _), etag(ep, old).\n\
         etag(ep, tag) <- @next etag_next(ep, tag).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = CountingGh { calls: Mutex::new(Vec::new()) };

    // Bucket 1_000_000/300 = 3333: the loop settles. "" -> 200 (carry etagA) ->
    // etagA -> 304, then the args stop changing. Ten cycles at the SAME injected
    // time proves the quiet: exactly two requests, no matter how many ticks.
    set_now(1_000_000);
    for _ in 0..10 {
        eng.tick(&prog, true).unwrap();
        eng.drain_effects(&prog, &exec).unwrap();
    }
    assert_eq!(
        exec.calls.lock().unwrap().len(),
        2,
        "one bucket makes exactly two calls then is silent across re-ticks: {:?}",
        exec.calls.lock().unwrap()
    );

    // Cross one 300s boundary (3333 -> 3334): the etagA poll re-fires under the new
    // bucket id. Several ticks, but only ONE new call (a 304).
    set_now(1_000_300);
    for _ in 0..5 {
        eng.tick(&prog, true).unwrap();
        eng.drain_effects(&prog, &exec).unwrap();
    }
    assert_eq!(
        exec.calls.lock().unwrap().len(),
        3,
        "one cadence boundary adds exactly one conditional re-poll: {:?}",
        exec.calls.lock().unwrap()
    );

    // A second boundary (3334 -> 3335): again exactly one re-poll.
    set_now(1_000_600);
    for _ in 0..5 {
        eng.tick(&prog, true).unwrap();
        eng.drain_effects(&prog, &exec).unwrap();
    }
    assert_eq!(
        exec.calls.lock().unwrap().len(),
        4,
        "second boundary adds exactly one more: {:?}",
        exec.calls.lock().unwrap()
    );

    // The carried etag never regressed across buckets.
    let etag = rows(&dbp, "SELECT tag FROM rel_etag");
    assert_eq!(etag, vec![vec!["etagA".to_string()]], "etag stable across buckets");
    clear_now();
}

/// A gh LIST endpoint (`/pulls`) returns a JSON array; one `json` brace pattern
/// normalizes it into one `pull_request` row per element, sibling fields
/// correlated and the nested `user.login` descended in the same match. This is
/// the whole "API JSON -> normalized SQLite tables, in pure dl" claim, on the
/// real shape (array of objects with nesting). No effect needed — the body is a
/// bound column, so the hybrid join+extract pass does it on an ordinary tick.
#[test]
fn list_endpoint_body_normalizes_into_entity_rows() {
    let d = sandbox("list");
    let dbp = d.join("db");
    let body = r#"[{"number": 1, "title": "fix bug", "state": "open", "user": {"login": "alice"}}, {"number": 2, "title": "add feat", "state": "closed", "user": {"login": "bob"}}]"#;
    fs::write(
        d.join("p.dl"),
        format!(
            "rel resp(ep: text, body: text).\n\
             resp(\"cli/cli\", {body:?}).\n\
             rel pull_request(ep: text, num: text, title: text, state: text, author: text).\n\
             pull_request(ep, num, title, state, author) <-\n\
                 resp(ep, body),\n\
                 json(body, q:[... {{ number: $num, title: $title, state: $state, user: {{ login: $author }} }} ]).\n\
             ? pull_request(ep, num, title, state, author).\n"
        ),
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();

    let mut got = rows(&dbp, "SELECT num, title, state, author FROM rel_pull_request ORDER BY num");
    got.sort();
    assert_eq!(
        got,
        vec![
            vec!["1".to_string(), "fix bug".into(), "open".into(), "alice".into()],
            vec!["2".to_string(), "add feat".into(), "closed".into(), "bob".into()],
        ],
        "each array element is one row, flat + nested fields correlated"
    );
}

/// LIVE against real GitHub (ignored by default: needs `gh` authed + network).
/// Run: `cargo test --test it gh_cache_live -- --ignored --nocapture`. Drives the
/// gh-cache loop with a REAL ShellEffectExec (no mock): the effect_cmd template
/// shells `gh api -i`, a formatter splits status/etag/body, term-form jsonp
/// normalizes the live body, and a second drain carrying the etag must get a 304.
/// This is the actual utility check against the thing ghcacher does.
#[test]
#[ignore]
fn gh_cache_live_against_github() {
    use sprefa_v5::engine::{async_effect_arity, ShellEffectExec};
    let _g = CLOCK_LOCK.lock().unwrap();
    let d = sandbox("live");
    let dbp = d.join("db");
    // Newline-separated outputs (status\netag\nbody) — `run` splits stdout by line,
    // last slot absorbs the body. {ep}/{prev} are filled from the request args.
    let tmpl = "R=$(gh api {ep} -i -H \"If-None-Match: $prev\" 2>/dev/null); \
                C=$(printf '%s' \"$R\" | head -1 | grep -oE '[0-9]{3}' | head -1); \
                E=$(printf '%s' \"$R\" | grep -iE '^etag:' | head -1 | sed -E 's/^[Ee]tag:[[:space:]]*//; s/\\r$//'); \
                B=$(printf '%s' \"$R\" | awk 'f{print} /^\\r?$/{f=1}' | tr -d '\\n'); \
                printf '%s\\n%s\\n%s' \"$C\" \"$E\" \"$B\"";
    fs::write(
        d.join("p.dl"),
        format!(
            "rel watch(ep: text).\n\
             watch(\"repos/cli/cli\").\n\
             rel etag(ep: text, tag: text).\n\
             rel etag_next(ep: text, tag: text).\n\
             rel poll(ep: text, prev: text, bucket: int).\n\
             poll(ep, prev, b) <- watch(ep), etag(ep, prev), clock(300, b).\n\
             poll(ep, \"\",  b) <- watch(ep), !etag(ep, _),   clock(300, b).\n\
             rel resp(ep: text, status: int, tag: text, body: text).\n\
             resp(ep, status, tag, body) <- @async poll(ep, prev, bucket).\n\
             rel effect_cmd(kind: text, template: text).\n\
             effect_cmd(\"resp\", {tmpl:?}).\n\
             etag_next(ep, tag) <- resp(ep, 200, tag, _).\n\
             etag_next(ep, old) <- resp(ep, 304, _, _), etag(ep, old).\n\
             etag(ep, tag) <- @next etag_next(ep, tag).\n\
             rel stars(ep: text, n: text).\n\
             stars(ep, n) <- resp(ep, 200, _, body), jsonp(body, \"stargazers_count\", n).\n\
             rel full_name(ep: text, name: text).\n\
             full_name(ep, name) <- resp(ep, 200, _, body), jsonp(body, \"full_name\", name).\n\
             ? stars(ep, n).\n"
        ),
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    for i in 0..4 {
        // Cross a clock bucket each cycle so the carried-etag poll re-fires (live:
        // the 2nd+ cycle must produce a real 304). The daemon uses wall-clock; here
        // we inject `now` 300s forward per cycle so clock(300,_) advances.
        set_now(1_000_000 + (i as i64) * 300);
        eng.tick(&prog, true).unwrap();
        let exec = {
            let mut templates = HashMap::new();
            for row in eng.query_sql("SELECT kind, template FROM rel_effect_cmd", &[]).unwrap() {
                templates.insert(row[0].as_str().unwrap().to_string(), row[1].as_str().unwrap().to_string());
            }
            ShellEffectExec { templates, n_out: async_effect_arity(&prog), cwd: eng.root() }
        };
        let n = eng.drain_effects(&prog, &exec).unwrap();
        eprintln!("cycle {i}: drained {n} | resp={:?} | etag={:?}",
            rows(&dbp, "SELECT status, substr(tag,1,12) FROM rel_resp"),
            rows(&dbp, "SELECT substr(tag,1,12) FROM rel_etag"));
    }
    eng.tick(&prog, true).unwrap();
    eprintln!("stars={:?} full_name={:?}",
        rows(&dbp, "SELECT n FROM rel_stars"),
        rows(&dbp, "SELECT name FROM rel_full_name"));

    assert!(!rows(&dbp, "SELECT n FROM rel_stars").is_empty(), "live body normalized into stars");
    let statuses: Vec<String> = rows(&dbp, "SELECT status FROM rel_resp").into_iter().flatten().collect();
    assert!(statuses.contains(&"200".to_string()), "first poll was a live 200: {statuses:?}");
    assert!(statuses.contains(&"304".to_string()), "carried-etag re-poll got a live 304: {statuses:?}");
    clear_now();
}
