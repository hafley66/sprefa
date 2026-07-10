//! Derived shapes (2026-07-09, Phase 5): the `type_decl_row(shape, pos, col, type)`
//! sink lets a DERIVED rule compute a relation schema. The engine persists the
//! sink's rows to the `_shapes` meta table at the END of a tick; a
//! `rel name: shape.` decl whose shape has no syntax `type name(...)` resolves
//! its columns from those rows at the NEXT tick's declare — a one-tick phase
//! delay (the @next carry precedent). Until then the rel is pending: rules
//! heading it are skipped, queries over it fail loudly, and --check carries a
//! `shape-pending` info diag. Syntax shapes win a name clash (`shape-shadowed`
//! warn); an unknown type keeps the shape pending (`shape-unknown-type` warn).
//! Same sandbox harness as tests/it/type_decls.rs; each tick = one `dl` run
//! against the same --db.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_decl_row_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

fn check(dir: &Path, prog: &str) -> (i32, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .args(["--check", dir.join("p.dl").to_str().unwrap()])
        .args(["--db", dir.join("db").to_str().unwrap(), "--no-daemon"])
        .current_dir(dir)
        .output().expect("run dl --check");
    (out.status.code().unwrap_or(-1),
     format!("{}{}",
         String::from_utf8_lossy(&out.stdout),
         String::from_utf8_lossy(&out.stderr)))
}

/// The base program: a derived rule computes the `point` shape and a
/// `rel point_rel: point.` references it before it exists.
const POINT: &str = concat!(
    "rel col_spec(shape: text, pos: int, col_name: text, type: text).\n",
    "col_spec(\"point\", 0, \"x\", \"int\").\n",
    "col_spec(\"point\", 1, \"y\", \"int\").\n",
    "type_decl_row(shape, pos, col, type) <- col_spec(shape, pos, col, type).\n",
    "rel point_rel: point.\n",
    "point_rel(3, 4).\n",
    "? point_rel(x, y).\n");

/// Tick 1 the shape rel is pending (query fails loudly, run still exits 0);
/// tick 2 it resolves from the persisted `_shapes` rows and the fact heading it
/// runs — the one-tick phase delay end to end.
#[test]
fn derived_shape_resolves_on_next_tick() {
    let dir = sandbox("resolve");
    let (code1, _out1, err1) = run(&dir, POINT);
    assert_eq!(code1, 0, "tick 1 pending is loud but non-fatal:\n{err1}");
    assert!(err1.contains("unknown relation point_rel"),
        "tick 1 the rel does not exist yet:\n{err1}");
    let (code2, out2, err2) = run(&dir, POINT);
    assert_eq!(code2, 0, "{err2}");
    assert!(out2.contains("(1 rows)") && out2.contains("3\t4"),
        "tick 2 the derived shape is live and the fact row landed:\n{out2}");
}

/// --check on tick 1 carries the `shape-pending` info diag naming the rel and
/// the shape; on tick 2 (shape persisted by the first check's tick) it is gone.
#[test]
fn shape_pending_diag_then_clears() {
    let dir = sandbox("pending");
    let prog = concat!(
        "rel col_spec(shape: text, pos: int, col_name: text, type: text).\n",
        "col_spec(\"point\", 0, \"x\", \"int\").\n",
        "type_decl_row(shape, pos, col, type) <- col_spec(shape, pos, col, type).\n",
        "rel point_rel: point.\n");
    let (_code1, out1) = check(&dir, prog);
    assert!(out1.contains("shape-pending") && out1.contains("point_rel"),
        "tick 1 --check names the pending rel:\n{out1}");
    let (code2, out2) = check(&dir, prog);
    assert_eq!(code2, 0, "{out2}");
    assert!(!out2.contains("shape-pending"),
        "tick 2 the shape resolved, no pending diag:\n{out2}");
}

/// A syntax `type point(...)` and derived rows sharing the name: syntax wins
/// (the rel works with the syntax columns on tick 1 already) and once the
/// derived rows persist, --check carries the `shape-shadowed` warn.
#[test]
fn syntax_shape_shadows_derived() {
    let dir = sandbox("shadow");
    let prog = concat!(
        "type point(x: int, y: int).\n",
        "rel col_spec(shape: text, pos: int, col_name: text, type: text).\n",
        "col_spec(\"point\", 0, \"only_x\", \"int\").\n",
        "type_decl_row(shape, pos, col, type) <- col_spec(shape, pos, col, type).\n",
        "rel point_rel: point.\n",
        "point_rel(1, 2).\n",
        "? point_rel(x, y).\n");
    let (code1, out1, err1) = run(&dir, prog);
    assert_eq!(code1, 0, "{err1}");
    assert!(out1.contains("(1 rows)"),
        "syntax shape resolves immediately, tick 1:\n{out1}");
    let (_code2, out2) = check(&dir, prog);
    assert!(out2.contains("shape-shadowed"),
        "persisted derived rows under a syntax shape name warn:\n{out2}");
}

/// A derived shape row naming an unknown type: `shape-unknown-type` warn, and the
/// shape never persists, so the rel referencing it stays pending on tick 2.
#[test]
fn unknown_ty_keeps_shape_pending() {
    let dir = sandbox("badty");
    let prog = concat!(
        "rel col_spec(shape: text, pos: int, col_name: text, type: text).\n",
        "col_spec(\"point\", 0, \"x\", \"flavor\").\n",
        "type_decl_row(shape, pos, col, type) <- col_spec(shape, pos, col, type).\n",
        "rel point_rel: point.\n");
    let (_code1, out1) = check(&dir, prog);
    assert!(out1.contains("shape-unknown-type") && out1.contains("flavor"),
        "the unknown type is named:\n{out1}");
    let (_code2, out2) = check(&dir, prog);
    assert!(out2.contains("shape-pending"),
        "the shape never persisted, still pending on tick 2:\n{out2}");
    assert!(out2.contains("shape-unknown-type"),
        "the type warn is steady, not one-shot:\n{out2}");
}

/// `rel type_decl_row(...)` bails: the sink is reserved, head it directly.
#[test]
fn rel_decl_of_the_sink_bails() {
    let dir = sandbox("redecl");
    let prog = "rel type_decl_row(shape: text, pos: int, col: text, type: text).\n";
    let (code, _out, err) = run(&dir, prog);
    assert_ne!(code, 0, "reserved-name decl must fail");
    assert!(err.contains("derived-shape sink") && err.contains("head it directly"),
        "{err}");
}

/// A shape's row set changing = column drift on the rel using it: the stale
/// schema serves one more tick (loud arity failure), then the table migrates
/// and re-derives with the new column set.
#[test]
fn shape_row_change_migrates_the_rel() {
    let dir = sandbox("migrate");
    let two_cols = concat!(
        "rel col_spec(shape: text, pos: int, col_name: text, type: text).\n",
        "col_spec(\"point\", 0, \"x\", \"int\").\n",
        "col_spec(\"point\", 1, \"y\", \"int\").\n",
        "type_decl_row(shape, pos, col, type) <- col_spec(shape, pos, col, type).\n",
        "rel point_rel: point.\n",
        "? point_rel(x, y).\n");
    run(&dir, two_cols);
    let (code2, out2, err2) = run(&dir, two_cols);
    assert_eq!(code2, 0, "{err2}");
    assert!(out2.contains("(0 rows)"), "2-col shape live on tick 2:\n{out2}");

    let three_cols = concat!(
        "rel col_spec(shape: text, pos: int, col_name: text, type: text).\n",
        "col_spec(\"point\", 0, \"x\", \"int\").\n",
        "col_spec(\"point\", 1, \"y\", \"int\").\n",
        "col_spec(\"point\", 2, \"z\", \"int\").\n",
        "type_decl_row(shape, pos, col, type) <- col_spec(shape, pos, col, type).\n",
        "rel point_rel: point.\n",
        "? point_rel(x, y, z).\n");
    let (code3, _out3, err3) = run(&dir, three_cols);
    assert_eq!(code3, 0, "{err3}");
    assert!(err3.contains("expects 2 cols, got 3"),
        "tick 3 still serves the stale 2-col shape, loudly:\n{err3}");
    let (code4, out4, err4) = run(&dir, three_cols);
    assert_eq!(code4, 0, "{err4}");
    assert!(out4.contains("(0 rows)"),
        "tick 4 the table migrated to the 3-col shape:\n{out4}");
}

/// The shipped example end to end: a schema inferred from a JSON sample (int
/// when every observed value is an integer, else text) becomes a live checked
/// rel on tick 2, filled by jsonp-extracted rows.
#[test]
fn json_inference_example_end_to_end() {
    let dir = sandbox("json_example");
    let example = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/type-from-json.dl")).unwrap();
    run(&dir, &example);
    let (code2, out2, err2) = run(&dir, &example);
    assert_eq!(code2, 0, "{err2}");
    assert!(out2.contains("age\tcity\tname"),
        "inferred columns in alphabetical order:\n{out2}");
    assert!(out2.contains("30\tparis\talice") && out2.contains("(1 rows)"),
        "the shaped row landed:\n{out2}");
}

/// The mapped-type trick: `type_decl_row("partial_" + rel, ...)` over the
/// `rel_col` catalog mints a partial_* shape per built-in relation, usable by a
/// `rel _: partial_<name>.` decl one tick later.
#[test]
fn mapped_type_mints_partial_shapes() {
    let dir = sandbox("mapped");
    let prog = concat!(
        "type_decl_row(\"partial_\" + rel, pos, col, type) <- rel_col(rel, pos, col, type, _).\n",
        "rel partial_checkout: partial_checkout_done.\n",
        "? partial_checkout(repo, branch, action, ok, detail).\n");
    run(&dir, prog);
    let (code2, out2, err2) = run(&dir, prog);
    assert_eq!(code2, 0, "{err2}");
    assert!(out2.contains("repo\tbranch\taction\tok\tdetail") && out2.contains("(0 rows)"),
        "checkout_done's 5 columns copied into the derived partial shape:\n{out2}");
}
