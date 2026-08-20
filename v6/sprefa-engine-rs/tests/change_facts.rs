//! CONTROL: 10 passed, 0 failed.
//!
//! SABOTAGE 1, drop the exact-content rename pass (return an empty Vec from
//! `take_renames`): `moved.txt` falls back into `deleted` and
//! `elsewhere.txt` into `created`, so every kind assertion moves and
//! `elsewhere.txt` starts contributing a changed line it should not have.
//!
//! SABOTAGE 2, drop the `is_binary` guard in `changed_lines_of`: the two
//! NUL-bearing blobs are diffed as text and `shot.bin` gains line rows in
//! every changed_line assertion.
//!
//! The `ChangeFactExecutor` memo and in-process-routing receipts this file
//! once carried (SABOTAGE 3/4, the diff-memo-key and WORK-not-memoised
//! guards) moved with the executor's deletion; PR #370 already made that
//! executor unreachable from either host door.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sprefa_engine_rs::change_facts::{
    parse_revision, ChangeKind, IRevisionDiffer, SoopyRevisionDiffer,
};

// ═══ the fixture ════════════════════════════════════════════════════════════

/// Three commits, and the middle pair carries all four change kinds at once:
///
/// ```text
///   A   keep.txt edit.txt gone.txt moved.txt shot.bin
///   B   keep.txt edit.txt(2 lines touched) fresh.txt elsewhere.txt shot.bin(binary)
///   C   keep.txt only -- the second pair, with a DIFFERENT answer
/// ```
struct Fixture {
    root: PathBuf,
}

const WHEN: i64 = 1_700_000_000;

/// A clock reading cannot separate two fixtures under parallel test threads
/// (docs/failure-modes.md, fixture-temp-dir-clock-collision).
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const EDIT_BASE: &str = "one\ntwo\nthree\nfour\n";
const EDIT_HEAD: &str = "one\nTWO\nthree\nfour\nfive\n";
const FRESH_HEAD: &str = "new one\nnew two\n";
const MOVED_BODY: &str = "identical content\n";
const BINARY_BASE: &[u8] = b"\x00\x01header\nbody\n";
const BINARY_HEAD: &[u8] = b"\x00\x01header\nbody changed\n";
const KEEP_DIRTY: &str = "alpha\nBETA\ngamma\n";
const ADDED_DIRTY: &str = "added one\nadded two\n";

impl Fixture {
    fn build() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sprefa_change_facts_{}_{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory");
        let fixture = Fixture { root };
        fixture.git(&["init", "-q"]);
        // `init.defaultBranch` is machine configuration, never inherited here.
        fixture.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);

        fixture.write("keep.txt", b"alpha\nbeta\ngamma\n");
        fixture.write("edit.txt", EDIT_BASE.as_bytes());
        fixture.write("gone.txt", b"removed\n");
        fixture.write("moved.txt", MOVED_BODY.as_bytes());
        fixture.write("shot.bin", BINARY_BASE);
        fixture.commit("base");
        fixture.git(&["tag", "at_base"]);

        std::fs::remove_file(fixture.root.join("gone.txt")).expect("remove gone.txt");
        std::fs::remove_file(fixture.root.join("moved.txt")).expect("remove moved.txt");
        fixture.write("elsewhere.txt", MOVED_BODY.as_bytes());
        fixture.write("edit.txt", EDIT_HEAD.as_bytes());
        fixture.write("fresh.txt", FRESH_HEAD.as_bytes());
        fixture.write("shot.bin", BINARY_HEAD);
        fixture.commit("head");
        fixture.git(&["tag", "at_head"]);

        for name in ["edit.txt", "fresh.txt", "elsewhere.txt", "shot.bin"] {
            std::fs::remove_file(fixture.root.join(name)).expect("remove for the third commit");
        }
        fixture.commit("pruned");
        fixture.git(&["tag", "at_pruned"]);
        fixture
    }

    fn git(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "sprefa-engine-rs")
            .env("GIT_AUTHOR_EMAIL", "sprefa-engine-rs@example.invalid")
            .env("GIT_COMMITTER_NAME", "sprefa-engine-rs")
            .env("GIT_COMMITTER_EMAIL", "sprefa-engine-rs@example.invalid")
            .env("GIT_AUTHOR_DATE", format!("{WHEN} +0000"))
            .env("GIT_COMMITTER_DATE", format!("{WHEN} +0000"))
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn write(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.root.join(name), bytes).expect("write fixture file");
    }

    /// `git ls-files` reads the index, so an uncommitted path is a tracked
    /// worktree path only once it is added.
    fn dirty(&self, name: &str, bytes: &[u8]) {
        self.write(name, bytes);
        self.git(&["add", name]);
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-qm", message]);
    }

    fn path(&self) -> String {
        self.root.display().to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// ═══ the differ ═════════════════════════════════════════════════════════════

fn kinds(fixture: &Fixture, kind: ChangeKind) -> Vec<String> {
    SoopyRevisionDiffer
        .diff(
            &fixture.path(),
            &parse_revision("at_base"),
            &parse_revision("at_head"),
        )
        .expect("the differ answers")
        .changes
        .into_iter()
        .filter(|change| change.kind == kind)
        .map(|change| change.path)
        .collect()
}

#[test]
fn a_new_path_is_created() {
    let fixture = Fixture::build();
    assert_eq!(kinds(&fixture, ChangeKind::Created), vec!["fresh.txt"]);
}

#[test]
fn a_removed_path_is_deleted() {
    let fixture = Fixture::build();
    assert_eq!(kinds(&fixture, ChangeKind::Deleted), vec!["gone.txt"]);
}

/// An unchanged path produces NO row, which is what makes the rel a diff rather
/// than a listing: `keep.txt` is tracked at both revisions and appears nowhere.
#[test]
fn a_changed_blob_is_modified_and_an_unchanged_one_is_absent() {
    let fixture = Fixture::build();
    assert_eq!(
        kinds(&fixture, ChangeKind::Modified),
        vec!["edit.txt", "shot.bin"]
    );
}

/// The four kinds PARTITION the diff: the rename's two paths are in `renames`
/// and in neither `created` nor `deleted`.
#[test]
fn a_rename_is_not_a_creation_plus_a_deletion() {
    let fixture = Fixture::build();
    let answer = SoopyRevisionDiffer
        .diff(
            &fixture.path(),
            &parse_revision("at_base"),
            &parse_revision("at_head"),
        )
        .expect("the differ answers");
    let renames: Vec<(String, String)> = answer
        .renames
        .iter()
        .map(|rename| (rename.path_from.clone(), rename.path_to.clone()))
        .collect();
    assert_eq!(
        renames,
        vec![("moved.txt".to_string(), "elsewhere.txt".to_string())]
    );
    let touched: Vec<&str> = answer
        .changes
        .iter()
        .map(|change| change.path.as_str())
        .collect();
    assert!(!touched.contains(&"moved.txt") && !touched.contains(&"elsewhere.txt"));
}

/// Head-side line numbers only: `edit.txt` line 2 changed and line 5 arrived,
/// `fresh.txt` is new so every line is its own, and the deleted path is silent.
#[test]
fn changed_line_names_the_head_side_lines() {
    let fixture = Fixture::build();
    let lines: Vec<(String, i64)> = SoopyRevisionDiffer
        .diff(
            &fixture.path(),
            &parse_revision("at_base"),
            &parse_revision("at_head"),
        )
        .expect("the differ answers")
        .changed_lines
        .into_iter()
        .map(|line| (line.path, line.line_number))
        .collect();
    assert_eq!(
        lines,
        vec![
            ("edit.txt".to_string(), 2),
            ("edit.txt".to_string(), 5),
            ("fresh.txt".to_string(), 1),
            ("fresh.txt".to_string(), 2),
        ]
    );
}

/// A binary blob is `modified` and contributes no line, which is the pair of
/// rows `git diff -U0` prints for one: a header and no hunk.
#[test]
fn a_binary_change_names_no_line() {
    let fixture = Fixture::build();
    let answer = SoopyRevisionDiffer
        .diff(
            &fixture.path(),
            &parse_revision("at_base"),
            &parse_revision("at_head"),
        )
        .expect("the differ answers");
    assert!(answer
        .changes
        .iter()
        .any(|change| change.path == "shot.bin" && change.kind == ChangeKind::Modified));
    assert!(!answer
        .changed_lines
        .iter()
        .any(|line| line.path == "shot.bin"));
}

/// The pair is ORDERED: swapping base and head turns every creation into a
/// deletion, so a projection that ignored the order could not pass both.
#[test]
fn the_pair_is_ordered() {
    let fixture = Fixture::build();
    let backwards = SoopyRevisionDiffer
        .diff(
            &fixture.path(),
            &parse_revision("at_head"),
            &parse_revision("at_base"),
        )
        .expect("the differ answers");
    let created: Vec<&str> = backwards
        .changes
        .iter()
        .filter(|change| change.kind == ChangeKind::Created)
        .map(|change| change.path.as_str())
        .collect();
    assert_eq!(created, vec!["gone.txt"]);
}

/// Equal revisions are an empty diff, never an error and never a full listing.
#[test]
fn a_revision_against_itself_answers_no_row() {
    let fixture = Fixture::build();
    let answer = SoopyRevisionDiffer
        .diff(
            &fixture.path(),
            &parse_revision("at_head"),
            &parse_revision("at_head"),
        )
        .expect("the differ answers");
    assert_eq!(answer.changes.len(), 0);
    assert_eq!(answer.renames.len(), 0);
    assert_eq!(answer.changed_lines.len(), 0);
}

/// `WORK` names the dirty checkout. Its oids are `git hash-object` answers that
/// were never written to the object database, so a passing line assertion is
/// also the receipt that the bytes came off disk.
#[test]
fn work_revision_diffs_the_dirty_worktree() {
    let fixture = Fixture::build();
    let head_sha = fixture.git(&["rev-parse", "HEAD"]);
    fixture.dirty("keep.txt", KEEP_DIRTY.as_bytes());
    fixture.dirty("added.txt", ADDED_DIRTY.as_bytes());
    let answer = SoopyRevisionDiffer
        .diff(
            &fixture.path(),
            &parse_revision(&head_sha),
            &parse_revision("WORK"),
        )
        .expect("the differ answers");
    let changes: Vec<(ChangeKind, &str)> = answer
        .changes
        .iter()
        .map(|change| (change.kind, change.path.as_str()))
        .collect();
    assert_eq!(
        changes,
        vec![
            (ChangeKind::Created, "added.txt"),
            (ChangeKind::Modified, "keep.txt"),
        ]
    );
    let lines: Vec<(&str, i64)> = answer
        .changed_lines
        .iter()
        .map(|line| (line.path.as_str(), line.line_number))
        .collect();
    assert_eq!(
        lines,
        vec![("added.txt", 1), ("added.txt", 2), ("keep.txt", 2)]
    );
}

#[test]
fn parse_revision_maps_work_and_names() {
    assert_eq!(parse_revision("WORK"), soopy::Revision::Worktree);
    assert_eq!(
        parse_revision("main"),
        soopy::Revision::Named(std::sync::Arc::from("main"))
    );
}
