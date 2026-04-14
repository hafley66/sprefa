use std::path::PathBuf;
use std::process::Command;

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME",     "test")
        .env("GIT_AUTHOR_EMAIL",    "test@example.com")
        .env("GIT_COMMITTER_NAME",  "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_AUTHOR_DATE",     "2000-01-01T00:00:00")
        .env("GIT_COMMITTER_DATE",  "2000-01-01T00:00:00")
        .status()
        .expect("git command failed");
    assert!(status.success(), "git {:?} failed in {}", args, dir.display());
}

fn binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sprefa-v2"))
}

#[test]
fn smoke_json_version() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path();

    git(repo, &["init", "-b", "main"]);
    git(repo, &["config", "user.email", "test@example.com"]);
    git(repo, &["config", "user.name",  "test"]);

    std::fs::write(repo.join("package.json"), r#"{"version":"9.9.9"}"#).unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-m", "init"]);

    let sprf = tmp.path().join("test.sprf");
    // Use the repo dir basename as slug since discover_repos uses basename
    let slug = repo.file_name().unwrap().to_string_lossy();
    std::fs::write(&sprf, format!(
        "rule(v) {{ > repo({slug}) > rev(main) > fs(**/package.json) > json({{ version: $V }}) }};\n"
    )).unwrap();

    let out = Command::new(binary_path())
        .args(["run", sprf.to_str().unwrap(), "--root", repo.to_str().unwrap()])
        .output()
        .expect("failed to run sprefa-v2");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    eprintln!("stdout:\n{stdout}");
    eprintln!("stderr:\n{stderr}");

    assert!(out.status.success(), "sprefa-v2 exited non-zero: {stderr}");
    assert!(stdout.contains("V=9.9.9"), "expected V=9.9.9 in output:\n{stdout}");
}
