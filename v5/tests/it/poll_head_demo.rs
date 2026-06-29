//! End-to-end demo of the effect runtime against a LIVE subprocess: it runs
//! examples/poll-head.dl exactly as the daemon would — tick (emit pending), build
//! a ShellEffectExec from the program's `effect_cmd` rows, drain off-tick (real
//! `git rev-parse HEAD`), tick again, read the cached oid. This is the ghcacher
//! shape with no mock anywhere: an interval clock + an effect + an idempotent
//! content-addressed cache. See examples/poll-head.dl.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use sprefa_v5::db;
use sprefa_v5::engine::{async_effect_arity, Engine, ShellEffectExec};
use sprefa_v5::prepare_paths;

/// Build the same ShellEffectExec the daemon's poll_tick builds: templates from
/// the program's `effect_cmd(kind, template)` rows, arity from the @async heads.
fn shell_exec(eng: &Engine, prog: &sprefa_v5::ast::Program) -> ShellEffectExec {
    let mut templates = HashMap::new();
    for row in eng.query_sql("SELECT kind, template FROM rel_effect_cmd", &[]).unwrap() {
        let kind = row[0].as_str().unwrap().to_string();
        let tmpl = row[1].as_str().unwrap().to_string();
        templates.insert(kind, tmpl);
    }
    ShellEffectExec { templates, n_out: async_effect_arity(prog), cwd: eng.root() }
}

#[test]
fn poll_head_caches_live_git_head() {
    // A real git repo with a known HEAD: init one in a sandbox so the oid is
    // stable and the test is hermetic (no dependency on the checkout's HEAD).
    let dir = std::env::temp_dir().join(format!("dl_pollhead_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git").args(args).current_dir(&dir).output().unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "t@t"]);
    git(&["config", "user.name", "t"]);
    std::fs::write(dir.join("f"), "x").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "one"]);
    let want = String::from_utf8(
        Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&dir).output().unwrap().stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    assert_eq!(want.len(), 40, "a real 40-char oid");

    // The example program, run from the repo root so `git rev-parse HEAD` resolves.
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/poll-head.dl");
    let (prog, _diags, _) = prepare_paths(&[example]).unwrap();
    let conn = db::open(Some(dir.join("dl.db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());

    // Tick 1: every(2) fires on the first tick, so the @async rule queues one
    // request (the `self` watch). Nothing cached yet.
    eng.tick(&prog, true).unwrap();
    let pending: i64 = eng
        .query_sql("SELECT COUNT(*) FROM pending_effect WHERE done = 0", &[])
        .unwrap()[0][0]
        .as_i64()
        .unwrap();
    assert_eq!(pending, 1, "one queued effect after the first clock tick");
    assert!(
        eng.query_sql("SELECT * FROM rel_head_at", &[]).unwrap().is_empty(),
        "no cached head before the drain"
    );

    // Off-tick drain: runs `git rev-parse HEAD` for real.
    let exec = shell_exec(&eng, &prog);
    let n = eng.drain_effects(&prog, &exec).unwrap();
    assert_eq!(n, 1, "the live git command drained");

    // Tick 2: the cached oid is readable, and it matches the real HEAD.
    eng.tick(&prog, true).unwrap();
    let got = eng.query_sql("SELECT name, oid FROM rel_head_at", &[]).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0][0].as_str().unwrap(), "self");
    assert_eq!(got[0][1].as_str().unwrap(), want, "cached the live HEAD oid");

    // Idempotence: re-firing the same request (HEAD unmoved) does not re-run the
    // command — the digest id collides, the queue holds.
    eng.tick(&prog, true).unwrap();
    let n2 = eng.drain_effects(&prog, &exec).unwrap();
    assert_eq!(n2, 0, "unmoved HEAD does not re-poll");

    let _ = std::fs::remove_dir_all(&dir);
}

/// Per-row `cwd`: one effect kind, two watched repos in different directories,
/// each request runs `git rev-parse HEAD` in its OWN repo. Proves the reserved
/// `cwd` arg routes the working directory per request (Option B).
#[test]
fn per_row_cwd_routes_each_effect_to_its_repo() {
    let base = std::env::temp_dir().join(format!("dl_cwd_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // Two independent repos with distinct single commits → distinct oids.
    let mk_repo = |name: &str, content: &str| -> (PathBuf, String) {
        let r = base.join(name);
        std::fs::create_dir_all(&r).unwrap();
        let git = |args: &[&str]| {
            Command::new("git").args(args).current_dir(&r).output().unwrap();
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(r.join("f"), content).unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", content]);
        let oid = String::from_utf8(
            Command::new("git").args(["rev-parse", "HEAD"]).current_dir(&r).output().unwrap().stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        (r, oid)
    };
    let (ra, oid_a) = mk_repo("a", "alpha");
    let (rb, oid_b) = mk_repo("b", "bravo");
    assert_ne!(oid_a, oid_b);

    // Program: two watches, each carrying its absolute repo dir as `cwd`. The
    // effect template is a bare `git rev-parse HEAD`; the runtime supplies the dir.
    let prog_dir = base.join("prog");
    std::fs::create_dir_all(&prog_dir).unwrap();
    let p = prog_dir.join("p.dl");
    std::fs::write(
        &p,
        format!(
            "rel watch(name: str, cwd: str).\n\
             watch(\"a\", \"{}\").\n\
             watch(\"b\", \"{}\").\n\
             rel effect_cmd(kind: str, template: str).\n\
             effect_cmd(\"head_at\", \"git rev-parse HEAD\").\n\
             rel head_at(name: str, oid: str).\n\
             head_at(name, oid) <- @async watch(name, cwd), every(1).\n\
             ? head_at(name, oid).\n",
            ra.display(),
            rb.display(),
        ),
    )
    .unwrap();
    let (prog, _diags, _) = prepare_paths(&[p]).unwrap();
    let conn = db::open(Some(prog_dir.join("dl.db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, prog_dir.clone());

    eng.tick(&prog, true).unwrap();
    let exec = shell_exec(&eng, &prog);
    assert_eq!(eng.drain_effects(&prog, &exec).unwrap(), 2, "both repos polled");
    eng.tick(&prog, true).unwrap();

    let mut got = eng.query_sql("SELECT name, oid FROM rel_head_at ORDER BY name", &[]).unwrap();
    got.sort_by(|x, y| x[0].as_str().cmp(&y[0].as_str()));
    assert_eq!(got[0][0].as_str().unwrap(), "a");
    assert_eq!(got[0][1].as_str().unwrap(), oid_a, "repo a's head, from dir a");
    assert_eq!(got[1][0].as_str().unwrap(), "b");
    assert_eq!(got[1][1].as_str().unwrap(), oid_b, "repo b's head, from dir b");

    let _ = std::fs::remove_dir_all(&base);
}
