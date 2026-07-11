//! Mixed source+derived / extract+derived rel desugar
//! (plans/2026-07-10-mixed-rel-desugar.md).
//!
//! A relation headed by both a source-shaped rule (scan/match/ast/sg/json/cmd/
//! comment) and a derived rule -- or by a term-extract rule (json/jsonp body
//! form) and a derived rule -- USED to bail loudly and tell the user to split
//! it into two relations and union them by hand. The engine now performs that
//! split ITSELF: hidden `<rel>__src`/`<rel>__drv` twins plus a synthesized
//! union, invisibly and deterministically (see `src/engine/desugar.rs`). These
//! are the flipped tests: what used to assert the bail now asserts the union
//! works, survives a `--changed` retick, and reaches a fixpoint through a
//! self-recursive read of the visible rel. The lattice exclusion is the one
//! combination still refused (its own bail).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mixed_source_derived_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output()
        .expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

fn strs(db_path: &Path, rel: &str, col: &str) -> Vec<String> {
    let conn = db::open(Some(db_path.to_str().unwrap())).unwrap();
    let mut s = conn.prepare(&format!("SELECT \"{col}\" FROM rel_{rel} ORDER BY \"{col}\"")).unwrap();
    let mut v: Vec<String> = s.query_map([], |r| r.get(0)).unwrap().filter_map(|x| x.ok()).collect();
    v.sort();
    v
}

// ---- D1: source + derived --------------------------------------------------

/// The core proof: a rel headed by both a scan-sourced rule and a plain
/// derived rule no longer bails. The union holds BOTH the scanned row and the
/// derived-only row after one tick, and a `--changed` retick that deletes the
/// scanned file retracts ONLY the scanned side -- the derived-only row
/// survives untouched.
#[test]
fn source_plus_derived_unions_and_retracts_scanned_side_only() {
    let d = sandbox("union_src");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.txt"), "hello\n").unwrap();
    fs::write(d.join("p.dl"),
        "rel other(tag: text).\n\
         other(\"derived-only\").\n\
         rel mixed(x: text).\n\
         mixed(p) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /./, line).\n\
         mixed(x) <- other(x).\n").unwrap();
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    assert!(diags.iter().all(|dd| dd.severity != sprefa_v5::ast::Severity::Error), "unexpected diags: {diags:?}");

    let dbp = d.join("db");
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    assert_eq!(strs(&dbp, "mixed", "x"), vec!["derived-only".to_string(), "src/a.txt".to_string()],
        "the union must hold both the scanned row and the derived-only row");

    // Delete the scanned file and retick incrementally: only the scanned side
    // retracts (this used to be the exact silent-loss repro; now it is the
    // positive proof).
    fs::remove_file(d.join("src/a.txt")).unwrap();
    eng.tick_paths(&prog, &[d.join("src/a.txt")], true).unwrap();
    assert_eq!(strs(&dbp, "mixed", "x"), vec!["derived-only".to_string()],
        "the scanned row must retract while the derived-only row survives");
}

/// A derived rule on the mixed rel that reads the mixed rel itself
/// (self-recursive through the synthesized union) reaches a fixpoint seeded by
/// the scanned row -- `rel_components`/`stratify` treat the visible rel <->
/// its `__drv` twin as an ordinary recursive component, no special-casing
/// needed.
#[test]
fn recursive_derived_rule_over_mixed_rel_reaches_fixpoint_from_scanned_seed() {
    let d = sandbox("recursive");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/seed.txt"), "n1\n").unwrap();
    fs::write(d.join("p.dl"),
        "rel step(a: text, b: text).\n\
         step(\"n1\", \"n2\").\n\
         step(\"n2\", \"n3\").\n\
         step(\"n3\", \"n4\").\n\
         rel mixed(x: text).\n\
         mixed(seed) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /(?<seed>.+)/, line).\n\
         mixed(b) <- mixed(a), step(a, b).\n").unwrap();
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    assert!(diags.iter().all(|dd| dd.severity != sprefa_v5::ast::Severity::Error), "unexpected diags: {diags:?}");

    let dbp = d.join("db");
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    assert_eq!(strs(&dbp, "mixed", "x"),
        vec!["n1".to_string(), "n2".to_string(), "n3".to_string(), "n4".to_string()],
        "recursion through the union must chain from the scanned seed to a fixpoint");
}

// ---- D2: term-extract + derived --------------------------------------------

/// The term-extract twin of the same hazard: a rel headed by a term-form
/// `jsonp` rule and a plain derived rule no longer bails either. The union
/// holds both the extracted value and the derived-only value.
#[test]
fn extract_plus_derived_unions_both_rows() {
    let d = sandbox("union_extract");
    let (code, out, err) = run(&d,
        "rel src(x: text).\n\
         src(\"keep\").\n\
         rel body_rel(b: text).\n\
         body_rel(\"{\\\"n\\\": 7}\").\n\
         rel mixed(v: text).\n\
         mixed(n) <- body_rel(b), jsonp(b, \"n\", n).\n\
         mixed(x) <- src(x).\n\
         ? mixed(v).\n");
    assert_eq!(code, 0, "stderr: {err}");
    assert!(out.contains("keep"), "derived-side row missing from union:\n{out}");
    assert!(out.contains('7'), "extract-side row missing from union:\n{out}");
}

// ---- exclusions kept -------------------------------------------------------

/// The one combination still refused: a lattice (`key`/`merge`) rel mixed with
/// a source rule. The union step's upsert-winner semantics are order-
/// dependent and not designed yet.
#[test]
fn lattice_mixed_rel_still_bails() {
    let d = sandbox("lattice_bail");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.txt"), "hello\n").unwrap();
    let (code, _out, err) = run(&d,
        "rel mixed(k: text, v: int) key(k) merge(MaxBy(v)).\n\
         mixed(p, 1) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /./, line).\n\
         mixed(k, 2) <- mixed(k, _).\n");
    assert_ne!(code, 0, "expected a non-zero exit");
    assert!(err.contains("lattice rels cannot be mixed yet") && err.contains("mixed"),
        "expected the narrowed lattice-exclusion bail naming 'mixed'; got: {err}");
}

/// The sanctioned manual split (extract into its own rel, derive into another,
/// union in a third) still works unchanged -- no rel mixes rule kinds, so the
/// desugar never engages.
#[test]
fn split_extract_and_derived_into_two_rels_is_fine() {
    let d = sandbox("extract_split");
    let (code, _out, err) = run(&d,
        "rel body_rel(b: text).\n\
         body_rel(\"{\\\"n\\\": 7}\").\n\
         rel xrow(v: text).\n\
         xrow(n) <- body_rel(b), jsonp(b, \"n\", n).\n\
         rel src(x: text).\n\
         src(\"keep\").\n\
         rel both(v: text).\n\
         both(v) <- xrow(v).\n\
         both(v) <- src(v).\n\
         ? both(v).\n");
    assert_eq!(code, 0, "split program should succeed; stderr: {err}");
}

/// Same for the source+derived manual split.
#[test]
fn split_source_and_derived_into_two_rels_is_fine() {
    let d = sandbox("split");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.txt"), "hello\n").unwrap();
    let (code, _out, err) = run(&d,
        "rel spin(p: file).\n\
         spin(p) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /./, line).\n\
         rel dpin(p: file).\n\
         dpin(p) <- spin(p).\n\
         rel both(p: file).\n\
         both(p) <- spin(p).\n\
         both(p) <- dpin(p).\n\
         ? both(p).\n");
    assert_eq!(code, 0, "split program should succeed; stderr: {err}");
}

// ---- D4: telemetry twin-name mapping ---------------------------------------

/// `rel_count`/`stmt_ms` must report a mixed rel under its own visible name
/// only -- the hidden `__src`/`__drv` twins (`engine::desugar::display_rel_name`)
/// are folded in, never surfaced as their own rows (src/rels/perf.rs
/// `fold_twins`). Runs the mixed program TWICE (`stmt_ms` is empty until a
/// derived rebuild has landed once; the daemon-equivalent second tick is a
/// second one-shot invocation against the same `--db`).
#[test]
fn rel_count_and_stmt_ms_never_leak_twin_names() {
    let d = sandbox("telemetry");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.txt"), "hello\n").unwrap();
    let program =
        "rel other(tag: text).\n\
         other(\"derived-only\").\n\
         rel mixed_telemetry(x: text).\n\
         mixed_telemetry(p) <- scan(\"WORK\", \"src/**/*.txt\", p, rev), match(p, rev, /./, line).\n\
         mixed_telemetry(x) <- other(x).\n\
         ? rel_count(rel, rows).\n\
         ? stmt_ms(rel, ms).\n";
    let (code, _out, err) = run(&d, program);
    assert_eq!(code, 0, "stderr: {err}");
    // Second invocation against the same db: stmt_ms only populates once a
    // derived rebuild has landed (see perf.rs's doc comment).
    let (code, out, err) = run(&d, program);
    assert_eq!(code, 0, "stderr: {err}");
    assert!(!out.contains("__src"), "rel_count/stmt_ms leaked a __src twin name:\n{out}");
    assert!(!out.contains("__drv"), "rel_count/stmt_ms leaked a __drv twin name:\n{out}");
    assert!(out.contains("mixed_telemetry"),
        "expected the visible rel name to appear in rel_count/stmt_ms output:\n{out}");
}
