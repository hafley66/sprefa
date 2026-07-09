//! The `checkout` demand sink: the ghcacher keep-current half. A rule heads
//! `checkout(repo, branch, pr_heads)` and the engine fetches origin + fast-
//! forwards `branch` to origin/<branch> for each named repo — hard-reset when
//! on that branch, `git branch -f` otherwise.

use std::fs;
use std::path::Path;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").current_dir(dir).args(args).output().expect("git").status.success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

fn head(dir: &Path) -> String {
    let out = Command::new("git").current_dir(dir).args(["rev-parse", "HEAD"]).output().expect("git");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
}

/// A config repo cloned from an upstream, upstream advances, the checkout sink
/// fast-forwards the clone's `main` to origin/main via hard reset (it is on
/// main). Reads the repo set from config (`repo(slug, root, url)`), the
/// "keep every configured repo current" shape.
#[test]
fn checkout_sink_fast_forwards_current_branch_via_hard_reset() {
    let d = std::env::temp_dir().join("checkout_ff_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    // upstream: one commit on main
    let upstream = d.join("upstream");
    init_repo(&upstream);
    fs::write(upstream.join("a.txt"), "one\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c1"]);

    // work: a clone of upstream (origin = upstream), on main, in sync
    let work = d.join("work");
    let ok = Command::new("git").args(["clone", "-q", upstream.to_str().unwrap(), work.to_str().unwrap()])
        .output().expect("git clone").status.success();
    assert!(ok, "clone upstream -> work");
    let work_before = head(&work);

    // upstream advances on main
    fs::write(upstream.join("a.txt"), "two\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c2"]);
    let upstream_head = head(&upstream);
    assert_ne!(work_before, upstream_head, "upstream advanced past the clone");

    // a neutral self root so --root does not resolve to `work`
    let selfdir = d.join("selfrepo");
    init_repo(&selfdir);
    fs::write(selfdir.join("x.txt"), "x\n").unwrap();
    git(&selfdir, &["add", "-A"]);
    git(&selfdir, &["commit", "-qm", "s1"]);

    fs::write(d.join("cfg.toml"), format!("\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
        root = work.display(), url = upstream.display())).unwrap();

    // Head `checkout` off the `repo` builtin (which is populated from config):
    // keep every configured repo (url set) current on main.
    fs::write(d.join("p.dl"), "\
        checkout(slug, \"main\", \"0\") <- repo(slug, root, url), url != \"\".\n\
        ? checkout(slug, branch, pr).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "run failed: {stdout}\n{stderr}");

    // the sink logged the sweep
    assert!(stderr.contains("[checkout] work: reset main -> origin/main"),
        "checkout logged a hard reset: {stderr}");
    // and the clone actually advanced to upstream's HEAD
    assert_eq!(head(&work), upstream_head, "work clone fast-forwarded to upstream HEAD");

    // the outcome is queryable via the checkout_done rel (repo, branch, action, ok, detail)
    let conn = rusqlite::Connection::open(d.join("db")).unwrap();
    let (repo, action, ok): (String, String, i64) = conn.query_row(
        "SELECT repo, action, ok FROM rel_checkout_done", [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).expect("checkout_done row");
    assert_eq!((repo.as_str(), action.as_str(), ok), ("work", "reset", 1),
        "checkout_done records the successful reset");
}

/// On a NON-default branch, the sink moves the `main` ref (`git branch -f`) to
/// origin/main without touching the working tree (the checkout stays put).
#[test]
fn checkout_sink_moves_ref_off_current_branch() {
    let d = std::env::temp_dir().join("checkout_branchf_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    let upstream = d.join("upstream");
    init_repo(&upstream);
    fs::write(upstream.join("a.txt"), "one\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c1"]);

    let work = d.join("work");
    let ok = Command::new("git").args(["clone", "-q", upstream.to_str().unwrap(), work.to_str().unwrap()])
        .output().expect("git clone").status.success();
    assert!(ok, "clone");
    // switch the clone onto a feature branch and commit local work there
    git(&work, &["checkout", "-q", "-b", "feature"]);
    fs::write(work.join("local.txt"), "local\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-qm", "local"]);
    let feature_head = head(&work);

    // upstream advances on main
    fs::write(upstream.join("a.txt"), "two\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c2"]);
    let upstream_head = head(&upstream);

    let selfdir = d.join("selfrepo");
    init_repo(&selfdir);
    fs::write(selfdir.join("x.txt"), "x\n").unwrap();
    git(&selfdir, &["add", "-A"]);
    git(&selfdir, &["commit", "-qm", "s1"]);

    fs::write(d.join("cfg.toml"), format!("\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
        root = work.display(), url = upstream.display())).unwrap();
    fs::write(d.join("p.dl"), "\
        checkout(\"work\", \"main\", \"0\") <- repo(\"work\", root, url).\n\
        ? checkout(slug, branch, pr).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .output().expect("run dl");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "run failed: {}\n{stderr}", String::from_utf8_lossy(&out.stdout));

    assert!(stderr.contains("[checkout] work: branch-f main -> origin/main"),
        "checkout logged a branch-f (checkout left on feature): {stderr}");
    // working tree untouched: still on feature at the local commit
    assert_eq!(head(&work), feature_head, "feature checkout left in place");
    // but the main ref now points at upstream HEAD
    let main_ref = {
        let out = Command::new("git").current_dir(&work).args(["rev-parse", "main"]).output().expect("git");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(main_ref, upstream_head, "main ref fast-forwarded to upstream HEAD");
}

/// `DL_NO_FETCH` skips the network: with no prior fetch the clone cannot learn
/// upstream's new commit, so the reset is a no-op (stays at the cloned HEAD)
/// and the sink does not error.
#[test]
fn checkout_sink_offline_does_not_fetch() {
    let d = std::env::temp_dir().join("checkout_offline_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    let upstream = d.join("upstream");
    init_repo(&upstream);
    fs::write(upstream.join("a.txt"), "one\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c1"]);

    let work = d.join("work");
    let ok = Command::new("git").args(["clone", "-q", upstream.to_str().unwrap(), work.to_str().unwrap()])
        .output().expect("git clone").status.success();
    assert!(ok, "clone");
    let work_before = head(&work);

    // upstream advances, but offline mode must not pull it in
    fs::write(upstream.join("a.txt"), "two\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c2"]);

    let selfdir = d.join("selfrepo");
    init_repo(&selfdir);
    fs::write(selfdir.join("x.txt"), "x\n").unwrap();
    git(&selfdir, &["add", "-A"]);
    git(&selfdir, &["commit", "-qm", "s1"]);

    fs::write(d.join("cfg.toml"), format!("\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
        root = work.display(), url = upstream.display())).unwrap();
    fs::write(d.join("p.dl"), "\
        checkout(\"work\", \"main\", \"0\") <- repo(\"work\", root, url).\n").unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .env("DL_NO_FETCH", "1")
        .output().expect("run dl");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "run failed: {}\n{stderr}", String::from_utf8_lossy(&out.stdout));
    // offline: no new objects, clone stays at its original HEAD
    assert_eq!(head(&work), work_before, "offline sweep did not advance the clone");
}
