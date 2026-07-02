//! End-to-end pin-skew chain (the examples/pin-skew.dl shape): go.mod manifest
//! seam -> internal `pin` edges -> derived `rev_cmp_want` demand -> built-in
//! `rev_behind` ancestry counts -> `stale_pin` / `diverged_pin`. Two config
//! repos, real git: the consumer pins the dep at v1 while the dep's main moved
//! on (stale), and pins a side-branch tag main never had (diverged).

use std::fs;
use std::path::{Path, PathBuf};

use sprefa_v5::config::RepoConfig;
use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_pinskew_{tag}_{}", std::process::id()));
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
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "t@t"]);
    git(dir, &["config", "user.name", "t"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
}

fn commit_file(dir: &Path, file: &str, content: &str, msg: &str) -> String {
    fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", msg]);
    git(dir, &["rev-parse", "HEAD"])
}

#[test]
fn pin_skew_finds_stale_and_diverged_pins() {
    let d = sandbox("e2e");

    // dep repo: module example.com/dep; v1.0.0 two commits behind main, and
    // v1.0.1-hotfix tagged on a side branch main never merged.
    let dep = d.join("dep");
    init_repo(&dep);
    let c1 = commit_file(&dep, "go.mod", "module example.com/dep\n\ngo 1.22\n", "c1");
    git(&dep, &["tag", "v1.0.0", &c1]);
    commit_file(&dep, "a.go", "package dep\n", "c2");
    commit_file(&dep, "b.go", "package dep\n", "c3");
    git(&dep, &["checkout", "-q", "-b", "hotfix", &c1]);
    commit_file(&dep, "fix.go", "package dep\n", "hotfix");
    git(&dep, &["tag", "v1.0.1-hotfix"]);
    git(&dep, &["checkout", "-q", "main"]);

    // consumer repo: pins both refs.
    let consumer = d.join("consumer");
    init_repo(&consumer);
    commit_file(&consumer, "go.mod",
        "module example.com/consumer\n\ngo 1.22\n\nrequire (\n\
         \texample.com/dep v1.0.0\n\
         \texample.com/dep v1.0.1-hotfix\n)\n", "c1");

    fs::write(d.join("p.dl"), r#"
rel module_id(repo: text, module: text).
module_id(r, mod) <- repo(r, _, _), scan(r, "HEAD", "go.mod", p, rev),
                     match(p, rev, /^module\s+(?<mod>\S+)/, l).
rel gomod_pin(repo: text, module: text, ver: text).
gomod_pin(r, mod, ver) <- repo(r, _, _), scan(r, "HEAD", "go.mod", p, rev),
                          match(p, rev, /(?<mod>\S+\/\S+)\s+(?<ver>v[0-9]\S*)/, l).
rel pin(consumer: text, dep: text, ref: text).
pin(a, b, v) <- gomod_pin(a, m, v), module_id(b, m), a != b.
rel rev_cmp_want(repo: text, refname: text, upstream: text).
rev_cmp_want(b, v, "HEAD") <- pin(_, b, v).
rel stale_pin(consumer: text, dep: text, ref: text, behind: int).
stale_pin(a, b, v, n) <- pin(a, b, v), rev_behind(b, v, "HEAD", n, _), n > 0.
rel diverged_pin(consumer: text, dep: text, ref: text).
diverged_pin(a, b, v) <- pin(a, b, v), rev_behind(b, v, "HEAD", _, k), k > 0.
"#).unwrap();

    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.set_repos(vec![
        RepoConfig { slug: "dep".into(), root: dep.clone(), url: None, allow_missing: false },
        RepoConfig { slug: "consumer".into(), root: consumer.clone(), url: None, allow_missing: false },
    ]);
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags.iter().filter(|x| x.severity == sprefa_v5::ast::Severity::Error).count();
    assert_eq!(errs, 0, "pin-skew program should typecheck: {diags:?}");

    // One-tick-latency hops compose, one tick each: the manifest scan is itself
    // data-driven (`r` bound by `repo(r,_,_)`, which fills on tick 1), so the
    // scan lands tick 2 (deriving pin + rev_cmp_want), and rev_behind + the
    // verdict rels land tick 3. Under the daemon this is just convergence.
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();

    let pins = eng.rel_rows("pin", 3);
    assert!(pins.iter().any(|r| r == &["consumer", "dep", "v1.0.0"]),
        "manifest seam should yield the internal pin edge: {pins:?}");

    let stale = eng.rel_rows("stale_pin", 4);
    let v1 = stale.iter().find(|r| r[2] == "v1.0.0")
        .unwrap_or_else(|| panic!("v1.0.0 should be stale: {stale:?}"));
    assert_eq!(v1[0], "consumer");
    assert_eq!(v1[1], "dep");
    assert_eq!(v1[3], "2", "main moved two commits past v1.0.0: {stale:?}");

    let diverged = eng.rel_rows("diverged_pin", 3);
    assert!(diverged.iter().any(|r| r[2] == "v1.0.1-hotfix"),
        "the side-branch tag should be diverged: {diverged:?}");
    assert!(!diverged.iter().any(|r| r[2] == "v1.0.0"),
        "v1.0.0 is an ancestor of main, not diverged: {diverged:?}");
}
