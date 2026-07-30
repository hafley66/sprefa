//! Perf/structural guard for the `created(path, name, email, ts)` built-in
//! (file-authorship from `git log --diff-filter=A`). Two properties, both
//! always-run (no fixture gate):
//!
//! 1. `created_tick_has_no_n1`: a repo with MORE than N1_THRESHOLD (64) tracked
//!    files must populate `created` without tripping the per-tick N+1 detector —
//!    proves the whole row set goes through one `refresh_rel`/`insert_rows`, not
//!    a per-file write. Structural: asserts `eng.last_n1.is_none()`.
//! 2. `created_retick_is_noop`: history is append-only, so a second tick over an
//!    unchanged repo must early-out (the stored-set compare returns false) — the
//!    `created` row count is identical and no N+1 fires on the re-tick either.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// A program that references `created` (gates the git-log refresh on).
const PROG: &str = r#"
rel author(path: file, email: text).
author(p, e) <- created(p, _, e, _).
"#;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn count(eng: &Engine, rel: &str) -> i64 {
    eng.count_rows(rel).unwrap_or(-1)
}

/// A git repo with N committed files (each added in its own commit so the
/// per-file author rows are real `--diff-filter=A` adds), returns the root.
fn fixture(tag: &str, n: usize) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("created_perf_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "perf@t"]);
    git(&dir, &["config", "user.name", "perf"]);
    for i in 0..n {
        fs::write(
            dir.join("src").join(format!("m{i}.rs")),
            format!("fn f{i}() {{}}\n"),
        )
        .unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-qm", &format!("add m{i}")]);
    }
    fs::canonicalize(&dir).unwrap()
}

#[test]
fn created_tick_has_no_n1() {
    let root = fixture("n1", 80); // > N1_THRESHOLD (64) so a per-row write would trip
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.clone());
    eng.tick(&prog, true).unwrap();

    let rows = count(&eng, "created");
    assert!(
        rows > 64,
        "fixture must exceed the N+1 threshold (got {rows})"
    );
    assert!(
        eng.last_n1.is_none(),
        "created tick tripped the N+1 detector: {:?} — a per-row write slipped the plural API",
        eng.last_n1
    );
}

#[test]
fn created_retick_is_noop() {
    let root = fixture("noop", 70);
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.clone());
    eng.tick(&prog, true).unwrap();
    let first = count(&eng, "created");
    assert!(
        first > 0,
        "created populated on the cold tick (got {first})"
    );

    // Re-tick an unchanged repo: append-only history -> stored-set early-out.
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        count(&eng, "created"),
        first,
        "re-tick changed the created set"
    );
    assert!(
        eng.last_n1.is_none(),
        "re-tick tripped N+1: {:?}",
        eng.last_n1
    );
}
