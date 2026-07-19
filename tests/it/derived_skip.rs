//! Digest-before-write for derived rel rebuilds (failure-modes gap rows 3/7:
//! ~4MB of WAL churn per NO-OP tick on a clock-fed program, 2026-07-18).
//!
//! The fix: a non-recursive component evaluates into a TEMP mirror table
//! (SQLite TEMP schema has its own pager under `temp_store=FILE`, so mirror
//! writes never reach the main-db WAL), compares mirror vs live, and only a
//! DIFFERENT rowset pays the unmark/wipe/refill/mark bracket. An identical
//! re-derivation writes NOTHING to the main db and never enters the crash
//! window.
//!
//! `unchanged_rederive_writes_zero_wal_bytes` is the fail-pre-fix receipt: it
//! uses only pre-fix APIs (tick / count_rows / the WAL file on disk), so
//! stashing the engine change makes it fail with megabytes of WAL where the
//! fixed engine writes kilobytes.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("derived_skip_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

// The storm shape: `flag_word` is the clock-analog input that MOVES every
// tick (a new row lands), while the downstream derived rels re-derive to
// IDENTICAL contents. `word_pair` is a 400x400 self-join = 160_000 rows, so a
// pre-fix wipe+reinsert of it towers over the tick's fixed WAL noise floor
// (declare_all's per-tick view DROP+CREATE schema churn is ~2.5MB by itself).
const PROG: &str = r#"
rel stable_word(path: file, word: text).
stable_word(path, word) <- scan("WORK", "stable.txt", path, rev), match(path, rev, /w_(?<word>\w+)/, line).
rel flag_word(path: file, word: text).
flag_word(path, word) <- scan("WORK", "flag.txt", path, rev), match(path, rev, /f_(?<word>\w+)/, line).
rel marker(tag: text).
marker("go") <- flag_word(_, _).
rel word_pair(word_a: text, word_b: text).
word_pair(word_a, word_b) <- stable_word(_, word_a), stable_word(_, word_b), marker("go").
"#;

fn write_corpus(dir: &Path) {
    let stable: String = (0..400).map(|n| format!("w_{n:03}\n")).collect();
    fs::write(dir.join("stable.txt"), stable).unwrap();
    fs::write(dir.join("flag.txt"), "f_start\n").unwrap();
}

fn fresh_engine(dir: &Path) -> (Engine, sprefa_v5::ast::Program) {
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    (Engine::new(conn, dir.to_path_buf()), prog)
}

fn append_flag(dir: &Path, token: &str) {
    let mut flag = fs::OpenOptions::new().append(true).open(dir.join("flag.txt")).unwrap();
    writeln!(flag, "{token}").unwrap();
}

fn wal_bytes(dir: &Path) -> u64 {
    fs::metadata(dir.join("db-wal")).map(|meta| meta.len()).unwrap_or(0)
}

/// THE fail-pre-fix receipt. A tick whose changed input re-derives
/// `marker`/`word_pair` to identical contents must leave the main db's WAL
/// (checkpoint-truncated to zero right before the tick) in the tens of
/// kilobytes — source-side bookkeeping only. The pre-fix engine DELETEs and
/// re-INSERTs word_pair's 160_000 rows, which lands megabytes in the WAL.
#[test]
fn unchanged_rederive_writes_zero_wal_bytes() {
    let d = sandbox("wal");
    write_corpus(&d);
    let (mut eng, prog) = fresh_engine(&d);

    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.count_rows("word_pair").unwrap(), 160_000, "cold tick derives the pair table");
    assert_eq!(eng.count_rows("marker").unwrap(), 1);

    // Zero the WAL so tick 2's write churn is the only thing in it.
    eng.query_sql("PRAGMA wal_checkpoint(TRUNCATE)", &[]).unwrap();
    assert_eq!(wal_bytes(&d), 0, "checkpoint TRUNCATE must zero the WAL");

    // The storm input: flag.txt moves (flag_word gains a row), but marker
    // stays {"go"} and word_pair stays the same 160k rows.
    append_flag(&d, "f_more");
    eng.tick(&prog, true).unwrap();

    assert_eq!(eng.count_rows("word_pair").unwrap(), 160_000, "identical re-derivation kept the rows");
    assert_eq!(eng.count_rows("marker").unwrap(), 1);

    let churn = wal_bytes(&d);
    eprintln!("[receipt] no-op re-derive WAL churn: {churn} bytes");
    // Measured receipts (this tree, 2026-07-18): no derived skip
    // 11,470,112 bytes (word_pair's 160k-row wipe+reinsert; identical under
    // the DL_NO_DERIVED_SKIP=1 lever); derived skip alone 2,484,392 bytes
    // (declare_all's unconditional per-rel view DROP+CREATE schema churn);
    // derived skip + unchanged-view skip 222,512 bytes (residual source-side
    // bookkeeping; the derived phase itself adds ~16KB). The 1MB bound is a
    // ~4.5x margin over the measured floor and trips hard on either
    // regression (+2.3MB view churn, +9MB derived rewrite).
    assert!(churn < 1024 * 1024,
        "a no-op re-derive tick must stay under the quiet-tick write budget: \
         WAL grew by {churn} bytes (view-churn regression ~2.5MB, derived-rewrite regression ~11.5MB)");
}

/// The skip is attributed: a tick that re-derives to identical contents lists
/// the rels under `last_derived_skipped` (and perf.jsonl's `derived.skipped`
/// mirrors that field), while `last_derived_rebuilt` keeps its meaning of
/// "rels whose rebuild pass RAN". A genuinely changed input still rewrites.
#[test]
fn skip_is_attributed_and_changed_inputs_still_rewrite() {
    let d = sandbox("attr");
    write_corpus(&d);
    let (mut eng, prog) = fresh_engine(&d);

    // Cold tick: live tables are empty, so nothing can be skipped.
    eng.tick(&prog, true).unwrap();
    assert!(eng.last_derived_skipped.is_empty(),
        "a cold tick fills empty tables, nothing is skippable, got {:?}",
        eng.last_derived_skipped);

    // No-op re-derive: both affected rels run and both skip the write.
    append_flag(&d, "f_more");
    eng.tick(&prog, true).unwrap();
    let mut rebuilt = eng.last_derived_rebuilt.clone();
    rebuilt.sort();
    assert_eq!(rebuilt, vec!["marker".to_string(), "word_pair".to_string()],
        "the flag edit reaches both derived rels");
    let mut skipped = eng.last_derived_skipped.clone();
    skipped.sort();
    assert_eq!(skipped, vec!["marker".to_string(), "word_pair".to_string()],
        "identical re-derivations must be attributed as skipped");

    // A change that MOVES the derived contents still lands: no f_ token left
    // means marker (and so word_pair) derive to empty.
    fs::write(d.join("flag.txt"), "nothing here\n").unwrap();
    eng.tick(&prog, true).unwrap();
    assert!(eng.last_derived_skipped.is_empty(),
        "a moved rowset must rewrite, not skip, got {:?}", eng.last_derived_skipped);
    assert_eq!(eng.count_rows("marker").unwrap(), 0, "marker re-derived to empty");
    assert_eq!(eng.count_rows("word_pair").unwrap(), 0, "word_pair re-derived to empty");
}

/// tick_paths (the watcher path) shares `rebuild_derived`, so the same edit
/// arriving as a changed-path delta also skips the rewrite.
#[test]
fn tick_paths_shares_the_skip() {
    let d = sandbox("paths");
    write_corpus(&d);
    let (mut eng, prog) = fresh_engine(&d);
    eng.tick(&prog, true).unwrap();

    append_flag(&d, "f_more");
    let changed = vec![d.join("flag.txt")];
    eng.tick_paths(&prog, &changed, true).unwrap();
    let mut skipped = eng.last_derived_skipped.clone();
    skipped.sort();
    assert_eq!(skipped, vec!["marker".to_string(), "word_pair".to_string()],
        "the incremental path must skip identical re-derivations too");
    assert_eq!(eng.count_rows("word_pair").unwrap(), 160_000);
}
