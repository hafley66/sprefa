use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Arc;

use sprefa_engine_rs::hosts::{HostLiveRunner, IHostExecutor, SoopyMutationExecutor};
use sprefa_engine_rs::types::{HostColumnPlan, HostPlanData, RelDelta, TickDeltas, Value};
use tempfile::TempDir;

fn text(value: impl AsRef<str>) -> String {
    value.as_ref().to_string()
}

fn run(host: &str, values: &[(&str, String)]) -> serde_json::Value {
    let env = values
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    serde_json::from_str(
        &SoopyMutationExecutor
            .run(host, "unused", &env)
            .expect("source mutation host response"),
    )
    .expect("source mutation response JSON")
}

fn directory_id(root: &std::path::Path) -> soopy::SourceRootId {
    match soopy::SourceRoot::open_directory(root).expect("open directory root") {
        soopy::SourceRoot::Directory(directory) => soopy::SourceRootId::Directory {
            directory: directory.identity,
        },
        soopy::SourceRoot::GitWorktree(_) => unreachable!("open_directory stays filesystem-only"),
    }
}

fn git_worktree_id(root: &std::path::Path) -> soopy::SourceRootId {
    match soopy::SourceRoot::discover_git(root).expect("discover Git worktree") {
        soopy::SourceRoot::GitWorktree(git) => soopy::SourceRootId::GitWorktree {
            repository: git.repository.identity,
            worktree: git.repository.worktree,
        },
        soopy::SourceRoot::Directory(_) => unreachable!("discover_git returns a Git root"),
    }
}

fn stage_request(root: soopy::SourceRootId, path: soopy::SourcePath) -> String {
    serde_json::to_string(&soopy::StageRequest::new(
        root,
        vec![soopy::SourceAction::Create {
            path,
            bytes: b"created by source_stage\n".to_vec(),
        }],
    ))
    .expect("stage request JSON")
}

fn stage_directory(root: &TempDir, state: &TempDir) -> serde_json::Value {
    let request = stage_request(
        directory_id(root.path()),
        soopy::SourcePath::Directory {
            path: soopy::RootPath(Arc::from("created.txt")),
        },
    );
    run(
        "source_stage",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("request", request),
        ],
    )
}

fn mutation_plan(
    name: &str,
    inputs: &[&str],
    outputs: &[(&str, &str)],
    demand_rel: &str,
    response_rel: &str,
) -> HostPlanData {
    HostPlanData {
        name: name.to_string(),
        inputs: inputs
            .iter()
            .map(|name| HostColumnPlan {
                name: (*name).to_string(),
                column_type: "text".to_string(),
            })
            .collect(),
        outputs: outputs
            .iter()
            .map(|(name, column_type)| HostColumnPlan {
                name: (*name).to_string(),
                column_type: (*column_type).to_string(),
            })
            .collect(),
        template: "unused".to_string(),
        demand_rel: demand_rel.to_string(),
        response_rel: response_rel.to_string(),
        execution: "soopy_mutation".to_string(),
    }
}

#[test]
fn stage_and_commit_responses_reenter_the_generic_host_runner_on_later_ticks() {
    let root = TempDir::new().expect("target root");
    let state = TempDir::new().expect("state root");
    let request = stage_request(
        directory_id(root.path()),
        soopy::SourcePath::Directory {
            path: soopy::RootPath(Arc::from("from-runner.txt")),
        },
    );
    let stage_plan = mutation_plan(
        "source_stage",
        &["root", "state", "request"],
        &[
            ("stage_id", "text"),
            ("outcome", "text"),
            ("detail", "text"),
            ("document", "json"),
        ],
        "stage_demand",
        "stage_response",
    );
    let mut columns = std::collections::HashMap::from([
        (
            "stage_demand".to_string(),
            vec![
                "identity_digest".to_string(),
                "witness_digest".to_string(),
                "root".to_string(),
                "state".to_string(),
                "request".to_string(),
            ],
        ),
        (
            "stage_response".to_string(),
            vec![
                "witness_digest".to_string(),
                "ordinal".to_string(),
                "root".to_string(),
                "state".to_string(),
                "request".to_string(),
                "stage_id".to_string(),
                "outcome".to_string(),
                "detail".to_string(),
                "document".to_string(),
            ],
        ),
    ]);
    let stage_plans = [stage_plan];
    let mut stage_runner = HostLiveRunner::new(&stage_plans, &columns).expect("stage runner");
    let staged = stage_runner
        .collect(&TickDeltas {
            rels: vec![RelDelta {
                rel: "stage_demand".to_string(),
                add: vec![vec![
                    Value::Text("stage-identity".to_string()),
                    Value::Text("stage-witness".to_string()),
                    Value::Text(root.path().display().to_string()),
                    Value::Text(state.path().display().to_string()),
                    Value::Text(request),
                ]],
                del: vec![],
            }],
            carry_pending: false,
        })
        .expect("stage response tick");
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].row[6], Value::Text("staged".to_string()));
    let stage_id = match &staged[0].row[5] {
        Value::Text(stage_id) => stage_id.clone(),
        value => panic!("stage id was not text: {value:?}"),
    };

    let commit_plan = mutation_plan(
        "source_commit",
        &["root", "state", "stage_id"],
        &[
            ("outcome", "text"),
            ("detail", "text"),
            ("document", "json"),
        ],
        "commit_demand",
        "commit_response",
    );
    columns.insert(
        "commit_demand".to_string(),
        vec![
            "identity_digest".to_string(),
            "witness_digest".to_string(),
            "root".to_string(),
            "state".to_string(),
            "stage_id".to_string(),
        ],
    );
    columns.insert(
        "commit_response".to_string(),
        vec![
            "witness_digest".to_string(),
            "ordinal".to_string(),
            "root".to_string(),
            "state".to_string(),
            "stage_id".to_string(),
            "outcome".to_string(),
            "detail".to_string(),
            "document".to_string(),
        ],
    );
    let commit_plans = [commit_plan];
    let mut commit_runner = HostLiveRunner::new(&commit_plans, &columns).expect("commit runner");
    let committed = commit_runner
        .collect(&TickDeltas {
            rels: vec![RelDelta {
                rel: "commit_demand".to_string(),
                add: vec![vec![
                    Value::Text("commit-identity".to_string()),
                    Value::Text("commit-witness".to_string()),
                    Value::Text(root.path().display().to_string()),
                    Value::Text(state.path().display().to_string()),
                    Value::Text(stage_id),
                ]],
                del: vec![],
            }],
            carry_pending: false,
        })
        .expect("commit response tick");
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].row[5], Value::Text("committed".to_string()));
    assert_eq!(
        std::fs::read(root.path().join("from-runner.txt")).expect("committed bytes"),
        b"created by source_stage\n"
    );
}

#[test]
fn directory_stage_waits_for_its_exact_approval_then_commits_idempotently() {
    let root = TempDir::new().expect("target root");
    let state = TempDir::new().expect("state root");
    let staged = stage_directory(&root, &state);
    assert_eq!(staged["outcome"], "staged");
    assert!(staged["document"].is_array());
    let stage_id = staged["stage_id"].as_str().expect("stage id").to_string();
    assert!(stage_id.len() == 64);
    assert!(
        !root.path().join("created.txt").exists(),
        "staging must not mutate the target root"
    );

    let wrong = run(
        "source_commit",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("stage_id", "00".repeat(32)),
        ],
    );
    assert_eq!(wrong["outcome"], "refused");
    assert!(
        !root.path().join("created.txt").exists(),
        "an unrelated approval id cannot mutate the target root"
    );

    let committed = run(
        "source_commit",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("stage_id", stage_id.clone()),
        ],
    );
    assert_eq!(committed["outcome"], "committed");
    assert_eq!(
        std::fs::read(root.path().join("created.txt")).expect("committed bytes"),
        b"created by source_stage\n"
    );

    let again = run(
        "source_commit",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("stage_id", stage_id),
        ],
    );
    assert_eq!(again["outcome"], "committed");
}

#[test]
fn stale_stage_is_a_response_row_and_performs_zero_writes() {
    let root = TempDir::new().expect("target root");
    let state = TempDir::new().expect("state root");
    std::fs::write(root.path().join("present.txt"), b"actual\n").expect("seed source");
    let directory = match directory_id(root.path()) {
        soopy::SourceRootId::Directory { directory } => directory,
        soopy::SourceRootId::GitWorktree { .. } => unreachable!(),
    };
    let request = soopy::StageRequest::new(
        soopy::SourceRootId::Directory {
            directory: directory.clone(),
        },
        vec![soopy::SourceAction::Replace {
            source: soopy::ActionSource::Directory {
                file: soopy::FileRef {
                    directory,
                    path: soopy::RootPath(Arc::from("present.txt")),
                },
            },
            expected: soopy::ContentId::Blake3([99; 32]),
            edits: vec![],
        }],
    );
    let refused = run(
        "source_stage",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            (
                "request",
                serde_json::to_string(&request).expect("request JSON"),
            ),
        ],
    );
    assert_eq!(refused["outcome"], "refused");
    assert!(refused["detail"]
        .as_str()
        .expect("detail")
        .contains("Stale"));
    assert_eq!(
        std::fs::read(root.path().join("present.txt")).expect("source bytes"),
        b"actual\n"
    );
}

#[test]
fn state_directory_inside_the_target_is_refused_before_stage_storage_is_created() {
    let root = TempDir::new().expect("target root");
    let state = root.path().join(".source-state");
    std::fs::create_dir(&state).expect("seed invalid state directory");
    let request = stage_request(
        directory_id(root.path()),
        soopy::SourcePath::Directory {
            path: soopy::RootPath(Arc::from("created.txt")),
        },
    );
    let refused = run(
        "source_stage",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.display().to_string())),
            ("request", request),
        ],
    );
    assert_eq!(refused["outcome"], "refused");
    assert!(refused["detail"]
        .as_str()
        .expect("detail")
        .contains("outside"));
    assert!(
        !state.join("stages").exists(),
        "the host must reject before it creates state under the target"
    );
}

#[test]
fn git_worktree_stage_and_commit_use_the_worktree_identity() {
    let root = TempDir::new().expect("Git worktree root");
    let state = TempDir::new().expect("state root");
    let init = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root.path())
        .status()
        .expect("run git init");
    assert!(init.success());
    let request = stage_request(
        git_worktree_id(root.path()),
        soopy::SourcePath::Git {
            path: soopy::RepoPath(Arc::from("git-created.txt")),
        },
    );
    let staged = run(
        "source_stage",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("request", request),
        ],
    );
    assert_eq!(staged["outcome"], "staged");
    let stage_id = staged["stage_id"].as_str().expect("stage id").to_string();
    let committed = run(
        "source_commit",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("stage_id", stage_id),
        ],
    );
    assert_eq!(committed["outcome"], "committed");
    assert_eq!(
        std::fs::read(root.path().join("git-created.txt")).expect("committed bytes"),
        b"created by source_stage\n"
    );
}
