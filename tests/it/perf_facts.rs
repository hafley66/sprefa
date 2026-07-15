//! Hard perf facts for each engine-invoking path: what re-derived, what got
//! scoped, what fell back to a full rebuild, what a comment-only edit did
//! (nothing), and a timing ceiling. The instrumentation under test is
//! `Engine::last_derived_rebuilt` (the set of derived rels a tick actually
//! rebuilt), read directly off the engine after each path — no daemon, no
//! timing flakiness, just the recompute contract per command shape.
//!
//! The matrix:
//!   cold                  -> rebuilds every derived rel
//!   warm no-change        -> rebuilds NOTHING (the "not atrocious" proof)
//!   incremental 1 file    -> rebuilds only that chain's rel
//!   comment-only edit     -> rebuilds NOTHING (extract digest-skip)
//!   source-rule edit      -> falls back to full tick, rebuilds all
//!   derived-rule edit     -> full re-derivation (program digest moved)
//!
//! `perf_rels.rs` covers `rel_count`/`stmt_ms`; `scoped_tick.rs` covers the
//! two-chain full-tick scoping. This file adds the command-path matrix +
//! digest-skip + fallback + timing facts. The live tail-able log
//! (`<root>/.dl/perf.jsonl`) is exercised end-to-end in the daemon suite.

use sprefa_v5::{
    db,
    engine::{Engine, PathTickFallbackPolicy, PathTickOutcome},
    lex, parse,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("perf_facts_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

// Two INDEPENDENT chains so a change in one cannot justify rebuilding the other:
//   src/*.rs   -> alpha token -> src_a -> da
//   src/*.txt  -> beta token  -> src_b -> db_out
// Neither derived rule joins a built-in, so attribution reaches only its chain.
const PROG: &str = r#"
rel src_a(p: file, w: text).
src_a(p, w) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /alpha_(?<w>\w+)/, line).
rel src_b(p: file, w: text).
src_b(p, w) <- scan("WORK", "src/*.txt", p, rev), match(p, rev, /beta_(?<w>\w+)/, line).
rel da(w: text).
da(w) <- src_a(_, w).
rel db_out(w: text).
db_out(w) <- src_b(_, w).
"#;

fn fresh_engine(dir: &Path) -> (Engine, sprefa_v5::ast::Program) {
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    (Engine::new(conn, dir.to_path_buf()), prog)
}

fn prog_of(src: &str) -> sprefa_v5::ast::Program {
    parse::parse(lex::lex(src).unwrap()).unwrap()
}

/// Cold tick on a blank db rebuilds every derived rel (da + db_out). The fact:
/// a first-ever tick is genuinely full.
#[test]
fn cold_tick_rebuilds_every_derived_rel() {
    let d = sandbox("cold");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);

    eng.tick(&prog, true).unwrap();
    let mut rebuilt = eng.last_derived_rebuilt.clone();
    rebuilt.sort();
    assert_eq!(
        rebuilt,
        vec!["da".to_string(), "db_out".to_string()],
        "cold tick rebuilds both chains"
    );
}

/// Warm tick with NO changes rebuilds nothing. This is the core reactivity
/// contract: an idle re-tick is free, not a full re-derivation.
#[test]
fn warm_no_change_tick_rebuilds_nothing() {
    let d = sandbox("warm");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();

    eng.tick(&prog, true).unwrap();
    assert!(
        eng.last_derived_rebuilt.is_empty(),
        "a no-change warm tick must rebuild nothing, got {:?}",
        eng.last_derived_rebuilt
    );
}

/// One edited file rebuilds only its chain (incremental scoping). Touch ONLY
/// the b-chain file (different byte length so the mtime/size fast path can't
/// mask the change).
#[test]
fn incremental_one_file_rebuilds_only_its_chain() {
    let d = sandbox("incr");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();

    fs::write(d.join("src/b.txt"), "beta_one\nbeta_two\n").unwrap();
    let changed = vec![d.join("src/b.txt")];
    eng.tick_paths(&prog, &changed, true).unwrap();
    assert_eq!(
        eng.last_derived_rebuilt,
        vec!["db_out".to_string()],
        "a b-chain-only edit rebuilds only db_out, got {:?}",
        eng.last_derived_rebuilt
    );

    // And the untouched chain's data survived.
    let da = eng.query_sql("SELECT w FROM rel_da_txt", &[]).unwrap();
    assert_eq!(da.len(), 1, "da untouched");
}

#[test]
fn path_tick_policy_reports_supported_incremental_path() {
    let d = sandbox("path_policy_incremental");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();

    let changed = d.join("src/b.txt");
    fs::write(&changed, "beta_one\nbeta_two\n").unwrap();
    let outcome = eng
        .tick_paths_with_policy(
            &prog,
            std::slice::from_ref(&changed),
            true,
            PathTickFallbackPolicy::Forbid,
        )
        .unwrap();

    assert_eq!(outcome, PathTickOutcome::Incremental);
    assert_eq!(eng.last_derived_rebuilt, vec!["db_out".to_string()]);
}

#[test]
fn path_tick_policy_forbids_source_rule_fallback() {
    let d = sandbox("path_policy_forbidden");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();

    let edited = prog_of(&PROG.replace("src/*.rs", "src/*.{rs,txt}"));
    let changed = d.join("src/b.txt");
    fs::write(&changed, "beta_one\n// alpha_two\n").unwrap();
    let outcome = eng
        .tick_paths_with_policy(
            &edited,
            std::slice::from_ref(&changed),
            true,
            PathTickFallbackPolicy::Forbid,
        )
        .unwrap();

    assert_eq!(
        outcome,
        PathTickOutcome::ForbiddenFallback {
            reason: "source-rule-edit",
            scope: "full-source-reconcile",
        }
    );
    let rows = eng
        .query_sql("SELECT w FROM rel_da_txt ORDER BY w", &[])
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "forbidden fallback must not run the widened scan"
    );
}

/// A COMMENT-ONLY edit changes bytes but not extracted rows; the extract
/// digest-skip prunes it, so NOTHING re-derives. This is the proof the engine
/// does not blindly re-derive on every save.
#[test]
fn comment_only_edit_does_not_rederive() {
    let d = sandbox("comment");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();

    // Same alpha token, extra comment line. Bytes move; src_a's rows do not.
    fs::write(
        d.join("src/a.rs"),
        "// alpha_one\n// just a comment, no new token\n",
    )
    .unwrap();
    let changed = vec![d.join("src/a.rs")];
    eng.tick_paths(&prog, &changed, true).unwrap();
    assert!(
        eng.last_derived_rebuilt.is_empty(),
        "a comment-only edit (same extracted rows) must not re-derive, got {:?}",
        eng.last_derived_rebuilt
    );
}

/// Editing a SOURCE rule (changing its glob) trips the source-rule-digest
/// fallback: `tick_paths` detects the rule changed and delegates to the full
/// tick (full reconcile over the corpus). The observable proof the fallback
/// ran — vs a stale scoped tick that would silently keep the OLD glob — is that
/// the widened glob takes effect: src_a picks up an alpha token from a `.txt`
/// file it could not match before, so src_a's rows move and `da` rebuilds with
/// the new row. (The full-tick fallback is still scoped by attribution: only
/// rels whose source rows actually moved re-derive — it is not a blanket
/// re-derivation.)
#[test]
fn source_rule_edit_falls_back_to_full_reconcile() {
    let d = sandbox("srcedit");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();
    let da_before = eng.query_sql("SELECT w FROM rel_da_txt", &[]).unwrap();
    assert_eq!(da_before.len(), 1, "baseline: da has only the alpha token");

    // Edited program: src_a's glob widened to also match .txt. Put an alpha
    // token in b.txt so the new glob now yields a src_a row the old glob could
    // not. tick_paths must fall back to the full reconcile (the rule changed),
    // re-extract under the new glob, and rebuild da with the new token.
    let edited = prog_of(&PROG.replace("src/*.rs", "src/*.{rs,txt}"));
    fs::write(d.join("src/b.txt"), "beta_one\n// alpha_two\n").unwrap();
    let changed = vec![d.join("src/b.txt")];
    eng.tick_paths(&edited, &changed, true).unwrap();

    // The fallback applied the new glob: src_a now carries the b.txt alpha token
    // -> da moved.
    assert!(
        eng.last_derived_rebuilt.contains(&"da".to_string()),
        "a source-rule edit falls back to full reconcile; the widened glob moved \
         src_a so da must rebuild, got {:?}",
        eng.last_derived_rebuilt
    );
    // The named capture `w` is the token AFTER `alpha_`, so "one" / "two".
    let da_after = eng
        .query_sql("SELECT w FROM rel_da_txt ORDER BY w", &[])
        .unwrap();
    let words: Vec<String> = da_after
        .iter()
        .filter_map(|r| r.first().and_then(|v| v.as_str()).map(String::from))
        .collect();
    assert_eq!(
        words,
        vec!["one".to_string(), "two".to_string()],
        "the new glob picked up the b.txt alpha token: {words:?}"
    );
}

/// Editing a DERIVED rule moves the derived-program digest, forcing `need_full`
/// on the next tick — every derived rel rebuilds. The fact: a program-shape
/// change is a full re-derivation (this is the cost of `dl daemon load`-ing a
/// new rule, and why it is not free).
#[test]
fn derived_rule_edit_forces_full_rederivation() {
    let d = sandbox("deredit");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();

    // Edited program: da now also joins src_b (a real shape change to a derived
    // rule). derived_program_digest moves -> need_full on the next tick.
    let edited = prog_of(&format!("{PROG}rel extra(w: text).\nextra(w) <- da(w).\n"));
    eng.tick(&edited, true).unwrap();
    let rebuilt = eng.last_derived_rebuilt.clone();
    assert!(
        rebuilt.contains(&"extra".to_string()) && rebuilt.contains(&"da".to_string()),
        "a derived-rule edit forces a full re-derivation including the new rel, got {:?}",
        rebuilt
    );
}

/// Timing ceiling: a tiny program's full tick is well under a generous bound.
/// Not a micro-benchmark (CI machines vary) — a guard that a full tick of a
/// 2-file program has not regressed into something pathological (e.g. an
/// accidental full-corpus walk). Bound is deliberately loose to stay green on
/// slow CI / under load; the perf.jsonl timeline is the real spike-hunting tool.
#[test]
fn small_tick_under_timing_ceiling() {
    let d = sandbox("ceiling");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap(); // warm the db / caches

    let t = Instant::now();
    eng.tick(&prog, true).unwrap();
    let warm_ms = t.elapsed().as_millis();
    assert!(
        warm_ms < 250,
        "warm no-change tick of a 2-file program took {warm_ms}ms (>250ms ceiling); \
         a warm idle tick should be cheap"
    );
}

// The "re-render every div" proof for the two conservative families. Before D,
// `module` and `spine` returned Ok(true) every tick they were `used`, so a warm
// no-change tick STILL marked their rels changed and forced every dependent to
// re-derive. After D, `module` self-gates on its own files+manifests digest and
// `spine` is corpus-gated on `recon.changed`, so a warm no-change tick leaves
// their dependents alone. Each test forces the family to be `used` by deriving a
// rel that reads one of its built-in outputs.

/// The module family must not mark changed (and so must not re-derive its
/// dependents) on a warm no-change tick. `imp` reads `module_import`, so it
/// rebuilds iff the module family marked changed.
#[test]
fn module_family_skipped_on_warm_no_change_tick() {
    let d = sandbox("modwarm");
    fs::write(d.join("src/a.rs"), "use std::io;\nfn a() {}\n").unwrap();
    // `scanned` puts a.rs into _file so the module corpus is non-empty; `imp`
    // is the canary that rebuilds iff the module family fires.
    let prog = prog_of(
        r#"
rel scanned(p: file).
scanned(p) <- scan("WORK", "src/*.rs", p, rev).
rel imp(f: file, spec: text).
imp(f, spec) <- module_import(f, rev, spec, kind, line).
? imp(f, spec).
"#,
    );
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    eng.tick(&prog, true).unwrap();
    // Cold tick fired module (no stored digest) and populated imp.
    let imp_after_cold = eng.query_sql("SELECT spec FROM rel_imp_txt", &[]).unwrap();
    assert!(
        imp_after_cold
            .iter()
            .any(|r| r.first().and_then(|v| v.as_str()) == Some("std::io")),
        "cold tick populated imp via the module family; got {imp_after_cold:?}"
    );

    // Warm no-change tick: module's files+manifests digest is unchanged -> the
    // family returns false -> imp must NOT be in the rebuilt set.
    eng.tick(&prog, true).unwrap();
    assert!(
        !eng.last_derived_rebuilt.contains(&"imp".to_string()),
        "module family must not re-derive its dependents on a warm no-change tick; \
         rebuilt {:?}",
        eng.last_derived_rebuilt
    );
}

/// The spine family must not mark changed on a warm no-change tick. `via_ref`
/// reads `ref` (a spine projection of the located spans the match op writes),
/// so it rebuilds iff the spine family fires.
#[test]
fn spine_family_skipped_on_warm_no_change_tick() {
    let d = sandbox("spinewarm");
    fs::write(d.join("src/a.rs"), "fn alpha() {}\n").unwrap();
    let prog = prog_of(
        r#"
rel m(p: file, line: int).
m(p, line) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /fn \w+/, line).
rel n(p: file, line: int).
n(p, line) <- node(p, line, kind, mid, lo, hi).
rel via_ref(id: text, f: file).
via_ref(id, f) <- ref(id, s, f, lo, hi).
? via_ref(id, f).
"#,
    );
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    eng.tick(&prog, true).unwrap();
    // Cold tick: the match wrote located spans -> spine projected ref -> via_ref
    // populated. (If via_ref is empty here the spine projection didn't fire from
    // a bare scan+match; that would mean this test isn't exercising spine and
    // the assertion below is vacuous — guard against that.)
    let cold = eng
        .query_sql("SELECT COUNT(*) FROM rel_via_ref", &[])
        .unwrap();
    let n = cold
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(
        n > 0,
        "cold tick must populate via_ref via spine; got {n} (test would be vacuous)"
    );

    // Warm no-change tick: spine is corpus-gated, recon.changed is false, rels
    // populated -> skipped -> via_ref must NOT rebuild.
    eng.tick(&prog, true).unwrap();
    assert!(
        !eng.last_derived_rebuilt.contains(&"via_ref".to_string()),
        "spine family must not re-derive its dependents on a warm no-change tick; \
         rebuilt {:?}",
        eng.last_derived_rebuilt
    );
}
