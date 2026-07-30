//! `repo`-sink dynamic pull: a rule whose head is `repo(slug, root, url)` is a
//! sink drained post-fixpoint. Each row pulls (clone-if-missing + register) when
//! its github org is in the `org` allowlist; a disallowed org is skipped. This
//! drives the engine directly over two ticks (tick 1 drains + registers, tick 2
//! reflects the registration in the `repo` builtin) against pre-created local
//! roots, so no network clone is required.

use std::fs;
use std::path::PathBuf;

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_reposink_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn repo_sink_pulls_only_allowlisted_orgs_and_registers() {
    let d = sandbox("pull");
    // Pre-create the candidate roots so ensure_cloned is a no-op (no network).
    // The root just needs to exist; a missing root would require a real clone.
    let ok_root = d.join("ok-repo");
    let no_root = d.join("no-repo");
    fs::create_dir_all(&ok_root).unwrap();
    fs::create_dir_all(&no_root).unwrap();

    let prog_text = format!(
        "rel org(name: text).\n\
         org(\"good-org\").\n\
         rel candidate(slug: text, root: text, url: text).\n\
         candidate(\"ok\", \"{ok}\", \"https://github.com/good-org/ok\").\n\
         candidate(\"no\", \"{no}\", \"https://github.com/evil-org/no\").\n\
         repo(slug, root, url) <- candidate(slug, root, url).\n",
        ok = ok_root.to_string_lossy(),
        no = no_root.to_string_lossy(),
    );
    fs::write(d.join("p.dl"), prog_text).unwrap();

    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags
        .iter()
        .filter(|x| x.severity == sprefa_v5::ast::Severity::Error)
        .count();
    assert_eq!(errs, 0, "program should typecheck");
    eng.set_repos(vec![]); // no config repos; every repo arrives via the sink

    // Tick 1: candidate + org derive. The `repo` builtin is emitted from
    // self.repos BEFORE the drain, so it is still empty this tick (1-tick
    // latency). Sinks drain in a separate phase from tick_report (a `?` query
    // is a pure read); call drain_external_sinks to fire the pull explicitly.
    eng.tick(&prog, true).unwrap();
    eng.drain_external_sinks(&prog).unwrap();
    let registered: Vec<String> = eng
        .snapshot_repos()
        .iter()
        .map(|r| r.slug.clone())
        .collect();
    assert!(
        registered.contains(&"ok".to_string()),
        "ok (good-org, allowlisted) should register: {registered:?}"
    );
    assert!(
        !registered.contains(&"no".to_string()),
        "no (evil-org, not listed) must be filtered: {registered:?}"
    );

    // Tick 2: refresh_builtin_rels emits the registered repo into `repo`.
    eng.tick(&prog, true).unwrap();
    let rel: Vec<String> = eng
        .repo_relation()
        .iter()
        .map(|(s, _, _)| s.clone())
        .collect();
    assert!(
        rel.contains(&"ok".to_string()),
        "repo builtin reflects the pulled repo: {:?}",
        eng.repo_relation()
    );
    assert!(
        !rel.contains(&"no".to_string()),
        "filtered repo must not appear in repo builtin: {:?}",
        eng.repo_relation()
    );
}

/// A repo-sink rule whose body carries a scan/match/... op is rejected somewhere
/// in the pipeline: the head-var binding check (a scan binds head vars only from
/// its own captures, and `slug`/`root`/`url` are not scan captures) fires at tick
/// time; the drain-time "derived-style" check is the defense-in-depth if the
/// binding validator ever loosens. Either error keeps the author from writing a
/// sink that silently does nothing.
#[test]
fn repo_sink_with_source_body_is_rejected() {
    let d = sandbox("reject");
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("src/a.rs"), "fn a() {}\n").unwrap();
    fs::write(
        d.join("p.dl"),
        "repo(slug, root, url) <- scan(\"WORK\", \"src/**/*.rs\", p, rev).\n",
    )
    .unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    let (prog, _, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    // Collect the FIRST rejection (tick or drain), then assert it names a
    // repo-sink source-body problem (either error message is acceptable).
    let msg = match eng.tick(&prog, true) {
        Ok(()) => match eng.drain_external_sinks(&prog) {
            Ok(_) => String::new(),
            Err(e) => format!("{e}"),
        },
        Err(e) => format!("{e}"),
    };
    assert!(
        (msg.contains("repo-sink") && msg.contains("derived-style")) || msg.contains("head var"),
        "expected a repo-sink source-body rejection (drain-time OR head-var binding), got: {msg}"
    );
}
