//! CONTROL: 17 passed, 0 failed.
//!
//! FAIL-PRE-FIX: `the_ref_memo_sees_a_moved_ref` against the pre-witness memo,
//! 16 passed, 1 failed. The rest of the file was written against
//! `hosts::{GitRefExecutor, GitRevisionExecutor}` before either existed --
//! `error[E0432]: unresolved import`, rc=101.
//!
//! SABOTAGE 1, drop the `peeled` fallback in `ref_target` and read `direct`:
//! the three ref/tag row assertions go RED (14 passed, 3 failed); every
//! revision-graph test stays green, because only annotated tags move.
//!
//! SABOTAGE 2, ask ancestry in one direction only: `the_ancestor_host_answers_
//! both_directions` alone goes RED, zero rows where one is expected
//! (16 passed, 1 failed).
//!
//! SABOTAGE 3, key the revision memo on `repo` alone: the second pair in
//! `the_revision_memo_keys_on_the_whole_triple` reads the first pair's counts
//! (16 passed, 1 failed).
//!
//! SABOTAGE 4, always serve the ref memo (`if true` in place of the witness
//! comparison in `GitRefExecutor::snapshot`): 16 passed, 1 failed. Only
//! `the_ref_memo_sees_a_moved_ref` can see it, which is the same single test
//! that fails on the pre-fix tree. Four guards, four discriminating tests.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sprefa_engine_rs::hosts::{
    decode_output, GitRefExecutor, GitRevisionExecutor, HostLiveRunner, IHostExecutor,
};
use sprefa_engine_rs::types::{HostColumnPlan, HostPlanData, RelDelta, TickDeltas, Value};

// ═══ the fixture ════════════════════════════════════════════════════════════

/// A one-repository history with a fork, both tag kinds, and a known divergence:
///
/// ```text
///   A ── B ── C   refs/heads/main, refs/tags/v1.0.0 (annotated, at C)
///   └─── D        refs/heads/feature
///   ^ refs/tags/v0.1.0 (lightweight, at A)
/// ```
///
/// main is 2 ahead / 1 behind feature, their merge base is A, and A is an
/// ancestor of both tips. Nothing here reaches the network.
struct Fixture {
    root: PathBuf,
}

/// The tagger date every annotated tag in this fixture carries, so `tagged_at`
/// is a fixed number rather than the wall clock.
const TAG_WHEN: i64 = 1_700_000_000;

/// A clock reading cannot separate two fixtures: `SystemTime::now()` on macOS
/// repeats across parallel test threads, and two `git init` runs in one
/// directory race (docs/failure-modes.md, fixture-temp-dir-clock-collision).
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl Fixture {
    fn build() -> Self {
        let root = std::env::temp_dir().join(format!(
            "sprefa_git_refs_{}_{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        // The name carries this process's own pid and sequence, so anything
        // already at it is a leftover from a crashed run.
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture directory");
        let fixture = Fixture { root };
        fixture.git(&["init", "-q"]);
        // `init.defaultBranch` is machine configuration, so the branch this
        // fixture asserts on is named rather than inherited.
        fixture.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        fixture.commit("a.txt", "base");
        fixture.git(&["tag", "v0.1.0"]);
        fixture.git(&["branch", "feature"]);
        fixture.commit("b.txt", "second");
        fixture.commit("c.txt", "third");
        fixture.git(&["tag", "-a", "v1.0.0", "-m", "the first release"]);
        fixture.git(&["checkout", "-q", "feature"]);
        fixture.commit("d.txt", "diverged");
        fixture.git(&["checkout", "-q", "main"]);
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
            .env("GIT_AUTHOR_DATE", format!("{TAG_WHEN} +0000"))
            .env("GIT_COMMITTER_DATE", format!("{TAG_WHEN} +0000"))
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit(&self, name: &str, body: &str) {
        std::fs::write(self.root.join(name), body).expect("write fixture file");
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", body]);
    }

    fn sha(&self, revision: &str) -> String {
        self.git(&["rev-parse", revision])
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

// ═══ plan shapes ════════════════════════════════════════════════════════════

fn columns(names: &[(&str, &str)]) -> Vec<HostColumnPlan> {
    names
        .iter()
        .map(|(name, column_type)| HostColumnPlan {
            name: (*name).to_string(),
            column_type: (*column_type).to_string(),
        })
        .collect()
}

const REF_OUTPUTS: &[(&str, &str)] = &[
    ("ref_name", "text"),
    ("kind", "text"),
    ("target_sha", "text"),
];
const TAG_OUTPUTS: &[(&str, &str)] = &[
    ("tag_name", "text"),
    ("target_sha", "text"),
    ("tagged_at", "int"),
    ("annotated", "bool"),
];
const MERGE_BASE_OUTPUTS: &[(&str, &str)] = &[("base_sha", "text")];
const AHEAD_BEHIND_OUTPUTS: &[(&str, &str)] = &[("ahead_count", "int"), ("behind_count", "int")];
const ANCESTOR_OUTPUTS: &[(&str, &str)] = &[("ancestor_sha", "text"), ("descendant_sha", "text")];

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn repo_inputs(repo: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("repo".to_string(), repo.to_string())])
}

fn pair_inputs(repo: &str, rev_a: &str, rev_b: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("repo".to_string(), repo.to_string()),
        ("rev_a".to_string(), rev_a.to_string()),
        ("rev_b".to_string(), rev_b.to_string()),
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

// ═══ the ref snapshot ═══════════════════════════════════════════════════════

#[test]
fn the_ref_host_names_branches_tags_and_head() {
    let fixture = Fixture::build();
    let rows = answer(
        &GitRefExecutor::default(),
        "git_ref",
        &repo_inputs(&fixture.path()),
        REF_OUTPUTS,
    );
    let (main, feature) = (fixture.sha("main"), fixture.sha("feature"));
    let base = fixture.sha("v0.1.0");
    assert_eq!(
        rows,
        vec![
            vec![text("refs/heads/feature"), text("branch"), text(&feature)],
            vec![text("refs/heads/main"), text("branch"), text(&main)],
            vec![text("refs/tags/v0.1.0"), text("tag"), text(&base)],
            vec![text("refs/tags/v1.0.0"), text("tag"), text(&main)],
            vec![text("HEAD"), text("head"), text(&main)],
        ]
    );
}

/// `refs/tags/v1.0.0` names a tag OBJECT, and its own OID is not a commit. A
/// ref row that reported it would not join against any commit-keyed relation.
#[test]
fn the_ref_host_peels_an_annotated_tag() {
    let fixture = Fixture::build();
    let tag_object = fixture.git(&["rev-parse", "refs/tags/v1.0.0"]);
    let commit = fixture.sha("refs/tags/v1.0.0^{commit}");
    assert_ne!(tag_object, commit, "the fixture tag must be annotated");

    let rows = answer(
        &GitRefExecutor::default(),
        "git_ref",
        &repo_inputs(&fixture.path()),
        REF_OUTPUTS,
    );
    let annotated = rows
        .iter()
        .find(|row| row[0] == text("refs/tags/v1.0.0"))
        .expect("the annotated tag has a row");
    assert_eq!(annotated[2], text(&commit));
}

#[test]
fn the_tag_host_separates_annotated_from_lightweight() {
    let fixture = Fixture::build();
    let rows = answer(
        &GitRefExecutor::default(),
        "git_tag",
        &repo_inputs(&fixture.path()),
        TAG_OUTPUTS,
    );
    assert_eq!(
        rows,
        vec![
            vec![
                text("v0.1.0"),
                text(&fixture.sha("v0.1.0")),
                Value::Integer(0),
                Value::Bool(false),
            ],
            vec![
                text("v1.0.0"),
                text(&fixture.sha("main")),
                Value::Integer(TAG_WHEN),
                Value::Bool(true),
            ],
        ]
    );
}

/// A quiet ref store answers from the memo: the four host names still settle
/// ONE `for-each-ref` pass between them.
#[test]
fn the_ref_memo_settles_one_snapshot_per_quiet_repository() {
    let fixture = Fixture::build();
    let executor = GitRefExecutor::default();
    let inputs = repo_inputs(&fixture.path());
    let first = answer(&executor, "git_ref", &inputs, REF_OUTPUTS);
    let second = answer(&executor, "git_ref", &inputs, REF_OUTPUTS);
    assert_eq!(first, second);
    let tags = answer(&executor, "git_tag", &inputs, TAG_OUTPUTS);
    assert_eq!(tags.len(), 2, "both tags come off the same snapshot");
}

/// SEMANTICS CHANGE. This test replaces the assertion that a ref added after
/// the first demand does NOT appear. Refs move and the memo has to follow.
#[test]
fn the_ref_memo_sees_a_moved_ref() {
    let fixture = Fixture::build();
    let executor = GitRefExecutor::default();
    let inputs = repo_inputs(&fixture.path());
    let before = answer(&executor, "git_ref", &inputs, REF_OUTPUTS);
    fixture.git(&["branch", "later"]);
    let after = answer(&executor, "git_ref", &inputs, REF_OUTPUTS);
    assert_eq!(after.len(), before.len() + 1);
    assert!(
        after
            .iter()
            .any(|row| row.first() == Some(&text("refs/heads/later"))),
        "the memo never saw the new branch: {after:?}"
    );
}

/// `pack-refs` rewrites `packed-refs` and DELETES the loose ref files, so a
/// witness that watched only loose refs would read the store as emptied.
#[test]
fn the_ref_memo_survives_a_pack_refs() {
    let fixture = Fixture::build();
    let executor = GitRefExecutor::default();
    let inputs = repo_inputs(&fixture.path());
    let before = answer(&executor, "git_ref", &inputs, REF_OUTPUTS);
    fixture.git(&["pack-refs", "--all"]);
    let after = answer(&executor, "git_ref", &inputs, REF_OUTPUTS);
    assert_eq!(before, after, "packing refs moved the rows");
}

// ═══ the revision graph ═════════════════════════════════════════════════════

#[test]
fn the_merge_base_host_answers_the_fork_point() {
    let fixture = Fixture::build();
    let rows = answer(
        &GitRevisionExecutor::default(),
        "git_merge_base",
        &pair_inputs(&fixture.path(), "main", "feature"),
        MERGE_BASE_OUTPUTS,
    );
    assert_eq!(rows, vec![vec![text(&fixture.sha("v0.1.0"))]]);
}

#[test]
fn the_ahead_behind_host_counts_both_sides() {
    let fixture = Fixture::build();
    let rows = answer(
        &GitRevisionExecutor::default(),
        "git_ahead_behind",
        &pair_inputs(&fixture.path(), "main", "feature"),
        AHEAD_BEHIND_OUTPUTS,
    );
    assert_eq!(rows, vec![vec![Value::Integer(2), Value::Integer(1)]]);
}

/// Two diverged tips are ancestors of neither direction, and that is zero rows
/// rather than a row carrying a false.
#[test]
fn diverged_tips_produce_no_ancestor_row() {
    let fixture = Fixture::build();
    let rows = answer(
        &GitRevisionExecutor::default(),
        "git_ancestor",
        &pair_inputs(&fixture.path(), "main", "feature"),
        ANCESTOR_OUTPUTS,
    );
    assert!(rows.is_empty(), "{rows:?}");
}

/// The host answers the RELATION, so the ancestor may arrive as either input.
#[test]
fn the_ancestor_host_answers_both_directions() {
    let fixture = Fixture::build();
    let (base, tip) = (fixture.sha("v0.1.0"), fixture.sha("main"));
    let forward = answer(
        &GitRevisionExecutor::default(),
        "git_ancestor",
        &pair_inputs(&fixture.path(), "v0.1.0", "main"),
        ANCESTOR_OUTPUTS,
    );
    assert_eq!(forward, vec![vec![text(&base), text(&tip)]]);

    let reverse = answer(
        &GitRevisionExecutor::default(),
        "git_ancestor",
        &pair_inputs(&fixture.path(), "main", "v0.1.0"),
        ANCESTOR_OUTPUTS,
    );
    assert_eq!(reverse, vec![vec![text(&base), text(&tip)]]);
}

/// One revision against itself is an ancestor of itself, asked ONCE: the memo
/// drops the mirrored question rather than emitting the row twice.
#[test]
fn one_revision_against_itself_answers_a_single_row() {
    let fixture = Fixture::build();
    let tip = fixture.sha("main");
    let rows = answer(
        &GitRevisionExecutor::default(),
        "git_ancestor",
        &pair_inputs(&fixture.path(), "main", "HEAD"),
        ANCESTOR_OUTPUTS,
    );
    assert_eq!(rows, vec![vec![text(&tip), text(&tip)]]);
}

/// Three host names, one memo entry, and the key is the whole triple: a second
/// pair in the same repository must not read the first pair's answer.
#[test]
fn the_revision_memo_keys_on_the_whole_triple() {
    let fixture = Fixture::build();
    let executor = GitRevisionExecutor::default();
    let repo = fixture.path();
    let diverged = answer(
        &executor,
        "git_ahead_behind",
        &pair_inputs(&repo, "main", "feature"),
        AHEAD_BEHIND_OUTPUTS,
    );
    assert_eq!(diverged, vec![vec![Value::Integer(2), Value::Integer(1)]]);
    let linear = answer(
        &executor,
        "git_ahead_behind",
        &pair_inputs(&repo, "main", "v0.1.0"),
        AHEAD_BEHIND_OUTPUTS,
    );
    assert_eq!(linear, vec![vec![Value::Integer(2), Value::Integer(0)]]);
}

/// A spelling that resolves to nothing stops by name. Zero rows would read as
/// "these two revisions share no history".
#[test]
fn an_unresolvable_revision_is_a_named_stop() {
    let fixture = Fixture::build();
    let failure = GitRevisionExecutor::default()
        .run(
            "git_merge_base",
            "deliberately not a shell command",
            &pair_inputs(&fixture.path(), "main", "refs/heads/not-a-branch"),
        )
        .expect_err("an absent revision stops");
    assert_eq!(failure.host, "git_merge_base");
    assert!(
        failure.message.contains("refs/heads/not-a-branch")
            && failure.message.contains("does not resolve"),
        "{}",
        failure.message
    );
}

#[test]
fn a_missing_host_input_is_a_named_stop() {
    let failure = GitRevisionExecutor::default()
        .run(
            "git_ancestor",
            "deliberately not a shell command",
            &repo_inputs("/does/not/matter"),
        )
        .expect_err("rev_a is required");
    assert!(failure.message.contains("`rev_a`"), "{}", failure.message);
}

// ═══ the arm itself ═════════════════════════════════════════════════════════

/// The plan the emitter writes for one `sh` decl. Every template exits 3, so a
/// row reaching a response rel proves `HostLiveRunner` routed the name
/// in-process rather than to `ShellExecutor`.
fn git_plan(name: &str, inputs: &[(&str, &str)], outputs: &[(&str, &str)]) -> HostPlanData {
    HostPlanData {
        name: name.to_string(),
        inputs: columns(inputs),
        outputs: columns(outputs),
        template: format!("echo '{name} is linked in-process' >&2; exit 3"),
        demand_rel: format!("__host_demand_{name}"),
        response_rel: format!("__host_response_{name}"),
        execution: "shell".to_string(),
        request_type: None,
        response_type: None,
    }
}

const REPO_INPUT: &[(&str, &str)] = &[("repo", "text")];
const PAIR_INPUT: &[(&str, &str)] = &[("repo", "text"), ("rev_a", "text"), ("rev_b", "text")];

fn wire(
    plans: &[HostPlanData],
    demand_row: &dyn Fn(&HostPlanData) -> Vec<Value>,
) -> (HashMap<String, Vec<String>>, TickDeltas) {
    let mut rel_columns: HashMap<String, Vec<String>> = HashMap::new();
    let mut rels = Vec::new();
    for plan in plans {
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
            add: vec![demand_row(plan)],
            del: vec![],
        });
    }
    (
        rel_columns,
        TickDeltas {
            rels,
            carry_pending: false,
        },
    )
}

fn response_counts(arrivals: &[sprefa_engine_rs::types::Arrival]) -> BTreeMap<&str, usize> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for arrival in arrivals {
        *counts.entry(arrival.rel.as_str()).or_default() += 1;
    }
    counts
}

#[test]
fn the_five_ruled_names_reach_the_linked_arm_through_the_host_plan() {
    let fixture = Fixture::build();
    let repo = fixture.path();
    let plans = vec![
        git_plan("git_ref", REPO_INPUT, REF_OUTPUTS),
        git_plan("git_tag", REPO_INPUT, TAG_OUTPUTS),
        git_plan("git_merge_base", PAIR_INPUT, MERGE_BASE_OUTPUTS),
        git_plan("git_ahead_behind", PAIR_INPUT, AHEAD_BEHIND_OUTPUTS),
        git_plan("git_ancestor", PAIR_INPUT, ANCESTOR_OUTPUTS),
    ];
    let (rel_columns, deltas) = wire(&plans, &|plan| {
        let mut row = vec![
            text(&format!("identity|{}", plan.name)),
            text(&format!("witness|{}", plan.name)),
            text(&repo),
        ];
        if plan.inputs.len() == 3 {
            row.push(text("v0.1.0"));
            row.push(text("main"));
        }
        row
    });

    let mut runner = HostLiveRunner::new(&plans, &rel_columns).expect("every plan has an executor");
    let arrivals = runner.collect(&deltas).expect("the linked arm answers");
    let counts = response_counts(&arrivals);
    assert_eq!(counts.get("__host_response_git_ref").copied(), Some(5));
    assert_eq!(counts.get("__host_response_git_tag").copied(), Some(2));
    assert_eq!(
        counts.get("__host_response_git_merge_base").copied(),
        Some(1)
    );
    assert_eq!(
        counts.get("__host_response_git_ahead_behind").copied(),
        Some(1)
    );
    assert_eq!(counts.get("__host_response_git_ancestor").copied(), Some(1));
}

/// The response rel carries the demand's own input columns back, so a program
/// joining on `repo` reads the repository it asked about.
#[test]
fn the_response_rows_carry_the_demanded_inputs() {
    let fixture = Fixture::build();
    let repo = fixture.path();
    let plans = vec![git_plan(
        "git_ahead_behind",
        PAIR_INPUT,
        AHEAD_BEHIND_OUTPUTS,
    )];
    let (rel_columns, deltas) = wire(&plans, &|_| {
        vec![
            text("identity|git_ahead_behind"),
            text("witness|git_ahead_behind"),
            text(&repo),
            text("main"),
            text("feature"),
        ]
    });
    let mut runner = HostLiveRunner::new(&plans, &rel_columns).expect("every plan has an executor");
    let arrivals = runner.collect(&deltas).expect("the linked arm answers");
    assert_eq!(arrivals.len(), 1);
    assert_eq!(
        arrivals[0].row,
        vec![
            text("witness|git_ahead_behind"),
            Value::Integer(0),
            text(&repo),
            text("main"),
            text("feature"),
            Value::Integer(2),
            Value::Integer(1),
        ]
    );
}

/// A repository path that is not a checkout stops by name rather than
/// answering zero refs.
#[test]
fn a_path_outside_a_repository_is_a_named_stop() {
    let root = std::env::temp_dir().join(format!(
        "sprefa_git_refs_bare_{}_{}",
        std::process::id(),
        FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("fixture directory");
    let failure = GitRefExecutor::default()
        .run(
            "git_ref",
            "deliberately not a shell command",
            &repo_inputs(&root.display().to_string()),
        )
        .expect_err("a non-repository stops");
    assert!(
        failure.message.contains("open repository"),
        "{}",
        failure.message
    );
    let _ = std::fs::remove_dir_all::<&Path>(root.as_ref());
}
