//! Christmas #1 — data-driven scan coordinates. A scan whose `repo`/`rev` is a
//! `Term::Var` is bound by a preceding body atom reading a (previous-tick)
//! relation, then enumerated per binding. This drives the engine directly over
//! two ticks against a real git repo: tick 1 derives the `pin` set, tick 2's
//! scan reads it and walks each pinned rev.

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_ddscan_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

/// `pin(R, V)` drives `scan(R, V, glob, p, rout)`: HEAD walks the worktree's
/// latest commit (both files), c1 walks the earlier commit (one file). Proves a
/// derived row set fans the scan over multiple revs in one pass.
#[test]
fn data_driven_scan_reads_each_pinned_rev() {
    let d = sandbox("tworev");
    init_repo(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn a() {}\n").unwrap();
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "c1"]);
    let c1 = git(&d, &["rev-parse", "HEAD"]);
    fs::write(d.join("src/b.rs"), "fn b() {}\n").unwrap();
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "c2"]);

    let prog_text = format!(
        "rel pin(repo: text, rev: text).\n\
         pin(\".\", \"HEAD\").\n\
         pin(\".\", \"{c1}\").\n\
         rel scanned(rev: text, path: file).\n\
         scanned(V, p) <- pin(R, V), scan(R, V, \"src/**/*.rs\", p, rout).\n",
        c1 = c1,
    );
    fs::write(d.join("p.dl"), prog_text).unwrap();

    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags
        .iter()
        .filter(|x| x.severity == sprefa_v5::ast::Severity::Error)
        .count();
    assert_eq!(errs, 0, "data-driven scan program should typecheck");

    // Tick 1: `pin` derives (after the source phase), so the scan reads an empty
    // pin table and yields nothing. Tick 2: the scan reads tick 1's pin rows and
    // walks each rev.
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();

    let rows = eng.rel_rows("scanned", 2);
    let mut got: Vec<(String, String)> = rows
        .into_iter()
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect();
    got.sort();
    // HEAD (c2) has both files; c1 has only a.rs.
    let head_rows: Vec<&str> = got
        .iter()
        .filter(|(v, _)| v == "HEAD")
        .map(|(_, p)| p.as_str())
        .collect();
    let c1_rows: Vec<&str> = got
        .iter()
        .filter(|(v, _)| v == &c1)
        .map(|(_, p)| p.as_str())
        .collect();
    assert!(
        head_rows.contains(&"src/a.rs") && head_rows.contains(&"src/b.rs"),
        "HEAD should scan both files: {head_rows:?}"
    );
    assert!(
        c1_rows.contains(&"src/a.rs") && !c1_rows.contains(&"src/b.rs"),
        "c1 should scan only a.rs: {c1_rows:?}"
    );
}

/// A data-driven scan with no coordinate-providing atom (unbound variable) is
/// rejected at resolve time, not silently dropped.
#[test]
fn data_driven_scan_rejects_unbound_coord() {
    let d = sandbox("unbound");
    init_repo(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn a() {}\n").unwrap();
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "c1"]);
    fs::write(
        d.join("p.dl"),
        "rel scanned(rev: text, path: file).\n\
         scanned(V, p) <- scan(R, V, \"src/**/*.rs\", p, rout).\n",
    )
    .unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, _, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let err = eng.tick(&prog, true).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("coordinate-providing body atom"),
        "expected an unbound-coordinate rejection, got: {msg}"
    );
}
