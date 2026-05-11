use std::process::Command;

use effect_runtime::v2::{FactStore, SqliteFactStore};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn sprefa_run_prints_runtime_diags_and_fact_rows() {
    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let sprf = format!("{manifest_dir}/examples/dev-missing-frontend-hook.sprf");

    let output = Command::new(bin)
        .arg(&sprf)
        .arg("--show-rows")
        .output()
        .expect("sprefa-run executes");

    assert!(
        output.status.success(),
        "sprefa-run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("warning:missing_frontend_hook: missing frontend hook for listPets"),
        "stdout missing runtime warning:\n{stdout}",
    );
    assert!(
        stdout.contains("── facts ──"),
        "stdout missing facts header:\n{stdout}"
    );
    assert!(
        stdout.contains("openapi_ops: 2 rows"),
        "stdout missing openapi_ops rows:\n{stdout}"
    );
    assert!(
        stdout.contains("missing_frontend_hooks: 1 rows"),
        "stdout missing missing_frontend_hooks rows:\n{stdout}",
    );
}

#[test]
fn sprefa_run_accepts_sqlite_queue_db() {
    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let sprf = format!("{manifest_dir}/examples/dev-missing-frontend-hook.sprf");
    let dir = tempfile::tempdir().unwrap();
    let queue_db = dir.path().join("queue.db");

    let output = Command::new(bin)
        .arg(&sprf)
        .arg("--queue-db")
        .arg(&queue_db)
        .arg("--no-show-rows")
        .output()
        .expect("sprefa-run executes with sqlite queue");

    assert!(
        output.status.success(),
        "sprefa-run --queue-db failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(queue_db.exists(), "sqlite queue db should be created");
}

#[test]
fn sprefa_run_telemetry_flag_prints_runtime_counters() {
    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let sprf = format!("{manifest_dir}/examples/dev-missing-frontend-hook.sprf");

    let output = Command::new(bin)
        .arg(&sprf)
        .arg("--no-show-rows")
        .arg("--telemetry")
        .output()
        .expect("sprefa-run executes with telemetry");

    assert!(
        output.status.success(),
        "sprefa-run --telemetry failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("── telemetry ──"),
        "stdout missing telemetry:\n{stdout}"
    );
    assert!(
        stdout.contains("rendered="),
        "stdout missing expand counters:\n{stdout}"
    );
    assert!(
        stdout.contains("stage"),
        "stdout missing stage counters:\n{stdout}"
    );
    assert!(
        stdout.contains("store insert_calls="),
        "stdout missing fact store counters:\n{stdout}"
    );
}

#[test]
fn sprefa_run_accepts_sqlite_fact_db() {
    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let sprf = format!("{manifest_dir}/examples/dev-missing-frontend-hook.sprf");
    let dir = tempfile::tempdir().unwrap();
    let fact_db = dir.path().join("facts.db");

    let output = Command::new(bin)
        .arg(&sprf)
        .arg("--fact-db")
        .arg(&fact_db)
        .arg("--no-show-rows")
        .output()
        .expect("sprefa-run executes with sqlite facts");

    assert!(
        output.status.success(),
        "sprefa-run --fact-db failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let facts = SqliteFactStore::<v4::Cursor>::open_file(&fact_db).unwrap();
    assert_eq!(facts.len("openapi_ops"), 2);
    assert_eq!(facts.len("missing_frontend_hooks"), 1);
}

#[test]
fn primitive_examples_run_through_cli() {
    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let root = repo_root();
    let cases = [
        ("str-rule.sprf", "greet: 1 rows", "MSG"),
        ("json-extract.sprf", "people: 1 rows", "NAME"),
        ("rule-sink-fact.sprf", "echo_words: 2 rows", "WORD"),
        ("fs-glob-read-re.sprf", "rule_mentions:", "MATCH"),
    ];

    for (file, table, field) in cases {
        let sprf = format!("{manifest_dir}/examples/{file}");
        let output = Command::new(bin)
            .current_dir(&root)
            .arg(&sprf)
            .arg("--show-rows")
            .output()
            .unwrap_or_else(|e| panic!("sprefa-run {file} failed to spawn: {e}"));

        assert!(
            output.status.success(),
            "sprefa-run {file} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(table), "{file} missing {table:?}\n{stdout}");
        assert!(stdout.contains(field), "{file} missing {field:?}\n{stdout}");
    }
}

#[test]
fn runtime_map_example_writes_html_and_warns_on_missing_source_cursor() {
    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let root = repo_root();
    let sprf = format!("{manifest_dir}/examples/runtime-map-html.sprf");
    let html = root.join("v4/examples/runtime-map.html");

    let output = Command::new(bin)
        .current_dir(&root)
        .arg(&sprf)
        .arg("--root")
        .arg(".")
        .arg("--show-rows")
        .output()
        .expect("sprefa-run runtime-map-html spawns");

    assert!(
        output.status.success(),
        "sprefa-run runtime-map-html failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("runtime_nodes: 8 rows"),
        "stdout missing source rows:\n{stdout}"
    );
    assert!(
        stdout.contains("missing_runtime_nodes: 0 rows"),
        "stdout missing drift row:\n{stdout}"
    );

    let body = std::fs::read_to_string(&html).expect("runtime map html should be written");
    assert!(
        body.contains("<h1>sprefa runtime map</h1>"),
        "html missing heading:\n{body}"
    );
    assert!(
        body.contains(">QueueRow<"),
        "html missing QueueRow node:\n{body}"
    );
    assert!(
        body.contains(">CursorTerm<"),
        "html missing CursorTerm node:\n{body}"
    );
    assert!(
        body.contains(">RenderCtx<"),
        "html missing RenderCtx node:\n{body}"
    );
}

#[test]
fn repo_rev_fs_read_example_runs_with_example_config() {
    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let root = repo_root();
    let sprf = format!("{manifest_dir}/examples/repo-rev-fs-read.sprf");
    let config = format!("{manifest_dir}/examples/sprefa.config.example.toml");

    let output = Command::new(bin)
        .current_dir(&root)
        .env("SPREFA_CONFIG", config)
        .arg(&sprf)
        .arg("--show-rows")
        .output()
        .expect("sprefa-run repo-rev-fs-read spawns");

    assert!(
        output.status.success(),
        "sprefa-run repo-rev-fs-read failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("cargo_name_lines:"),
        "stdout missing table:\n{stdout}"
    );
    assert!(
        stdout.contains("MATCH"),
        "stdout missing match field:\n{stdout}"
    );
}
