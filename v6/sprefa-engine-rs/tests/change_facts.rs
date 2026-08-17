//! CONTROL: 14 passed, 0 failed.
//!
//! SABOTAGE 1, drop the exact-content rename pass (return an empty Vec from
//! `take_renames`): 5 passed, 9 failed. `moved.txt` falls back into `deleted`
//! and `elsewhere.txt` into `created`, so every kind assertion moves and
//! `elsewhere.txt` starts contributing a changed line it should not have.
//!
//! SABOTAGE 2, drop the `is_binary` guard in `changed_lines_of`: 10 passed,
//! 4 failed. The two NUL-bearing blobs are diffed as text and `shot.bin` gains
//! line rows in every changed_line assertion.
//!
//! SABOTAGE 3, key the diff memo on `repo` alone: 13 passed, 1 failed. Only
//! `the_diff_memo_keys_on_the_whole_triple` can see it, which is the test that
//! exists for it.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use sprefa_engine_rs::change_facts::{ChangeKind, IRevisionDiffer, SoopyRevisionDiffer};
use sprefa_engine_rs::hosts::{decode_output, ChangeFactExecutor, HostLiveRunner, IHostExecutor};
use sprefa_engine_rs::types::{HostColumnPlan, HostPlanData, RelDelta, TickDeltas, Value};

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
        .diff(&fixture.path(), "at_base", "at_head")
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
        .diff(&fixture.path(), "at_base", "at_head")
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
        .diff(&fixture.path(), "at_base", "at_head")
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
        .diff(&fixture.path(), "at_base", "at_head")
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
        .diff(&fixture.path(), "at_head", "at_base")
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
        .diff(&fixture.path(), "at_head", "at_head")
        .expect("the differ answers");
    assert_eq!(answer.changes.len(), 0);
    assert_eq!(answer.renames.len(), 0);
    assert_eq!(answer.changed_lines.len(), 0);
}

// ═══ the executor ═══════════════════════════════════════════════════════════

fn columns(names: &[(&str, &str)]) -> Vec<HostColumnPlan> {
    names
        .iter()
        .map(|(name, column_type)| HostColumnPlan {
            name: (*name).to_string(),
            column_type: (*column_type).to_string(),
        })
        .collect()
}

const CHANGE_OUTPUTS: &[(&str, &str)] = &[("change_kind", "text"), ("path", "text")];
const RENAME_OUTPUTS: &[(&str, &str)] = &[("path_from", "text"), ("path_to", "text")];
const CHANGED_LINE_OUTPUTS: &[(&str, &str)] = &[("path", "text"), ("line_number", "int")];
const PAIR_INPUT: &[(&str, &str)] = &[("repo", "text"), ("rev_base", "text"), ("rev_head", "text")];

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn pair_inputs(repo: &str, rev_base: &str, rev_head: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("repo".to_string(), repo.to_string()),
        ("rev_base".to_string(), rev_base.to_string()),
        ("rev_head".to_string(), rev_head.to_string()),
    ])
}

fn answer(
    executor: &dyn IHostExecutor,
    host: &str,
    inputs: &BTreeMap<String, String>,
    outputs: &[(&str, &str)],
) -> Vec<Vec<Value>> {
    let stdout = executor
        .run(host, "deliberately not a shell command", inputs)
        .expect("the linked arm answers");
    decode_output(host, &stdout, &columns(outputs)).expect("decode")
}

#[test]
fn the_change_host_names_its_kind_in_the_row() {
    let fixture = Fixture::build();
    let rows = answer(
        &ChangeFactExecutor::default(),
        "git_change",
        &pair_inputs(&fixture.path(), "at_base", "at_head"),
        CHANGE_OUTPUTS,
    );
    assert_eq!(
        rows,
        vec![
            vec![text("created"), text("fresh.txt")],
            vec![text("deleted"), text("gone.txt")],
            vec![text("modified"), text("edit.txt")],
            vec![text("modified"), text("shot.bin")],
        ]
    );
}

#[test]
fn the_changed_line_host_answers_integers() {
    let fixture = Fixture::build();
    let rows = answer(
        &ChangeFactExecutor::default(),
        "git_changed_line",
        &pair_inputs(&fixture.path(), "at_base", "at_head"),
        CHANGED_LINE_OUTPUTS,
    );
    assert_eq!(
        rows,
        vec![
            vec![text("edit.txt"), Value::Integer(2)],
            vec![text("edit.txt"), Value::Integer(5)],
            vec![text("fresh.txt"), Value::Integer(1)],
            vec![text("fresh.txt"), Value::Integer(2)],
        ]
    );
}

/// Three host names, one memo entry, and the key is the whole triple: a second
/// pair in the same repository must not read the first pair's answer.
#[test]
fn the_diff_memo_keys_on_the_whole_triple() {
    let fixture = Fixture::build();
    let executor = ChangeFactExecutor::default();
    let repo = fixture.path();
    let first = answer(
        &executor,
        "git_change",
        &pair_inputs(&repo, "at_base", "at_head"),
        CHANGE_OUTPUTS,
    );
    assert_eq!(first.len(), 4);
    let second = answer(
        &executor,
        "git_change",
        &pair_inputs(&repo, "at_head", "at_pruned"),
        CHANGE_OUTPUTS,
    );
    assert_eq!(
        second,
        vec![
            vec![text("deleted"), text("edit.txt")],
            vec![text("deleted"), text("elsewhere.txt")],
            vec![text("deleted"), text("fresh.txt")],
            vec![text("deleted"), text("shot.bin")],
        ]
    );
}

/// A spelling that resolves to nothing stops by name. Zero rows would read as
/// "these two trees are identical".
#[test]
fn an_unresolvable_revision_is_a_named_stop() {
    let fixture = Fixture::build();
    let failure = ChangeFactExecutor::default()
        .run(
            "git_change",
            "deliberately not a shell command",
            &pair_inputs(&fixture.path(), "at_base", "refs/heads/not-a-branch"),
        )
        .expect_err("an absent revision stops");
    assert_eq!(failure.host, "git_change");
    assert!(
        failure.message.contains("refs/heads/not-a-branch"),
        "{}",
        failure.message
    );
}

#[test]
fn a_missing_host_input_is_a_named_stop() {
    let failure = ChangeFactExecutor::default()
        .run(
            "git_changed_line",
            "deliberately not a shell command",
            &BTreeMap::from([("repo".to_string(), "/does/not/matter".to_string())]),
        )
        .expect_err("rev_base is required");
    assert!(
        failure.message.contains("`rev_base`"),
        "{}",
        failure.message
    );
}

// ═══ the arm itself ═════════════════════════════════════════════════════════

/// Every template exits 3, so a row reaching a response rel proves
/// `HostLiveRunner` routed the name in-process rather than to `ShellExecutor`.
fn change_plan(name: &str, outputs: &[(&str, &str)]) -> HostPlanData {
    HostPlanData {
        name: name.to_string(),
        inputs: columns(PAIR_INPUT),
        outputs: columns(outputs),
        template: format!("echo '{name} is linked in-process' >&2; exit 3"),
        demand_rel: format!("__host_demand_{name}"),
        response_rel: format!("__host_response_{name}"),
        execution: "shell".to_string(),
        request_type: None,
        response_type: None,
    }
}

#[test]
fn the_three_ruled_names_reach_the_linked_arm_through_the_host_plan() {
    let fixture = Fixture::build();
    let repo = fixture.path();
    let plans = vec![
        change_plan("git_change", CHANGE_OUTPUTS),
        change_plan("git_rename", RENAME_OUTPUTS),
        change_plan("git_changed_line", CHANGED_LINE_OUTPUTS),
    ];
    let mut rel_columns: HashMap<String, Vec<String>> = HashMap::new();
    let mut rels = Vec::new();
    for plan in &plans {
        let demand: Vec<String> = ["identity_digest", "witness_digest"]
            .iter()
            .map(|name| (*name).to_string())
            .chain(plan.inputs.iter().map(|input| input.name.clone()))
            .collect();
        let response: Vec<String> = ["witness_digest", "ordinal"]
            .iter()
            .map(|name| (*name).to_string())
            .chain(plan.inputs.iter().map(|input| input.name.clone()))
            .chain(plan.outputs.iter().map(|output| output.name.clone()))
            .collect();
        rel_columns.insert(plan.demand_rel.clone(), demand);
        rel_columns.insert(plan.response_rel.clone(), response);
        rels.push(RelDelta {
            rel: plan.demand_rel.clone(),
            add: vec![vec![
                text(&format!("identity|{}", plan.name)),
                text(&format!("witness|{}", plan.name)),
                text(&repo),
                text("at_base"),
                text("at_head"),
            ]],
            del: vec![],
        });
    }
    let deltas = TickDeltas {
        rels,
        carry_pending: false,
    };
    let mut runner = HostLiveRunner::new(&plans, &rel_columns).expect("every plan has an executor");
    let arrivals = runner.collect(&deltas).expect("the linked arm answers");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for arrival in &arrivals {
        *counts.entry(arrival.rel.as_str()).or_default() += 1;
    }
    assert_eq!(counts.get("__host_response_git_change").copied(), Some(4));
    assert_eq!(counts.get("__host_response_git_rename").copied(), Some(1));
    assert_eq!(
        counts.get("__host_response_git_changed_line").copied(),
        Some(4)
    );

    // The response rel carries the demand's own input columns back, so a rule
    // joining on `rev_head` reads the revision it asked about.
    let rename = arrivals
        .iter()
        .find(|arrival| arrival.rel == "__host_response_git_rename")
        .expect("the rename response arrives");
    assert_eq!(
        rename.row,
        vec![
            text("witness|git_rename"),
            Value::Integer(0),
            text(&repo),
            text("at_base"),
            text("at_head"),
            text("moved.txt"),
            text("elsewhere.txt"),
        ]
    );
}
