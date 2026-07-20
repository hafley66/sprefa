//! INV-1: no rev column in any table ever holds the `WORK` alias.
//!
//! `WORK` is surface syntax resolved at `Engine::resolve_rev`; every stored rev
//! matches `^[0-9a-f]{40}\+?$` (HEAD's oid, `+` when the working tree differs).
//! The rail is a Rust test rather than a `.dl` program because the assertion
//! ranges over engine-internal tables (`_file`) that no `.dl` rel exposes.
//!
//! Second test: the `_file` mtime/size fast path. `enumerate_with_hash` probes
//! the prior `_file` set with a `(repo, path, rev)` key. That key held a
//! hardcoded `"WORK"` before this arc; leaving it would make every probe miss
//! against oid-bearing rows and re-read + re-hash the whole corpus on every
//! tick, with no compile error and no other test failing.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROG: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "src/**/*.rs", path, rev), match(path, rev, /fn/, line).
seen(path) <- scan("HEAD", "src/**/*.rs", path, rev), match(path, rev, /fn/, line).
# Consuming the twins is what makes the type/call families actually run, so the
# invariant is checked against populated tables rather than empty ones.
rel ent(sym: text, rev: text).
ent(sym, rev) <- type_entity_rev(_, sym, _, _, _, _, _, rev).
rel def(sym: text, rev: text).
def(sym, rev) <- call_def_rev(_, sym, _, _, _, _, rev).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rev_alias_leak_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

fn commit_fixture(dir: &Path) {
    fs::write(dir.join("src/a.rs"), "pub struct Alpha;\npub fn alpha() {}\n").unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "T"]);
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
}

fn column(eng: &Engine, sql: &str) -> Vec<String> {
    eng.query_sql(sql, &[]).unwrap().into_iter()
        .map(|row| match &row[0] {
            serde_json::Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .collect()
}

fn is_stored_rev(text: &str) -> bool {
    let oid = text.strip_suffix('+').unwrap_or(text);
    oid.len() == 40 && oid.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[test]
fn no_stored_rev_holds_the_work_alias() {
    let dir = sandbox("inv1");
    commit_fixture(&dir);

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.tick(&prog, true).unwrap();

    let file_revs = column(&eng, "SELECT DISTINCT rev FROM _file");
    assert_eq!(file_revs.len(), 2, "the corpus spans the worktree and HEAD: {file_revs:?}");
    for rev in &file_revs {
        assert!(is_stored_rev(rev), "_file.rev holds a non-rev value: {rev:?}");
    }
    assert!(file_revs.iter().any(|rev| rev.ends_with('+')),
        "the worktree rev carries the dirty marker (the db files are untracked): {file_revs:?}");

    for table in [
        "rel_rev_txt",
        "rel_type_entity_rev_txt",
        "rel_call_def_rev_txt",
    ] {
        let column_name = if table == "rel_rev_txt" { "oid" } else { "rev" };
        let revs = column(&eng, &format!("SELECT DISTINCT {column_name} FROM {table}"));
        assert!(!revs.is_empty(), "{table} is populated");
        for rev in &revs {
            assert!(is_stored_rev(rev), "{table}.{column_name} holds a non-rev value: {rev:?}");
        }
    }
}

#[test]
fn a_second_tick_over_an_unchanged_corpus_rehashes_nothing() {
    let dir = sandbox("fastpath");
    commit_fixture(&dir);
    // Push the fixture's mtime clear of the racy window (`_file_walk.ref_secs`
    // disqualifies a cached hash whose mtime shares the walk's second), so the
    // fast path is reachable without a wall-clock wait. Same technique as
    // racy_mtime.rs, pointed the other way.
    let stamp = std::time::SystemTime::now() - std::time::Duration::from_secs(30);
    fs::File::open(dir.join("src/a.rs")).unwrap().set_modified(stamp).unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.tick(&prog, true).unwrap();
    let hashes_after_first = column(&eng, "SELECT hash FROM _file ORDER BY repo, path, rev");

    // The mtime/size fast path only engages outside the racy window, which the
    // walk-reference second advances tick over tick, so give it one warm-up
    // tick. Then the signal that matters: zero files re-read and re-hashed.
    // `extract_files_parsed` cannot see this — a re-hash of unchanged bytes
    // produces the identical hash, so no digest moves and no family re-parses.
    // A probe keyed on the `WORK` alias instead of the stored rev would miss
    // every `_file` row and re-read the whole corpus here, silently.
    eng.tick(&prog, true).unwrap();
    let reads_before = eng.file_hash_reads.get();
    eng.tick(&prog, true).unwrap();

    assert_eq!(eng.file_hash_reads.get(), reads_before,
        "an unchanged corpus re-hashes nothing on the next tick");
    assert_eq!(column(&eng, "SELECT hash FROM _file ORDER BY repo, path, rev"), hashes_after_first,
        "the stored content hashes are reused, not recomputed into new rows");
    assert!(hashes_after_first.iter().all(|hash| !hash.is_empty()),
        "every _file row carries a content hash: {hashes_after_first:?}");
}

/// A dirty working tree whose scan touches a path absent from HEAD.
///
/// Regression: `read_content` chose its read path by comparing the rev against
/// the alias text. Once the alias resolved, a dirty rev (`<sha>+`) failed that
/// compare and fell through to `git cat-file <sha>+:<path>`, which can never
/// resolve — `<sha>+` is a display encoding, not a git object name. Nearly
/// every caller swallows a read error with `unwrap_or_default`, so the symptom
/// was empty content rather than a failure; only the call family's delta route
/// propagates, which is why it surfaced intermittently. `GitOid` now makes the
/// display form unable to reach a git subprocess at all.
#[test]
fn a_dirty_rev_reads_from_disk_instead_of_shelling_out_to_git() {
    let dir = sandbox("dirty");
    commit_fixture(&dir);
    // Untracked: absent from HEAD, and the reason the tree reads dirty.
    fs::write(dir.join("src/b.rs"), "pub fn beta_only_on_disk() {}\n").unwrap();

    let prog_text = r#"
rel hit(path: file, line: int).
hit(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
                   match(path, rev, /beta_only_on_disk/, line).
"#;
    let prog = parse::parse(lex::lex(prog_text).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.tick(&prog, true).unwrap();

    let revs = column(&eng, "SELECT DISTINCT rev FROM _file");
    assert_eq!(revs.len(), 1);
    assert!(revs[0].ends_with('+'), "the tree is dirty, so the rev carries the marker: {revs:?}");

    let paths = column(&eng, "SELECT path FROM rel_hit_txt");
    assert_eq!(paths, vec!["src/b.rs".to_string()],
        "a file present only on disk is read from disk, not from a git object");
}
