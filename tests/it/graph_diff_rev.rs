//! D5.7: the rev-pair graph diff on ONE checkout. `.dl/graph-diff.dl` moved
//! from a two-worktree config-repo pair to `diff_pair(base_rev, head_rev)` over
//! the rev-aware extraction twins (type_entity_rev / type_link_rev /
//! call_edge_rev). This exercises the exact rule shape that file ships: scan
//! both a committed rev (HEAD) and WORK, build a per-rev node/edge set from the
//! twins, and anti-join to get node/edge added/removed. The counts are exact.
//!
//! The inline program mirrors `.dl/graph-diff.dl`'s core (types + type_links);
//! it uses `diff_pair("HEAD", "WORK")` instead of the served default
//! `diff_pair("main", "WORK")` so a git fixture with a single commit resolves
//! (HEAD = the base commit, WORK = the edited working tree).

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("graph_diff_rev_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git");
    assert!(out.status.success(), "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr));
}

fn col_set(eng: &Engine, sql: &str) -> BTreeSet<String> {
    eng.query_sql(sql, &[]).unwrap().into_iter()
        .map(|r| r[0].as_str().unwrap().to_string()).collect()
}

fn count(eng: &Engine, sql: &str) -> i64 {
    eng.query_sql(sql, &[]).unwrap().into_iter().next().unwrap()[0].as_i64().unwrap()
}

/// The rev-pair diff program: the core rules from `.dl/graph-diff.dl`,
/// with `diff_pair("HEAD", "WORK")`. Types + type_link edges only (the exact
/// diff-count surface); the df-derived fill member edges are covered by
/// flow-panel's own tests and are name-joined the same way.
const DIFF_PROG: &str = r#"
rel diff_pair(base_rev: text, head_rev: text).
diff_pair("HEAD", "WORK").

rel diff_seen(path: file).
diff_seen(path) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
diff_seen(path) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

rel head_rev(rev: text).
head_rev(resolved) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, resolved).

rel base_rev(rev: text).
base_rev(resolved) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, resolved).

rel bare_node(repo: text, sym: text, kind: text).
bare_node(rev, sym, "struct") <- type_entity_rev(_, sym, _, "struct", _, _, _, rev).
bare_node(rev, sym, "enum")   <- type_entity_rev(_, sym, _, "enum", _, _, _, rev).
bare_node(rev, sym, "trait")  <- type_entity_rev(_, sym, _, "trait", _, _, _, rev).

rel bare_edge(repo: text, a: text, b: text, kind: text).
bare_edge(rev, src, dst, kind) <-
  type_link_rev(src, dst, kind, rev),
  bare_node(rev, src, _),
  bare_node(rev, dst, _).

rel node_added(sym: text, kind: text).
node_added(sym, kind) <-
  head_rev(head), base_rev(base),
  bare_node(head, sym, kind),
  !bare_node(base, sym, kind).

rel node_removed(sym: text, kind: text).
node_removed(sym, kind) <-
  head_rev(head), base_rev(base),
  bare_node(base, sym, kind),
  !bare_node(head, sym, kind).

rel edge_added(a: text, b: text, kind: text).
edge_added(a, b, kind) <-
  head_rev(head), base_rev(base),
  bare_edge(head, a, b, kind),
  !bare_edge(base, a, b, kind).

rel edge_removed(a: text, b: text, kind: text).
edge_removed(a, b, kind) <-
  head_rev(head), base_rev(base),
  bare_edge(base, a, b, kind),
  !bare_edge(head, a, b, kind).
"#;

fn init_git(d: &Path) {
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@example.com"]);
    git(d, &["config", "user.name", "T"]);
}

/// Add a struct (`Gamma`) with a link to `Alpha`, remove a struct (`Beta`) with
/// a link to `Alpha`, between the committed base and the working tree. The diff
/// reports exactly one node added/removed and one edge added/removed.
#[test]
fn rev_pair_diff_reports_exact_added_and_removed() {
    let d = sandbox("addremove");
    // BASE (committed): Alpha, Beta; Beta has a field of type Alpha (a type_link).
    fs::write(d.join("src/a.rs"),
        "pub struct Alpha { pub n: i64 }\n\
         pub struct Beta { pub a: Alpha }\n").unwrap();
    init_git(&d);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "base"]);

    // WORK: Beta -> Gamma (Beta removed, Gamma added), both linking Alpha.
    fs::write(d.join("src/a.rs"),
        "pub struct Alpha { pub n: i64 }\n\
         pub struct Gamma { pub a: Alpha }\n").unwrap();

    let prog = parse::parse(lex::lex(DIFF_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    // The base/head scans are data-driven (the rev slot is a var bound by the
    // diff_pair FACT), so the fact lands tick 1, the scans + extraction land
    // tick 2, and the derived diff rels converge after. Tick to a fixpoint,
    // exactly the daemon's steady-state convergence.
    for _ in 0..4 { eng.tick(&prog, true).unwrap(); }

    // The twins spanned both revs (sanity: the data-driven base scan resolved).
    assert_eq!(col_set(&eng, "SELECT DISTINCT rev FROM rel_type_entity_rev_txt").len(), 2,
        "type_entity_rev spans HEAD + WORK");

    // Nodes: exactly Gamma added, Beta removed (Alpha unchanged, present both).
    let added = col_set(&eng, "SELECT sym FROM rel_node_added_txt");
    let removed = col_set(&eng, "SELECT sym FROM rel_node_removed_txt");
    assert_eq!(added.len(), 1, "one node added: {added:?}");
    assert_eq!(removed.len(), 1, "one node removed: {removed:?}");
    assert!(added.iter().next().unwrap().ends_with("::struct::Gamma"),
        "Gamma is the added node: {added:?}");
    assert!(removed.iter().next().unwrap().ends_with("::struct::Beta"),
        "Beta is the removed node: {removed:?}");
    // Alpha never shows up in either set.
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_added_txt WHERE sym LIKE '%Alpha'"), 0);
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_removed_txt WHERE sym LIKE '%Alpha'"), 0);

    // Edges: exactly one field link added (Gamma->Alpha) and one removed
    // (Beta->Alpha).
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_added_txt"), 1,
        "one edge added (Gamma -> Alpha)");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_removed_txt"), 1,
        "one edge removed (Beta -> Alpha)");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_edge_added_txt WHERE a LIKE '%::struct::Gamma' AND b LIKE '%::struct::Alpha'"), 1,
        "the added edge is Gamma -> Alpha");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_edge_removed_txt WHERE a LIKE '%::struct::Beta' AND b LIKE '%::struct::Alpha'"), 1,
        "the removed edge is Beta -> Alpha");
}

/// The noise gate: when WORK content is byte-identical to the committed base
/// (two DIFFERENT rev strings, same code), every node/edge is present at both
/// revs, so the diff is empty. This is the test that fails if a line-keyed id
/// or a rev-fold leaks into the sym-keyed diff.
#[test]
fn rev_pair_diff_is_empty_when_work_equals_base() {
    let d = sandbox("nochange");
    fs::write(d.join("src/a.rs"),
        "pub struct Alpha { pub n: i64 }\n\
         pub struct Beta { pub a: Alpha }\n\
         pub fn make() -> Beta { Beta { a: Alpha { n: 1 } } }\n").unwrap();
    init_git(&d);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "base"]);
    // WORK is untouched: identical to HEAD.

    let prog = parse::parse(lex::lex(DIFF_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    // The base/head scans are data-driven (the rev slot is a var bound by the
    // diff_pair FACT), so the fact lands tick 1, the scans + extraction land
    // tick 2, and the derived diff rels converge after. Tick to a fixpoint,
    // exactly the daemon's steady-state convergence.
    for _ in 0..4 { eng.tick(&prog, true).unwrap(); }

    // Both revs populated, with identical sym sets (the diff's premise).
    assert_eq!(col_set(&eng, "SELECT DISTINCT rev FROM rel_type_entity_rev_txt").len(), 2,
        "type_entity_rev spans HEAD + WORK");
    // `WORK` is an ALIAS resolved at the scan seam, so the stored rev is an oid
    // (`<sha>+` here: the sandbox's db files are untracked, so the tree is
    // dirty). Read the head rev the program itself bound rather than a literal.
    let work = col_set(&eng,
        "SELECT sym FROM rel_bare_node_txt WHERE repo IN (SELECT rev FROM rel_head_rev_txt)");
    assert!(!work.is_empty(), "WORK nodes exist");
    assert_eq!(work, col_set(&eng,
        "SELECT sym FROM rel_bare_node_txt WHERE repo NOT IN (SELECT rev FROM rel_head_rev_txt)"),
        "the two revs carry identical sym sets");

    // The diff is empty across the board.
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_added_txt"), 0, "no nodes added");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_removed_txt"), 0, "no nodes removed");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_added_txt"), 0, "no edges added");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_removed_txt"), 0, "no edges removed");
}

// ============================================================================
// D5.9 — the OLD bench harness's four synthetic-PR scenarios, on the rev axis
// (one checkout: base = the committed HEAD, head = the edited WORK tree). The
// old bench/graph_diff/harness.sh drove these against a two-worktree pair with
// the mutation INVERTED (it patched BASE's working tree, so add -> *_removed);
// here the mutation lands naturally in WORK (= head), so the counts are the
// un-inverted mirror. Scenario map (harness S1..S4):
//   S1 call-move   -> `scenario_s1_call_site_move`      (1 edge added, 1 removed)
//   S2 field-fill  -> `scenario_s2_field_fill_df`       (1 field node + 1 fill added)
//   S3 delete-fn   -> `scenario_s3_delete_fn`           (1 fn node + 1 call removed)
//   S4 noise-gate  -> `rev_pair_diff_is_empty_when_work_equals_base` (above; 0/0/0/0)
// The two tests above (`rev_pair_diff_reports_exact_added_and_removed` +
// the noise gate) stay; these three add the call-edge and df-fill surfaces the
// type-only DIFF_PROG doesn't reach. FULL_PROG mirrors .dl/graph-diff.dl's core
// (all node kinds + type_link + call + the ctor-fill member edge).
// ============================================================================

/// The full graph-diff core from `.dl/graph-diff.dl` (types + calls + df-fill
/// member edges), with `diff_pair("HEAD", "WORK")` for a single-commit fixture.
const FULL_PROG: &str = r#"
rel diff_pair(base_rev: text, head_rev: text).
diff_pair("HEAD", "WORK").

rel diff_seen(path: file).
diff_seen(path) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, rev).
diff_seen(path) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, rev).

rel head_rev(rev: text).
head_rev(resolved) <- diff_pair(_, head_ref), scan(head_ref, "src/**/*.rs", path, resolved).
rel base_rev(rev: text).
base_rev(resolved) <- diff_pair(base_ref, _), scan(base_ref, "src/**/*.rs", path, resolved).

rel bare_node(repo: text, sym: text, kind: text).
bare_node(rev, sym, "struct")    <- type_entity_rev(_, sym, _, "struct", _, _, _, rev).
bare_node(rev, sym, "enum")      <- type_entity_rev(_, sym, _, "enum", _, _, _, rev).
bare_node(rev, sym, "trait")     <- type_entity_rev(_, sym, _, "trait", _, _, _, rev).
bare_node(rev, sym, "class")     <- type_entity_rev(_, sym, _, "class", _, _, _, rev).
bare_node(rev, sym, "interface") <- type_entity_rev(_, sym, _, "interface", _, _, _, rev).
bare_node(rev, sym, "function")  <- type_entity_rev(_, sym, _, "function", _, _, _, rev).
bare_node(rev, sym, "method")    <- type_entity_rev(_, sym, _, "method", _, _, _, rev).
bare_node(rev, sym, "const")     <- type_entity_rev(_, sym, _, "const", _, _, _, rev).

rel owner_at_rev(owner_sym: text, name: text, repo: text, rev: text).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "struct", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "enum", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "trait", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "class", _, _, _, rev).
owner_at_rev(owner_sym, name, replace_re(owner_sym, "::.*$", ""), rev) <-
  type_entity_rev(_, owner_sym, name, "interface", _, _, _, rev).

rel fn_at_rev(fn_sym: text, bare: text, repo: text, rev: text).
fn_at_rev(fn_sym, replace_re(fn_sym, "^[^:]*::", ""), replace_re(fn_sym, "::.*$", ""), rev) <-
  type_entity_rev(_, fn_sym, _, "function", _, _, _, rev).
fn_at_rev(fn_sym, replace_re(fn_sym, "^[^:]*::", ""), replace_re(fn_sym, "::.*$", ""), rev) <-
  type_entity_rev(_, fn_sym, _, "method", _, _, _, rev).

bare_node(rev, "${owner_sym}::field::${fld}", "field") <-
  df_node_rev(node, "new", ty, _, _, _, _, rev),
  df_node_repo_rev(node, repo, rev),
  df_field_rev(node, fld, _, rev), fld != "..",
  owner_at_rev(owner_sym, ty, repo, rev).

rel bare_edge(repo: text, a: text, b: text, kind: text).
bare_edge(rev, src, dst, kind) <-
  type_link_rev(src, dst, kind, rev),
  bare_node(rev, src, _),
  bare_node(rev, dst, _).
bare_edge(rev, caller, callee, "call") <-
  call_edge_rev(caller, callee, _, rev),
  bare_node(rev, caller, _),
  bare_node(rev, callee, _).
bare_edge(rev, fn_sym, "${owner_sym}::field::${fld}", "fill") <-
  df_node_rev(node, "new", ty, fill_fn, _, _, _, rev),
  df_node_repo_rev(node, repo, rev),
  df_field_rev(node, fld, _, rev), fld != "..",
  owner_at_rev(owner_sym, ty, repo, rev),
  fn_at_rev(fn_sym, fill_fn, repo, rev).

rel node_added(sym: text, kind: text).
node_added(sym, kind) <-
  head_rev(head), base_rev(base),
  bare_node(head, sym, kind),
  !bare_node(base, sym, kind).
rel node_removed(sym: text, kind: text).
node_removed(sym, kind) <-
  head_rev(head), base_rev(base),
  bare_node(base, sym, kind),
  !bare_node(head, sym, kind).
rel edge_added(a: text, b: text, kind: text).
edge_added(a, b, kind) <-
  head_rev(head), base_rev(base),
  bare_edge(head, a, b, kind),
  !bare_edge(base, a, b, kind).
rel edge_removed(a: text, b: text, kind: text).
edge_removed(a, b, kind) <-
  head_rev(head), base_rev(base),
  bare_edge(base, a, b, kind),
  !bare_edge(head, a, b, kind).
"#;

/// Drive the full-program engine to a fixpoint against a fixture whose committed
/// HEAD is the base and whose working tree is the head. Four ticks converge the
/// data-driven base scan + the derived diff (same as the tests above).
fn run_full(d: &Path) -> Engine {
    let prog = parse::parse(lex::lex(FULL_PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.to_path_buf());
    for _ in 0..4 { eng.tick(&prog, true).unwrap(); }
    eng
}

/// S1 (call-site move): the base calls `helper()` from `run()`; the head moves
/// that call into `other()`. The call edge moves, so exactly one edge added
/// (other -> helper) and one removed (run -> helper); no node changes.
#[test]
fn scenario_s1_call_site_move() {
    let d = sandbox("s1_callmove");
    fs::write(d.join("src/a.rs"),
        "pub fn helper() {}\n\
         pub fn run() { helper(); }\n\
         pub fn other() {}\n").unwrap();
    init_git(&d);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "base"]);
    // WORK: move the helper() call from run() to other().
    fs::write(d.join("src/a.rs"),
        "pub fn helper() {}\n\
         pub fn run() {}\n\
         pub fn other() { helper(); }\n").unwrap();

    let eng = run_full(&d);
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_added_txt"), 0, "S1: no nodes added");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_removed_txt"), 0, "S1: no nodes removed");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_added_txt"), 1, "S1: one edge added");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_removed_txt"), 1, "S1: one edge removed");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_edge_added_txt WHERE a LIKE '%::function::other' AND b LIKE '%::function::helper' AND kind = 'call'"),
        1, "S1: added call edge is other -> helper");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_edge_removed_txt WHERE a LIKE '%::function::run' AND b LIKE '%::function::helper' AND kind = 'call'"),
        1, "S1: removed call edge is run -> helper");
}

/// S2 (field fill, df family): the head adds a `level` field to `Config` and
/// fills it at the single ctor site. The df-derived field node + fill edge
/// surface added; the pre-existing `name` fill is unchanged. Exactly one node
/// added (the field) + one edge added (the fill); nothing removed.
#[test]
fn scenario_s2_field_fill_df() {
    let d = sandbox("s2_fieldfill");
    fs::write(d.join("src/a.rs"),
        "pub struct Config { pub name: i64 }\n\
         pub fn make() -> Config { Config { name: 0 } }\n").unwrap();
    init_git(&d);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "base"]);
    // WORK: add the `level` field and fill it at the ctor.
    fs::write(d.join("src/a.rs"),
        "pub struct Config { pub name: i64, pub level: i64 }\n\
         pub fn make() -> Config { Config { name: 0, level: 0 } }\n").unwrap();

    let eng = run_full(&d);
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_removed_txt"), 0, "S2: no nodes removed");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_removed_txt"), 0, "S2: no edges removed");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_added_txt"), 1, "S2: one node added");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_added_txt"), 1, "S2: one edge added");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_node_added_txt WHERE sym LIKE '%::struct::Config::field::level' AND kind = 'field'"),
        1, "S2: added node is the Config.level field");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_edge_added_txt WHERE a LIKE '%::function::make' AND b LIKE '%::struct::Config::field::level' AND kind = 'fill'"),
        1, "S2: added edge is make -> Config.level fill");
}

/// S3 (delete a self-contained fn): the base defines `helper()` and calls it
/// from `run()`; the head deletes the def (the call in `run()` stays, but now
/// resolves to nothing, so its edge drops). Exactly one node removed (the fn)
/// + one edge removed (run -> helper); nothing added.
#[test]
fn scenario_s3_delete_fn() {
    let d = sandbox("s3_deletefn");
    fs::write(d.join("src/a.rs"),
        "pub fn helper() {}\n\
         pub fn run() { helper(); }\n").unwrap();
    init_git(&d);
    git(&d, &["add", "."]);
    git(&d, &["commit", "-q", "-m", "base"]);
    // WORK: delete helper()'s def (the call in run() stays, now unresolved).
    fs::write(d.join("src/a.rs"),
        "pub fn run() { helper(); }\n").unwrap();

    let eng = run_full(&d);
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_added_txt"), 0, "S3: no nodes added");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_added_txt"), 0, "S3: no edges added");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_node_removed_txt"), 1, "S3: one node removed");
    assert_eq!(count(&eng, "SELECT COUNT(*) FROM rel_edge_removed_txt"), 1, "S3: one edge removed");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_node_removed_txt WHERE sym LIKE '%::function::helper' AND kind = 'function'"),
        1, "S3: removed node is the helper fn");
    assert_eq!(count(&eng,
        "SELECT COUNT(*) FROM rel_edge_removed_txt WHERE a LIKE '%::function::run' AND b LIKE '%::function::helper' AND kind = 'call'"),
        1, "S3: removed edge is run -> helper");
}
