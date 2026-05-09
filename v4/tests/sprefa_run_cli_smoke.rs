use std::process::Command;

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
    assert!(stdout.contains("── facts ──"), "stdout missing facts header:\n{stdout}");
    assert!(stdout.contains("openapi_ops: 2 rows"), "stdout missing openapi_ops rows:\n{stdout}");
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
