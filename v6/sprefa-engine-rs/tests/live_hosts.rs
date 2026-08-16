//! Live host execution receipts. FAIL-FIRST: before driver::run_schedule_live
//! existed, the happy-path test below failed with "no rows in spanned" because
//! nothing executed the `look` template; the runtime treated hosts as
//! schedule-replay only (the network-replay gap this arc closes).

use std::collections::BTreeMap;

use sprefa_engine_rs::driver::{run_schedule, run_schedule_live};
use sprefa_engine_rs::hosts::{
    HostLiveRunner, IHostExecutor, ShellExecutor, SoopyFilesExecutor, SprefaExtractExecutor,
};
use sprefa_engine_rs::sql::{SqlRunner, SqliteSeam};
use sprefa_engine_rs::types::{
    Arrival, ArrivalSign, HostColumnPlan, HostPlanData, ProgramJson, RelDelta, SqlStatement,
    TickDeltas, Value,
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

#[tokio::test]
async fn live_shell_probe_answers_without_a_scripted_response() {
    let program = fixture_program("live_shell_probe");
    let seam = SqliteSeam::in_memory().expect("seam");
    let schedule = vec![vec![add("source_file", vec![text("a.rs")])]];
    let fold = run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run");
    assert_eq!(
        table_rows(&program, &seam, "spanned"),
        vec![vec![text("a.rs"), Value::Integer(3), Value::Integer(9)]],
        "the printf template's span must land through demand -> execute -> response"
    );
    assert_eq!(
        fold.lines.len(),
        2,
        "one scheduled arrival tick, then exactly one host-response tick"
    );
}

/// SABOTAGE. The same program with the template forced to answer a wrong span
/// lands the wrong bytes, so the happy path's exact-row assert is a real detector.
#[tokio::test]
async fn live_shell_probe_sabotaged_template_lands_the_wrong_span() {
    let mut program = fixture_program("live_shell_probe");
    program.host_plans[0].template = "printf '{\"start\":4,\"end\":9}' # {path}".to_string();
    let seam = SqliteSeam::in_memory().expect("seam");
    let schedule = vec![vec![add("source_file", vec![text("a.rs")])]];
    run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run");
    assert_eq!(
        table_rows(&program, &seam, "spanned"),
        vec![vec![text("a.rs"), Value::Integer(4), Value::Integer(9)]],
    );
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
    let schedule = vec![vec![add("file", vec![text(&target), text("digest-1")])]];
    run_schedule_live(&program, &seam, &schedule, 100)
        .await
        .expect("live run");
    let rows = table_rows(&program, &seam, "call_site");
    assert!(
        rows.iter()
            .any(|row| row == &vec![text(&target), text("digest-1"), text("helper")]),
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
    let env = BTreeMap::new();

    let mode = SprefaExtractExecutor
        .run(
            "extract",
            &format!("$DL_EXTRACT_BIN --family diet_scip {target}"),
            &env,
        )
        .err()
        .expect("a mode name is not linked in-process");
    assert!(mode.message.contains("diet_scip"), "{mode}");
    assert!(mode.message.contains("not linked in-process"), "{mode}");

    let unknown = SprefaExtractExecutor
        .run(
            "extract",
            &format!("$DL_EXTRACT_BIN --family nonsense {target}"),
            &env,
        )
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

fn git_fixture_repo() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "sprefa_engine_soopy_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("src")).expect("fixture directory");
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
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
    };
    git(&["init", "-q"]);
    std::fs::write(root.join("src/file.rs"), "pub const VERSION: u8 = 1;\n").expect("tracked file");
    git(&["add", "."]);
    git(&["commit", "-qm", "initial"]);
    std::fs::write(root.join("src/file.rs"), "pub const VERSION: u8 = 2;\n").expect("dirty file");
    std::fs::write(
        root.join("src/untracked.rs"),
        "pub const UNTRACKED: u8 = 9;\n",
    )
    .expect("untracked file");
    root
}

fn output_lines(output: String) -> Vec<String> {
    output.lines().map(str::to_string).collect()
}

#[test]
fn linked_soopy_file_hosts_match_the_existing_shell_contracts() {
    let root = git_fixture_repo();
    let mut scoped = BTreeMap::new();
    scoped.insert("repo".to_string(), root.display().to_string());
    scoped.insert("glob".to_string(), "**/*.rs".to_string());
    let mut scoped_at = scoped.clone();
    scoped_at.insert("rev".to_string(), "HEAD".to_string());

    let cases = [
        (
            "repo_files",
            scoped,
            format!(
                "git -C '{}' ls-files -- '**/*.rs' | while IFS= read -r entry; do printf '%s %s\\n' \"$entry\" \"$(git -C '{}' hash-object -- \"$entry\")\"; done",
                root.display(), root.display()
            ),
        ),
        (
            "repo_files_at",
            scoped_at,
            format!(
                "git -C '{}' ls-files --with-tree='HEAD' -- '**/*.rs' | while IFS= read -r entry; do oid=\"$(git -C '{}' rev-parse 'HEAD':\"$entry\" 2>/dev/null)\"; [ -n \"$oid\" ] && printf '%s %s\\n' \"$entry\" \"$oid\"; done",
                root.display(), root.display()
            ),
        ),
    ];
    for (host, env, shell_contract) in cases {
        let linked = SoopyFilesExecutor
            .run(host, "deliberately not a shell command", &env)
            .expect("linked result");
        let shell = ShellExecutor
            .run(host, &shell_contract, &BTreeMap::new())
            .expect("shell contract result");
        assert_eq!(output_lines(linked), output_lines(shell), "{host}");
    }
    std::fs::remove_dir_all(root).expect("remove fixture repository");
}

#[test]
fn linked_soopy_unscoped_hosts_match_the_existing_shell_contracts() {
    let mut work = BTreeMap::new();
    work.insert(
        "glob".to_string(),
        "../dl/fixtures/files-hosts.dl6".to_string(),
    );
    let mut at = work.clone();
    at.insert("rev".to_string(), "HEAD".to_string());
    let cases = [
        (
            "files",
            work,
            "git ls-files -- '../dl/fixtures/files-hosts.dl6' | while IFS= read -r entry; do printf '%s %s\\n' \"$entry\" \"$(git hash-object -- \"$entry\")\"; done",
        ),
        (
            "files_at",
            at,
            "git ls-files --with-tree='HEAD' -- '../dl/fixtures/files-hosts.dl6' | while IFS= read -r entry; do oid=\"$(git rev-parse 'HEAD':\"$entry\" 2>/dev/null)\"; [ -n \"$oid\" ] && printf '%s %s\\n' \"$entry\" \"$oid\"; done",
        ),
    ];
    for (host, env, shell_contract) in cases {
        let linked = SoopyFilesExecutor
            .run(host, "deliberately not a shell command", &env)
            .expect("linked result");
        let shell = ShellExecutor
            .run(host, shell_contract, &BTreeMap::new())
            .expect("shell contract result");
        assert_eq!(output_lines(linked), output_lines(shell), "{host}");
    }
}

#[test]
fn live_runner_selects_soopy_for_the_unchanged_shell_host_plan() {
    let plan = HostPlanData {
        name: "files".to_string(),
        inputs: vec![HostColumnPlan {
            name: "glob".to_string(),
            column_type: "text".to_string(),
        }],
        outputs: vec![
            HostColumnPlan {
                name: "path".to_string(),
                column_type: "text".to_string(),
            },
            HostColumnPlan {
                name: "digest".to_string(),
                column_type: "text".to_string(),
            },
        ],
        template: "this must never execute {glob}".to_string(),
        demand_rel: "__host_demand_files".to_string(),
        response_rel: "__host_response_files".to_string(),
        execution: "shell".to_string(),
    };
    let rel_columns = std::collections::HashMap::from([
        (
            "__host_demand_files".to_string(),
            vec!["glob".to_string(), "witness_digest".to_string()],
        ),
        (
            "__host_response_files".to_string(),
            vec![
                "witness_digest".to_string(),
                "ordinal".to_string(),
                "path".to_string(),
                "digest".to_string(),
            ],
        ),
    ]);
    let mut runner = HostLiveRunner::new(std::slice::from_ref(&plan), &rel_columns)
        .expect("linked host plan accepted");
    let arrivals = runner
        .collect(&TickDeltas {
            rels: vec![RelDelta {
                rel: "__host_demand_files".to_string(),
                add: vec![vec![
                    text("../dl/fixtures/files-hosts.dl6"),
                    text("witness-1"),
                ]],
                del: vec![],
            }],
            carry_pending: false,
        })
        .expect("native file host, not the sabotaged template");
    assert_eq!(arrivals.len(), 1);
    assert_eq!(arrivals[0].rel, "__host_response_files");
    assert_eq!(arrivals[0].row[0], text("witness-1"));
    assert_eq!(arrivals[0].row[2], text("../dl/fixtures/files-hosts.dl6"));
}
