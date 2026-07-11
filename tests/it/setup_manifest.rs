//! Hermetic red-side coverage for setup's ownership journal.
use std::{collections::BTreeSet, fs, path::{Path, PathBuf}, process::{Command, Output}};

const DL: &str = env!("CARGO_BIN_EXE_dl");

struct Sandbox { home: PathBuf, state: PathBuf, repo: PathBuf }

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_manifest_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("home");
        let state = base.join("state");
        let repo = base.join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&repo).unwrap();
        Self { home, state, repo }
    }
    fn dl(&self, args: &[&str]) -> Output {
        Command::new(DL).args(args).current_dir(&self.repo)
            .env("HOME", &self.home).env("XDG_STATE_HOME", &self.state)
            .env("DL_NO_DAEMON", "1").env("SPREFA_SETUP_NO_VSCODE", "1")
            .output().unwrap()
    }
    fn setup_yes(&self) -> Output { self.dl(&["setup", "--project", ".", "--yes"]) }
    fn manifest(&self) -> PathBuf { self.state.join("sprefa/setup-manifest.json") }
}

fn output_text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

fn hook_commands(path: &Path, event: &str) -> Vec<String> {
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    value["hooks"][event].as_array().unwrap().iter()
        .flat_map(|entry| entry["hooks"].as_array().unwrap())
        .filter_map(|hook| hook["command"].as_str().map(str::to_owned)).collect()
}

#[test]
fn preexisting_user_skill_with_our_target_name_survives_byte_identical() {
    let sandbox = Sandbox::new("user_skill");
    fs::create_dir_all(sandbox.repo.join("assets")).unwrap();
    fs::write(sandbox.repo.join("assets/testing.skill.md"), "# dl version\n").unwrap();
    let target = sandbox.repo.join(".claude/skills/testing/SKILL.md");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let bytes = b"# user's testing skill\n\0private";
    fs::write(&target, bytes).unwrap();
    let output = sandbox.setup_yes();
    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert!(output_text(&output).contains("exists, not ours, left alone"));
}

#[test]
fn user_hook_entries_survive_merge_and_undo() {
    let sandbox = Sandbox::new("user_hooks");
    let settings = sandbox.repo.join(".claude/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, r#"{"theme":"user","hooks":{"PostToolUse":[{"hooks":[{"type":"command","command":"user-hook"}]}]}}"#).unwrap();
    assert!(sandbox.setup_yes().status.success());
    assert_eq!(hook_commands(&settings, "PostToolUse"), vec!["user-hook", "dl --hook"]);
    let undo = sandbox.dl(&["setup", "--undo"]);
    assert!(undo.status.success(), "{}", output_text(&undo));
    assert_eq!(hook_commands(&settings, "PostToolUse"), vec!["user-hook"]);
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(settings).unwrap()).unwrap();
    assert_eq!(value["theme"], "user");
}

#[test]
fn malformed_settings_json_is_loud_skip_not_crash() {
    let sandbox = Sandbox::new("malformed");
    let settings = sandbox.repo.join(".claude/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, "{ definitely not json").unwrap();
    let output = sandbox.setup_yes();
    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(fs::read_to_string(settings).unwrap(), "{ definitely not json");
    assert!(output_text(&output).contains("is not valid JSON"));
}

#[test]
fn marker_tampered_agents_file_is_loud_skip() {
    let sandbox = Sandbox::new("marker");
    let agents = sandbox.repo.join("AGENTS.md");
    let content = "user preface\n<!-- BEGIN: sprefa-dl -->\nuser removed the end\n";
    fs::write(&agents, content).unwrap();
    let output = sandbox.dl(&["setup", "--project", "."]);
    assert!(output.status.success(), "{}", output_text(&output));
    assert_eq!(fs::read_to_string(agents).unwrap(), content);
    assert!(output_text(&output).contains("marker tampered, left alone"));
}

#[cfg(unix)]
#[test]
fn symlinked_parent_escape_refuses_write_and_undo() {
    use std::os::unix::fs::symlink;
    let write_box = Sandbox::new("escape_write");
    let outside = write_box.repo.parent().unwrap().join("outside-write");
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, write_box.repo.join(".claude")).unwrap();
    let output = write_box.dl(&["setup", "--project", "."]);
    assert!(output.status.success(), "{}", output_text(&output));
    assert!(!outside.join("skills").exists());

    let undo_box = Sandbox::new("escape_undo");
    fs::create_dir_all(undo_box.repo.join("assets")).unwrap();
    fs::write(undo_box.repo.join("assets/testing.skill.md"), "# testing\n").unwrap();
    assert!(undo_box.dl(&["setup", "--project", "."]).status.success());
    let link = undo_box.repo.join(".claude/skills/testing/SKILL.md");
    assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    fs::remove_file(&link).unwrap();
    fs::remove_dir(link.parent().unwrap()).unwrap();
    fs::remove_dir(undo_box.repo.join(".claude/skills")).unwrap();
    fs::remove_dir(undo_box.repo.join(".claude")).unwrap();
    let outside = undo_box.repo.parent().unwrap().join("outside-undo");
    fs::create_dir_all(outside.join("skills/testing")).unwrap();
    let sentinel = outside.join("skills/testing/SKILL.md");
    fs::write(&sentinel, "outside sentinel\n").unwrap();
    symlink(&outside, undo_box.repo.join(".claude")).unwrap();
    let undo = undo_box.dl(&["setup", "--undo"]);
    assert!(undo.status.success(), "{}", output_text(&undo));
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "outside sentinel\n");
    assert!(output_text(&undo).contains("SKIP unsafe path"));
}

#[test]
fn modified_since_install_file_is_left_in_place() {
    let sandbox = Sandbox::new("modified");
    assert!(sandbox.dl(&["setup", "--project", "."]).status.success());
    let starter = sandbox.repo.join(".dl/dl-self-lint.dl");
    fs::write(&starter, "user changed this\n").unwrap();
    let undo = sandbox.dl(&["setup", "--undo"]);
    assert!(undo.status.success(), "{}", output_text(&undo));
    assert_eq!(fs::read_to_string(starter).unwrap(), "user changed this\n");
    assert!(output_text(&undo).contains("modified since install, left in place"));
}

#[test]
fn dry_run_and_real_undo_report_identical_path_sets() {
    let sandbox = Sandbox::new("parity");
    assert!(sandbox.setup_yes().status.success());
    let dry = sandbox.dl(&["setup", "--undo", "--dry-run"]);
    let real = sandbox.dl(&["setup", "--undo"]);
    assert!(dry.status.success() && real.status.success());
    let paths = |output: &Output| -> BTreeSet<String> {
        String::from_utf8_lossy(&output.stdout).lines()
            .filter(|line| line.starts_with("[dl setup] remove ") || line.starts_with("[dl setup] strip markers "))
            .filter_map(|line| line.split_whitespace().last().map(str::to_owned)).collect()
    };
    assert_eq!(paths(&dry), paths(&real), "dry:\n{}\nreal:\n{}", output_text(&dry), output_text(&real));
    assert!(!sandbox.repo.join(".claude/settings.json").exists());
    assert!(!sandbox.repo.join(".codex/hooks.json").exists());
}

#[test]
fn adopt_records_only_verified_wiring_and_undo_reverses_it() {
    #[cfg(unix)] use std::os::unix::fs::symlink;
    let sandbox = Sandbox::new("adopt");
    let agents = sandbox.repo.join("AGENTS.md");
    fs::write(&agents, "user\n<!-- BEGIN: sprefa-dl -->\nadopted\n<!-- END: sprefa-dl -->\n").unwrap();
    let settings = sandbox.repo.join(".claude/settings.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(&settings, r#"{"hooks":{"PostToolUse":[{"hooks":[{"type":"command","command":"dl --hook"}]}]}}"#).unwrap();
    #[cfg(unix)] {
        let source = sandbox.repo.join("assets/testing.skill.md");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "# testing\n").unwrap();
        let link = sandbox.repo.join(".claude/skills/testing/SKILL.md");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink("../../../assets/testing.skill.md", &link).unwrap();
    }
    let adopt = sandbox.dl(&["setup", "--adopt"]);
    assert!(adopt.status.success(), "{}", output_text(&adopt));
    assert!(output_text(&adopt).contains("adopted markers"));
    assert!(output_text(&adopt).contains("adopted hook"));
    let undo = sandbox.dl(&["setup", "--undo"]);
    assert!(undo.status.success(), "{}", output_text(&undo));
    assert!(!fs::read_to_string(&agents).unwrap().contains("BEGIN: sprefa-dl"));
    assert!(hook_commands(&settings, "PostToolUse").is_empty());
}

#[test]
fn uninstall_removes_journal_and_wiring_but_leaves_unowned_content() {
    let sandbox = Sandbox::new("uninstall");
    let unowned = sandbox.repo.join("unowned.txt");
    fs::write(&unowned, "keep me\n").unwrap();
    assert!(sandbox.dl(&["setup", "--project", "."]).status.success());
    assert!(sandbox.manifest().exists());
    let uninstall = sandbox.dl(&["uninstall"]);
    assert!(uninstall.status.success(), "{}", output_text(&uninstall));
    assert_eq!(fs::read_to_string(unowned).unwrap(), "keep me\n");
    assert!(!sandbox.repo.join(".dl/dl-self-lint.dl").exists());
    assert!(!sandbox.manifest().exists());
    assert!(!sandbox.state.join("sprefa").exists());
}

#[test]
fn undo_root_and_global_filters_are_disjoint() {
    let sandbox = Sandbox::new("filters");
    assert!(sandbox.dl(&["setup"]).status.success());
    assert!(sandbox.dl(&["setup", "--project", "."]).status.success());
    let global_skill = sandbox.home.join(".agents/skills/sprefa-dl/SKILL.md");
    let project_starter = sandbox.repo.join(".dl/dl-self-lint.dl");
    assert!(global_skill.exists() && project_starter.exists());
    assert!(sandbox.dl(&["setup", "--undo", "--global"]).status.success());
    assert!(!global_skill.exists());
    assert!(project_starter.exists());
    assert!(sandbox.dl(&["setup", "--undo", "--root", "."]).status.success());
    assert!(!project_starter.exists());
}
