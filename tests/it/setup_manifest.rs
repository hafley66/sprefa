//! Hermetic red-side coverage for setup's ownership journal.
use std::{fs, path::PathBuf, process::Command};
const DL: &str = env!("CARGO_BIN_EXE_dl");
fn box_dir(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("dl_manifest_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("home");
    let state = base.join("state");
    let repo = base.join("repo");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&repo).unwrap();
    (home, state, repo)
}
fn dl(home: &PathBuf, state: &PathBuf, repo: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(DL)
        .args(args)
        .current_dir(repo)
        .env("HOME", home)
        .env("XDG_STATE_HOME", state)
        .env("DL_NO_DAEMON", "1")
        .output()
        .unwrap()
}
#[test]
fn user_file_malformed_json_and_tampered_marker_survive() {
    let (home, state, repo) = box_dir("red");
    fs::create_dir_all(repo.join(".claude/skills/sprefa-dl")).unwrap();
    fs::write(repo.join(".claude/skills/sprefa-dl/SKILL.md"), "user\n").unwrap();
    fs::create_dir_all(repo.join(".claude")).unwrap();
    fs::write(repo.join(".claude/settings.json"), "{ nope").unwrap();
    fs::write(
        repo.join("AGENTS.md"),
        "<!-- BEGIN: sprefa-dl --> user edit\n",
    )
    .unwrap();
    let out = dl(&home, &state, &repo, &["setup", "--project", ".", "--yes"]);
    assert!(out.status.success());
    assert_eq!(
        fs::read_to_string(repo.join(".claude/skills/sprefa-dl/SKILL.md")).unwrap(),
        "user\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join(".claude/settings.json")).unwrap(),
        "{ nope"
    );
    assert_eq!(
        fs::read_to_string(repo.join("AGENTS.md")).unwrap(),
        "<!-- BEGIN: sprefa-dl --> user edit\n"
    );
}
#[test]
fn modified_undo_dry_run_and_uninstall_preserve_not_ours() {
    let (home, state, repo) = box_dir("undo");
    let setup = dl(&home, &state, &repo, &["setup", "--project", "."]);
    assert!(setup.status.success());
    let starter = repo.join(".dl/dl-self-lint.dl");
    fs::write(&starter, "user changed\n").unwrap();
    let dry = dl(&home, &state, &repo, &["setup", "--undo", "--dry-run"]);
    assert!(String::from_utf8_lossy(&dry.stdout).contains("remove"));
    assert!(starter.exists());
    let real = dl(&home, &state, &repo, &["setup", "--undo"]);
    assert!(real.status.success());
    assert_eq!(fs::read_to_string(&starter).unwrap(), "user changed\n");
    let untouched = repo.join("unowned.txt");
    fs::write(&untouched, "keep").unwrap();
    let un = dl(&home, &state, &repo, &["uninstall"]);
    assert!(un.status.success());
    assert!(untouched.exists());
    assert!(!state.join("sprefa/setup-manifest.json").exists());
}
#[cfg(unix)]
#[test]
fn escaping_symlink_parent_refuses_write_and_undo() {
    use std::os::unix::fs::symlink;
    let (home, state, repo) = box_dir("symlink");
    let outside = repo.parent().unwrap().join("outside");
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, repo.join(".claude")).unwrap();
    let out = dl(&home, &state, &repo, &["setup", "--project", "."]);
    assert!(out.status.success());
    assert!(!outside.join("skills/sprefa-dl/SKILL.md").exists());
    let undo = dl(&home, &state, &repo, &["setup", "--undo"]);
    assert!(undo.status.success());
}
