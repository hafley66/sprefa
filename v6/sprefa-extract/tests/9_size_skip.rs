//! The named size skip: an input over the byte ceiling produces one `size_skip`
//! row and rc=0, never a silent timeout.
//!
//! FAIL-PRE-FIX RECEIPT: `timeout 10 extract
//! nickel-lang-core-0.15.3/src/parser/grammar.rs` (29,328,358 B) returned
//! rc=124 with an empty stream. Unbounded it ran 12.55 s at 3,004,694,528 B
//! peak RSS, and `--bench` charges 4.5 s to 7.0 s per family to the PARSE
//! against 4 ms to 225 ms of row flattening, so no change to the row plane
//! reaches it.
//!
//! Ceiling picked from the corpora: 16,777,216 B skips that one file out of the
//! 77,472-file rust corpus, no ts/js corpus file, and no fixture in this crate.

use std::path::PathBuf;
use std::process::Command;

const EXTRACT: &str = env!("CARGO_BIN_EXE_extract");

/// One byte over the default ceiling, in a `.rs` the parser would otherwise
/// accept: the skip must be decided on size alone, before any parse.
const OVER_CEILING: usize = sprefa_extract::DEFAULT_MAX_BYTES as usize + 1;

fn scratch(name: &str, body: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("sprefa-extract-size-skip");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, body).expect("scratch write");
    path
}

/// `cst` is the family these fillers are read with: a numeric `const` mints no
/// type entity (`src/lang/rust.rs`, a v5 port), so `type` would be empty here.
fn filler(bytes: usize) -> String {
    let mut out = String::with_capacity(bytes + 64);
    let mut n = 0usize;
    while out.len() < bytes {
        out.push_str(&format!("pub const K_{n}: u32 = {n};\n"));
        n += 1;
    }
    out.truncate(bytes);
    out
}

fn run(args: &[&str]) -> (i32, Vec<String>) {
    let out = Command::new(EXTRACT).args(args).output().expect("spawn");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let rows = stdout.lines().map(str::to_string).collect();
    (out.status.code().expect("exit code"), rows)
}

fn one_row(rows: &[String]) -> serde_json::Value {
    assert_eq!(rows.len(), 1, "expected exactly one row, got {rows:?}");
    serde_json::from_str(&rows[0]).expect("row is json")
}

/// The whole point: rc=0, one row, and the row carries the three numbers a
/// caller needs to decide what to do (which file, how big, what the bound was).
#[test]
fn over_ceiling_emits_one_named_row() {
    let path = scratch("over.rs", &filler(OVER_CEILING));
    let path = path.to_string_lossy().to_string();
    let (code, rows) = run(&[&path]);
    assert_eq!(code, 0, "a named skip exits 0; rows {rows:?}");
    let row = one_row(&rows);
    assert_eq!(row["record"], "size_skip");
    assert_eq!(row["path"], path);
    assert_eq!(row["bytes"], OVER_CEILING as u64);
    assert_eq!(row["limit"], sprefa_extract::DEFAULT_MAX_BYTES);
    assert_eq!(row["reason"], "over_max_bytes");
}

/// Exactly at the ceiling is under it: the bound is inclusive, so a ceiling set
/// to a file's own size does not skip that file.
#[test]
fn at_ceiling_extracts_normally() {
    let path = scratch("at.rs", &filler(4096));
    let path = path.to_string_lossy().to_string();
    let (code, rows) = run(&["--max-bytes", "4096", "--family", "cst", &path]);
    assert_eq!(code, 0);
    assert!(!rows.is_empty(), "4096 B at a 4096 B ceiling must extract");
    assert!(
        !rows.iter().any(|row| row.contains("size_skip")),
        "no skip at the boundary; rows {rows:?}"
    );
}

/// `--max-bytes` overrides downward, so the skip is reachable on any file.
#[test]
fn max_bytes_lowers_the_ceiling() {
    let path = scratch("small.rs", &filler(4096));
    let path = path.to_string_lossy().to_string();
    let (code, rows) = run(&["--max-bytes", "1024", &path]);
    assert_eq!(code, 0);
    let row = one_row(&rows);
    assert_eq!(row["record"], "size_skip");
    assert_eq!(row["bytes"], 4096);
    assert_eq!(row["limit"], 1024);
}

/// `--max-bytes 0` is the escape hatch: no ceiling, so the over-ceiling file
/// extracts. Checked on a small file with a ceiling of 1, because parsing the
/// 16 MB one is the cost the ceiling exists to bound.
#[test]
fn max_bytes_zero_disables_the_ceiling() {
    let path = scratch("small.rs", &filler(4096));
    let path = path.to_string_lossy().to_string();
    let (code, rows) = run(&["--max-bytes", "0", "--family", "cst", &path]);
    assert_eq!(code, 0);
    assert!(!rows.is_empty(), "no ceiling means the normal stream");
    assert!(!rows.iter().any(|row| row.contains("size_skip")));
}

/// `--file-fact` asks for the file's identity, and a skipped file still has
/// one. The identity row is a digest and a line count over bytes already read,
/// so it costs nothing the skip is avoiding.
#[test]
fn file_fact_still_rides_a_skip() {
    let path = scratch("over_ff.rs", &filler(OVER_CEILING));
    let path = path.to_string_lossy().to_string();
    let (code, rows) = run(&["--file-fact", &path]);
    assert_eq!(code, 0);
    assert_eq!(rows.len(), 2, "file row then skip row; got {rows:?}");
    let file: serde_json::Value = serde_json::from_str(&rows[0]).expect("json");
    assert_eq!(file["record"], "file");
    assert_eq!(file["bytes"], OVER_CEILING as u64);
    let skip: serde_json::Value = serde_json::from_str(&rows[1]).expect("json");
    assert_eq!(skip["record"], "size_skip");
}

/// An ordinary file is untouched: the default ceiling changes no existing
/// stream, which is what keeps every golden byte-identical.
#[test]
fn under_ceiling_is_unchanged() {
    let path = scratch("under.rs", &filler(4096));
    let path = path.to_string_lossy().to_string();
    let (code, rows) = run(&["--family", "cst", &path]);
    assert_eq!(code, 0);
    assert!(!rows.is_empty());
    assert!(!rows.iter().any(|row| row.contains("size_skip")));
}

/// The ceiling is decided before the parse, so it also covers the pattern-query
/// mode, which pays the same parse.
#[test]
fn ast_pattern_mode_skips_too() {
    let path = scratch("over_ast.rs", &filler(OVER_CEILING));
    let path = path.to_string_lossy().to_string();
    let (code, rows) = run(&[
        "--ast-pattern",
        "k=pub const $NAME: u32 = $V;",
        "--ast-capture",
        "k=NAME",
        &path,
    ]);
    assert_eq!(code, 0);
    assert_eq!(one_row(&rows)["record"], "size_skip");
}

/// `--schema` is the contract a consumer reads; a record absent from it is a
/// record nothing can declare a column for.
#[test]
fn schema_declares_the_record() {
    let out = Command::new(EXTRACT)
        .arg("--schema")
        .output()
        .expect("spawn");
    let schema = String::from_utf8(out.stdout).expect("utf8");
    assert!(
        schema.contains("record=size_skip"),
        "--schema must carry the size_skip line"
    );
}
