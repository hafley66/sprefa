use std::collections::BTreeMap;
use std::process::Command;
use std::sync::Arc;

use sprefa_engine_rs::hosts::{HostLiveRunner, IHostExecutor, SoopyMutationExecutor};
use sprefa_engine_rs::program::run_boot;
use sprefa_engine_rs::sql::SqliteSeam;
use sprefa_engine_rs::types::{
    Arrival, ArrivalSign, HostAdapterRow, HostColumnPlan, HostPlanData, RelDelta, TickDeltas, Value,
};
use sprefa_engine_rs::GenProgram;
use tempfile::TempDir;

// Regenerate with:
// swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
//   -g "compile_dl6('v6/dl/fixtures/source-mutations.dl6', \
//       'v6/sprefa-engine-rs/tests/fixtures/source-mutations.program.rs', \
//       [emitter(emit_rust:emit_program)])" -g halt
#[path = "fixtures/source-mutations.program.rs"]
mod source_mutations_program;

fn text(value: impl AsRef<str>) -> String {
    value.as_ref().to_string()
}

fn add(rel: &str, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row,
    }
}

fn generated_source_mutations_program() -> GenProgram {
    GenProgram::from_json(source_mutations_program::program())
}

fn evidence_span() -> Value {
    Value::Text(
        serde_json::json!({
            "file": {
                "rev": {
                    "repo": { "root": "golden-evidence-root" },
                    "oid": "golden-evidence-revision"
                },
                "path": "evidence.rs",
                "blob": { "oid": "golden-evidence-blob" }
            },
            "start_byte": 7,
            "end_byte": 19
        })
        .to_string(),
    )
}

fn delta_adds<'a>(deltas: &'a TickDeltas, rel: &str) -> &'a [Vec<Value>] {
    deltas
        .rels
        .iter()
        .find(|delta| delta.rel == rel)
        .map(|delta| delta.add.as_slice())
        .unwrap_or(&[])
}

fn run(host: &str, values: &[(&str, String)]) -> serde_json::Value {
    let env = values
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let rows = SoopyMutationExecutor
        .run(host, "unused", &env)
        .expect("source mutation host response");
    let [row] = rows.as_slice() else {
        panic!("a source-mutation host answers exactly one row, got {rows:?}");
    };
    serde_json::Value::Object(row.clone())
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
        "soopy__stage",
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
        execution: "shell".to_string(),
        request_type: None,
        response_type: None,
    }
}

#[test]
fn compiled_dl6_source_mutation_golden_stages_approves_commits_and_replays() {
    let root = TempDir::new().expect("target root");
    let state = TempDir::new().expect("state root");
    let target = root.path().join("compiled-dl6.txt");
    let request = stage_request(
        directory_id(root.path()),
        soopy::SourcePath::Directory {
            path: soopy::RootPath(Arc::from("compiled-dl6.txt")),
        },
    );
    let proposal = "proposal-from-compiled-dl6";
    let span = evidence_span();
    let program = generated_source_mutations_program();
    let seam = SqliteSeam::in_memory().expect("program seam");
    seam.run_ddl(&program.ddl).expect("program DDL");
    run_boot(&seam, &program.boot);
    let mut runner = HostLiveRunner::new(&program.host_plans, &program.rel_columns)
        .expect("the rel names route the generated hosts in process");

    // Tick 1: authored source evidence joins to the generated source_stage
    // demand. Its only side effect is durable staging outside the target root.
    let staged_demand = program
        .run_tick(
            &seam,
            &[
                add(
                    "source_request",
                    vec![
                        Value::Text(proposal.to_string()),
                        Value::Text(root.path().display().to_string()),
                        Value::Text(state.path().display().to_string()),
                        Value::Text(request),
                        span.clone(),
                    ],
                ),
                add(
                    "source_dependency",
                    vec![
                        span.clone(),
                        Value::Text("dependency-target".to_string()),
                        Value::Text("uses".to_string()),
                    ],
                ),
                add(
                    "source_ownership",
                    vec![span.clone(), Value::Text("owner".to_string())],
                ),
                add(
                    "source_type",
                    vec![
                        span,
                        Value::Text("type-name".to_string()),
                        Value::Text("struct".to_string()),
                    ],
                ),
            ],
        )
        .expect("source evidence tick");
    assert_eq!(
        delta_adds(&staged_demand, "__host_demand_soopy__stage").len(),
        1
    );
    let staged_response = runner.collect(&staged_demand).expect("stage host response");
    assert_eq!(staged_response.len(), 1);
    assert_eq!(staged_response[0].rel, "__host_response_soopy__stage");
    assert_eq!(staged_response[0].row[6], Value::Text("staged".to_string()));
    let stage_id = match &staged_response[0].row[5] {
        Value::Text(stage_id) => stage_id.clone(),
        value => panic!("generated stage response has a non-text stage id: {value:?}"),
    };
    assert!(
        !target.exists(),
        "the generated stage host may only write durable state before approval"
    );

    // Tick 2: the actual Rust host response re-enters the compiled program.
    let staged = program
        .run_tick(&seam, &staged_response)
        .expect("stage response tick");
    assert_eq!(delta_adds(&staged, "source_stage_preview").len(), 1);
    assert!(
        delta_adds(&staged, "__host_demand_soopy__commit").is_empty(),
        "a stage result alone must derive no commit host demand"
    );
    assert!(runner
        .collect(&staged)
        .expect("no commit without approval")
        .is_empty());
    assert!(
        !target.exists(),
        "an unapproved stage must leave target bytes absent"
    );

    // Tick 3: a proposal approval carrying another StageId has no join.
    let wrong_approval = program
        .run_tick(
            &seam,
            &[add(
                "source_approval",
                vec![
                    Value::Text(proposal.to_string()),
                    Value::Text("00".repeat(32)),
                ],
            )],
        )
        .expect("wrong approval tick");
    assert!(
        delta_adds(&wrong_approval, "__host_demand_soopy__commit").is_empty(),
        "a wrong StageId must derive no commit host demand"
    );
    assert!(runner
        .collect(&wrong_approval)
        .expect("wrong approval has no host response")
        .is_empty());
    assert!(
        !target.exists(),
        "a wrong approval must leave target bytes absent"
    );

    // Tick 4: the exact stage id reaches source_commit. Tick 5 lands the
    // response and exposes the authored receipt relation.
    let approved = program
        .run_tick(
            &seam,
            &[add(
                "source_approval",
                vec![
                    Value::Text(proposal.to_string()),
                    Value::Text(stage_id.clone()),
                ],
            )],
        )
        .expect("exact approval tick");
    assert_eq!(
        delta_adds(&approved, "__host_demand_soopy__commit").len(),
        1
    );
    let committed_response = runner.collect(&approved).expect("commit host response");
    assert_eq!(committed_response.len(), 1);
    assert_eq!(committed_response[0].rel, "__host_response_soopy__commit");
    assert_eq!(
        committed_response[0].row[5],
        Value::Text("committed".to_string())
    );
    assert_eq!(
        std::fs::read(&target).expect("host committed target bytes"),
        b"created by source_stage\n"
    );
    let committed = program
        .run_tick(&seam, &committed_response)
        .expect("commit response tick");
    let receipts = delta_adds(&committed, "source_commit_receipt");
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0][0], Value::Text(stage_id.clone()));
    let receipt = match &receipts[0][1] {
        Value::Text(document) => {
            serde_json::from_str::<serde_json::Value>(document).expect("receipt JSON document")
        }
        value => panic!("receipt document was not text JSON: {value:?}"),
    };
    assert_eq!(receipt["applied_files"], 1);
    assert_eq!(receipt["watch"]["paths"][0]["path"], "compiled-dl6.txt");
    assert_eq!(
        std::fs::read(&target).expect("committed target bytes"),
        b"created by source_stage\n"
    );

    // A replacement runner models a runtime restart before acknowledgement.
    // Replaying the emitted commit demand uses the durable Soopy receipt and
    // leaves the target bytes unchanged.
    let mut restarted =
        HostLiveRunner::new(&program.host_plans, &program.rel_columns).expect("restarted hosts");
    let replayed = restarted
        .collect(&approved)
        .expect("idempotent replay host response");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].row[5], Value::Text("committed".to_string()));
    let replayed_tick = program
        .run_tick(&seam, &replayed)
        .expect("idempotent replay response tick");
    assert!(delta_adds(&replayed_tick, "source_commit_refusal").is_empty());
    assert_eq!(
        std::fs::read(&target).expect("target bytes after replay"),
        b"created by source_stage\n"
    );
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
        "soopy__stage",
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
    let stage_rows = [HostAdapterRow {
        adapter: "/soopy/stage".to_string(),
        demand_rel: "stage_demand".to_string(),
        response_rel: "stage_response".to_string(),
    }];
    let mut stage_runner = HostLiveRunner::with_adapter_rows(&stage_plans, &columns, &stage_rows)
        .expect("sidecar routes stage runner in process");
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
        "soopy__commit",
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
    let commit_rows = [HostAdapterRow {
        adapter: "/soopy/commit".to_string(),
        demand_rel: "commit_demand".to_string(),
        response_rel: "commit_response".to_string(),
    }];
    let mut commit_runner =
        HostLiveRunner::with_adapter_rows(&commit_plans, &columns, &commit_rows)
            .expect("sidecar routes commit runner in process");
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
        "soopy__commit",
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
        "soopy__commit",
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
        "soopy__commit",
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
        "soopy__stage",
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
        "soopy__stage",
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
        "soopy__stage",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("request", request),
        ],
    );
    assert_eq!(staged["outcome"], "staged");
    let stage_id = staged["stage_id"].as_str().expect("stage id").to_string();
    let committed = run(
        "soopy__commit",
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

/// The executor mints the root identity when a program omits it, so a datalog
/// program never has to spell the blake3 DirectoryId/RepositoryId/WorktreeId.
/// A git checkout derives a GitWorktree root. A Create carries its shim body as
/// `text` (the executor rewrites it to `bytes`), and a Move carries a source
/// path plus a committed-file `expected` digest, so the derived revision object
/// is proven to pass plan_mutations.
#[test]
fn stage_with_omitted_root_derives_git_worktree_and_stages() {
    let root = TempDir::new().expect("target root");
    let state = TempDir::new().expect("state root");
    git_init(root.path());
    let blob = String::from_utf8(
        Command::new("git")
            .arg("-C")
            .arg(root.path())
            .args(["rev-parse", "HEAD:seed.txt"])
            .output()
            .expect("blob oid of the committed seed")
            .stdout,
    )
    .expect("utf8 blob oid")
    .trim()
    .to_string();
    let request = serde_json::json!({
        "schema_version": 1,
        "actions": [
            {
                "kind": "create",
                "path": {"kind": "git", "path": "derived-root-created.txt"},
                "text": "created by derived-root stage\n",
            },
            {
                "kind": "move",
                "source": {
                    "kind": "git",
                    "source": {"repository": "", "revision": "", "path": "seed.txt"},
                },
                "expected": {"GitBlob": blob},
                "destination": {"kind": "git", "path": "next/seed.txt"},
            }
        ]
    })
    .to_string();
    let staged = run(
        "soopy__stage",
        &[
            ("root", text(root.path().display().to_string())),
            ("state", text(state.path().display().to_string())),
            ("request", request),
        ],
    );
    assert_eq!(staged["outcome"], "staged");
    let previews = staged["document"]
        .as_array()
        .expect("document is the preview array");
    assert_eq!(previews.len(), 2);
    let created = previews
        .iter()
        .find(|preview| preview["path_after"]["path"] == "derived-root-created.txt")
        .expect("create preview");
    assert_eq!(
        created["after_bytes"],
        serde_json::json!("created by derived-root stage\n".as_bytes().len()),
        "the host rewrote the Create `text` body into previewed bytes"
    );
    let moved = previews
        .iter()
        .find(|preview| preview["path_before"]["path"] == "seed.txt")
        .expect("move preview");
    assert_eq!(moved["path_after"]["path"], "next/seed.txt");
    assert!(
        !root.path().join("derived-root-created.txt").exists(),
        "staging must not mutate the target root"
    );
    assert!(
        !root.path().join("next/seed.txt").exists(),
        "staging must not mutate the target root"
    );
}

fn git_init(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed");
    };
    run(&["init", "-q"]);
    std::fs::write(dir.join("seed.txt"), "seed\n").unwrap();
    run(&["add", "seed.txt"]);
    run(&[
        "-c", "user.name=test", "-c", "user.email=test@test", "commit", "-qm", "seed",
    ]);
}
