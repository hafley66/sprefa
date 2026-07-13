//! The `every(N)` clock relation: an engine-populated source rel that holds the
//! interval `N` only on the tick that crosses an N-second boundary (and the first
//! tick), so a body atom `every(30)` self-throttles the rule that joins it. Time
//! is injected via `DL_NOW_SECS` so the cadence is deterministic without sleeping.
//! See docs/research-reactive-effectful-datalog.md §9 and the Phase-2 plan.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use serde_json::{Map, Value as Json};

use sprefa_v5::db;
use sprefa_v5::engine::{Engine, EffectExec};
use sprefa_v5::prepare_paths;

use crate::clock_lock::{clear_now, set_now, CLOCK_LOCK};

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_every_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn count(db_path: &Path, table: &str) -> i64 {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap()
}

/// Echoes the bound `n` as the single output slot.
struct EchoExec;
impl EffectExec for EchoExec {
    fn run(&self, _kind: &str, args: &Map<String, Json>) -> Result<Vec<String>> {
        let n = args.get("n").map(|v| v.to_string()).unwrap_or_default();
        Ok(vec![format!("body-{n}")])
    }
}

/// Edge-triggered: fires on the first tick, stays quiet inside the bucket, fires
/// again on the next N-boundary. A 30s and a 60s interval keep independent buckets.
#[test]
fn every_fires_on_boundary_only() {
    let _g = CLOCK_LOCK.lock().unwrap();
    let d = sandbox("edge");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel tick30(t: int).\n\
         tick30(1) <- every(30).\n\
         rel tick60(t: int).\n\
         tick60(1) <- every(60).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // t=1000: 1000/30=33, 1000/60=16 — both first observation, both fire.
    set_now(1000);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "rel_tick30"), 1, "30s fires on first tick");
    assert_eq!(count(&dbp, "rel_tick60"), 1, "60s fires on first tick");

    // t=1010: same buckets (33, 16) — neither fires; the rel clears.
    set_now(1010);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "rel_tick30"), 0, "inside the 30s bucket: quiet");
    assert_eq!(count(&dbp, "rel_tick60"), 0, "inside the 60s bucket: quiet");

    // t=1020: 1020/30=34 (crossed) but 1020/60=17 (crossed too). both fire.
    set_now(1020);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "rel_tick30"), 1, "crossed a 30s boundary");
    assert_eq!(count(&dbp, "rel_tick60"), 1, "crossed a 60s boundary");

    // t=1035: 1035/30=34 (same as 34 — 30s quiet), 1035/60=17 (same — 60s quiet).
    set_now(1035);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "rel_tick30"), 0, "still inside the next 30s bucket");
    assert_eq!(count(&dbp, "rel_tick60"), 0, "still inside the 60s bucket");

    // t=1050: 1050/30=35 (30s crossed), 1050/60=17 (60s NOT crossed). only 30s.
    set_now(1050);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "rel_tick30"), 1, "30s crossed at 1050");
    assert_eq!(count(&dbp, "rel_tick60"), 0, "60s boundary is 1080, not yet");

    std::env::remove_var("DL_NOW_SECS");
}

/// Stress: a 500-row fact relation fanned through `every(5)` into an `@async`
/// effect. Boundary ticks emit the whole fanout; quiet ticks add nothing
/// (the gate is empty) and the digest keeps re-fired requests idempotent. Also
/// eyeballs per-tick cost: a quiet tick over the large corpus must stay cheap.
#[test]
fn every_gated_async_fanout_under_load() {
    let _g = CLOCK_LOCK.lock().unwrap();
    let d = sandbox("load");
    let dbp = d.join("db");
    const N: usize = 500;
    let mut src = String::from(
        "rel big(n: int).\n\
         rel resp(n: int, out: str).\n\
         resp(n, out) <- @async big(n), every(5).\n",
    );
    for i in 0..N {
        src.push_str(&format!("big({i}).\n"));
    }
    fs::write(d.join("p.dl"), src).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Boundary tick: the gate is non-empty, all 500 rows fan into pending_effect.
    set_now(5000);
    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let boundary_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(count(&dbp, "pending_effect"), N as i64, "full fanout on the boundary");

    // Quiet tick (same 5s bucket): gate empty, NO new pending rows, and it stays
    // cheap — the 500-row join is not re-emitted because every(5) is empty.
    set_now(5001);
    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let quiet_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(count(&dbp, "pending_effect"), N as i64, "quiet tick queues nothing new");

    // Drain off-tick (parallel), then read back on the next tick.
    let n = eng.drain_effects(&prog, &EchoExec).unwrap();
    assert_eq!(n, N, "all 500 drained");
    set_now(5002);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "rel_resp"), N as i64, "responses landed");

    // Deep-quiet tick: two into the same bucket as 5002, nothing fired last tick
    // and nothing to clear, so the clock refresh returns "unchanged" and the full
    // rebuild is skipped entirely. This is the steady-state floor between polls.
    set_now(5003);
    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let deep_quiet_ms = t.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(count(&dbp, "pending_effect"), N as i64, "deep-quiet tick is a no-op for the clock");

    // Next boundary (5005/5=1001, was 1000): every(5) fires again, but the same
    // 500 requests collide on their digest id — no duplicate queue, no re-fire.
    set_now(5005);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "pending_effect"), N as i64, "re-fire is idempotent on digest");
    assert_eq!(count(&dbp, "rel_resp"), N as i64, "responses unchanged");

    eprintln!(
        "[every-stress] boundary {boundary_ms:.1}ms, clear-after {quiet_ms:.1}ms, \
         deep-quiet {deep_quiet_ms:.1}ms (N={N})"
    );
    std::env::remove_var("DL_NOW_SECS");
    let _ = HashMap::<String, usize>::new(); // keep import set stable
}

/// `clock(secs, bucket)` is the PERSISTENT sibling of `every`: it holds the
/// current bucket `now/secs` on EVERY tick (not only on the boundary), so a join
/// `clock(30, b)` binds `b` to the live time bucket and that value advances only
/// when a boundary is crossed. This is the cadence primitive — a varying token to
/// fold into a request digest, no `@next` counter. Contrast `every`, which clears
/// inside the bucket.
#[test]
fn clock_holds_current_bucket_persistently() {
    let _g = CLOCK_LOCK.lock().unwrap();
    let d = sandbox("clockrel");
    let dbp = d.join("db");
    fs::write(
        d.join("p.dl"),
        "rel cbucket(b: int).\n\
         cbucket(b) <- clock(30, b).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    let bucket = |dbp: &Path| -> Vec<i64> {
        let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
        let mut s = conn.prepare("SELECT b FROM rel_cbucket_txt ORDER BY b").unwrap();
        s.query_map([], |r| r.get::<_, i64>(0)).unwrap().filter_map(|x| x.ok()).collect()
    };

    // t=1000: bucket 1000/30 = 33, present.
    set_now(1000);
    eng.tick(&prog, true).unwrap();
    assert_eq!(bucket(&dbp), vec![33], "clock binds now/secs");

    // t=1010: same bucket — STILL present (every would have cleared here).
    set_now(1010);
    eng.tick(&prog, true).unwrap();
    assert_eq!(bucket(&dbp), vec![33], "clock persists inside the bucket");

    // t=1020: 1020/30 = 34 — the bucket advanced.
    set_now(1020);
    eng.tick(&prog, true).unwrap();
    assert_eq!(bucket(&dbp), vec![34], "clock advances on a boundary");

    clear_now();
}
