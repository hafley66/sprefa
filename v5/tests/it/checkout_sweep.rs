//! The `checkout` demand sink: the ghcacher keep-current half. A rule heads
//! `checkout(repo, branch, pr_heads)` and the engine fetches origin, then
//! NON-DESTRUCTIVELY keeps the named branch current:
//!   * on that branch + clean → fast-forward via `merge --ff-only origin/<branch>`
//!   * on that branch + dirty → SKIP (never stash, never reset)
//!   * on any other branch    → `git branch -f` the ref only, working tree alone

use std::fs;
use std::path::Path;
use std::process::Command;

use sprefa_v5::db;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git")
        .status
        .success();
    assert!(ok, "git {args:?} in {}", dir.display());
}

fn head(dir: &Path) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn init_repo(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
}

/// A config repo cloned from an upstream, upstream advances, the checkout sink
/// fast-forwards the clone's `main` to origin/main via `merge --ff-only` (it is
/// on main, working tree clean). Reads the repo set from config
/// (`repo(slug, root, url)`), the "keep every configured repo current" shape.
#[test]
fn checkout_sink_fast_forwards_current_branch_via_ff_only() {
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
    let ok = Command::new("git")
        .args([
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .output()
        .expect("git clone")
        .status
        .success();
    assert!(ok, "clone upstream -> work");
    let work_before = head(&work);

    // upstream advances on main
    fs::write(upstream.join("a.txt"), "two\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c2"]);
    let upstream_head = head(&upstream);
    assert_ne!(
        work_before, upstream_head,
        "upstream advanced past the clone"
    );

    // a neutral self root so --root does not resolve to `work`
    let selfdir = d.join("selfrepo");
    init_repo(&selfdir);
    fs::write(selfdir.join("x.txt"), "x\n").unwrap();
    git(&selfdir, &["add", "-A"]);
    git(&selfdir, &["commit", "-qm", "s1"]);

    fs::write(
        d.join("cfg.toml"),
        format!(
            "\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
            root = work.display(),
            url = upstream.display()
        ),
    )
    .unwrap();

    // Head `checkout` off the `repo` builtin (which is populated from config):
    // keep every configured repo (url set) current on main.
    fs::write(
        d.join("p.dl"),
        "\
        checkout(slug, \"main\", \"0\") <- repo(slug, root, url), url != \"\".\n\
        ? checkout(slug, branch, pr).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .env("DL_APPLY_SINKS", "1")
        .output()
        .expect("run dl");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "run failed: {stdout}\n{stderr}");

    // the sink logged the sweep
    assert!(
        stderr.contains("[checkout] work: ff main -> origin/main"),
        "checkout logged a fast-forward: {stderr}"
    );
    // and the clone actually advanced to upstream's HEAD
    assert_eq!(
        head(&work),
        upstream_head,
        "work clone fast-forwarded to upstream HEAD"
    );

    // the outcome is queryable via the checkout_done rel (repo, branch, action, ok, detail)
    let read_db = db::ReadDb::open(d.join("db").to_str().unwrap()).unwrap();
    let (repo, action, ok): (String, String, i64) = read_db
        .query_one(
            "rel_checkout_done_txt",
            "SELECT repo, action, ok FROM rel_checkout_done_txt",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("checkout_done row");
    assert_eq!(
        (repo.as_str(), action.as_str(), ok),
        ("work", "ff", 1),
        "checkout_done records the successful fast-forward"
    );
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
    let ok = Command::new("git")
        .args([
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .output()
        .expect("git clone")
        .status
        .success();
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

    fs::write(
        d.join("cfg.toml"),
        format!(
            "\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
            root = work.display(),
            url = upstream.display()
        ),
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "\
        checkout(\"work\", \"main\", \"0\") <- repo(\"work\", root, url).\n\
        ? checkout(slug, branch, pr).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .env("DL_APPLY_SINKS", "1")
        .output()
        .expect("run dl");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "run failed: {}\n{stderr}",
        String::from_utf8_lossy(&out.stdout)
    );

    assert!(
        stderr.contains("[checkout] work: branch-f main -> origin/main"),
        "checkout logged a branch-f (checkout left on feature): {stderr}"
    );
    // working tree untouched: still on feature at the local commit
    assert_eq!(head(&work), feature_head, "feature checkout left in place");
    // but the main ref now points at upstream HEAD
    let main_ref = {
        let out = Command::new("git")
            .current_dir(&work)
            .args(["rev-parse", "main"])
            .output()
            .expect("git");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(
        main_ref, upstream_head,
        "main ref fast-forwarded to upstream HEAD"
    );
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
    let ok = Command::new("git")
        .args([
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .output()
        .expect("git clone")
        .status
        .success();
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

    fs::write(
        d.join("cfg.toml"),
        format!(
            "\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
            root = work.display(),
            url = upstream.display()
        ),
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "\
        checkout(\"work\", \"main\", \"0\") <- repo(\"work\", root, url).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .env("DL_APPLY_SINKS", "1")
        .env("DL_NO_FETCH", "1")
        .output()
        .expect("run dl");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "run failed: {}\n{stderr}",
        String::from_utf8_lossy(&out.stdout)
    );
    // offline: no new objects, clone stays at its original HEAD
    assert_eq!(
        head(&work),
        work_before,
        "offline sweep did not advance the clone"
    );
}

/// On the default branch with a DIRTY working tree, the sink MUST leave it
/// alone: no stash, no reset, no overwrite. Surfaces a skip outcome naming the
/// dirty state; HEAD and the dirty file are byte-identical after the sweep.
#[test]
fn checkout_sink_dirty_working_tree_left_untouched() {
    let d = std::env::temp_dir().join("checkout_dirty_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    let upstream = d.join("upstream");
    init_repo(&upstream);
    fs::write(upstream.join("a.txt"), "one\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c1"]);

    let work = d.join("work");
    let ok = Command::new("git")
        .args([
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .output()
        .expect("git clone")
        .status
        .success();
    assert!(ok, "clone");
    let work_before = head(&work);

    // upstream advances on main
    fs::write(upstream.join("a.txt"), "two\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c2"]);
    let upstream_head = head(&upstream);

    // the clone has UNCOMMITTED local work on main — must not be touched
    fs::write(work.join("local-dirty.txt"), "scratch\n").unwrap();
    let dirty_blob_before = fs::read(work.join("local-dirty.txt")).unwrap();

    let selfdir = d.join("selfrepo");
    init_repo(&selfdir);
    fs::write(selfdir.join("x.txt"), "x\n").unwrap();
    git(&selfdir, &["add", "-A"]);
    git(&selfdir, &["commit", "-qm", "s1"]);

    fs::write(
        d.join("cfg.toml"),
        format!(
            "\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
            root = work.display(),
            url = upstream.display()
        ),
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "\
        checkout(\"work\", \"main\", \"0\") <- repo(\"work\", root, url).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .env("DL_APPLY_SINKS", "1")
        .output()
        .expect("run dl");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "run failed: {}\n{stderr}",
        String::from_utf8_lossy(&out.stdout)
    );

    // skip outcome naming the dirty state
    assert!(
        stderr.contains("[checkout] work: skip main: working tree dirty; left untouched"),
        "dirty working tree skipped, not mutated: {stderr}"
    );
    // HEAD did not move (no merge, no reset)
    assert_eq!(head(&work), work_before, "dirty clone's HEAD must not move");
    // the dirty file is byte-identical (no stash created, no overwrite)
    assert_eq!(
        fs::read(work.join("local-dirty.txt")).unwrap(),
        dirty_blob_before,
        "dirty file content preserved"
    );
    // upstream advanced but we did not pull it in
    assert_ne!(
        head(&work),
        upstream_head,
        "dirty clone did not fast-forward"
    );
    // no stash was created: stash list is empty
    let stash_out = Command::new("git")
        .current_dir(&work)
        .args(["stash", "list"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&stash_out.stdout).trim().is_empty(),
        "no stash created: {}",
        String::from_utf8_lossy(&stash_out.stdout)
    );

    // checkout_done carries the skip row
    let read_db = db::ReadDb::open(d.join("db").to_str().unwrap()).unwrap();
    let (action, ok): (String, i64) = read_db
        .query_one(
            "rel_checkout_done_txt",
            "SELECT action, ok FROM rel_checkout_done_txt",
            &[],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("checkout_done row");
    assert_eq!(
        (action.as_str(), ok),
        ("skip", 1),
        "checkout_done records the dirty-skip with ok=1 (intentional)"
    );
}

/// On the default branch, clean, but DIVERGED from origin (a local commit not
/// on the remote). `merge --ff-only` fails and the sink MUST leave HEAD where
/// it is — no force-update, no reset. The local commit survives.
#[test]
fn checkout_sink_diverged_left_untouched() {
    let d = std::env::temp_dir().join("checkout_diverged_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    let upstream = d.join("upstream");
    init_repo(&upstream);
    fs::write(upstream.join("a.txt"), "one\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c1"]);

    let work = d.join("work");
    let ok = Command::new("git")
        .args([
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .output()
        .expect("git clone")
        .status
        .success();
    assert!(ok, "clone");

    // upstream advances on main
    fs::write(upstream.join("a.txt"), "two\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c2"]);

    // the clone ALSO advances on main with a different commit → diverged
    fs::write(work.join("a.txt"), "local-two\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-qm", "local-c2"]);
    let work_before = head(&work);

    let selfdir = d.join("selfrepo");
    init_repo(&selfdir);
    fs::write(selfdir.join("x.txt"), "x\n").unwrap();
    git(&selfdir, &["add", "-A"]);
    git(&selfdir, &["commit", "-qm", "s1"]);

    fs::write(
        d.join("cfg.toml"),
        format!(
            "\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
            root = work.display(),
            url = upstream.display()
        ),
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "\
        checkout(\"work\", \"main\", \"0\") <- repo(\"work\", root, url).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .env("DL_APPLY_SINKS", "1")
        .output()
        .expect("run dl");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "run failed: {}\n{stderr}",
        String::from_utf8_lossy(&out.stdout)
    );

    // skip outcome naming divergence
    assert!(
        stderr.contains("[checkout] work: skip main:") && stderr.contains("diverged"),
        "diverged clone skipped, not force-updated: {stderr}"
    );
    // HEAD did not move (no merge, no reset, no force-update)
    assert_eq!(
        head(&work),
        work_before,
        "diverged clone's HEAD must not move"
    );
}

/// DL_CHECKOUT_DRY_RUN=1: the sink previews what it WOULD do without running
/// `merge --ff-only` or `git branch -f`. Rows land in `checkout_plan`, not
/// `checkout_done`. Here a clean clone that WOULD fast-forward reports `ff`
/// without advancing HEAD.
#[test]
fn checkout_sink_dry_run_emits_plan_without_mutating() {
    let d = std::env::temp_dir().join("checkout_dryrun_test");
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();

    let upstream = d.join("upstream");
    init_repo(&upstream);
    fs::write(upstream.join("a.txt"), "one\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c1"]);

    let work = d.join("work");
    let ok = Command::new("git")
        .args([
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            work.to_str().unwrap(),
        ])
        .output()
        .expect("git clone")
        .status
        .success();
    assert!(ok, "clone");
    let work_before = head(&work);

    // upstream advances; dry-run would ff but must not actually advance the clone
    fs::write(upstream.join("a.txt"), "two\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "c2"]);

    let selfdir = d.join("selfrepo");
    init_repo(&selfdir);
    fs::write(selfdir.join("x.txt"), "x\n").unwrap();
    git(&selfdir, &["add", "-A"]);
    git(&selfdir, &["commit", "-qm", "s1"]);

    fs::write(
        d.join("cfg.toml"),
        format!(
            "\
        [[repos]]\n\
        slug = \"work\"\n\
        root = \"{root}\"\n\
        url = \"{url}\"\n",
            root = work.display(),
            url = upstream.display()
        ),
    )
    .unwrap();
    fs::write(
        d.join("p.dl"),
        "\
        checkout(\"work\", \"main\", \"0\") <- repo(\"work\", root, url).\n",
    )
    .unwrap();

    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--db", d.join("db").to_str().unwrap()])
        .current_dir(&selfdir)
        .env("SPREFA_CONFIG", d.join("cfg.toml"))
        .env("DL_CHECKOUT_DRY_RUN", "1")
        .output()
        .expect("run dl");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "run failed: {}\n{stderr}",
        String::from_utf8_lossy(&out.stdout)
    );

    // plan outcome names a would-ff
    assert!(
        stderr.contains("[checkout (plan)] work: ff main -> origin/main"),
        "dry-run logs the planned ff: {stderr}"
    );
    // HEAD did NOT move — no merge ran
    assert_eq!(
        head(&work),
        work_before,
        "dry-run must not advance the clone"
    );
    // rows land in checkout_plan (not checkout_done)
    let read_db = db::ReadDb::open(d.join("db").to_str().unwrap()).unwrap();
    let plan_count: i64 = read_db
        .query_one(
            "rel_checkout_plan",
            "SELECT COUNT(*) FROM rel_checkout_plan",
            &[],
            |r| Ok(r.get(0)?),
        )
        .unwrap_or(0);
    assert_eq!(plan_count, 1, "checkout_plan has the one preview row");
    let done_count: i64 = read_db
        .query_one(
            "rel_checkout_done",
            "SELECT COUNT(*) FROM rel_checkout_done",
            &[],
            |r| Ok(r.get(0)?),
        )
        .unwrap_or(0);
    assert_eq!(
        done_count, 0,
        "checkout_done empty in dry-run (nothing was done)"
    );
}
