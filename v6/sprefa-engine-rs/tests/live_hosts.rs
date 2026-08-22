//! Live host execution receipts. Every host answers through a LINKED Rust
//! executor; an arrival rel the roster does not name is a named stop at
//! runner construction, so no template ever reaches a process.
//!
//! FAIL-FIRST: before driver::run_schedule_live existed, the extract happy path
//! below failed with "no rows in call_site" because nothing executed the host.

use std::collections::BTreeMap;

use sprefa_engine_rs::driver::{run_schedule, run_schedule_live};
use sprefa_engine_rs::hosts::{
    AstRuleExecutor, HostLiveRunner, IHostExecutor, SprefaExtractExecutor,
};
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{
    Arrival, ArrivalSign, HostColumnPlan, HostPlanData, HostRow, HostTypeDescriptor, HostTypeField,
    ProgramJson, RelDelta, SqlStatement, TickDeltas, Value,
};
use sprefa_engine_rs::GenProgram;

fn fixture_program(name: &str) -> GenProgram {
    let path = format!(
        "{}/tests/fixtures/{name}.program.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let module_text = std::fs::read_to_string(&path).expect("read fixture program");
    let start = module_text.find("r#\"").expect("raw string open") + 3;
    let end = module_text[start..].find("\"#;").expect("raw string close") + start;
    let program_json: ProgramJson =
        serde_json::from_str(&module_text[start..end]).expect("fixture program json");
    GenProgram::from_json(program_json)
}

fn add(rel: &str, row: Vec<Value>) -> Arrival {
    Arrival {
        rel: rel.to_string(),
        sign: ArrivalSign::Add,
        row,
    }
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn rows_text(rows: &[HostRow]) -> String {
    serde_json::to_string(rows).expect("host rows serialize")
}

/// The scalar shell adapter preserves legacy command arguments at the typed
/// template seam. The process-free host runner consumes the filled command
/// bytes through its linked executor path.
#[test]
fn scalar_shell_adapter_preserves_legacy_command_arguments() {
    let filled = sprefa_engine_rs::hosts::fill_template(
        "printf '%s\\n' {path} {count}",
        &BTreeMap::from([
            (
                "path".to_string(),
                sprefa_engine_rs::types::ScalarValue::Text("path with spaces".to_string()),
            ),
            (
                "count".to_string(),
                sprefa_engine_rs::types::ScalarValue::Integer(7),
            ),
        ]),
    );
    assert_eq!(filled, "printf '%s\\n' 'path with spaces' '7'");
}

#[test]
fn ast_rule_executor_runs_the_typed_request_in_process() {
    let directory = tempfile::tempdir().expect("temporary source directory");
    git_run(directory.path(), &["init", "-q"]);
    let path = directory.path().join("sample.rs");
    std::fs::write(&path, "fn main() { println!(\"ok\"); }").expect("write source");
    git_run(directory.path(), &["add", "."]);
    git_run(directory.path(), &["commit", "-qm", "initial"]);
    let digest = git_run(directory.path(), &["rev-parse", "HEAD:sample.rs"]);
    let output = AstRuleExecutor
        .run(
            "ast_rule",
            "$SPREFA_AST_RULE_HOST",
            &BTreeMap::from([
                ("path".into(), path.display().to_string()),
                ("repo".into(), directory.path().display().to_string()),
                ("digest".into(), digest),
                (
                    "request".into(),
                    "id: print\nrule:\n  pattern: println!($MESSAGE)\n".into(),
                ),
            ]),
        )
        .expect("in-process ast-rule execution");
    let row = output.first().expect("ast-rule row");
    assert_eq!(row["record"], "ast_rule");
    assert_eq!(row["query"], "print");
    assert_eq!(row["captures"][0]["name"], "MESSAGE");
}

#[test]
fn ast_rule_digest_selects_pinned_blob_and_rejects_changed_worktree_identity() {
    let directory = tempfile::tempdir().expect("temporary source directory");
    git_run(directory.path(), &["init", "-q"]);
    let path = directory.path().join("sample.rs");
    std::fs::write(&path, "fn old() { println!(\"old\"); }\n").expect("old source");
    git_run(directory.path(), &["add", "."]);
    git_run(directory.path(), &["commit", "-qm", "initial"]);
    let old = git_run(directory.path(), &["rev-parse", "HEAD:sample.rs"]);
    std::fs::write(&path, "fn current() { println!(\"current\"); }\n").expect("current source");
    let current = git_run(directory.path(), &["hash-object", "sample.rs"]);
    let env = |digest: String| {
        BTreeMap::from([
            ("path".into(), path.display().to_string()),
            ("repo".into(), directory.path().display().to_string()),
            ("digest".into(), digest),
            (
                "request".into(),
                "id: print\nrule:\n  pattern: println!($MESSAGE)\n".into(),
            ),
        ])
    };
    let pinned = AstRuleExecutor
        .run("ast_rule", "ignored", &env(old.clone()))
        .expect("pinned old blob");
    let pinned = serde_json::to_string(&pinned).expect("serialize pinned rows");
    assert!(pinned.contains("old"));
    assert!(!pinned.contains("current"));
    let changed = AstRuleExecutor.run(
        "ast_rule",
        "ignored",
        &env("0000000000000000000000000000000000000000".into()),
    );
    assert!(changed.unwrap_err().message.contains("hashes to"));
    let matching = AstRuleExecutor
        .run("ast_rule", "ignored", &env(current))
        .expect("matching current worktree blob");
    assert!(serde_json::to_string(&matching)
        .expect("serialize current rows")
        .contains("current"));
}

#[test]
fn ast_rule_executor_reads_the_blake3_identity_emitted_by_source_watch() {
    let directory = tempfile::tempdir().expect("temporary source directory");
    git_run(directory.path(), &["init", "-q"]);
    let path = directory.path().join("sample.rs");
    let source = b"fn main() { println!(\"worktree\"); }\n";
    std::fs::write(&path, source).expect("write source");

    let repository = soopy::discover(directory.path()).expect("discover repository");
    let mut tree = soopy::SourceTree::open(repository);
    let revision = tree
        .resolve_revision(soopy::Revision::Worktree)
        .expect("resolve worktree");
    let entries = tree
        .enumerate(&revision, &[soopy::Pattern("**/*.rs".into())])
        .expect("enumerate worktree source");
    let entry = entries
        .into_iter()
        .find(|entry| entry.source.path.0.as_ref() == "sample.rs")
        .expect("watch source entry");
    assert!(matches!(entry.content, soopy::ContentId::Blake3(_)));

    let output = AstRuleExecutor
        .run(
            "ast_rule",
            "ignored",
            &BTreeMap::from([
                ("path".into(), path.display().to_string()),
                ("digest".into(), entry.content.to_string()),
                (
                    "request".into(),
                    "id: print\nrule:\n  pattern: println!($MESSAGE)\n".into(),
                ),
            ]),
        )
        .expect("in-process ast-rule execution for source-watch content");
    assert_eq!(output[0]["captures"][0]["text"], "\"worktree\"");
}

#[test]
fn ast_rule_executor_reads_a_plain_filesystem_blake3_identity_without_git() {
    let directory = tempfile::tempdir().expect("temporary source directory");
    let path = directory.path().join("sample.rs");
    let source = b"fn main() { println!(\"filesystem\"); }\n";
    std::fs::write(&path, source).expect("write source");
    let digest = soopy::ContentId::blake3(source).to_string();

    let output = AstRuleExecutor
        .run(
            "ast_rule",
            "ignored",
            &BTreeMap::from([
                ("path".into(), path.display().to_string()),
                ("digest".into(), digest),
                (
                    "request".into(),
                    "id: print\nrule:\n  pattern: println!($MESSAGE)\n".into(),
                ),
            ]),
        )
        .expect("in-process ast-rule execution for plain filesystem content");
    assert_eq!(output[0]["captures"][0]["text"], "\"filesystem\"");
}

#[test]
fn ast_rule_executor_accepts_the_legacy_live_watch_sha256_digest() {
    use sha2::{Digest, Sha256};

    let directory = tempfile::tempdir().expect("temporary source directory");
    let path = directory.path().join("sample.rs");
    let source = b"fn main() { println!(\"legacy-watch\"); }\n";
    std::fs::write(&path, source).expect("write source");
    let digest = format!("{:x}", Sha256::digest(source));

    let output = AstRuleExecutor
        .run(
            "ast_rule",
            "ignored",
            &BTreeMap::from([
                ("path".into(), path.display().to_string()),
                ("digest".into(), digest),
                (
                    "request".into(),
                    "id: print\nrule:\n  pattern: println!($MESSAGE)\n".into(),
                ),
            ]),
        )
        .expect("in-process ast-rule execution for legacy live_watch content");
    assert_eq!(output[0]["captures"][0]["text"], "\"legacy-watch\"");
}

/// An arrival rel the roster does not link is named at construction.
/// FAIL-PRE-FIX: this plan used to reach `sh -c` and run the template; the
/// marker below proves no process starts now.
#[test]
fn an_unrostered_arrival_rel_is_a_named_stop_at_construction() {
    let marker = std::env::temp_dir().join(format!("sprefa-unrouted-host-{}", std::process::id()));
    let _ = std::fs::remove_file(&marker);
    let plan = HostPlanData {
        name: "structured_shell".to_string(),
        inputs: vec![HostColumnPlan {
            name: "request".to_string(),
            column_type: "stage_request".to_string(),
        }],
        outputs: vec![],
        template: format!("touch {} # {{request}}", marker.display()),
        demand_rel: "__host_demand_structured_shell".to_string(),
        response_rel: "__host_response_structured_shell".to_string(),
        execution: "shell".to_string(),
        request_type: Some(HostTypeDescriptor {
            type_ref: "__host_demand_structured_shell/1".to_string(),
            fields: vec![HostTypeField {
                name: "request".to_string(),
                field_type: "stage_request".to_string(),
            }],
        }),
        response_type: None,
    };
    let rel_columns = std::collections::HashMap::from([(
        "__host_demand_structured_shell".to_string(),
        vec!["request".to_string(), "witness_digest".to_string()],
    )]);
    let failure = HostLiveRunner::new(std::slice::from_ref(&plan), &rel_columns)
        .err()
        .expect("an unrouted host must stop at construction");
    assert_eq!(failure.host, "structured_shell");
    assert!(
        failure.message.contains("no executor links host"),
        "{failure}"
    );
    // The stop names the roster, so the author sees what IS linked.
    assert!(failure.message.contains("/extract/records"), "{failure}");
    assert!(failure.message.contains("/soopy/files"), "{failure}");
    assert!(!marker.exists(), "no template may reach a process");
}

#[test]
fn native_structured_input_does_not_enter_a_scalar_transport_check() {
    let plan = HostPlanData {
        name: "native_structured".to_string(),
        inputs: vec![
            HostColumnPlan {
                name: "request".to_string(),
                column_type: "stage_request".to_string(),
            },
            HostColumnPlan {
                name: "path".to_string(),
                column_type: "text".to_string(),
            },
        ],
        outputs: vec![],
        template: String::new(),
        demand_rel: "__host_demand_native_structured".to_string(),
        response_rel: "__host_response_native_structured".to_string(),
        execution: "/extract/records".to_string(),
        request_type: Some(HostTypeDescriptor {
            type_ref: "__host_demand_native_structured/2".to_string(),
            fields: vec![
                HostTypeField {
                    name: "request".to_string(),
                    field_type: "stage_request".to_string(),
                },
                HostTypeField {
                    name: "path".to_string(),
                    field_type: "text".to_string(),
                },
            ],
        }),
        response_type: None,
    };
    let rel_columns = std::collections::HashMap::from([(
        "__host_demand_native_structured".to_string(),
        vec![
            "request".to_string(),
            "path".to_string(),
            "witness_digest".to_string(),
        ],
    )]);
    let mut runner = HostLiveRunner::new(std::slice::from_ref(&plan), &rel_columns)
        .expect("native host plan is known");
    let failure = runner
        .collect(&TickDeltas {
            rels: vec![RelDelta {
                rel: "__host_demand_native_structured".to_string(),
                // The struct plane may carry a reference id at this seam.
                add: vec![vec![
                    Value::Integer(41),
                    text("/definitely/missing/native-typed-input"),
                    text("witness-native"),
                ]],
                del: vec![],
            }],
            carry_pending: false,
        })
        .expect_err("the in-process native executor should report its missing file");
    assert_eq!(failure.host, "native_structured");
    assert!(failure
        .message
        .contains("read /definitely/missing/native-typed-input"));
    assert!(!failure.message.contains("typed_host_transport_unsupported"));
}

// final_select is the program's own decoded read (dict ids back to text),
// the same SQL the tick-final output uses.
fn table_rows(program: &GenProgram, seam: &SqliteSeam, rel: &str) -> Vec<Vec<Value>> {
    let select = program.final_select.get(rel).expect("final select for rel");
    let result = seam
        .execute(&SqlStatement {
            sql: format!("SELECT * FROM ({select}) ORDER BY 1, 2, 3"),
            args: vec![],
        })
        .expect("select rows");
    result.rows
}

/// The linked twin: DL_EXTRACT_BIN is absent from the environment, so a
/// subprocess spelling would fail; rows landing proves the in-process call.
#[tokio::test]
async fn live_extract_runs_in_process_with_no_binary_configured() {
    std::env::remove_var("DL_EXTRACT_BIN");
    let program = fixture_program("live_extract_calls");
    let seam = SqliteSeam::in_memory().expect("seam");
    let target = format!(
        "{}/tests/fixtures/live_extract_target.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    // An empty digest names the no-digest branch: the unscoped extract host
    // reads the worktree file, unchanged.
    let schedule = vec![vec![add("file", vec![text(&target), text("")])]];
    run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run");
    let rows = table_rows(&program, &seam, "call_site");
    assert!(
        rows.iter()
            .any(|row| row == &vec![text(&target), text(""), text("helper")]),
        "extracted call facts must include main's call to helper, got {rows:?}"
    );
}

#[tokio::test]
async fn unknown_executor_is_named_at_construction() {
    let mut program = fixture_program("live_shell_probe");
    program.host_plans[0].execution = "warp_drive".to_string();
    let failure = HostLiveRunner::new(&program.host_plans, &program.rel_columns)
        .err()
        .expect("unknown executor must be an error");
    assert!(failure.message.contains("warp_drive"), "{failure}");
    assert_eq!(failure.host, "look");
}

/// The replay door stays byte-identical: the scripted-response path still runs
/// through run_schedule with hosts never executing.
#[tokio::test]
async fn scripted_replay_still_runs_without_executing_hosts() {
    let program = fixture_program("live_shell_probe");
    let seam = SqliteSeam::in_memory().expect("seam");
    let schedule = vec![vec![add("source_file", vec![text("nope.rs")])]];
    let fold = run_schedule(&program, &seam, &schedule, 100)
        .await
        .expect("schedule fold");
    assert_eq!(fold.lines.len(), 1);
    assert_eq!(
        table_rows(&program, &seam, "spanned"),
        Vec::<Vec<Value>>::new()
    );
}

#[test]
fn live_flag_rejects_a_scripted_response_row() {
    let program_path = format!(
        "{}/tests/fixtures/live_shell_probe.program.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    let schedule_path = format!(
        "{}/tests/fixtures/live_shell_probe.scripted-response.schedule.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_emit_rust_harness"))
        .args([&program_path, &schedule_path, "--live-hosts"])
        .output()
        .expect("spawn harness");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("__host_response_look"), "{stderr}");
}

#[test]
fn unknown_family_name_is_a_named_stop_in_the_extract_twin() {
    let target = format!(
        "{}/tests/fixtures/live_extract_target.rs",
        env!("CARGO_MANIFEST_DIR")
    );
    // The dead template's `--family` flag is the `families` INPUT now, so the
    // family name reaches the executor as a named input and never as argv.
    let demand = |families: &str| {
        BTreeMap::from([
            ("path".to_string(), target.clone()),
            ("families".to_string(), families.to_string()),
        ])
    };

    let mode = SprefaExtractExecutor::default()
        .run("extract", "", &demand("diet_scip"))
        .err()
        .expect("a mode name is not linked in-process");
    assert!(mode.message.contains("diet_scip"), "{mode}");
    assert!(mode.message.contains("not linked in-process"), "{mode}");

    let unknown = SprefaExtractExecutor::default()
        .run("extract", "", &demand("nonsense"))
        .err()
        .expect("an unknown family is a named stop");
    assert!(unknown.message.contains("nonsense"), "{unknown}");
    assert!(unknown.message.contains("not a known family"), "{unknown}");
}

#[test]
fn template_fill_escapes_for_the_landing_quote_context() {
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "path".to_string(),
        sprefa_engine_rs::types::ScalarValue::Text("a'b c.rs".to_string()),
    );
    let filled =
        sprefa_engine_rs::hosts::fill_template("head -1 {path} '{path}' \"{path}\"", &inputs);
    assert_eq!(filled, "head -1 'a'\\''b c.rs' 'a'\\''b c.rs' \"a'b c.rs\"");
}

fn git_run(root: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "sprefa-engine-rs")
        .env("GIT_AUTHOR_EMAIL", "sprefa-engine-rs@example.invalid")
        .env("GIT_COMMITTER_NAME", "sprefa-engine-rs")
        .env("GIT_COMMITTER_EMAIL", "sprefa-engine-rs@example.invalid")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A digest-carrying demand must extract the BLOB named by that oid, not the
/// worktree bytes. The committed file defines `committed_fn`; the dirty worktree
/// replaces it with `worktree_fn`; a demand pinned to the committed oid extracts
/// only `committed_fn`. This proves the executor reads by oid at the unit level,
/// independent of the door diff the fixture grades.
#[test]
fn digest_carrying_demand_reads_the_blob_not_the_worktree() {
    let root = std::env::temp_dir().join(format!(
        "sprefa_engine_soopy_ts_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("src")).expect("fixture directory");
    git_run(&root, &["init", "-q"]);
    std::fs::write(
        root.join("src/file.ts"),
        "export function committed_fn(): number { return 1; }\n",
    )
    .expect("committed file");
    git_run(&root, &["add", "."]);
    git_run(&root, &["commit", "-qm", "initial"]);
    let committed_oid = git_run(&root, &["rev-parse", "HEAD:src/file.ts"]);
    std::fs::write(
        root.join("src/file.ts"),
        "export function worktree_fn(): number { return 2; }\n",
    )
    .expect("dirty file");

    let env = BTreeMap::from([
        ("repo".to_string(), root.display().to_string()),
        ("digest".to_string(), committed_oid),
        ("path".to_string(), format!("{}/src/file.ts", root.display())),
        ("families".to_string(), "call".to_string()),
    ]);
    let answered = SprefaExtractExecutor::default()
        .run("extract", "", &env)
        .expect("extract the committed blob");
    let output = rows_text(&answered);
    assert!(
        output.contains("committed_fn"),
        "committed function missing: {output}"
    );
    assert!(
        !output.contains("worktree_fn"),
        "worktree function leaked through the digest: {output}"
    );

    std::fs::remove_dir_all(root).expect("remove fixture repository");
}
