//! CONTROL: 11 passed, 0 failed.
//!
//! SABOTAGE 1, answer `git_ref` with `observation.direct` instead of the peeled
//! oid: `git_ref_names_the_namespace_and_peels_the_tag` goes RED at the
//! target_sha equality (10 passed, 1 failed). An annotated tag's row would
//! carry the TAG object and join nothing commit-keyed.
//!
//! SABOTAGE 2, push a `g<n>` column only when the capture group participated:
//! `repo_grep_at_carries_every_declared_group` goes RED with `left: []` (10
//! passed, 1 failed). `select_columns` drops a row missing any declared column,
//! so a two-group pattern would lose every row it matched.
//!
//! NO NETWORK. Every fixture is a temp repository this file builds with `git`,
//! which is a test fixture and not the engine; the executors under test reach
//! Git only through soopy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sprefa_engine_rs::executors::{
    git_history::GitHistoryExecutor, git_refs::GitRefsExecutor, repo_at::RepoAtExecutor,
};
use sprefa_engine_rs::hosts::IHostExecutor;
use sprefa_engine_rs::types::HostRow;

/// A clock reading cannot separate two fixtures under parallel test threads
/// (docs/failure-modes.md, fixture-temp-dir-clock-collision).
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const WHEN: &str = "1700000000 +0000";

struct Repo {
    root: PathBuf,
}

impl Repo {
    fn build(name: &str) -> Repo {
        let root = std::env::temp_dir().join(format!(
            "sprefa_crosswalk_{name}_{}_{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory");
        let repo = Repo { root };
        repo.git(&["init", "-q"]);
        // `init.defaultBranch` is machine configuration, never inherited here.
        repo.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        repo
    }

    fn git(&self, args: &[&str]) {
        let status = std::process::Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "crosswalk-rig")
            .env("GIT_AUTHOR_EMAIL", "rig@sprefa")
            .env("GIT_COMMITTER_NAME", "crosswalk-rig")
            .env("GIT_COMMITTER_EMAIL", "rig@sprefa")
            .env("GIT_AUTHOR_DATE", WHEN)
            .env("GIT_COMMITTER_DATE", WHEN)
            .status()
            .expect("run git");
        assert!(
            status.success(),
            "git {args:?} failed in {}",
            self.root.display()
        );
    }

    fn write(&self, path: &str, body: &str) {
        let target = self.root.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent");
        }
        std::fs::write(target, body).expect("fixture write");
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn path(&self) -> String {
        self.root.display().to_string()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn env_of(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect()
}

/// The rows a declaring host would keep: `select_columns` drops any row missing
/// one of its declared columns, so a test selects the same way.
fn carrying<'r>(rows: &'r [HostRow], columns: &[&str]) -> Vec<&'r HostRow> {
    rows.iter()
        .filter(|row| columns.iter().all(|name| row.contains_key(*name)))
        .collect()
}

fn text(row: &HostRow, column: &str) -> String {
    row.get(column)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn number(row: &HostRow, column: &str) -> i64 {
    row.get(column)
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(-1)
}

fn tuples(rows: &[&HostRow], columns: &[&str]) -> Vec<Vec<String>> {
    let mut out: Vec<Vec<String>> = rows
        .iter()
        .map(|row| columns.iter().map(|name| text(row, name)).collect())
        .collect();
    out.sort();
    out
}

// ═══ git_refs ═══════════════════════════════════════════════════════════════

fn refs_fixture() -> Repo {
    let repo = Repo::build("refs");
    repo.write("one.txt", "one\n");
    repo.commit("base");
    repo.git(&["tag", "v0.1.0"]);
    repo.git(&["branch", "feature"]);
    repo.write("two.txt", "two\n");
    repo.commit("second");
    repo.git(&["tag", "-a", "v1.0.0", "-m", "the first release"]);
    repo
}

#[test]
fn git_ref_names_the_namespace_and_peels_the_tag() {
    let repo = refs_fixture();
    let rows = GitRefsExecutor::new()
        .run("git_ref", "", &env_of(&[("repo", &repo.path())]))
        .expect("git_ref answers");
    let selected = carrying(&rows, &["ref_name", "kind", "target_sha"]);
    let named: Vec<(String, String)> = selected
        .iter()
        .map(|row| (text(row, "ref_name"), text(row, "kind")))
        .collect();
    assert!(named.contains(&("refs/heads/main".to_string(), "branch".to_string())));
    assert!(named.contains(&("refs/heads/feature".to_string(), "branch".to_string())));
    // A branch tip and a tag are both commit OBJECTS, so the object type could
    // never separate these two rows; only the namespace can.
    assert!(named.contains(&("refs/tags/v1.0.0".to_string(), "tag".to_string())));
    assert!(named.contains(&("HEAD".to_string(), "head".to_string())));

    let head = selected
        .iter()
        .find(|row| text(row, "ref_name") == "HEAD")
        .expect("a HEAD row");
    let annotated = selected
        .iter()
        .find(|row| text(row, "ref_name") == "refs/tags/v1.0.0")
        .expect("the annotated tag's ref row");
    // Peeled: the annotated tag's row carries the COMMIT, so it joins a
    // commit-keyed relation. Unpeeled it would carry the tag object instead.
    assert_eq!(text(annotated, "target_sha"), text(head, "target_sha"));
}

#[test]
fn git_tag_keeps_the_lightweight_tag_undated() {
    let repo = refs_fixture();
    let rows = GitRefsExecutor::new()
        .run("git_tag", "", &env_of(&[("repo", &repo.path())]))
        .expect("git_tag answers");
    let selected = carrying(&rows, &["tag_name", "target_sha", "tagged_at", "annotated"]);
    let by_name = |name: &str| {
        selected
            .iter()
            .find(|row| text(row, "tag_name") == name)
            .copied()
            .unwrap_or_else(|| panic!("no {name} row"))
    };
    let annotated = by_name("v1.0.0");
    assert_eq!(annotated.get("annotated"), Some(&serde_json::json!(true)));
    assert_eq!(number(annotated, "tagged_at"), 1_700_000_000);
    let lightweight = by_name("v0.1.0");
    assert_eq!(
        lightweight.get("annotated"),
        Some(&serde_json::json!(false))
    );
    // Zero and NOT the epoch: `annotated` is the column that says so.
    assert_eq!(number(lightweight, "tagged_at"), 0);
}

#[test]
fn the_ref_family_selects_disjoint_rows_from_one_pass() {
    let repo = refs_fixture();
    let executor = GitRefsExecutor::new();
    let env = env_of(&[("repo", &repo.path())]);
    let first = executor.run("git_ref", "", &env).expect("git_ref answers");
    let second = executor.run("git_tag", "", &env).expect("git_tag answers");
    assert_eq!(first, second, "one memoised pass answers both names");
    let ref_rows = carrying(&first, &["ref_name", "kind", "target_sha"]).len();
    let tag_rows = carrying(
        &second,
        &["tag_name", "target_sha", "tagged_at", "annotated"],
    )
    .len();
    assert_eq!(ref_rows, 5, "2 branches, 2 tags and HEAD");
    assert_eq!(tag_rows, 2, "one annotated, one lightweight");
}

// ═══ git_history, the pairwise family ═══════════════════════════════════════

fn fork_fixture() -> Repo {
    let repo = Repo::build("fork");
    repo.write("one.txt", "one\n");
    repo.commit("base");
    repo.git(&["tag", "v0.1.0"]);
    repo.git(&["branch", "feature"]);
    repo.write("two.txt", "two\n");
    repo.commit("second");
    repo.write("three.txt", "three\n");
    repo.commit("third");
    repo.git(&["checkout", "-q", "feature"]);
    repo.write("diverged.txt", "diverged\n");
    repo.commit("diverged");
    repo.git(&["checkout", "-q", "main"]);
    repo
}

#[test]
fn the_pair_family_answers_base_counts_and_ancestry_from_one_graph_call() {
    let repo = fork_fixture();
    let executor = GitHistoryExecutor::new();
    let env = env_of(&[
        ("repo", &repo.path()),
        ("rev_a", "HEAD"),
        ("rev_b", "feature"),
    ]);
    let rows = executor
        .run("git_merge_base", "", &env)
        .expect("merge base answers");
    assert_eq!(
        rows,
        executor
            .run("git_ahead_behind", "", &env)
            .expect("counts answer")
    );

    let bases = carrying(&rows, &["base_sha"]);
    assert_eq!(bases.len(), 1, "one best common ancestor");

    let counts = carrying(&rows, &["ahead_count", "behind_count"]);
    assert_eq!(counts.len(), 1);
    assert_eq!(number(counts[0], "ahead_count"), 2);
    // Asymmetric on purpose: a swapped projection cannot pass this.
    assert_eq!(number(counts[0], "behind_count"), 1);

    // Two diverged tips: neither reaches the other, so ZERO rows and not a null.
    let edges = carrying(&rows, &["ancestor_sha", "descendant_sha"]);
    assert!(
        edges.is_empty(),
        "diverged tips answer no ancestry, got {edges:?}"
    );
}

#[test]
fn ancestry_answers_one_row_when_one_direction_holds() {
    let repo = fork_fixture();
    let rows = GitHistoryExecutor::new()
        .run(
            "git_ancestor",
            "",
            &env_of(&[
                ("repo", &repo.path()),
                ("rev_a", "v0.1.0"),
                ("rev_b", "HEAD"),
            ]),
        )
        .expect("ancestry answers");
    let edges = carrying(&rows, &["ancestor_sha", "descendant_sha"]);
    assert_eq!(
        edges.len(),
        1,
        "v0.1.0 is an ancestor of HEAD and not the reverse"
    );
}

#[test]
fn an_unresolvable_revision_answers_no_rows_rather_than_stopping() {
    let repo = fork_fixture();
    let rows = GitHistoryExecutor::new()
        .run(
            "git_merge_base",
            "",
            &env_of(&[
                ("repo", &repo.path()),
                ("rev_a", "HEAD"),
                ("rev_b", "no-such-ref"),
            ]),
        )
        .expect("an absent revision is data, never a stop");
    assert!(rows.is_empty());
}

// ═══ git_history, the diff family ═══════════════════════════════════════════

const BINARY_BASE: &str = "\u{0}header\nbody\n";
const BINARY_HEAD: &str = "\u{0}header\nbody changed\n";

fn diff_fixture() -> Repo {
    let repo = Repo::build("diff");
    repo.write("keep.txt", "alpha\nbeta\n");
    repo.write("edit.txt", "one\ntwo\nthree\n");
    repo.write("gone.txt", "removed\n");
    repo.write("moves/origin.txt", "identical content\n");
    repo.write("blob.bin", BINARY_BASE);
    repo.commit("base");
    repo.git(&["tag", "change_base"]);

    std::fs::remove_file(repo.root.join("gone.txt")).expect("remove gone.txt");
    std::fs::remove_file(repo.root.join("moves/origin.txt")).expect("remove origin.txt");
    repo.write("moves/destination.txt", "identical content\n");
    repo.write("edit.txt", "one\nTWO\nthree\nfour\n");
    repo.write("arrived.txt", "fresh\n");
    repo.write("blob.bin", BINARY_HEAD);
    repo.commit("head");
    repo.git(&["tag", "change_head"]);
    repo
}

#[test]
fn the_four_change_kinds_partition_the_diff() {
    let repo = diff_fixture();
    let executor = GitHistoryExecutor::new();
    let env = env_of(&[
        ("repo", &repo.path()),
        ("rev_base", "change_base"),
        ("rev_head", "change_head"),
    ]);
    let rows = executor
        .run("git_change", "", &env)
        .expect("changes answer");
    assert_eq!(
        rows,
        executor
            .run("git_rename", "", &env)
            .expect("renames answer")
    );
    assert_eq!(
        rows,
        executor
            .run("git_changed_line", "", &env)
            .expect("lines answer"),
        "three names ride one diff"
    );

    assert_eq!(
        tuples(
            &carrying(&rows, &["change_kind", "path"]),
            &["change_kind", "path"]
        ),
        vec![
            vec!["created".to_string(), "arrived.txt".to_string()],
            vec!["deleted".to_string(), "gone.txt".to_string()],
            vec!["modified".to_string(), "blob.bin".to_string()],
            vec!["modified".to_string(), "edit.txt".to_string()],
        ],
        "a rename is neither a creation nor a deletion, and keep.txt is untouched"
    );
    assert_eq!(
        tuples(
            &carrying(&rows, &["path_from", "path_to"]),
            &["path_from", "path_to"]
        ),
        vec![vec![
            "moves/origin.txt".to_string(),
            "moves/destination.txt".to_string()
        ]]
    );
}

#[test]
fn a_binary_blob_is_modified_and_contributes_no_line() {
    let repo = diff_fixture();
    let rows = GitHistoryExecutor::new()
        .run(
            "git_changed_line",
            "",
            &env_of(&[
                ("repo", &repo.path()),
                ("rev_base", "change_base"),
                ("rev_head", "change_head"),
            ]),
        )
        .expect("lines answer");
    let lines = carrying(&rows, &["path", "line_number"]);
    let paths: Vec<String> = lines.iter().map(|row| text(row, "path")).collect();
    assert!(
        !paths.contains(&"blob.bin".to_string()),
        "a NUL-bearing blob has no hunk"
    );
    // Head-side and 1-based: the deleted path contributes nothing, the created
    // one contributes every line it has.
    assert!(paths.contains(&"arrived.txt".to_string()));
    assert!(!paths.contains(&"gone.txt".to_string()));
    let edited: Vec<i64> = lines
        .iter()
        .filter(|row| text(row, "path") == "edit.txt")
        .map(|row| number(row, "line_number"))
        .collect();
    assert_eq!(edited, vec![2, 4]);
}

// ═══ repo_at ════════════════════════════════════════════════════════════════

fn go_fixture(name: &str, module: &str, requires: &[(&str, &str)]) -> Repo {
    let repo = Repo::build(name);
    let mut manifest = format!("module {module}\n\ngo 1.21\n\nrequire (\n");
    for (target, version) in requires {
        manifest.push_str(&format!("\t{target} {version}\n"));
    }
    manifest.push_str(")\n");
    repo.write("go.mod", &manifest);
    repo.write("main.go", "package main\n\nfunc main() {}\n");
    repo.commit("pinned");
    repo.git(&["tag", "v1.0.0"]);
    repo.write("later.go", "package main\n\nfunc later() {}\n");
    repo.commit("after the tag");
    repo
}

#[test]
fn repo_files_at_reads_the_tag_and_not_the_tip() {
    let repo = go_fixture(
        "filesat",
        "example.com/alpha",
        &[("example.com/shared", "v1.2.0")],
    );
    let executor = RepoAtExecutor::new();
    let at_tag = executor
        .run(
            "repo_files_at",
            "",
            &env_of(&[("root", &repo.path()), ("rev", "v1.0.0"), ("glob", "*.go")]),
        )
        .expect("listing answers");
    let tagged = tuples(&carrying(&at_tag, &["path", "digest"]), &["path"]);
    assert_eq!(
        tagged,
        vec![vec!["main.go".to_string()]],
        "later.go is not in v1.0.0"
    );

    let at_head = executor
        .run(
            "repo_files_at",
            "",
            &env_of(&[("root", &repo.path()), ("rev", "HEAD"), ("glob", "*.go")]),
        )
        .expect("listing answers");
    assert_eq!(
        tuples(&carrying(&at_head, &["path", "digest"]), &["path"]),
        vec![vec!["later.go".to_string()], vec!["main.go".to_string()]],
        "a rev-pinned host is a different witness per rev, not one cached answer"
    );
}

#[test]
fn repo_grep_at_carries_every_declared_group() {
    let repo = go_fixture(
        "grepat",
        "example.com/alpha",
        &[
            ("example.com/shared", "v1.2.0"),
            ("github.com/pkg/errors", "v0.9.1"),
        ],
    );
    let rows = RepoAtExecutor::new()
        .run(
            "repo_grep_at",
            "",
            &env_of(&[
                ("root", &repo.path()),
                ("rev", "v1.0.0"),
                ("glob", "go.mod"),
                (
                    "pattern",
                    r"([a-zA-Z0-9._-]+/[a-zA-Z0-9._/-]+)[ \t]+(v[0-9][a-zA-Z0-9._+-]*)",
                ),
            ]),
        )
        .expect("grep answers");
    let selected = carrying(&rows, &["path", "line", "g1", "g2", "g3"]);
    assert_eq!(
        tuples(&selected, &["g1", "g2"]),
        vec![
            vec!["example.com/shared".to_string(), "v1.2.0".to_string()],
            vec!["github.com/pkg/errors".to_string(), "v0.9.1".to_string()],
        ]
    );
    // A group the pattern does not have renders "" and never null: a null drops
    // the whole row at `select_columns` and a dropped row is a lost fact.
    assert!(selected.iter().all(|row| text(row, "g3").is_empty()));
    assert!(
        selected.iter().all(|row| number(row, "line") > 0),
        "1-based"
    );
}

#[test]
fn a_host_outside_its_family_is_a_named_stop() {
    let repo = refs_fixture();
    let failure = GitRefsExecutor::new()
        .run("git_change", "", &env_of(&[("repo", &repo.path())]))
        .expect_err("a foreign host name stops");
    assert!(failure.message.contains("git_ref"), "{}", failure.message);
}
