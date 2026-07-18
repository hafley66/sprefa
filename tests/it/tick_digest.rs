//! P1/P2/P3 (--check perf defect ledger, 2026-07-10): proof that a
//! LEGITIMATELY-empty derived rel no longer defeats the digest skip.
//!
//! The bug: `any_derived_empty` (the old name) treated "this derived rel's
//! table has zero rows right now" as "must full-rebuild", so a program with
//! even one durably-empty derived rel (an inert rail, a diff view with
//! nothing to report — 34/154 on a real db) full-rebuilt EVERY derived rel on
//! EVERY tick, warm or not. The fix: `_derived_complete` marks a rel as
//! having completed a rebuild pass (whatever row count it ended with), and
//! `derived_incomplete_rels` asks "which of today's derived rels never
//! finished a pass" instead of "which are empty right now" — one query
//! against that marker table, not a `COUNT(*)` per rel.
//!
//! In-process invariants (a)-(e) below use `Engine::last_derived_rebuilt`
//! directly (the same harness style as `scoped_tick.rs`/`perf_facts.rs`);
//! the P2/P3 perf.jsonl wiring (pid field, phase record, tick `full_reason`)
//! is proven end-to-end against the real compiled binary in
//! `perf_log_end_to_end`, since `perflog`'s process-global `OnceLock`s make an
//! in-process assertion unreliable inside the shared `it` test binary.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tick_digest_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

// `live` mirrors `src_a` 1:1 (non-empty while a source row exists) — the
// control chain. `gate` is an inert-rail shape (like `diff_pair(WORK,WORK)`'s
// default in graph-diff.dl): it NEVER matches, so it is durably empty even
// after a fully-completed derive.
const PROG: &str = r#"
rel src_a(p: file, w: text).
src_a(p, w) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /alpha_(?<w>\w+)/, line).
rel live(w: text).
live(w) <- src_a(_, w).
rel gate(w: text).
gate(w) <- src_a(_, w), w != w.
"#;

fn fresh_engine(dir: &Path) -> (Engine, sprefa_v5::ast::Program) {
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    (Engine::new(conn, dir.to_path_buf()), prog)
}

/// (a) A blank-slate db still fully derives every rel, including the one
/// that will end up legitimately empty.
#[test]
fn a_blank_slate_still_full_derives() {
    let d = sandbox("blank_slate");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);

    eng.tick(&prog, true).unwrap();
    let mut rebuilt = eng.last_derived_rebuilt.clone();
    rebuilt.sort();
    assert_eq!(rebuilt, vec!["gate".to_string(), "live".to_string()],
        "cold tick must derive every rel, empty-destined or not");

    let gate_rows = eng.query_sql("SELECT w FROM rel_gate_txt", &[]).unwrap();
    assert!(gate_rows.is_empty(), "gate legitimately matches nothing");
    let live_rows = eng.query_sql("SELECT w FROM rel_live_txt", &[]).unwrap();
    assert_eq!(live_rows.len(), 1, "live has the one matched row");
}

/// (c) THE regression test: once `gate` has completed one derive (durably
/// empty), a subsequent unchanged tick must NOT force a full rebuild just
/// because `gate`'s table reads zero rows. Repeated across several ticks —
/// the marker must persist, not just skip once.
#[test]
fn c_inert_empty_rel_does_not_force_full_rebuild() {
    let d = sandbox("inert_empty");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);

    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.last_derived_rebuilt.len(), 2, "cold tick derives both rels");

    for i in 0..3 {
        eng.tick(&prog, true).unwrap();
        assert!(eng.last_derived_rebuilt.is_empty(),
            "unchanged tick #{i} must rebuild nothing despite `gate` being durably \
             empty, got {:?}", eng.last_derived_rebuilt);
    }
}

/// (b) Adding a NEW derived rel to the program still triggers ITS
/// derivation (the `derived:program` digest path, untouched by P1) — the P1
/// marker check must not somehow suppress this.
#[test]
fn b_new_derived_rel_is_derived_on_program_edit() {
    let d = sandbox("new_rel");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    let prog1 = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    eng.tick(&prog1, true).unwrap();
    eng.tick(&prog1, true).unwrap(); // settle to unchanged
    assert!(eng.last_derived_rebuilt.is_empty());

    const PROG2: &str = r#"
rel src_a(p: file, w: text).
src_a(p, w) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /alpha_(?<w>\w+)/, line).
rel live(w: text).
live(w) <- src_a(_, w).
rel gate(w: text).
gate(w) <- src_a(_, w), w != w.
rel brand_new(w: text).
brand_new(w) <- src_a(_, w).
"#;
    let prog2 = parse::parse(lex::lex(PROG2).unwrap()).unwrap();
    eng.tick(&prog2, true).unwrap();
    assert!(eng.last_derived_rebuilt.contains(&"brand_new".to_string()),
        "a program edit adding a derived rel must derive it, got {:?}", eng.last_derived_rebuilt);
    let rows = eng.query_sql("SELECT w FROM rel_brand_new_txt", &[]).unwrap();
    assert_eq!(rows.len(), 1);

    // The edit's full rebuild should also settle: a further unchanged tick
    // over the 3-rel program rebuilds nothing.
    eng.tick(&prog2, true).unwrap();
    assert!(eng.last_derived_rebuilt.is_empty());
}

/// (e) A rel whose rows are legitimately deleted by re-derivation to empty
/// is served as empty (never a stale row from before the deletion) — AND,
/// once settled, that newly-empty rel does not itself start forcing full
/// rebuilds either (same P1 property as `gate`, reached via retraction
/// instead of a rule that never matches).
#[test]
fn e_rel_deriving_to_empty_stays_empty_not_stale() {
    let d = sandbox("empty_after");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);

    eng.tick(&prog, true).unwrap();
    let rows = eng.query_sql("SELECT w FROM rel_live_txt", &[]).unwrap();
    assert_eq!(rows.len(), 1, "live starts with one row");

    fs::remove_file(d.join("src/a.rs")).unwrap();
    eng.tick(&prog, true).unwrap();
    let rows = eng.query_sql("SELECT w FROM rel_live_txt", &[]).unwrap();
    assert!(rows.is_empty(), "live must reflect the deletion, not serve a stale row");
    assert!(eng.last_derived_rebuilt.contains(&"live".to_string()),
        "the deletion must have actually re-derived `live`");

    eng.tick(&prog, true).unwrap();
    assert!(eng.last_derived_rebuilt.is_empty(),
        "`live` freshly-empty-by-deletion must not force full rebuilds either, got {:?}",
        eng.last_derived_rebuilt);
}

/// (d) Sanity: `tick_paths` (the incremental/watcher path) shares the same
/// P1 fix and the same non-regression — an inert-empty derived rel does not
/// force a full rebuild there either. `scoped_tick.rs`/`extract_cache.rs`
/// cover the rest of the incremental-path invariants and must stay green
/// (checked by the full suite, not re-derived here).
#[test]
fn d_tick_paths_also_respects_the_completion_marker() {
    let d = sandbox("tick_paths_inert");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);

    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.last_derived_rebuilt.len(), 2);

    // A path-scoped tick with an unrelated no-op changed-path list (the file
    // watcher's shape) must not force a full rebuild.
    eng.tick_paths(&prog, &[], true).unwrap();
    assert!(eng.last_derived_rebuilt.is_empty(),
        "tick_paths with no changed files must rebuild nothing, got {:?}",
        eng.last_derived_rebuilt);
}

/// (f) A term-extract (json-over-bound-string) head whose row set is in steady
/// state must NOT flip `extract_changed` — before the fix, `eval_extract_rules`
/// compared `SELECT *` rows (which carry the `__src` bookkeeping column) against
/// the freshly extracted rows (declared columns only), so the sets never matched
/// once the head had a row, and every full tick escalated into a whole-program
/// derived rebuild (271 `[derived] full wipe` lines per tick on the sprefa root,
/// 2026-07-18 daemon.log).
#[test]
fn f_term_extract_steady_state_does_not_force_full_rebuild() {
    const EXTRACT_PROG: &str = r#"
rel payload(body: text).
payload("[{\"word\": \"alpha\"}]").
rel item(word: text).
item(word) <- payload(body), json(body, q:[... { word: $word }]).
rel downstream(word: text).
downstream(word) <- item(word).
"#;
    let d = sandbox("extract_steady");
    let prog = parse::parse(lex::lex(EXTRACT_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    eng.tick(&prog, true).unwrap();
    let rows = eng.query_sql("SELECT word FROM rel_item_txt", &[]).unwrap();
    assert_eq!(rows.len(), 1, "the json extract must land its row");

    for i in 0..3 {
        eng.tick(&prog, true).unwrap();
        assert!(eng.last_derived_rebuilt.is_empty(),
            "unchanged tick #{i}: a steady-state extract head must not force a \
             full derived rebuild, got {:?}", eng.last_derived_rebuilt);
    }
}

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// P2/P3 end-to-end: run the real one-shot `--check --no-daemon` binary
/// twice (cold, then warm) against a sandbox whose program has a durably-
/// empty derived rel, and read `.dl/perf.jsonl` back. Confirms:
///   - P2: every record (phase + tick) carries a `pid` field.
///   - P3: a `"phase":"derived"` record is emitted on the ONE-SHOT path
///     (not just under the daemon), and the tick record's `full_reason`
///     names WHY a full rebuild happened.
///   - P1, externally observed: the cold run is full ("blank-slate"); the
///     warm rerun is NOT full despite `gate` being durably empty.
#[test]
fn perf_log_end_to_end() {
    let d = sandbox("perf_log_e2e");
    fs::create_dir_all(d.join(".dl")).unwrap();
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join(".dl/prog.dl"), PROG).unwrap();

    let perf_path = d.join(".dl/perf.jsonl");
    let run = |extra_env: &[(&str, &str)]| -> Vec<serde_json::Value> {
        let before = fs::read_to_string(&perf_path).unwrap_or_default();
        let before_lines = before.lines().count();
        let mut cmd = Command::new(DL);
        cmd.args([".dl/prog.dl", "--check", "--no-daemon", "--db", ".dl/cache.db"])
            .current_dir(&d)
            .env("DL_NO_DAEMON", "1");
        for (k, v) in extra_env { cmd.env(k, v); }
        let out = cmd.output().expect("run dl --check");
        assert!(out.status.success() || out.status.code() == Some(0),
            "dl --check should exit clean on this program: {}",
            String::from_utf8_lossy(&out.stderr));
        let after = fs::read_to_string(&perf_path).unwrap_or_default();
        after.lines().skip(before_lines)
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .collect()
    };

    // Cold run.
    let cold = run(&[]);
    assert!(!cold.is_empty(), "cold run must write at least one perf.jsonl record");
    for rec in &cold {
        assert!(rec.get("pid").and_then(|v| v.as_u64()).is_some(),
            "every perf.jsonl record carries a pid (P2), got {rec}");
    }
    let cold_derived_phase = cold.iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("phase")
                && r.get("phase").and_then(|v| v.as_str()) == Some("derived"));
    assert!(cold_derived_phase.is_some(),
        "the one-shot --check path must emit a \"derived\" phase record (P3), got {cold:?}");
    let cold_tick = cold.iter().find(|r| r.get("type").and_then(|v| v.as_str()) == Some("tick"))
        .expect("cold run emits a tick record");
    assert_eq!(cold_tick["derived"]["strategy"].as_str(), Some("full"),
        "cold run must be a full rebuild: {cold_tick}");
    assert_eq!(cold_tick["derived"]["full_reason"].as_str(), Some("blank-slate"),
        "cold run's reason must be blank-slate: {cold_tick}");

    // Warm rerun: nothing changed, and `gate` is durably empty in the db
    // that now exists — the P1 bug would force this to "full" again.
    let warm = run(&[]);
    let warm_tick = warm.iter().find(|r| r.get("type").and_then(|v| v.as_str()) == Some("tick"))
        .expect("warm run emits a tick record");
    assert_eq!(warm_tick["derived"]["strategy"].as_str(), Some("unchanged"),
        "warm rerun must NOT full-rebuild just because `gate` is durably empty (P1): {warm_tick}");
    assert!(warm_tick["derived"]["full_reason"].is_null(),
        "warm rerun must carry no full_reason: {warm_tick}");
}
