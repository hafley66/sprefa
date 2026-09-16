//! Wildcard-bucket clock salt: a `clock(secs, _)` atom in an `@async`/`@stream`
//! body folds (secs, now/secs) into the REQUEST digest while binding nothing —
//! "time lives in the request key, never in the row space". The request re-fires
//! once per bucket (the rate-cap/dedup contract of the bound-bucket form), but
//! no head stores the bucket, so a bucket flip invalidates zero derived rows.
//! Time is injected via `DL_NOW_SECS` (see clock_lock.rs).

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::{async_effect_arity, shell_templates, Engine, ShellEffectExec};
use sprefa_v5::prepare_paths;

use crate::clock_lock::{clear_now, set_now, CLOCK_LOCK};

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_clocksalt_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn count(db_path: &Path, table: &str) -> i64 {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

fn shell_exec(prog: &sprefa_v5::ast::Program) -> ShellEffectExec {
    ShellEffectExec {
        templates: shell_templates(prog),
        n_out: async_effect_arity(prog),
        cwd: PathBuf::new(),
    }
}

/// THE fail-pre-fix receipt. The fully-wildcard form binds no request args at
/// all: pre-fix `rebuild_async` bails ("binds no request args") so the program
/// cannot even tick; post-fix the clock atom keys the request on cadence. Same
/// bucket = one `pending_effect` row across repeated ticks (digest stable);
/// a bucket flip = a NEW request (the digest moved), which is exactly what the
/// pre-fix engine can never produce.
#[test]
fn wildcard_clock_bucket_salts_the_request_digest() {
    let _guard = CLOCK_LOCK.lock().unwrap();
    let dir = sandbox("wild");
    let dbp = dir.join("db");
    fs::write(
        dir.join("p.dl"),
        "sh fetch() -> (line: text) = `printf 'ok'`.\n\
         rel resp(line: text).\n\
         resp(line) <- @async clock(60, _), fetch() -> (line).\n",
    )
    .unwrap();
    let (prog, diags, _) = prepare_paths(&[dir.join("p.dl")]).unwrap();
    // Typecheck posture: the wildcard-bucket clock atom in an @async body is a
    // blessed form — no error, no warning.
    assert!(
        diags.is_empty(),
        "wildcard clock form must typecheck clean: {diags:?}"
    );
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());

    // Bucket 6000/60 = 100: one request, and re-ticking inside the bucket
    // (same or advanced-but-same-bucket now) never queues a second.
    set_now(6000);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "pending_effect"), 1, "one request per bucket");
    eng.tick(&prog, true).unwrap();
    set_now(6010);
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        count(&dbp, "pending_effect"),
        1,
        "same bucket: digest stable, dedup holds"
    );

    // Bucket flip (6060/60 = 101): the salt moves, a NEW request fires. Pre-fix
    // (unused-var variant) the digest never varies and this stays 1.
    set_now(6060);
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        count(&dbp, "pending_effect"),
        2,
        "a fresh bucket mints a fresh request id"
    );

    // Drain and land: the response carries no bucket, so the identical body
    // dedups into one resp row.
    let drained = eng.drain_effects(&prog, &shell_exec(&prog)).unwrap();
    assert_eq!(drained, 2, "both bucket requests drain");
    set_now(6061);
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        count(&dbp, "rel_resp"),
        1,
        "no bucket column: identical responses collapse"
    );
    clear_now();
}

/// The unused-var spelling `clock(60, bucket)` where `bucket` appears nowhere
/// else in the rule is wildcard-equivalent. This is the true fail-PRE-fix
/// direction on a program the old engine accepts: pre-fix the bucket var is
/// bound but never enters the request digest (the effect call has no args), so
/// the second bucket NEVER re-fires; post-fix it does.
#[test]
fn unused_var_bucket_is_wildcard_equivalent() {
    let _guard = CLOCK_LOCK.lock().unwrap();
    let dir = sandbox("unusedvar");
    let dbp = dir.join("db");
    fs::write(
        dir.join("p.dl"),
        "sh fetch() -> (line: text) = `printf 'ok'`.\n\
         rel resp(line: text).\n\
         resp(line) <- @async clock(60, bucket), fetch() -> (line).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[dir.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());

    set_now(6000);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "pending_effect"), 1);
    set_now(6060);
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        count(&dbp, "pending_effect"),
        2,
        "unused bucket var salts like a wildcard"
    );
    clear_now();
}

/// Regression pin: a bucket var the rule USES (here: stored in the head) is NOT
/// salted — the classic form's digest is unchanged, byte-identical ids and all.
/// In the explicit-call form the bucket was never part of the request digest
/// (only the effect args are), so the id stays constant across buckets and the
/// dedup keeps one row. Programs wanting the bound-bucket cadence thread the
/// bucket through the effect args or the head-response form (gh-cache.dl).
#[test]
fn used_bucket_var_keeps_classic_digest_behavior() {
    let _guard = CLOCK_LOCK.lock().unwrap();
    let dir = sandbox("usedvar");
    let dbp = dir.join("db");
    fs::write(
        dir.join("p.dl"),
        "sh fetch() -> (line: text) = `printf 'ok'`.\n\
         rel resp(bucket: int, line: text).\n\
         resp(bucket, line) <- @async clock(60, bucket), fetch() -> (line).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[dir.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());

    set_now(6000);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "pending_effect"), 1);
    set_now(6060);
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        count(&dbp, "pending_effect"),
        1,
        "a used bucket var is not salted; the explicit-call digest is unchanged"
    );
    clear_now();
}

/// The point of the feature: a derived rel downstream of the wildcard form sees
/// ZERO writes across a bucket flip when the response content is unchanged. The
/// flip re-fires the request (new pending row), the drain lands an identical
/// body (dedups to nothing), and the downstream derived rel is either not
/// rebuilt at all or rides the derived digest-skip — its live rowset never
/// rewrites.
#[test]
fn bucket_flip_invalidates_zero_derived_rows() {
    let _guard = CLOCK_LOCK.lock().unwrap();
    let dir = sandbox("zeroderive");
    let dbp = dir.join("db");
    fs::write(
        dir.join("p.dl"),
        "sh fetch() -> (line: text) = `printf 'ok'`.\n\
         rel resp(line: text).\n\
         resp(line) <- @async clock(60, _), fetch() -> (line).\n\
         rel seen(line: text).\n\
         seen(line) <- resp(line).\n",
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[dir.join("p.dl")]).unwrap();
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());

    // Bucket 100: request, drain, land, derive.
    set_now(6000);
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.drain_effects(&prog, &shell_exec(&prog)).unwrap(), 1);
    set_now(6001);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "rel_resp"), 1);
    assert_eq!(count(&dbp, "rel_seen"), 1);

    // Bucket 101: the flip re-fires; the response is byte-identical.
    set_now(6060);
    eng.tick(&prog, true).unwrap();
    assert_eq!(count(&dbp, "pending_effect"), 2, "flip re-fired");
    assert_eq!(eng.drain_effects(&prog, &shell_exec(&prog)).unwrap(), 1);
    set_now(6061);
    eng.tick(&prog, true).unwrap();

    // Zero derived-row motion: contents unchanged, and the rel either sat out
    // the rebuild scope entirely or the digest-skip wrote nothing.
    assert_eq!(
        count(&dbp, "rel_resp"),
        1,
        "identical response landed no new row"
    );
    assert_eq!(
        count(&dbp, "rel_seen"),
        1,
        "downstream derived rowset untouched"
    );
    let rebuilt = eng.last_derived_rebuilt.contains(&"seen".to_string());
    let skipped = eng.last_derived_skipped.contains(&"seen".to_string());
    assert!(
        !rebuilt || skipped,
        "a bucket flip with unchanged content must not rewrite `seen`: rebuilt={:?} skipped={:?}",
        eng.last_derived_rebuilt,
        eng.last_derived_skipped
    );
    clear_now();
}
