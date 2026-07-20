//! ghcacher, as a datalog program (examples/gh-cache.dl). The conditional-request
//! cache loop end to end, driving the REAL SHIPPED FILE (`include_str!`, not a
//! hand-written mimic of its rule shapes) through a fake GitHub: `fetch`'s `sh`
//! shell template (gh-cache.dl:75-80) is swapped for a script that reads a
//! RECORDED response from `tests/.fixtures/gh-cache/` instead of calling `gh
//! api`. No network, no `gh` CLI, no credentials — the fixture IS the fake
//! GitHub, and the swap is narrow (see `build_prog`/`REAL_FETCH_BLOCK`/
//! `FIXTURE_FETCH_TEMPLATE`) and asserted to have actually applied.
//!
//! Every rule under test — `poll`'s cadence-bucket join, `resp_latest`/
//! `resp_current`'s max-bucket latest-wins reduction, the `etag`/`etag_next`
//! `@next` carry, `stars`/`full_name`/`pull_request`'s jsonp/json extraction,
//! and `change_log`'s append-only `@next` carry — is the shipped file,
//! unmodified. A rule broken there fails these tests; see the mutation-test
//! evidence in the session report for a live demonstration.

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::{async_effect_arity, Engine, ShellEffectExec};
use sprefa_v5::prepare_paths;

use crate::clock_lock::{clear_now, set_now, CLOCK_LOCK};

const PROG: &str = include_str!("../../examples/gh-cache.dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_ghcache_{tag}_{}", std::process::id()));
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

/// The exact `sh fetch(...)` block shipped in examples/gh-cache.dl, copied
/// verbatim from gh-cache.dl:75-80. `build_prog` asserts this substring is
/// still present in `PROG` before replacing it, so a future edit to the
/// shipped file's shell body that changes its shape fails LOUDLY here
/// instead of silently leaving the real `gh api` call in place (which would
/// make every test below hit the network, or just hang/error).
const REAL_FETCH_BLOCK: &str = r#"sh fetch(ep, prev) -> (status: int, tag: text, body: text) =
  `R=$(gh api {ep} -i -H "If-None-Match: $prev" 2>/dev/null)
   C=$(printf '%s' "$R" | head -1 | grep -oE '[0-9]{3}' | head -1)
   E=$(printf '%s' "$R" | grep -iE '^etag:' | head -1 | sed -E 's/^[Ee]tag:[[:space:]]*//; s/\r$//')
   B=$(printf '%s' "$R" | awk 'f{print} /^\r?$/{f=1}' | tr -d '\n')
   printf '%s\n%s\n%s' "$C" "$E" "$B"`."#;

/// Fixture-backed replacement for `REAL_FETCH_BLOCK`. Same `sh` signature
/// (params `ep`/`prev`, outs `status`/`tag`/`body`), so every downstream rule
/// (`resp`, `resp_current`, `stars`, ...) is untouched — only WHERE the
/// response comes from changes. `{ep}` is dl's raw-hole substitution (same
/// convention the real template uses); `$prev` is read as an env var (dl's
/// metachar-safe form — an etag can carry a `W/"..."` that would corrupt a
/// raw `{prev}` inside a quoted span). `__FIXTURE_DIR__`/`__CALL_LOG__` are
/// filled by `build_prog` with absolute paths (plain Rust string
/// substitution, not a dl hole — neither is a request arg).
///
/// Every invocation — hit or miss — appends exactly one line to
/// `__CALL_LOG__`: `<ep>\t<key>` on a fixture hit, `MISS\t<ep>\t<key>` when
/// no fixture answers this (ep, prev) pair. This is the effect-invocation
/// counter the test module asserts on: it counts what the shell script
/// ACTUALLY ran, which `effect_log` (a projection of `pending_effect`,
/// deduped by request digest) cannot substitute for — a requeued/re-executed
/// request collapses to one `effect_log` row regardless of how many times the
/// underlying process actually ran (see `orphaned_effect_requeues_and_runs...`
/// in temporal_async.rs for exactly that requeue-then-rerun shape). Counting
/// at the point of execution is the only way to see an N+1 fan-out directly.
///
/// A missing fixture used to answer a bare 404 with NO record anywhere,
/// degrading an unanticipated extra poll into "just a clean cache miss" —
/// the defect this rewrite exists to fix. Now it still answers 404 (so the
/// effect pipeline stays well-formed) but ALSO writes the MISS line, and
/// every scenario below asserts the MISS count is exactly zero — an
/// unanticipated poll fails the test loudly and names the (ep, key) pair
/// instead of vanishing into "no new 200".
///
/// Fixture lookup: `${EP_SAFE}.${KEY}.resp` where `EP_SAFE` is the endpoint
/// with `/` replaced by `_`, and `KEY` is `cold` for an empty `prev` or the
/// (already-alphanumeric, in every fixture here) etag otherwise.
///
/// BODY read mirrors `REAL_FETCH_BLOCK`'s own transform exactly: everything
/// from line 3 to EOF, newlines stripped (`sed -n '3,$p' | tr -d '\n'` here;
/// `awk 'f{print}...' | tr -d '\n'` there), not just a bare line-3 read —
/// see `embedded_newline_in_body_is_silently_joined_a_known_gap` below for
/// why this matters: `tr -d '\n'` is a KNOWN, UNFIXED gap in
/// examples/gh-cache.dl:75-80 (silently joins a body that spans real
/// newlines instead of failing loudly), and this harness reproduces the same
/// transform on purpose rather than papering over it with a stricter read.
///
/// COVERAGE GAP this harness does NOT close: CRLF-terminated HTTP header
/// lines (the `s/\r$//` in `REAL_FETCH_BLOCK`'s etag `sed`, and the
/// `/^\r?$/` blank-line detector that splits headers from body) are never
/// exercised here, because the fixture format is pre-split (status/etag/body
/// each already on their own line) — it never feeds raw `-i` HTTP response
/// text through the awk/sed header-splitting step at all. Only the LIVE test
/// (`gh_cache_live_against_github`, `--ignored`, real `gh` + network)
/// exercises that path.
const FIXTURE_FETCH_TEMPLATE: &str = r#"sh fetch(ep, prev) -> (status: int, tag: text, body: text) =
  `EP_SAFE=$(printf '%s' '{ep}' | tr '/' '_')
   if [ -z "$prev" ]; then KEY=cold; else KEY=$(printf '%s' "$prev" | tr -c 'A-Za-z0-9' '_'); fi
   F="__FIXTURE_DIR__/${EP_SAFE}.${KEY}.resp"
   if [ -f "$F" ]; then
     printf '%s\t%s\n' '{ep}' "$KEY" >> "__CALL_LOG__"
     STATUS=$(sed -n '1p' "$F")
     ETAG=$(sed -n '2p' "$F")
     BODY=$(sed -n '3,$p' "$F" | tr -d '\n')
   else
     printf 'MISS\t%s\t%s\n' '{ep}' "$KEY" >> "__CALL_LOG__"
     STATUS=404; ETAG=""; BODY=""
   fi
   printf '%s\n%s\n%s' "$STATUS" "$ETAG" "$BODY"`."#;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/.fixtures/gh-cache")
}

/// Path to the sandbox's call-invocation log (one line per `fetch` shell
/// invocation, written by `FIXTURE_FETCH_TEMPLATE`). Lives inside the test's
/// own sandbox dir, not the committed fixture dir.
fn call_log_path(dir: &Path) -> PathBuf {
    dir.join("calls.log")
}

/// All lines recorded so far at a given call-log FILE path (not a sandbox
/// dir — see `call_log_lines` for the dir-relative convenience form). Each
/// line is either `<ep>\t<key>` (a fixture hit) or `MISS\t<ep>\t<key>` (no
/// fixture for that pair). Returns empty (not an error) before the first call.
fn read_call_log(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|line| line.to_string())
        .collect()
}

/// Convenience over `read_call_log` for callers holding the sandbox DIR
/// rather than the log file path directly.
fn call_log_lines(dir: &Path) -> Vec<String> {
    read_call_log(&call_log_path(dir))
}

/// Count of successful (non-MISS) invocations of `fetch` for one endpoint,
/// across the whole run so far.
fn call_count(dir: &Path, ep: &str) -> usize {
    call_log_lines(dir)
        .iter()
        .filter(|line| line.split('\t').next() == Some(ep))
        .count()
}

/// Count of MISS lines (an (ep, prev) pair with no committed fixture) across
/// every endpoint. Every scenario below asserts this is exactly 0 — a
/// nonzero count means an unanticipated poll happened and names it.
fn miss_count(dir: &Path) -> usize {
    call_log_lines(dir).iter().filter(|line| line.starts_with("MISS\t")).count()
}

/// Build the real gh-cache.dl program, transformed ONLY at the `fetch` shell
/// effect (the real `gh api` call -> a fixture read) plus zero or more extra
/// `watch(...)` facts appended for a scenario that needs a second endpoint
/// (data, not a rule change — every rule still comes from the shipped file).
/// Asserts the swap actually took: the real `gh api` substring is gone and
/// the fixture path is present. This is the ONLY divergence from the shipped
/// file anywhere in this test module.
fn build_prog(extra_watch: &[&str], call_log: &Path) -> String {
    assert!(
        PROG.contains(REAL_FETCH_BLOCK),
        "examples/gh-cache.dl's `fetch` sh block no longer matches the copy in \
         tests/it/gh_cache.rs::REAL_FETCH_BLOCK — the shipped file's shell body \
         changed shape; update the constant before trusting this suite again"
    );
    let dir = fixture_dir();
    let script = FIXTURE_FETCH_TEMPLATE
        .replace("__FIXTURE_DIR__", dir.to_str().unwrap())
        .replace("__CALL_LOG__", call_log.to_str().unwrap());
    let mut prog = PROG.replacen(REAL_FETCH_BLOCK, &script, 1);
    // Check for the exact `sh fetch` block (not a bare "gh api {ep}" substring
    // search) — the file's own doc comment near the top mentions `gh api {ep}`
    // as prose, which a loose substring check would false-positive on.
    assert!(
        !prog.contains(REAL_FETCH_BLOCK),
        "fetch override did not remove the real `sh fetch` block — the \
         transformation did not apply, the test would silently hit the network"
    );
    assert!(
        prog.contains(dir.to_str().unwrap()),
        "fetch override did not install the fixture-reading script"
    );
    for ep in extra_watch {
        prog.push_str(&format!("watch({ep:?}).\n"));
    }
    prog
}

/// Build a `ShellEffectExec` from the program's own `sh` decl registry (the
/// SAME mechanism the daemon uses — see `sh_decl_supplies_effect_template` in
/// temporal_async.rs), not a hand-rolled Rust mock of `fetch`'s behavior. The
/// fixture directory is an absolute path baked into the template by
/// `build_prog`, so `cwd` does not matter here.
fn shell_exec(prog: &sprefa_v5::ast::Program) -> ShellEffectExec {
    ShellEffectExec {
        templates: sprefa_v5::engine::shell_templates(prog),
        n_out: async_effect_arity(prog),
        cwd: PathBuf::new(),
    }
}

/// Drive 8 cycles of (advance one 300s cadence bucket, tick, drain), then one
/// final settle tick. `clock(300, b)` (gh-cache.dl:65-66) is what makes a
/// carried-etag poll re-fire at all, so every scenario that needs a SECOND
/// request (a revalidate, or a changed body) must cross a bucket boundary,
/// not just re-tick at the same injected time. 8 cycles is the same margin
/// already proven (in this file's prior draft) to reliably walk a changing
/// resource through cold -> etagA -> etagB -> stable-304. The final tick
/// materializes the LAST cycle's `@next` carry (`etag`, `change_log`), which
/// is not visible until the tick after the one that computed it. Never
/// insert a second tick between a cycle's own tick and its own drain — that
/// would let a mid-bucket etag update derive a SECOND, different-prev poll
/// row before the drain runs, corrupting the per-bucket call count this test
/// module exists to assert on exactly.
///
/// Returns the cumulative call-log line count observed after EACH of the 8
/// drains (`ret[i]` = total invocations, hit or miss, once bucket `i` has
/// been drained), so callers can assert exact per-bucket deltas instead of
/// only a final total. The trailing settle tick never drains, so it can
/// never itself add a call.
fn settle(
    eng: &mut Engine,
    prog: &sprefa_v5::ast::Program,
    exec: &ShellEffectExec,
    base_secs: i64,
    call_log: &Path,
) -> Vec<usize> {
    let mut totals_after_bucket = Vec::with_capacity(8);
    for i in 0..8i64 {
        set_now(base_secs + i * 300);
        eng.tick(prog, true).unwrap();
        eng.drain_effects(prog, exec).unwrap();
        totals_after_bucket.push(read_call_log(call_log).len());
    }
    eng.tick(prog, true).unwrap();
    totals_after_bucket
}

/// One more (tick, drain) cycle at the SAME injected `now` as whatever bucket
/// the caller last settled at (no `set_now` call here) — the retraction/
/// steady-state property: once an endpoint's current (ep, prev) pair has
/// already resolved a request (queued + drained to `done`), re-deriving
/// `poll` at the identical bucket must not issue a duplicate fetch. Asserts
/// the call-log length is unchanged; on failure names exactly how many new
/// (unexpected) invocations happened.
fn assert_steady_state(
    eng: &mut Engine,
    prog: &sprefa_v5::ast::Program,
    exec: &ShellEffectExec,
    call_log: &Path,
    label: &str,
) {
    let before = read_call_log(call_log);
    eng.tick(prog, true).unwrap();
    eng.drain_effects(prog, exec).unwrap();
    let after = read_call_log(call_log);
    assert_eq!(
        after.len(),
        before.len(),
        "{label}: an extra tick+drain in the SAME clock bucket issued {} new \
         fetch(es) — steady state must issue zero. before={before:?} after={after:?}",
        after.len().saturating_sub(before.len())
    );
}

/// Scenario 1 — cold fetch: no prior etag, the fixture answers 200 + a body +
/// a fresh etag (`tests/.fixtures/gh-cache/repos_cli_cli.cold.resp`).
/// `stars`/`full_name` land via the REAL jsonp rules (gh-cache.dl:109-113),
/// and the etag carries forward via the REAL `@next` chain (:103-104).
#[test]
fn cold_fetch_lands_entities_and_carries_etag() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = sandbox("cold");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    let totals = settle(&mut eng, &prog, &exec, 2_000_000, &call_log);

    // Bucket 0 is the ONLY bucket a cold poll (no prior etag) can happen in —
    // exactly 1 fetch, not 0 (never fetched) and not >1 (an N+1 fan-out would
    // show here first, since this is the smallest possible scenario: one
    // endpoint, one row).
    assert_eq!(totals[0], 1, "bucket 0 must fetch exactly once: {totals:?}");
    // Once the etag carries (visible 2 ticks after the response lands: one
    // ordinary tick for resp_current/etag_next, one more for the `@next`
    // carry into etag itself — see `settle`'s doc comment), exactly ONE
    // steady-state revalidate confirms the 304 and the request digest
    // (head_rel, kind, {ep, prev}) never changes again — no further bucket
    // boundary re-fires it, however many buckets pass. Total across the
    // whole 8-bucket settle is exactly 2, never climbing further.
    assert_eq!(totals[7], 2, "settled total must stay at cold+one-steady-304, no drift: {totals:?}");
    assert_eq!(
        call_count(&d, "repos/cli/cli"),
        2,
        "exactly 2 total invocations for the one watched, never-again-changing \
         endpoint across all 8 settled buckets: the cold 200 plus the one \
         confirming 304 — never a re-fetch per bucket"
    );
    assert_eq!(miss_count(&d), 0, "no unanticipated (ep, prev) poll should ever miss a fixture");

    assert_eq!(
        rows(&dbp, "rel_stars_txt", "SELECT n FROM rel_stars_txt WHERE ep = 'repos/cli/cli'"),
        vec![vec!["42".to_string()]],
        "cold 200's body normalizes into stars via the real jsonp rule"
    );
    assert_eq!(
        rows(&dbp, "rel_full_name_txt", "SELECT name FROM rel_full_name_txt WHERE ep = 'repos/cli/cli'"),
        vec![vec!["cli/cli".to_string()]],
        "cold 200's body normalizes into full_name via the real jsonp rule"
    );
    assert_eq!(
        rows(&dbp, "rel_etag_txt", "SELECT tag FROM rel_etag_txt WHERE ep = 'repos/cli/cli'"),
        vec![vec!["etagA".to_string()]],
        "the fresh etag carried into `etag` via the real @next chain"
    );

    // Retraction/steady-state (item 4): more ticks in the SAME bucket must
    // never issue another fetch.
    assert_steady_state(&mut eng, &prog, &exec, &call_log, "cold_fetch");

    clear_now();
}

/// Scenario 2 — warm revalidate: once an etag is carried, the fixture answers
/// 304 with an empty body (`repos_cli_cli.etagA.resp`). This is the actual
/// cache-hit claim gh-cache.dl exists to make: `resp_current` only reads
/// status=200 rows (gh-cache.dl:101-102), so a 304 lands no new entity row,
/// and `stars`/`full_name`/`change_log` stay at exactly one row each across
/// repeated revalidates — no duplicate accumulates.
#[test]
fn warm_revalidate_is_a_free_cache_hit_with_no_duplicate_rows() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = sandbox("warm");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    let totals = settle(&mut eng, &prog, &exec, 3_000_000, &call_log);

    // Bucket 0: exactly one cold fetch. The revalidate (a NEW request keyed
    // on the now-carried etag) becomes derivable at bucket 2 — one ordinary
    // tick materializes resp_current/etag_next, one more materializes the
    // `@next` carry into etag itself, and only THEN does `poll` re-derive
    // with the new prev — never at bucket 1, and never a THIRD fetch at any
    // later bucket once the 304 confirms steady state.
    assert_eq!(totals[0], 1, "bucket 0: exactly 1 cold fetch: {totals:?}");
    assert_eq!(totals[1], 1, "bucket 1: the etag carry has not materialized yet, zero new fetches: {totals:?}");
    assert_eq!(totals[2], 2, "bucket 2: exactly 1 more fetch (the revalidate), not 2, not 0: {totals:?}");
    assert_eq!(totals[7], 2, "no further bucket re-fires an already-resolved (ep, prev) pair: {totals:?}");
    assert_eq!(call_count(&d, "repos/cli/cli"), 2, "exactly 2 total invocations: cold + one revalidate");
    assert_eq!(miss_count(&d), 0, "no unanticipated (ep, prev) poll should ever miss a fixture");

    // A revalidate actually happened: the executor served both the cold 200
    // and a later 304 across the settle cycles (proves the fixture-driven
    // etag carry re-fired the poll, not just that nothing ran).
    let statuses: Vec<String> = rows(
        &dbp,
        "rel_resp_txt",
        "SELECT DISTINCT status FROM rel_resp_txt WHERE ep = 'repos/cli/cli'",
    )
    .into_iter()
    .flatten()
    .collect();
    assert!(
        statuses.contains(&"200".to_string()) && statuses.contains(&"304".to_string()),
        "expected both a cold 200 and a revalidate 304 in the accumulated resp history: {statuses:?}"
    );

    assert_eq!(
        rows(&dbp, "rel_stars_txt", "SELECT n FROM rel_stars_txt WHERE ep = 'repos/cli/cli'"),
        vec![vec!["42".to_string()]],
        "304s must not duplicate or blank out stars"
    );
    assert_eq!(
        rows(&dbp, "rel_full_name_txt", "SELECT name FROM rel_full_name_txt WHERE ep = 'repos/cli/cli'"),
        vec![vec!["cli/cli".to_string()]],
        "304s must not duplicate or blank out full_name"
    );
    assert_eq!(
        rows(
            &dbp,
            "rel_change_log_txt",
            "SELECT kind, val FROM rel_change_log_txt WHERE ep = 'repos/cli/cli' ORDER BY kind"
        ),
        vec![
            vec!["full_name".to_string(), "cli/cli".to_string()],
            vec!["stars".to_string(), "42".to_string()],
        ],
        "change_log holds exactly one row per kind — no duplicates from the repeated 304s"
    );

    assert_steady_state(&mut eng, &prog, &exec, &call_log, "warm_revalidate");

    clear_now();
}

/// Scenario 3 — body change: a resource whose stargazers_count actually
/// changes lands a SECOND 200 with a different etag
/// (`repos_octocat_helloworld.cold.resp` -> `.etagA.resp`, 10 -> 20). The
/// append-only `change_log_next <- change_log` carry (gh-cache.dl:134) means
/// `change_log` gains the new value while KEEPING the old one; the live view
/// (`stars`, over `resp_current`) reflects only the newest body.
#[test]
fn body_change_appends_to_change_log_and_keeps_old_value() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let ep = "repos/octocat/helloworld";
    let d = sandbox("bodychange");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[ep], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    let totals = settle(&mut eng, &prog, &exec, 4_000_000, &call_log);

    // Two watched endpoints this tick (the hard-coded `watch("repos/cli/cli")`
    // base fact PLUS this scenario's extra `ep`): bucket 0 fetches both cold
    // (delta 2); bucket 2 fetches both revalidates — cli/cli's 304 steady
    // state and octocat's SECOND 200 (delta 2); bucket 4 fetches octocat's
    // THIRD request, the 304 that confirms steady state on the new etag
    // (delta 1, cli/cli has nothing left to fetch — its 304 already resolved
    // at bucket 2). No bucket after 4 adds anything.
    assert_eq!(totals[0], 2, "bucket 0: both endpoints cold, exactly 2: {totals:?}");
    assert_eq!(totals[2], 4, "bucket 2: both endpoints' first revalidate, exactly 2 more: {totals:?}");
    assert_eq!(totals[4], 5, "bucket 4: octocat's steady-state 304 confirmation, exactly 1 more: {totals:?}");
    assert_eq!(totals[7], 5, "settled total holds at 5, no further drift: {totals:?}");
    assert_eq!(
        call_count(&d, ep),
        3,
        "exactly 3 invocations for the CHANGING endpoint: cold(10) + a SECOND \
         200(20) + the 304 that confirms it stopped changing — never a 4th"
    );
    assert_eq!(
        call_count(&d, "repos/cli/cli"),
        2,
        "exactly 2 invocations for the UNCHANGED base-fact endpoint: cold + one steady 304"
    );
    assert_eq!(miss_count(&d), 0, "no unanticipated (ep, prev) poll should ever miss a fixture");

    let mut stars_log: Vec<String> = rows(
        &dbp,
        "rel_change_log_txt",
        &format!("SELECT val FROM rel_change_log_txt WHERE ep = '{ep}' AND kind = 'stars'"),
    )
    .into_iter()
    .flatten()
    .collect();
    stars_log.sort();
    assert_eq!(
        stars_log,
        vec!["10".to_string(), "20".to_string()],
        "change_log keeps the old stars value (10) AND the new one (20), append-only"
    );

    assert_eq!(
        rows(&dbp, "rel_stars_txt", &format!("SELECT n FROM rel_stars_txt WHERE ep = '{ep}'")),
        vec![vec!["20".to_string()]],
        "the live view reflects only the NEWEST body (latest-wins over resp_current)"
    );

    assert_steady_state(&mut eng, &prog, &exec, &call_log, "body_change");

    clear_now();
}

/// Scenario 4 — list endpoint: a `pull_request` array response through the
/// REAL rule at gh-cache.dl:120-124 (one `json` brace pattern, sibling +
/// nested fields correlated), driven by the real fetch effect — not a
/// hand-fed `resp` fact standing in for a fetch.
#[test]
fn list_endpoint_normalizes_pull_requests_via_the_real_fetch() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let ep = "repos/cli/cli/pulls";
    let d = sandbox("list");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[ep], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    let totals = settle(&mut eng, &prog, &exec, 5_000_000, &call_log);

    // THE assertion this scenario exists for: the list fixture answers ONE
    // response body containing 2 PR elements, and the fetch fires ONCE for
    // the whole endpoint (bucket 0) — NOT once per pull_request row. Bucket 0
    // has 2 total invocations, one per WATCHED ENDPOINT (the hard-coded base
    // `watch("repos/cli/cli")` plus this scenario's list endpoint), never one
    // per pull_request ROW — a per-row fan-out would need a 3rd fixture hit
    // in the SAME bucket, which the exact-2 assertion rules out directly. If
    // the shell effect were ever driven per-row (the N+1 shape the mutation
    // test below reproduces on purpose), this scenario's fixture chain would
    // also 404-MISS immediately (there is no per-row fixture file), so
    // miss_count catches that shape too, independent of the exact-count check.
    assert_eq!(totals[0], 2, "bucket 0: exactly 2 fetches (one per endpoint, not one per PR row): {totals:?}");
    assert_eq!(totals[2], 4, "bucket 2: exactly 2 more (steady-state 304 confirms, one per endpoint): {totals:?}");
    assert_eq!(totals[7], 4, "no further drift: {totals:?}");
    assert_eq!(
        call_count(&d, ep),
        2,
        "exactly 2 invocations for the list endpoint across the whole settle \
         (cold + one steady 304) — NEVER one per pull_request row (2 rows landed, \
         not 2 extra fetches)"
    );
    assert_eq!(miss_count(&d), 0, "no unanticipated (ep, prev) poll should ever miss a fixture");

    let mut got = rows(
        &dbp,
        "rel_pull_request_txt",
        &format!("SELECT num, title, state, author FROM rel_pull_request_txt WHERE ep = '{ep}' ORDER BY num"),
    );
    got.sort();
    assert_eq!(
        got,
        vec![
            vec!["1".to_string(), "fix bug".to_string(), "open".to_string(), "alice".to_string()],
            vec!["2".to_string(), "add feat".to_string(), "closed".to_string(), "bob".to_string()],
        ],
        "the array fixture normalizes into one pull_request row per element, via the real fetch"
    );

    assert_steady_state(&mut eng, &prog, &exec, &call_log, "list_endpoint");

    clear_now();
}

/// Scenario 5 — `resp_latest`/`resp_current` latest-wins: two 200s land in
/// different clock buckets (etagA=100 then etagB=200,
/// `repos_example_latestwins.{cold,etagA}.resp`). `resp` (the accumulated
/// history, gh-cache.dl:99-100) keeps both; `resp_current` (the max-bucket
/// reduction, :101-102) collapses to exactly ONE row — the newer.
#[test]
fn resp_current_picks_the_newer_of_two_200s_across_clock_buckets() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let ep = "repos/example/latestwins";
    let d = sandbox("latest");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[ep], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    let totals = settle(&mut eng, &prog, &exec, 6_000_000, &call_log);

    // Same 2-endpoint shape as scenario 3 (the base-fact cli/cli PLUS this
    // scenario's `ep`), and `ep` here has the SAME 3-fixture chain shape as
    // octocat did (cold -> 200 etagA -> 200 etagB -> 304 steady): bucket 0
    // both cold (delta 2); bucket 2 both first revalidates — cli/cli's 304
    // steady state and this ep's SECOND 200 landing etagB (delta 2); bucket 4
    // this ep's 304 confirming etagB is steady (delta 1). Total 5, same
    // arithmetic as body_change_appends_to_change_log_and_keeps_old_value.
    assert_eq!(totals[0], 2, "bucket 0: both endpoints cold: {totals:?}");
    assert_eq!(totals[2], 4, "bucket 2: both endpoints' first revalidate: {totals:?}");
    assert_eq!(totals[4], 5, "bucket 4: this ep's steady-state 304 confirmation: {totals:?}");
    assert_eq!(totals[7], 5, "no further drift: {totals:?}");
    assert_eq!(
        call_count(&d, ep),
        3,
        "exactly 3 invocations for the latest-wins endpoint: the two 200 \
         LANDINGS (etagA, etagB) the row assertions below check, PLUS the one \
         304 that confirms etagB stopped changing — the row-level 'resp \
         accumulates BOTH 200 versions' claim is about LANDED ROWS (2), not \
         total invocations (3)"
    );
    assert_eq!(call_count(&d, "repos/cli/cli"), 2, "unchanged base-fact endpoint: cold + one steady 304");
    assert_eq!(miss_count(&d), 0, "no unanticipated (ep, prev) poll should ever miss a fixture");

    let mut tags: Vec<String> = rows(
        &dbp,
        "rel_resp_txt",
        &format!("SELECT DISTINCT tag FROM rel_resp_txt WHERE ep = '{ep}' AND status = 200"),
    )
    .into_iter()
    .flatten()
    .collect();
    tags.sort();
    assert_eq!(
        tags,
        vec!["etagA".to_string(), "etagB".to_string()],
        "resp accumulates BOTH 200 versions (the history)"
    );

    assert_eq!(
        rows(&dbp, "rel_resp_current_txt", &format!("SELECT tag FROM rel_resp_current_txt WHERE ep = '{ep}'")),
        vec![vec!["etagB".to_string()]],
        "resp_current collapses to exactly one row: the newer version"
    );
    assert_eq!(
        rows(&dbp, "rel_stars_txt", &format!("SELECT n FROM rel_stars_txt WHERE ep = '{ep}'")),
        vec![vec!["200".to_string()]],
        "stars reflects only the latest-wins body"
    );

    assert_steady_state(&mut eng, &prog, &exec, &call_log, "latest_wins");

    clear_now();
}

/// KNOWN GAP (not fixed here — examples/gh-cache.dl is out of scope for this
/// arc): `REAL_FETCH_BLOCK` (gh-cache.dl:75-80) reads the body with
/// `awk 'f{print} /^\r?$/{f=1}' | tr -d '\n'` — every real newline byte in
/// the body is unconditionally DELETED, not escaped or rejected. A GitHub
/// JSON response never legitimately contains a raw newline (JSON escapes
/// `\n` as two literal chars inside a string), so this never bites against
/// the real API. But if a response body EVER did carry a raw newline byte
/// (a misbehaving proxy, an HTML error page returned instead of JSON, a
/// GraphQL raw-text field), the two lines either side of it get silently
/// JOINED WITH NO SEPARATOR instead of the fetch failing loudly — a value
/// like `"weird/multiline\nname"` becomes `"weird/multilinename"`, silently
/// wrong, not an error anywhere in the pipeline. `FIXTURE_FETCH_TEMPLATE`
/// reproduces the identical transform (see its doc comment) rather than
/// papering over it, so this fixture is a faithful repro, not a test-harness
/// artifact.
#[test]
fn embedded_newline_in_body_is_silently_joined_a_known_gap() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let ep = "repos/weird/multiline";
    let d = sandbox("newline");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[ep], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    settle(&mut eng, &prog, &exec, 7_000_000, &call_log);

    assert_eq!(miss_count(&d), 0, "the fixture exists, this must be a hit not a miss");
    assert_eq!(
        rows(&dbp, "rel_full_name_txt", &format!("SELECT name FROM rel_full_name_txt WHERE ep = '{ep}'")),
        vec![vec!["weird/multilinename".to_string()]],
        "KNOWN GAP: the fixture's real newline in `full_name` (\"weird/multiline\\nname\") \
         is silently deleted and the two halves joined with no separator, not rejected \
         or escaped — this is the CURRENT (buggy) behavior of gh-cache.dl:75-80's \
         `tr -d '\\n'`, asserted here so a future fix to the shipped file is a visible, \
         deliberate, reviewed change to this test rather than a silent behavior flip"
    );
    clear_now();
}

/// KNOWN GAP (not fixed here): `REAL_FETCH_BLOCK`'s `gh api {ep} -i ...` call
/// (gh-cache.dl:75-80) never passes `--paginate`. GitHub REST list endpoints
/// (e.g. `repos/{owner}/{repo}/pulls`) page at 30 items by default (up to 100
/// with `per_page`), so any watched list endpoint with more open items than
/// one page silently returns ONLY page 1 forever — no error, no signal that
/// further pages exist, and `pull_request`'s `json(...)` array match has no
/// way to know rows are missing. A static canary, not a runtime repro
/// (faithfully simulating `gh api --paginate`'s multi-block concatenated `-i`
/// output through this fixture harness is out of scope for this arc): fails
/// LOUDLY, naming the exact gap, the moment someone adds `--paginate` to the
/// shipped block without updating this assertion.
#[test]
fn fetch_never_requests_pagination_a_known_gap() {
    assert!(
        !REAL_FETCH_BLOCK.contains("--paginate") && !REAL_FETCH_BLOCK.contains("per_page"),
        "gh-cache.dl's fetch block now requests pagination — update this canary's \
         doc comment (the gap it names is fixed) rather than deleting the test"
    );
}

/// Item 2 (retraction) — the highest-priority gap check: an entity that
/// DISAPPEARS from a list response must disappear from `pull_request` too.
/// `resp` only ever ACCUMULATES rows (`resp(ep, b, status, tag, body) <-
/// @async ...`, no retraction, gh-cache.dl:89-90), but `pull_request` is a
/// PLAIN derived rule over `resp_current` (gh-cache.dl:120-124) — not `@next`
/// carried, not accumulated — and `resp_current` itself is a max-bucket
/// reduction that holds exactly the latest body (gh-cache.dl:99-102). A plain
/// derived rule fully rebuilds from its current inputs every tick (no
/// incremental accumulation), so shrinking the LATEST body should retract
/// the dropped row for free. This test proves that prediction against the
/// real engine rather than asserting it from reading the rule shapes:
/// fixture chain PRs [1,2,3] (cold) -> PRs [1,3] (etagA, #2 dropped).
#[test]
fn pull_request_retracts_removed_rows_when_the_list_shrinks() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let ep = "repos/cli/cli/shrinklist";
    let d = sandbox("shrink");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[ep], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    settle(&mut eng, &prog, &exec, 8_000_000, &call_log);

    assert_eq!(miss_count(&d), 0, "both fixtures in the chain exist, this must never miss");

    let mut nums: Vec<String> = rows(
        &dbp,
        "rel_pull_request_txt",
        &format!("SELECT num FROM rel_pull_request_txt WHERE ep = '{ep}' ORDER BY num"),
    )
    .into_iter()
    .flatten()
    .collect();
    nums.sort();
    assert_eq!(
        nums,
        vec!["1".to_string(), "3".to_string()],
        "PR #2 was in the cold response and MUST be gone once the newer body \
         (etagA) no longer lists it — a lingering #2 here would be a real \
         retraction defect in gh-cache.dl, not a test-harness artifact, since \
         pull_request is a plain derived rule over resp_current (not @next- \
         carried, not accumulated)"
    );
}

/// Items 3 + 6 + 7 — cadence survival + change_log boundedness + insert-or-
/// ignore idempotency, all in one endpoint's long tail. A 304 must not
/// disturb the polling interval (no extra poll, no bucket reset): the 5
/// scenarios above already show this over the 8-bucket settle (`totals[2..7]`
/// all flat once steady), but this test pushes FAR past that margin —
/// 20 MORE distinct bucket boundaries after settle, an order of magnitude
/// more cadence flips than any scenario above exercises — and asserts ZERO
/// new fetches across every one of them. In the same pass: `change_log`
/// (append-only per gh-cache.dl:132-137) must NOT grow without bound as
/// identical content gets re-polled hundreds of times — dedup is structural
/// (a relation is a SET; the same (ep, kind, val) tuple re-deriving is a
/// no-op), so its row count must be pinned at exactly 2 (one per kind) for
/// the entire tail. That pinned count IS the insert-or-ignore idempotency
/// property item 7 asks for, proven here at 20x the repetition instead of
/// the single revalidate `warm_revalidate_is_a_free_cache_hit_with_no_duplicate_rows`
/// exercises.
#[test]
fn cadence_survives_many_buckets_past_steady_state_with_bounded_change_log() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = sandbox("cadence");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);
    fs::write(d.join("p.dl"), build_prog(&[], &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    settle(&mut eng, &prog, &exec, 9_000_000, &call_log);
    assert_eq!(call_count(&d, "repos/cli/cli"), 2, "settled at cold + one steady 304, as in cold_fetch");
    assert_eq!(miss_count(&d), 0);

    let change_log_row_count = || {
        rows(&dbp, "rel_change_log_txt", "SELECT COUNT(*) FROM rel_change_log_txt WHERE ep = 'repos/cli/cli'")
    };
    assert_eq!(change_log_row_count(), vec![vec!["2".to_string()]], "one row per kind (stars, full_name) after settle");

    for bucket in 0..20i64 {
        set_now(9_100_000 + bucket * 300);
        eng.tick(&prog, true).unwrap();
        eng.drain_effects(&prog, &exec).unwrap();
        assert_eq!(
            call_count(&d, "repos/cli/cli"),
            2,
            "bucket +{bucket} past settle: a 304 must not reset cadence or \
             trigger a re-fetch — the request digest (head_rel, kind, {{ep, \
             prev}}) already resolved and must stay resolved forever"
        );
        assert_eq!(
            change_log_row_count(),
            vec![vec!["2".to_string()]],
            "bucket +{bucket}: change_log must stay pinned at 2 rows — insert-or-ignore \
             idempotency, not unbounded growth from re-polling identical content"
        );
    }
    assert_eq!(miss_count(&d), 0, "20 extra buckets, still zero misses");

    clear_now();
}

/// Stress scenario (item 3): N=25 watched endpoints, each with the same
/// cold-then-steady-304 fixture chain shape as `cold_fetch_lands_entities...`
/// (`tests/.fixtures/gh-cache/repos_stress_ep{00..24}.{cold,tagNN}.resp`,
/// generated by a one-off bash loop — not hand-typed — then committed; see
/// the generator comment below). Proves the counting mechanism scales: fetch
/// count stays linear in ENDPOINTS, never in endpoints^2 or in
/// rows-per-endpoint, and the per-tick DB write count (the write ledger,
/// `tests/it/write_ledger.rs` precedent) never fans out either.
///
/// Fixture generator (recorded for reproducibility, not re-run by the test):
/// ```sh
/// cd tests/.fixtures/gh-cache
/// for i in $(seq -w 0 24); do
///   ep="repos_stress_ep${i}"; tag="tag${i}"
///   printf '200\n%s\n{"stargazers_count": %d, "full_name": "stress/ep%s"}' \
///     "$tag" "$((10#$i))" "$i" > "${ep}.cold.resp"
///   printf '304\n\n' > "${ep}.${tag}.resp"
/// done
/// ```
#[test]
fn stress_25_endpoints_fetch_linearly_not_quadratically() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    const N: usize = 25;
    let d = sandbox("stress25");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);

    let endpoints: Vec<String> = (0..N).map(|i| format!("repos/stress/ep{i:02}")).collect();
    let endpoint_refs: Vec<&str> = endpoints.iter().map(String::as_str).collect();
    fs::write(d.join("p.dl"), build_prog(&endpoint_refs, &call_log)).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    // watch("repos/cli/cli") (the shipped file's own base fact) is ALSO
    // in-scope every test, so the true endpoint count is N+1 — the harness
    // never pretends it isn't there (see `call_count(&d, "repos/cli/cli")`
    // below, asserted like every other endpoint, not ignored).
    let total_endpoints = N + 1;

    let totals = settle(&mut eng, &prog, &exec, 10_000_000, &call_log);

    // Fetches per bucket == total_endpoints EXACTLY, both at cold (bucket 0)
    // and at the steady-state revalidate (bucket 2, same 2-tick @next lag as
    // every scenario above) — never total_endpoints^2 (676) and never
    // total_endpoints * rows-per-endpoint (each stress body has exactly 1
    // field, so that distinction doesn't collapse into the endpoint count by
    // accident here).
    assert_eq!(
        totals[0], total_endpoints,
        "bucket 0: exactly {total_endpoints} fetches (one per watched endpoint), \
         not {} (endpoints^2): {totals:?}", total_endpoints * total_endpoints
    );
    assert_eq!(
        totals[2], total_endpoints * 2,
        "bucket 2: exactly {total_endpoints} MORE (one steady-304 confirm per \
         endpoint): {totals:?}"
    );
    assert_eq!(totals[7], total_endpoints * 2, "no drift once every endpoint is steady: {totals:?}");
    // total fetches across the whole settle == total_endpoints * 2 (cold +
    // one steady confirm per endpoint) — NOT total_endpoints * buckets (8):
    // gh-cache.dl's actual request digest is (head_rel, kind, {ep, prev})
    // with no bucket salt (see the `warm_revalidate`/`latest_wins` scenario
    // comments for the full mechanics), so once an (ep, prev) pair resolves
    // it never re-fires regardless of how many MORE buckets pass — proven at
    // scale by `cadence_survives_many_buckets_past_steady_state...` and by
    // this test's own steady-state check below.
    assert_eq!(call_log_lines(&d).len(), total_endpoints * 2, "grand total across the settle");

    for ep in endpoints.iter().chain(std::iter::once(&"repos/cli/cli".to_string())) {
        assert_eq!(call_count(&d, ep), 2, "endpoint {ep}: exactly 2 invocations (cold + one steady 304)");
    }
    assert_eq!(miss_count(&d), 0, "no unanticipated (ep, prev) poll across {total_endpoints} endpoints");

    // The per-tick write count does not scale with rows the way an N+1
    // would. Uses the existing write-ledger mechanism (_write_ledger, same
    // table `tests/it/write_ledger.rs` reads), not a bespoke counter — but
    // note WHICH rel: `resp` itself is written by `drain_effects` OFF-tick
    // (never inside `tick()`, by design — see effect.rs's own doc comment on
    // `drain_effects`), and `_write_ledger` is flushed only from inside
    // `tick()` (`flush_write_ledger`, engine/mod.rs), so `resp`'s writes do
    // not surface under their own name here — a real seam in the ledger's
    // coverage, not something this arc's file ownership can close.
    // `pending_effect` is the closest ledger-visible proxy for the same
    // fact: `rebuild_async` queues it INSIDE `tick()` with exactly one row
    // per NEW (ep, prev) request, so its per-tick row count is exactly the
    // fetch fan-out this test exists to bound.
    let ledger_pending_effect = eng
        .query_sql("SELECT tick, rows FROM _write_ledger WHERE rel = 'pending_effect' AND rows > 0 ORDER BY tick", &[])
        .unwrap();
    assert!(!ledger_pending_effect.is_empty(), "expected at least one write-ledger entry for `pending_effect`");
    for row in &ledger_pending_effect {
        let rows_written = row[1].as_i64().unwrap();
        assert!(
            rows_written <= total_endpoints as i64,
            "a single tick queued {rows_written} pending_effect rows, more than \
             the {total_endpoints} watched endpoints — an N+1 fan-out shape, not \
             N endpoints producing N requests: {ledger_pending_effect:?}"
        );
    }

    // Retraction/steady-state at scale (item 4): more ticks in the SAME
    // bucket, across every one of the 26 endpoints at once, must issue ZERO
    // new fetches.
    assert_steady_state(&mut eng, &prog, &exec, &call_log, "stress_25");

    clear_now();
}

/// MUTATION TEST — the deliverable that proves the counting mechanism can
/// actually DETECT an N+1 explosion, not merely count something. Starts from
/// the SAME fixture-swapped program `cold_fetch_lands_entities_and_carries_etag`
/// drives, then applies exactly ONE further textual transform to `resp`'s
/// rule body: instead of joining `poll(ep, prev, b)` alone (one solution per
/// watched endpoint), it fans through a `pr_shard(ep, shard_ep)` relation —
/// 2 synthetic rows standing in for 2 `pull_request`-style rows — and calls
/// `fetch(shard_ep, prev)` instead of `fetch(ep, prev)`. This is exactly the
/// failure mode named in the brief: the effect driven per ROW instead of per
/// ENDPOINT (one real shape this could be: a poll rule that joins the
/// endpoint's OWN prior pull_request rows to "detail-fetch" each one
/// individually instead of fetching the endpoint once).
///
/// `#[ignore]` for the usual reason every deliberately-broken program in
/// this module is ignored: it exists to be run BY HAND to prove the
/// harness's teeth, not as steady-state CI signal. Run it two ways:
///   1. `cargo test --test it gh_cache::mutation_n1_fan_out_per_row_is_caught -- --ignored --nocapture`
///      — RED, with the wrong count named (this test's own assertions, which
///      mirror `cold_fetch_lands_entities_and_carries_etag`'s exact-count
///      shape, fail and print the actual vs expected numbers).
///   2. Every OTHER test in this module, unmutated, run normally — GREEN,
///      proving the counting mechanism itself is not the thing that's broken.
#[test]
#[ignore = "run by hand to prove counting catches an N+1 fan-out — see doc comment"]
fn mutation_n1_fan_out_per_row_is_caught() {
    let _guard = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = sandbox("mutation");
    let dbp = d.join("db");
    let call_log = call_log_path(&d);

    let base_prog = build_prog(&[], &call_log);
    const ORIGINAL_RESP_RULE: &str =
        "resp(ep, b, status, tag, body) <- @async poll(ep, prev, b), fetch(ep, prev) -> (status, tag, body).";
    assert!(
        base_prog.contains(ORIGINAL_RESP_RULE),
        "gh-cache.dl's `resp` rule shape changed upstream (gh-cache.dl:90) — \
         update this mutation target string before trusting this test"
    );
    const MUTATED_RESP_RULE: &str = concat!(
        "rel pr_shard(ep: text, shard_ep: text).\n",
        "pr_shard(\"repos/cli/cli\", \"repos/cli/cli::row1\").\n",
        "pr_shard(\"repos/cli/cli\", \"repos/cli/cli::row2\").\n",
        "resp(ep, b, status, tag, body) <- @async poll(ep, prev, b), pr_shard(ep, shard_ep), ",
        "fetch(shard_ep, prev) -> (status, tag, body).",
    );
    let mutated_prog = base_prog.replacen(ORIGINAL_RESP_RULE, MUTATED_RESP_RULE, 1);
    assert!(!mutated_prog.contains(ORIGINAL_RESP_RULE), "mutation did not apply");

    fs::write(d.join("p.dl"), mutated_prog).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let exec = shell_exec(&prog);

    let totals = settle(&mut eng, &prog, &exec, 11_000_000, &call_log);

    // Mirrors cold_fetch_lands_entities_and_carries_etag's exact-count
    // assertions verbatim. Under the REAL (unmutated) program these pass —
    // this is proof-by-contradiction: if the mutated program ALSO satisfies
    // them, the counting mechanism has no teeth. It does not satisfy them:
    // bucket 0 now fires 2 fetches (one per shard row) instead of 1, so this
    // assert_eq! is expected and INTENDED to fail, naming the wrong count.
    assert_eq!(
        totals[0], 1,
        "N+1 CAUGHT: bucket 0 fetched {} times for ONE watched endpoint with a \
         per-row-fanned resp rule, not the 1 a correct per-endpoint fetch would \
         produce: {totals:?}", totals[0]
    );
    clear_now();
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
    let _g = CLOCK_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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
            let mut templates = std::collections::HashMap::new();
            for row in eng.query_sql("SELECT kind, template FROM rel_effect_cmd_txt", &[]).unwrap() {
                templates.insert(row[0].as_str().unwrap().to_string(), row[1].as_str().unwrap().to_string());
            }
            ShellEffectExec { templates, n_out: async_effect_arity(&prog), cwd: eng.root() }
        };
        let n = eng.drain_effects(&prog, &exec).unwrap();
        eprintln!("cycle {i}: drained {n} | resp={:?} | etag={:?}",
            rows(&dbp, "rel_resp_txt", "SELECT status, substr(tag,1,12) FROM rel_resp_txt"),
            rows(&dbp, "rel_etag_txt", "SELECT substr(tag,1,12) FROM rel_etag_txt"));
    }
    eng.tick(&prog, true).unwrap();
    eprintln!("stars={:?} full_name={:?}",
        rows(&dbp, "rel_stars_txt", "SELECT n FROM rel_stars_txt"),
        rows(&dbp, "rel_full_name_txt", "SELECT name FROM rel_full_name_txt"));

    assert!(!rows(&dbp, "rel_stars", "SELECT n FROM rel_stars").is_empty(), "live body normalized into stars");
    let statuses: Vec<String> = rows(&dbp, "rel_resp_txt", "SELECT status FROM rel_resp_txt").into_iter().flatten().collect();
    assert!(statuses.contains(&"200".to_string()), "first poll was a live 200: {statuses:?}");
    assert!(statuses.contains(&"304".to_string()), "carried-etag re-poll got a live 304: {statuses:?}");
    clear_now();
}
