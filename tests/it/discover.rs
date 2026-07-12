//! Repo-local program discovery: `dl` with no positional resolves
//! `<root>/.dl/*.dl` (lexicographic), merges the files into one program, and
//! defaults the db to `<root>/.dl/.state/cache.db`. A missing or empty `.dl` dir is a
//! loud error so a typo'd directory never makes `--check` pass green by
//! checking nothing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("discover_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run `dl` with NO program positional against `dir` as root. Hermetic:
/// `DL_NO_DAEMON=1` so these program-discovery/db-defaulting checks never
/// reach toward a real developer daemon (fixed after a live-daemon-pollution
/// incident: the discovery-mode --check daemon-first fix made this file's
/// unguarded invocations daemon-eligible for the first time).
fn run(dir: &Path, extra: &[&str]) -> (i32, String, String) {
    let out = Command::new(DL)
        .current_dir(dir)
        .env("DL_NO_DAEMON", "1")
        .args(extra)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Two rail files, each writing the built-in `diag` sink (no decl — `diag` is a
/// fixed-schema built-in). `10-` / `20-` prefixes pin the lexicographic merge
/// order. This is exactly the shape that USED to collide when `diag` was a
/// magic user-declared name: two files declaring it with (even identical)
/// columns fought over one merged namespace. Now there is nothing to declare.
fn fixture(tag: &str) -> PathBuf {
    let d = sandbox(tag);
    fs::create_dir_all(d.join(".dl")).unwrap();
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\nlet beta = 1;\n").unwrap();
    fs::write(d.join(".dl/10-a.dl"), concat!(
        "rel a_hit(p: file, l: int).\n",
        "a_hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /alpha/, l).\n",
        "diag(path: p, line: l, severity: \"error\", code: \"rail-a\", msg: \"alpha found\") <- a_hit(p, l).\n",
    )).unwrap();
    fs::write(d.join(".dl/20-b.dl"), concat!(
        "rel b_hit(p: file, l: int).\n",
        "b_hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /beta/, l).\n",
        "diag(path: p, line: l, severity: \"warn\", code: \"rail-b\", msg: \"beta found\") <- b_hit(p, l).\n",
    )).unwrap();
    d
}

/// (1) Both files contribute rules; both write the one built-in `diag` sink
/// with no decl and no collision. rail-a is error severity, so --check exits
/// non-zero.
#[test]
fn discovery_merges_files_and_dedupes_identical_decls() {
    let d = fixture("merge");
    let (code, _out, err) = run(&d, &["--check"]);
    assert_eq!(code, 2, "rail-a is error severity -> blocking-hook exit code:\n{err}");
    assert!(err.contains("rail-a"), "rule from 10-a.dl must fire:\n{err}");
    assert!(err.contains("rail-b"), "rule from 20-b.dl must fire:\n{err}");
}

/// (2) No `.dl` dir at all: loud error naming the missing directory.
#[test]
fn missing_dl_dir_is_a_loud_error() {
    let d = sandbox("nodir");
    let (code, _out, err) = run(&d, &["--check"]);
    assert_ne!(code, 0);
    assert!(err.contains(".dl"), "error must name the missing dir:\n{err}");
}

/// (3) `.dl` exists but holds no .dl files: also loud, never a green no-op.
#[test]
fn empty_dl_dir_is_a_loud_error() {
    let d = sandbox("empty");
    fs::create_dir_all(d.join(".dl")).unwrap();
    let (code, _out, err) = run(&d, &["--check"]);
    assert_ne!(code, 0);
    assert!(err.contains("no .dl files"), "error must say the dir is empty:\n{err}");
}

/// (4) The same relation declared with different columns across files is a
/// conflict, not a silent last-wins. `a_hit` is declared `(p: file, l: int)` in
/// 10-a.dl; 30-c.dl re-declares it with a different shape.
#[test]
fn conflicting_decl_across_files_errors() {
    let d = fixture("conflict");
    fs::write(d.join(".dl/30-c.dl"), "rel a_hit(path: text, line: int, extra: text).\n").unwrap();
    let (code, _out, err) = run(&d, &["--check"]);
    assert_eq!(code, 1, "a broken program is exit 1 (user-facing), not 2:\n{err}");
    assert!(err.contains("declared twice"), "conflict must be named:\n{err}");
}

/// (5) Discovery defaults the db to .dl/.state/cache.db (NOT .dl/cache.db) and
/// drops a .gitignore covering `.state/`, so hook invocations get warm ticks
/// without committing the cache and `.dl/` shows only authored files.
#[test]
fn discovery_defaults_db_into_dl_state_dir() {
    let d = fixture("db");
    let (code, _out, _err) = run(&d, &["--check"]);
    assert_ne!(code, 0); // rail-a error severity; db side effects still happen
    assert!(d.join(".dl/.state/cache.db").exists(), "default cache db missing under .state/");
    assert!(!d.join(".dl/cache.db").exists(), "cache db must NOT land directly in .dl/");
    let gi = fs::read_to_string(d.join(".dl/.gitignore")).expect("generated .gitignore");
    assert!(gi.contains(".state/"), "gitignore must cover the state dir: {gi}");
}

/// (5b) Migration: a pre-existing `.dl/cache.db` (old layout, a real sqlite
/// file produced by a first run) moves into `.dl/.state/cache.db` on the next
/// run instead of being ignored or duplicated. (A synthetic `-wal` sibling is
/// deliberately NOT exercised here: a `-wal` that does not match its `cache.db`
/// header is stale-journal input SQLite itself discards on open in WAL mode —
/// that recovery behavior is SQLite's, not this migration's, so asserting on
/// the sibling's post-open survival would test SQLite, not the rename.)
#[test]
fn discovery_migrates_old_cache_db_into_state_dir() {
    let d = fixture("migrate");
    // First run: produces a valid sqlite db at the CURRENT (state-dir) layout.
    let (_code, _out, _err) = run(&d, &["--check"]);
    assert!(d.join(".dl/.state/cache.db").is_file(), "setup: first run must create the db");
    // Simulate an old-layout install: move the valid db back up to
    // `.dl/cache.db`, remove the now-stale .state dir.
    fs::rename(d.join(".dl/.state/cache.db"), d.join(".dl/cache.db")).unwrap();
    fs::remove_dir_all(d.join(".dl/.state")).unwrap();

    let (code, _out, _err) = run(&d, &["--check"]);
    assert_ne!(code, 0);
    assert!(d.join(".dl/.state/cache.db").exists(), "migrated cache db missing");
    assert!(!d.join(".dl/cache.db").exists(), "old-layout cache db must be gone after migration");
}

/// (6) An explicit positional still works unchanged and does NOT touch .dl/.
#[test]
fn explicit_program_bypasses_discovery() {
    let d = sandbox("explicit");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/x.rs"), "fn alpha() {}\n").unwrap();
    fs::write(d.join("p.dl"), concat!(
        "rel hit(p: file, l: int).\n",
        "hit(p, l) <- scan(\"WORK\", \"src/**/*.rs\", p, rev), match(p, rev, /alpha/, l).\n",
        "? hit(p, l).\n",
    )).unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .current_dir(&d)
        .env("DL_NO_DAEMON", "1")
        .output().expect("run dl");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("src/x.rs"), "query rows expected:\n{stdout}");
    assert!(!d.join(".dl").exists(), "explicit program must not create .dl/");
}
