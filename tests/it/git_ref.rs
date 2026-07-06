//! `git_ref` inventory + `rev_behind` demand-driven ancestry. Drives the
//! engine directly against real git fixtures: git_ref lists branches/tags/HEAD
//! with annotated tags peeled; rev_behind fills behind/ahead counts for the
//! pairs a user-derived `rev_cmp_want` demands (one-tick latency, like a
//! data-driven scan).

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_gitref_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

fn commit(dir: &Path, file: &str, msg: &str) -> String {
    fs::write(dir.join(file), format!("// {msg}\n")).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", msg]);
    git(dir, &["rev-parse", "HEAD"])
}

/// git_ref lists HEAD + branches + tags per repo; an annotated tag's sha is the
/// PEELED commit, not the tag object.
#[test]
fn git_ref_inventories_refs_with_peeled_tags() {
    let d = sandbox("inv");
    init_repo(&d);
    let c1 = commit(&d, "a.rs", "c1");
    git(&d, &["tag", "light", &c1]);
    let c2 = commit(&d, "b.rs", "c2");
    git(&d, &["tag", "-a", "heavy", "-m", "annotated", &c2]);
    let tag_obj = git(&d, &["rev-parse", "heavy"]); // the tag object, NOT c2

    fs::write(d.join("p.dl"),
        "rel refs(refname: text, kind: text, sha: text).\n\
         refs(N, K, S) <- git_ref(R, N, K, S).\n").unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags.iter().filter(|x| x.severity == sprefa_v5::ast::Severity::Error).count();
    assert_eq!(errs, 0, "git_ref program should typecheck: {diags:?}");
    eng.tick(&prog, true).unwrap();

    let rows = eng.rel_rows("refs", 3);
    let find = |n: &str| rows.iter().find(|r| r[0] == n)
        .unwrap_or_else(|| panic!("no {n} row in {rows:?}"));
    assert_eq!(find("HEAD")[1], "head");
    assert_eq!(find("HEAD")[2], c2);
    assert_eq!(find("main")[1], "branch");
    assert_eq!(find("light")[1], "tag");
    assert_eq!(find("light")[2], c1);
    let heavy = find("heavy");
    assert_eq!(heavy[1], "tag");
    assert_eq!(heavy[2], c2, "annotated tag should peel to the commit");
    assert_ne!(heavy[2], tag_obj, "annotated tag must not surface the tag object");
}

/// rev_behind fills counts for rev_cmp_want pairs: a tag behind main counts
/// behind>0/ahead=0; a diverged branch counts ahead>0. Two ticks (the want rel
/// derives on tick 1, the counts fill on tick 2).
#[test]
fn rev_behind_counts_wanted_pairs() {
    let d = sandbox("behind");
    init_repo(&d);
    let c1 = commit(&d, "a.rs", "c1");
    git(&d, &["tag", "v1", &c1]);
    commit(&d, "b.rs", "c2");
    commit(&d, "c.rs", "c3");
    // diverged branch off c1
    git(&d, &["checkout", "-q", "-b", "div", &c1]);
    commit(&d, "d.rs", "d1");
    git(&d, &["checkout", "-q", "main"]);

    fs::write(d.join("p.dl"),
        "rev_cmp_want(\".\", \"v1\", \"main\").\n\
         rev_cmp_want(\".\", \"div\", \"main\").\n\
         rel skew(refname: text, behind: int, ahead: int).\n\
         skew(N, B, A) <- rev_behind(R, N, U, B, A).\n").unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags.iter().filter(|x| x.severity == sprefa_v5::ast::Severity::Error).count();
    assert_eq!(errs, 0, "rev_behind program should typecheck: {diags:?}");
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();

    let rows = eng.rel_rows("skew", 3);
    let find = |n: &str| rows.iter().find(|r| r[0] == n)
        .unwrap_or_else(|| panic!("no {n} row in {rows:?}"));
    // v1 = c1: main has c2+c3 it lacks, nothing of its own.
    assert_eq!(find("v1")[1], "2");
    assert_eq!(find("v1")[2], "0");
    // div: behind c2+c3, ahead by d1 => diverged.
    assert_eq!(find("div")[1], "2");
    assert_eq!(find("div")[2], "1");
}

/// A rev_cmp_want row naming an unknown ref is skipped (loudly), not an error;
/// resolvable rows still fill.
#[test]
fn rev_behind_skips_unresolvable_refs() {
    let d = sandbox("skip");
    init_repo(&d);
    let c1 = commit(&d, "a.rs", "c1");
    git(&d, &["tag", "v1", &c1]);
    commit(&d, "b.rs", "c2");

    fs::write(d.join("p.dl"),
        "rev_cmp_want(\".\", \"v1\", \"main\").\n\
         rev_cmp_want(\".\", \"no-such-ref\", \"main\").\n\
         rev_cmp_want(\"no-such-repo\", \"v1\", \"main\").\n\
         rel skew(refname: text, behind: int, ahead: int).\n\
         skew(N, B, A) <- rev_behind(R, N, U, B, A).\n").unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, _, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();

    let rows = eng.rel_rows("skew", 3);
    assert_eq!(rows.len(), 1, "only the resolvable pair fills: {rows:?}");
    assert_eq!(rows[0][0], "v1");
    assert_eq!(rows[0][1], "1");
}
