//! `--settle` / quiescence detection: drive ticks + off-tick effect drains to a
//! fixpoint, then answer once. See plans/2026-07-06-settle-quiescence.md.
//!
//! Two levels: engine-level tests of the `TickReport` fields the settle loop
//! reads (`is_settled`, `inflight_effects`, `staged_next`), and CLI e2e over the
//! real `dl --settle` binary (the only non-daemon path that drains effects).

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use anyhow::Result;
use serde_json::{Map, Value as Json};

use sprefa_v5::db;
use sprefa_v5::engine::{EffectExec, Engine, TickReport};
use sprefa_v5::prepare_paths;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_settle_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

/// The settled predicate is the whole point: only a report with no non-timer
/// motion, no staged carry, and no in-flight effect is settled. A `clock`/`every`
/// change is steady-state and must NOT block settlement.
#[test]
fn is_settled_truth_table() {
    // Empty report = settled.
    assert!(TickReport::default().is_settled());

    // Timer-only change is still settled (the recurring poll the user ignores).
    let timer = TickReport {
        changed: true,
        changed_rels: vec!["clock".into(), "every".into()],
        ..Default::default()
    };
    assert!(timer.is_settled(), "clock/every motion is steady state");

    // A real relation moving is NOT settled.
    let moved = TickReport {
        changed: true,
        changed_rels: vec!["clock".into(), "want".into()],
        ..Default::default()
    };
    assert!(!moved.is_settled(), "a non-timer rel moved");

    // A staged @next carry is pending -> not settled.
    let carry = TickReport { staged_next: true, ..Default::default() };
    assert!(!carry.is_settled());

    // An in-flight effect is owed a drain -> not settled.
    let fx = TickReport { inflight_effects: 1, ..Default::default() };
    assert!(!fx.is_settled());

    // derived_moved alone -> not settled.
    let der = TickReport { derived_moved: true, ..Default::default() };
    assert!(!der.is_settled());
}

/// A canned executor: one fixed response, records that it ran.
struct MockExec {
    body: Vec<String>,
    calls: Mutex<usize>,
}
impl EffectExec for MockExec {
    fn run(&self, _kind: &str, _args: &Map<String, Json>) -> Result<Vec<String>> {
        *self.calls.lock().unwrap() += 1;
        Ok(self.body.clone())
    }
}

/// Engine-level: a tick that emits an `@async` request reports `inflight_effects
/// > 0` and is NOT settled; after the off-tick drain the next tick reports zero
/// in-flight and settles. This is the exact signal the `--settle` loop reads.
#[test]
fn report_tracks_effect_inflight_then_settles() {
    let d = sandbox("inflight");
    let dbp = d.join("db");
    fs::write(d.join("p.dl"),
        "rel want(key: str, url: str).\n\
         want(\"home\", \"https://api/home\").\n\
         rel resp(key: str, body: str).\n\
         resp(key, body) <- @async want(key, url).\n").unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Tick 1 emits one pending_effect row -> in-flight, not settled.
    let r1 = eng.tick_report(&prog, true).unwrap();
    assert_eq!(r1.inflight_effects, 1, "one queued @async request");
    assert!(!r1.is_settled(), "cannot settle with an undrained effect");

    // Off-tick drain (what the daemon / --settle loop does).
    let exec = MockExec { body: vec!["HOME".into()], calls: Mutex::new(0) };
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 1);

    // Tick 2: the request is done (zero in-flight), but the just-landed response
    // is attributed as a changed rel this tick (the async-digest move), so it is
    // NOT yet settled — the settle loop ticks once more.
    let r2 = eng.tick_report(&prog, true).unwrap();
    assert_eq!(r2.inflight_effects, 0, "the drained request is done");
    assert!(!r2.is_settled(), "the response landing counts as motion this tick");

    // Tick 3: nothing moved (async digest stable), zero in-flight -> settled.
    let r3 = eng.tick_report(&prog, true).unwrap();
    assert!(r3.is_settled(), "quiet once the response is a steady input");
    assert_eq!(*exec.calls.lock().unwrap(), 1);
}

/// Engine-level: a `@next` carry reports `staged_next` until it reaches a
/// fixpoint (the staged carry equals the live rel), then settles.
#[test]
fn report_staged_next_clears_at_carry_fixpoint() {
    let d = sandbox("carry");
    let dbp = d.join("db");
    fs::write(d.join("p.dl"),
        "rel ping(v: int).\n\
         ping(7).\n\
         rel echo(v: int).\n\
         echo(v) <- @next ping(v).\n").unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Tick 1: echo empty, ping={7} staged into carry@1 -> the carry differs from
    // the (empty) live echo.
    let r1 = eng.tick_report(&prog, true).unwrap();
    assert!(r1.staged_next, "carry@1 (=7) differs from empty echo");
    assert!(!r1.is_settled());

    // Tick 2: carry@1 loads -> echo={7}; carry@2 re-stages {7}, now EQUAL to the
    // live echo -> no pending motion -> settled.
    let r2 = eng.tick_report(&prog, true).unwrap();
    assert!(!r2.staged_next, "carry reached a fixpoint");
    assert!(r2.is_settled());
}

fn run_settle(dir: &PathBuf, prog: &str, extra: &[&str]) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut args = vec![
        dir.join("p.dl").to_str().unwrap().to_string(),
        "--no-daemon".into(), "--settle".into(),
    ];
    for e in extra { args.push((*e).into()); }
    let out = Command::new(DL).args(&args).current_dir(dir).output().expect("run dl --settle");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// CLI e2e: a plain (no-effect) program settles and prints its `?` answer.
#[test]
fn cli_settle_converges_and_prints() {
    let d = sandbox("plain");
    let (code, out, err) = run_settle(&d,
        "rel greeting(who: text).\n\
         greeting(\"world\").\n\
         rel loud(who: text).\n\
         loud(who) <- greeting(who).\n\
         ? loud(who).\n", &[]);
    assert_eq!(code, 0, "clean program settles: {err}");
    assert!(out.contains("world"), "prints the ? answer: {out}");
    assert!(err.contains("converged"), "reports convergence: {err}");
}

/// CLI e2e: an `sh` effect (a real subprocess) is drained by the settle loop —
/// the daemon is off (`--no-daemon`), proving `--settle` owns the effect runtime.
/// The response feeds a derived rel that the printed `?` reads.
#[test]
fn cli_settle_drives_sh_effect() {
    let d = sandbox("sh");
    // `sh who -> echoes a fixed line`; the @async response lands in greet, which
    // a derived rel reads. Deterministic: `echo` has no network.
    let (code, out, err) = run_settle(&d,
        "sh greet(name) -> (line: text) = `echo hi-{name}`.\n\
         rel want(name: text).\n\
         want(\"bob\").\n\
         rel greet(name: text, line: text).\n\
         greet(name, line) <- @async want(name).\n\
         rel done(line: text).\n\
         done(line) <- greet(_, line).\n\
         ? done(line).\n", &["--settle-max", "50"]);
    assert_eq!(code, 0, "sh-effect program settles: {err}");
    assert!(out.contains("hi-bob"), "the drained echo response is present: {out}\n{err}");
}

/// CLI e2e: a non-converging `@next` accumulator (the value set grows every tick)
/// never settles — the loop bails loudly with non-zero exit instead of hanging.
#[test]
fn cli_settle_bails_on_non_convergence() {
    let d = sandbox("diverge");
    let (code, _out, err) = run_settle(&d,
        "rel base(v: int).\n\
         base(0).\n\
         rel grow(v: int).\n\
         rel grow_next(v: int).\n\
         grow_next(v) <- base(v).\n\
         grow_next(v2) <- grow(v1), v2 = v1 + 1.\n\
         grow(v) <- @next grow_next(v).\n\
         ? grow(v).\n", &["--settle-max", "80"]);
    assert_ne!(code, 0, "a divergent program must not exit 0");
    assert!(err.contains("did not settle"), "bails loudly naming the failure: {err}");
}

/// Engine-level: bookkeeping motion (`rel_count`/`stmt_ms`) must never seed
/// the scoped rebuild. The tick writes those rels' inputs itself and their
/// dependents' rebuilds re-jitter the timings they report, so counting them as
/// change can never converge — the 2026-07-17 perpetual-loop storm (75GB
/// written across 4k ticks of diag-rail rebuilds) was exactly this shape under
/// the daemon's poll loop. Sandbox timings are too stable to jitter on their
/// own, so the test forces PerfKind's moved-detection deterministically: poke
/// the materialized `rel_count` table so the refresh sees a mismatch, rewrites
/// it, and reports moved. That motion must stay out of `changed_rels` and the
/// tick must stay settled.
#[test]
fn bookkeeping_motion_never_seeds_the_scoped_rebuild() {
    let d = sandbox("bookkeeping");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel probe(rel: str, rows: int).\n\
         probe(rel, rows) <- rel_count(rel, rows), rows > 100000.\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    // The hazard exists only under a repeated-tick scheduler; one-shot ticks
    // keep bookkeeping as a seed (the perf rails' second-invocation contract,
    // tests/it/perf_woes.rs).
    eng.poll_loop = true;

    // Ticks 1-2: derive, then steady state.
    eng.tick_report(&prog, true).unwrap();
    eng.tick_report(&prog, true).unwrap();

    // Force the bookkeeping family to report moved on the next refresh.
    eng.query_sql("UPDATE rel_rel_count SET rows = rows + 1", &[]).unwrap();

    let r3 = eng.tick_report(&prog, true).unwrap();
    assert!(
        !r3.changed_rels.iter().any(|r| r == "rel_count" || r == "stmt_ms"),
        "bookkeeping rels must not seed the rebuild scope: {:?}",
        r3.changed_rels
    );
    assert!(r3.is_settled(), "bookkeeping-only motion is quiescent: {:?}", r3.changed_rels);
}
