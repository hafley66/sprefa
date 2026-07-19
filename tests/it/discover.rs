//! Repo-local program discovery: `dl` with no positional resolves
//! `<root>/.dl/*.dl` (lexicographic), merges the files into one program, and
//! defaults the db to the shared per-root
//! `$XDG_STATE_HOME/sprefa/roots/<key>/db.sqlite` (storage-endgame L2: one db
//! per corpus, daemon or not). A missing or empty `.dl` dir is a loud error so
//! a typo'd directory never makes `--check` pass green by checking nothing.

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

/// The sandboxed XDG state home for a fixture dir (beside the root, hermetic).
fn state_home(dir: &Path) -> PathBuf {
    dir.join(".xdg-state")
}

/// Every `roots/<key>/db.sqlite` under the sandboxed state home.
fn root_dbs(dir: &Path) -> Vec<PathBuf> {
    let roots = state_home(dir).join("sprefa").join("roots");
    fs::read_dir(&roots).into_iter().flatten().flatten()
        .map(|entry| entry.path().join("db.sqlite"))
        .filter(|db| db.is_file())
        .collect()
}

/// Run `dl` with NO program positional against `dir` as root. Hermetic:
/// `DL_NO_DAEMON=1` so these program-discovery/db-defaulting checks never
/// reach toward a real developer daemon (fixed after a live-daemon-pollution
/// incident: the discovery-mode --check daemon-first fix made this file's
/// unguarded invocations daemon-eligible for the first time), and
/// `XDG_STATE_HOME` sandboxed so the L2 defaulted per-root db never lands in a
/// developer's real daemon home.
fn run(dir: &Path, extra: &[&str]) -> (i32, String, String) {
    let out = Command::new(DL)
        .current_dir(dir)
        .env("DL_NO_DAEMON", "1")
        .env("XDG_STATE_HOME", state_home(dir))
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

/// (5) Discovery defaults the db to the shared per-root
/// `roots/<key>/db.sqlite` under the (sandboxed) daemon home — the SAME file a
/// daemon would serve — and creates no second `.dl/.state/cache.db` world
/// (storage-endgame L2, one db per corpus).
#[test]
fn discovery_defaults_db_to_shared_root_db() {
    let d = fixture("db");
    let (code, _out, _err) = run(&d, &["--check"]);
    assert_ne!(code, 0); // rail-a error severity; db side effects still happen
    let dbs = root_dbs(&d);
    assert_eq!(dbs.len(), 1, "exactly one roots/<key>/db.sqlite expected: {dbs:?}");
    assert!(!d.join(".dl/.state/cache.db").exists(), "no cache.db world may grow beside the root db");
    assert!(!d.join(".dl/cache.db").exists(), "cache db must NOT land in .dl/ either");
}

/// (5b) A pre-existing `.dl/.state/cache.db` (the pre-L2 one-shot world) is
/// purely historical: a discovery run neither reads, grows, nor migrates it —
/// it goes straight to the shared root db. `dl daemon gc` (L1) owns the sweep.
#[test]
fn old_cache_db_world_is_left_untouched() {
    let d = fixture("fossil");
    fs::create_dir_all(d.join(".dl/.state")).unwrap();
    let fossil = d.join(".dl/.state/cache.db");
    fs::write(&fossil, b"not-a-real-sqlite-file-and-never-opened").unwrap();

    let (code, _out, _err) = run(&d, &["--check"]);
    assert_ne!(code, 0);
    assert_eq!(fs::read(&fossil).unwrap(), b"not-a-real-sqlite-file-and-never-opened",
        "the historical cache.db must be byte-identical after the run");
    assert_eq!(root_dbs(&d).len(), 1, "the run must land in roots/<key>/db.sqlite");
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
