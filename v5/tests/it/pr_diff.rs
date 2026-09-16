//! D5.8 PR-as-rows: diff a GitHub PR's graph with NO second checkout. A
//! `pr_want(number)` fact drives a `gh` read-effect that resolves the PR's base
//! and head commit SHAs; those two SHAs become `scan` rev coordinates on ONE
//! working tree, and the rev-aware extraction twins (type_entity_rev /
//! type_link_rev) carry the `rev`, so the diff is a per-rev anti-join. This
//! mirrors examples/pr-diff.dl's core.
//!
//! Hermetic: a fake `gh` script on a PATH shim echoes fixed JSON pointing at the
//! two commits the test creates in a scratch git fixture; the real
//! `ShellEffectExec` shells `sh -c` (inheriting PATH), so it resolves the shim.
//! PATH is process-global, so a lock serializes the mutation (the `clock_lock`
//! idiom for `DL_NOW_SECS`). The shim dir is PREPENDED, so `git`/`sh` still
//! resolve normally for any test running in parallel.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use sprefa_v5::db;
use sprefa_v5::engine::{async_effect_arity, shell_templates, Engine, ShellEffectExec};
use sprefa_v5::prepare_paths;

/// Serializes PATH mutation across parallel tests in the single `it` binary.
static PATH_LOCK: Mutex<()> = Mutex::new(());

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_pr_diff_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
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

fn init_git(d: &Path) {
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@example.com"]);
    git(d, &["config", "user.name", "T"]);
    git(d, &["config", "commit.gpgsign", "false"]);
}

fn count(eng: &Engine, sql: &str) -> i64 {
    eng.query_sql(sql, &[]).unwrap().into_iter().next().unwrap()[0]
        .as_i64()
        .unwrap()
}

/// The PR-diff program: examples/pr-diff.dl's core (types + type_link edges),
/// with `pr_want("1")`. The `gh_pr` sh decl shells the real `gh pr view`, which
/// the PATH shim answers.
const PR_PROG: &str = r#"
rel pr_want(number: text).
pr_want("1").

sh gh_pr(number) -> (body: text) =
  `gh pr view {number} --json baseRefOid,headRefOid 2>/dev/null | tr -d '\n'`.

rel pr_view(number: text, body: text).
pr_view(number, body) <- @async pr_want(number), gh_pr(number) -> (body).

rel pr_base(number: text, base_sha: text).
pr_base(number, base_sha) <- pr_view(number, body), jsonp(body, "baseRefOid", base_sha).

rel pr_head(number: text, head_sha: text).
pr_head(number, head_sha) <- pr_view(number, body), jsonp(body, "headRefOid", head_sha).

rel pr_pair(base_sha: text, head_sha: text).
pr_pair(base_sha, head_sha) <- pr_base(number, base_sha), pr_head(number, head_sha).

rel pr_seen(path: file).
pr_seen(path) <- pr_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
pr_seen(path) <- pr_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

rel pr_head_rev(rev: text).
pr_head_rev(resolved) <- pr_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, resolved).

rel pr_base_rev(rev: text).
pr_base_rev(resolved) <- pr_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, resolved).

rel pr_bare_node(rev: text, sym: text, kind: text).
pr_bare_node(rev, sym, "struct") <- type_entity_rev(_, sym, _, "struct", _, _, _, rev).
pr_bare_node(rev, sym, "enum")   <- type_entity_rev(_, sym, _, "enum", _, _, _, rev).
pr_bare_node(rev, sym, "trait")  <- type_entity_rev(_, sym, _, "trait", _, _, _, rev).

rel pr_bare_edge(rev: text, a: text, b: text, kind: text).
pr_bare_edge(rev, src, dst, kind) <-
  type_link_rev(src, dst, kind, rev),
  pr_bare_node(rev, src, _),
  pr_bare_node(rev, dst, _).

rel pr_node_added(sym: text, kind: text).
pr_node_added(sym, kind) <-
  pr_head_rev(head), pr_base_rev(base),
  pr_bare_node(head, sym, kind),
  !pr_bare_node(base, sym, kind).

rel pr_node_removed(sym: text, kind: text).
pr_node_removed(sym, kind) <-
  pr_head_rev(head), pr_base_rev(base),
  pr_bare_node(base, sym, kind),
  !pr_bare_node(head, sym, kind).

rel pr_edge_added(a: text, b: text, kind: text).
pr_edge_added(a, b, kind) <-
  pr_head_rev(head), pr_base_rev(base),
  pr_bare_edge(head, a, b, kind),
  !pr_bare_edge(base, a, b, kind).

rel pr_edge_removed(a: text, b: text, kind: text).
pr_edge_removed(a, b, kind) <-
  pr_head_rev(head), pr_base_rev(base),
  pr_bare_edge(base, a, b, kind),
  !pr_bare_edge(head, a, b, kind).
"#;

/// The PR's base commit adds `Beta` (linking `Alpha`); the head commit swaps
/// `Beta` for `Gamma` (also linking `Alpha`). The gh effect resolves both SHAs
/// from a PATH-shimmed `gh pr view`, and the per-rev anti-join reports exactly
/// one node/edge added (Gamma) and one removed (Beta) -- no working tree fetched.
#[test]
fn pr_diff_resolves_shas_via_gh_and_reports_exact_delta() {
    let _lock = PATH_LOCK.lock().unwrap();
    let d = sandbox("delta");

    // BASE commit: Alpha, Beta; Beta has a field of type Alpha (a type_link).
    fs::write(
        d.join("src/a.rs"),
        "pub struct Alpha { pub n: i64 }\n\
         pub struct Beta { pub a: Alpha }\n",
    )
    .unwrap();
    init_git(&d);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "base"]);
    let base_sha = git(&d, &["rev-parse", "HEAD"]);

    // HEAD commit: Beta -> Gamma (Beta removed, Gamma added), both linking Alpha.
    fs::write(
        d.join("src/a.rs"),
        "pub struct Alpha { pub n: i64 }\n\
         pub struct Gamma { pub a: Alpha }\n",
    )
    .unwrap();
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "head"]);
    let head_sha = git(&d, &["rev-parse", "HEAD"]);

    // Fake `gh`: ignores its args, echoes the PR's base/head SHAs as gh's own
    // `--json baseRefOid,headRefOid` shape.
    let bin = d.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let gh = bin.join("gh");
    fs::write(
        &gh,
        format!(
        "#!/bin/sh\nprintf '%s' '{{\"baseRefOid\":\"{base_sha}\",\"headRefOid\":\"{head_sha}\"}}'\n"
    ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", bin.display(), old_path));

    fs::write(d.join("pr-diff.dl"), PR_PROG).unwrap();
    let (prog, _diags, _) = prepare_paths(&[d.join("pr-diff.dl")]).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Drive the effect chain to a fixpoint: tick 1 queues the gh effect; the
    // drain lands pr_view; the two shas extract, pr_pair joins, the data-driven
    // scans read pr_pair (a tick later), extraction builds the twins, and the
    // anti-joins converge. Interleave tick + drain (drain is idempotent once
    // pr_view is filled), then a final settling tick.
    let templates = shell_templates(&prog);
    let arity = async_effect_arity(&prog);
    for _ in 0..8 {
        eng.tick(&prog, true).unwrap();
        let exec = ShellEffectExec {
            templates: templates.clone(),
            n_out: arity.clone(),
            cwd: eng.root(),
        };
        eng.drain_effects(&prog, &exec).unwrap();
    }
    eng.tick(&prog, true).unwrap();

    std::env::set_var("PATH", old_path);

    // The gh effect resolved and pr_pair carried both committed shas.
    assert_eq!(
        count(&eng, &format!(
            "SELECT COUNT(*) FROM rel_pr_pair_txt WHERE base_sha = '{base_sha}' AND head_sha = '{head_sha}'"
        )),
        1,
        "pr_pair holds the gh-resolved base/head shas",
    );
    // The twins spanned both revs.
    assert_eq!(
        count(&eng, "SELECT COUNT(DISTINCT rev) FROM rel_type_entity_rev"),
        2,
        "type_entity_rev spans base + head shas",
    );

    // Nodes: exactly Gamma added, Beta removed (Alpha unchanged).
    assert_eq!(
        count(&eng, "SELECT COUNT(*) FROM rel_pr_node_added"),
        1,
        "one node added"
    );
    assert_eq!(
        count(&eng, "SELECT COUNT(*) FROM rel_pr_node_removed"),
        1,
        "one node removed"
    );
    assert_eq!(
        count(
            &eng,
            "SELECT COUNT(*) FROM rel_pr_node_added_txt WHERE sym LIKE '%::struct::Gamma'"
        ),
        1,
        "Gamma is the added node"
    );
    assert_eq!(
        count(
            &eng,
            "SELECT COUNT(*) FROM rel_pr_node_removed_txt WHERE sym LIKE '%::struct::Beta'"
        ),
        1,
        "Beta is the removed node"
    );
    assert_eq!(
        count(
            &eng,
            "SELECT COUNT(*) FROM rel_pr_node_added_txt WHERE sym LIKE '%Alpha'"
        ),
        0
    );
    assert_eq!(
        count(
            &eng,
            "SELECT COUNT(*) FROM rel_pr_node_removed_txt WHERE sym LIKE '%Alpha'"
        ),
        0
    );

    // Edges: exactly one field link added (Gamma->Alpha), one removed (Beta->Alpha).
    assert_eq!(
        count(&eng, "SELECT COUNT(*) FROM rel_pr_edge_added"),
        1,
        "one edge added"
    );
    assert_eq!(
        count(&eng, "SELECT COUNT(*) FROM rel_pr_edge_removed"),
        1,
        "one edge removed"
    );
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_pr_edge_added_txt WHERE a LIKE '%::struct::Gamma' AND b LIKE '%::struct::Alpha'"),
        1, "the added edge is Gamma -> Alpha");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_pr_edge_removed_txt WHERE a LIKE '%::struct::Beta' AND b LIKE '%::struct::Alpha'"),
        1, "the removed edge is Beta -> Alpha");
}
