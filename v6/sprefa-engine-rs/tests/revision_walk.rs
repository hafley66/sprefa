//! CONTROL: 7 passed, 0 failed.
//!
//! SABOTAGE 1, restore the pre-fix `soopy::Revision::Worktree` as `files_at`'s
//! arm of `files_revision`: `files_at_walks_the_commit_and_files_the_edit`,
//! `files_at_a_tag_does_not_see_the_tip` and `files_at_without_a_rev_stops` go
//! RED (4 passed, 3 failed). That arm is the defect this file pins: a demand
//! naming a rev was answered from the worktree, silently.
//!
//! SABOTAGE 2, make `files_revision` fall through to `Revision::Worktree` for an
//! unknown host instead of stopping: `an_unlisted_file_host_stops` goes RED
//! (6 passed, 1 failed).
//!
//! NO NETWORK. The fixture is a temp repository this file builds with `git`,
//! which is a test fixture and not the engine; the executor under test reaches
//! Git only through soopy.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sprefa_engine_rs::change_facts::{ChangeKind, IRevisionDiffer, SoopyRevisionDiffer};
use sprefa_engine_rs::hosts::{IHostExecutor, SoopyFilesExecutor};
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
            "sprefa_revwalk_{name}_{}_{}",
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

    fn git(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "revwalk-rig")
            .env("GIT_AUTHOR_EMAIL", "rig@sprefa")
            .env("GIT_COMMITTER_NAME", "revwalk-rig")
            .env("GIT_COMMITTER_EMAIL", "rig@sprefa")
            .env("GIT_AUTHOR_DATE", WHEN)
            .env("GIT_COMMITTER_DATE", WHEN)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed in {}: {}",
            self.root.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
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

    /// The oid the object database holds for a path at a revision.
    fn blob_at(&self, rev: &str, path: &str) -> String {
        self.git(&["rev-parse", &format!("{rev}:{path}")])
    }

    /// The oid the WORKING TREE bytes hash to. `hash-object` without `-w` never
    /// writes, so a dirty file's oid names content the database does not hold.
    fn worktree_oid(&self, path: &str) -> String {
        self.git(&["hash-object", "--", path])
    }

    fn in_object_database(&self, oid: &str) -> bool {
        std::process::Command::new("git")
            .current_dir(&self.root)
            .args(["cat-file", "-e", oid])
            .status()
            .expect("run git cat-file")
            .success()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A committed file plus an uncommitted edit to it: the one shape every test
/// here needs, because it is the only shape where the two walks disagree.
fn dirty_fixture(name: &str) -> Repo {
    let repo = Repo::build(name);
    repo.write("a.txt", "committed\n");
    repo.write("kept.txt", "unchanged\n");
    repo.commit("the base");
    repo.git(&["tag", "v1.0.0"]);
    repo.write("a.txt", "edited in the worktree\n");
    repo
}

fn env_of(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect()
}

fn text(row: &HostRow, column: &str) -> String {
    row.get(column)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// `{path: digest}` for one host name over one fixture.
fn listing(repo: &Repo, host: &str, inputs: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut env = env_of(inputs);
    env.insert("repo".to_string(), repo.path());
    SoopyFilesExecutor
        .run(host, "", &env)
        .unwrap_or_else(|error| panic!("{host} answers: {error:?}"))
        .iter()
        .map(|row| (text(row, "path"), text(row, "digest")))
        .collect()
}

/// THE DISTINCTION, as one assertion pair: the same path, two walks, two
/// content ids, and only the committed one is an object the database holds.
#[test]
fn files_at_walks_the_commit_and_files_the_edit() {
    let repo = dirty_fixture("split");
    let committed = repo.blob_at("HEAD", "a.txt");
    let edited = repo.worktree_oid("a.txt");
    assert_ne!(committed, edited, "the fixture must actually be dirty");

    let worktree = listing(&repo, "soopy__files", &[("glob", "*.txt")]);
    let at_head = listing(
        &repo,
        "soopy__files_at",
        &[("rev", "HEAD"), ("glob", "*.txt")],
    );

    assert_eq!(
        worktree.get("a.txt"),
        Some(&edited),
        "the worktree walk answers the edit"
    );
    assert_eq!(
        at_head.get("a.txt"),
        Some(&committed),
        "the commit walk answers the committed blob"
    );
    assert_eq!(
        worktree.get("kept.txt"),
        at_head.get("kept.txt"),
        "an unedited path is one blob under both walks"
    );
    assert!(
        repo.in_object_database(&committed),
        "a commit-walk digest is an object the database holds"
    );
    assert!(
        !repo.in_object_database(&edited),
        "a worktree-walk digest names bytes `hash-object` never wrote, which is \
         why the blob reader falls back to the file and re-hashes"
    );
}

/// `WORK` is the worktree's spelling on the wire, the same one
/// `change_facts::parse_revision` takes, so the marked host reaches the unmarked
/// host's answer when a program asks it to.
#[test]
fn files_at_work_is_the_worktree() {
    let repo = dirty_fixture("work");
    assert_eq!(
        listing(&repo, "soopy__files", &[("glob", "*.txt")]),
        listing(
            &repo,
            "soopy__files_at",
            &[("rev", "WORK"), ("glob", "*.txt")]
        ),
    );
}

/// The witness argument, made concrete: a rev-pinned answer must change when the
/// rev changes, or the host caches one listing for the life of the db.
#[test]
fn files_at_a_tag_does_not_see_the_tip() {
    let repo = dirty_fixture("tag");
    repo.write("later.txt", "after the tag\n");
    repo.commit("past the tag");

    let at_tag = listing(
        &repo,
        "soopy__files_at",
        &[("rev", "v1.0.0"), ("glob", "*.txt")],
    );
    let at_head = listing(
        &repo,
        "soopy__files_at",
        &[("rev", "HEAD"), ("glob", "*.txt")],
    );

    assert!(
        !at_tag.contains_key("later.txt"),
        "later.txt is not in v1.0.0"
    );
    assert!(at_head.contains_key("later.txt"), "later.txt is at the tip");
    assert_eq!(
        at_tag.get("a.txt"),
        Some(&repo.blob_at("v1.0.0", "a.txt")),
        "the tag's own blob, not the tip's"
    );
}

/// The revision is in the NAME (rulings.pl:544), so a name off the roster is a
/// stop. Falling through to the worktree is the defect, not the fallback.
#[test]
fn an_unlisted_file_host_stops() {
    let repo = dirty_fixture("unlisted");
    let failure = SoopyFilesExecutor
        .run(
            "soopy_files",
            "",
            &env_of(&[("repo", &repo.path()), ("glob", "*.txt")]),
        )
        .expect_err("an unlisted host name must not walk anything");
    assert!(
        failure.message.contains("/soopy/files and /soopy/files_at"),
        "the stop names the roster: {}",
        failure.message
    );
}

/// A pinned host with no rev is a stop, never a worktree answer.
#[test]
fn files_at_without_a_rev_stops() {
    let repo = dirty_fixture("norev");
    let failure = SoopyFilesExecutor
        .run(
            "soopy__files_at",
            "",
            &env_of(&[("repo", &repo.path()), ("glob", "*.txt")]),
        )
        .expect_err("files_at must not default its revision");
    assert!(
        failure.message.contains("rev"),
        "the stop names the missing column: {}",
        failure.message
    );
}

/// The differ side of the same distinction: HEAD against the worktree is one
/// modification, and HEAD against itself is nothing.
#[test]
fn the_differ_sees_the_uncommitted_edit_only_against_the_worktree() {
    let repo = dirty_fixture("differ");
    let differ = SoopyRevisionDiffer;

    let dirty = differ
        .diff(
            &repo.path(),
            &soopy::Revision::Named("HEAD".into()),
            &soopy::Revision::Worktree,
        )
        .expect("HEAD against the worktree");
    let touched: Vec<(ChangeKind, String)> = dirty
        .changes
        .iter()
        .map(|change| (change.kind, change.path.clone()))
        .collect();
    assert_eq!(touched, vec![(ChangeKind::Modified, "a.txt".to_string())]);

    let still = differ
        .diff(
            &repo.path(),
            &soopy::Revision::Named("HEAD".into()),
            &soopy::Revision::Named("HEAD".into()),
        )
        .expect("HEAD against HEAD");
    assert!(
        still.changes.is_empty() && still.renames.is_empty(),
        "a commit against itself changed nothing: {still:?}"
    );
}

/// The line plane rides the same split: the changed line comes from the DIRTY
/// bytes, which no `cat-file` could have produced.
#[test]
fn the_changed_line_comes_from_the_worktree_bytes() {
    let repo = dirty_fixture("lines");
    let dirty = SoopyRevisionDiffer
        .diff(
            &repo.path(),
            &soopy::Revision::Named("HEAD".into()),
            &soopy::Revision::Worktree,
        )
        .expect("HEAD against the worktree");
    let lines: Vec<(String, i64)> = dirty
        .changed_lines
        .iter()
        .map(|line| (line.path.clone(), line.line_number))
        .collect();
    assert_eq!(lines, vec![("a.txt".to_string(), 1)]);
}
