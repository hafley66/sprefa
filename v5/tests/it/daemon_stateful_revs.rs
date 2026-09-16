//! Daemon-stateful-revs: the engine half of the watched-rev feature. Persists
//! the loaded program + repo set into `_program`/`_repo`, tracks each watched
//! git ref's oid in `_ref`, logs advances into `_rev_log`, and projects all
//! three into the `program`/`head`/`rev_advanced` query relations. The daemon's
//! `on_git_event` drives `observe_ref` + `files_changed_between`; this test
//! exercises those engine methods directly on a real git repo (no socket).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_revstate_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
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

/// Tick a WORK-scanning program so `_file` is populated, then drive the engine's
/// daemon-state methods exactly as the daemon would.
#[test]
fn observe_ref_tracks_advances_and_projects_query_rels() {
    let d = sandbox("observe");
    init_repo(&d);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn a() {}\n").unwrap();
    fs::write(d.join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), sg(path, rev, :rust, \"fn $N() {}\", line).\n").unwrap();
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "c1"]);
    let c1 = git(&d, &["rev-parse", "HEAD"]);

    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    eng.tick(&prog, true).unwrap();

    let slug = d.file_name().unwrap().to_string_lossy().into_owned();

    // Persist program + repo meta (daemon cold-tick path).
    eng.save_program_meta(&[d.join("p.dl")]).unwrap();
    eng.save_repos_meta().unwrap();

    // First observation: oid unknown, so it advances (old = None).
    let adv = eng.observe_ref(&slug, &d, "HEAD").unwrap();
    assert_eq!(
        adv,
        Some((None, c1.clone())),
        "first sight is an advance with old=None"
    );
    // Re-observe with no commit: no advance.
    assert_eq!(
        eng.observe_ref(&slug, &d, "HEAD").unwrap(),
        None,
        "unchanged HEAD must not advance"
    );

    // New commit moves HEAD; observe again -> advance c1 -> c2.
    fs::write(d.join("src/a.rs"), "fn a() { let x = 1; }\n").unwrap();
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "c2"]);
    let c2 = git(&d, &["rev-parse", "HEAD"]);
    let adv2 = eng.observe_ref(&slug, &d, "HEAD").unwrap();
    assert_eq!(
        adv2,
        Some((Some(c1.clone()), c2.clone())),
        "second observe is c1 -> c2"
    );

    // The changed file (tracked in `_file`) shows in the diff.
    let changed = eng.files_changed_between(&slug, &d, &c1, &c2).unwrap();
    assert_eq!(
        changed,
        vec!["src/a.rs".to_string()],
        "only the edited tracked file: {changed:?}"
    );

    // Project into query rels and read them back through SQL.
    eng.refresh_daemon_rels().unwrap();
    let progs = eng
        .query_sql("SELECT path FROM rel_program_txt", &[])
        .unwrap();
    assert_eq!(progs.len(), 1, "one program row: {progs:?}");
    assert!(progs[0][0].as_str().unwrap().ends_with("p.dl"));

    let heads = eng
        .query_sql("SELECT repo, name, oid FROM rel_head_txt", &[])
        .unwrap();
    assert_eq!(heads.len(), 1, "one watched ref: {heads:?}");
    assert_eq!(
        heads[0][2].as_str().unwrap(),
        c2,
        "head rel holds the latest oid"
    );

    let log = eng
        .query_sql(
            "SELECT old, new FROM rel_rev_advanced_txt ORDER BY new",
            &[],
        )
        .unwrap();
    assert_eq!(
        log.len(),
        2,
        "two advances logged (first-sight + c1->c2): {log:?}"
    );
    // The c1->c2 advance row carries both endpoints.
    assert!(
        log.iter()
            .any(|r| r[0].as_str() == Some(c1.as_str()) && r[1].as_str() == Some(c2.as_str())),
        "c1->c2 row present: {log:?}"
    );
}

/// `program`/`head`/`rev_advanced` are reserved names — a `.dl` program may not
/// redeclare them.
#[test]
fn daemon_rels_are_reserved() {
    let d = sandbox("reserved");
    fs::write(
        d.join("p.dl"),
        "rel head(a: text).\nhead(\"x\").\n? head(a).\n",
    )
    .unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, _diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let err = eng.tick(&prog, true).unwrap_err().to_string();
    assert!(
        err.contains("built-in daemon-state relation"),
        "reserved-name guard: {err}"
    );
}
