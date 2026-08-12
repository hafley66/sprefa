//! Spawn directory preparation: the worktree gap. A spawn makes a worktree by
//! default (branch at the stated base sha) and refuses a non-fast-forward;
//! working in the main tree requires `main_tree: true`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use crate::harness::SpawnSpec;

/// Create the directory a spawn runs in and run its setup steps. Returns that
/// directory. In the main tree the repo is the working dir and must
/// fast-forward to the base sha; otherwise a worktree is created at
/// `worktree_dir` and its setup steps run there in order.
pub fn prepare_spawn_dir(spec: &SpawnSpec) -> Result<PathBuf> {
    if spec.main_tree {
        merge_ff_only(&spec.repo, &spec.base_sha)?;
        return Ok(spec.repo.clone());
    }
    let Some(worktree) = spec.worktree_dir.clone() else {
        anyhow::bail!("worktree spawn requires a worktree_dir");
    };
    if worktree.exists() {
        anyhow::bail!("worktree path already exists: {}", worktree.display());
    }
    if let Some(parent) = worktree.parent() {
        std::fs::create_dir_all(parent).context("create worktree parent")?;
    }
    run_git(
        &spec.repo,
        &[
            "worktree",
            "add",
            "-b",
            &spec.branch,
            &worktree.display().to_string(),
            &spec.base_sha,
        ],
    )?;
    merge_ff_only(&worktree, &spec.base_sha)?;
    if spec.warm_start {
        warm_start(&worktree)?;
    }
    for command in &spec.setup {
        run_shell(&worktree, command)?;
    }
    Ok(worktree)
}

/// Run the repo's `boop-start` recipe in a fresh worktree. A repo without one
/// needs no warmup and is skipped in silence.
///
/// A failure BLOCKS the spawn. The recipe exists because a worktree missing the
/// pre-commit hook's inputs makes every `git commit` abort, and a lane reads
/// that abort as success; warning instead would reproduce the loss it prevents.
pub fn warm_start(worktree: &Path) -> Result<()> {
    if !has_recipe(worktree, "boop-start") {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let output = Command::new("just")
        .arg("boop-start")
        .current_dir(worktree)
        .output()
        .context("run just boop-start")?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        println!("  {line}");
    }
    if !output.status.success() {
        anyhow::bail!(
            "just boop-start failed in {}: {}\n\
             the pre-commit hook needs it, so a lane spawned now could not commit; \
             fix it or respawn with --no-start",
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    println!("boop-start: {:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}

/// True when `just` is on PATH and this tree declares `name`.
fn has_recipe(worktree: &Path, name: &str) -> bool {
    Command::new("just")
        .args(["--show", name])
        .current_dir(worktree)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// `git -C repo merge --ff-only <sha>`; a non-fast-forward is an error.
fn merge_ff_only(repo: &PathBuf, sha: &str) -> Result<()> {
    run_git(repo, &["merge", "--ff-only", sha])
}

fn run_git(repo: &PathBuf, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .with_context(|| format!("run git in {}", repo.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Run one setup step (a shell command) inside `cwd`.
fn run_shell(cwd: &PathBuf, command: &str) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("run setup step in {}", cwd.display()))?;
    if !status.success() {
        anyhow::bail!("setup step failed in {}: {command}", cwd.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    /// A repo with no justfile needs no warmup, so the spawn path stays quiet
    /// instead of failing on a recipe nobody declared.
    #[test]
    fn warm_start_is_a_no_op_where_no_recipe_is_declared() {
        let dir = std::env::temp_dir().join(format!("boop-nostart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!super::has_recipe(&dir, "boop-start"));
        super::warm_start(&dir).expect("no recipe means no work");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A declared recipe is found and run, and its failure blocks the spawn.
    #[test]
    fn a_failing_boop_start_blocks_the_spawn() {
        let dir = std::env::temp_dir().join(format!("boop-badstart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("justfile"), "boop-start:\n    @exit 3\n").unwrap();
        assert!(super::has_recipe(&dir, "boop-start"));
        let error = super::warm_start(&dir).unwrap_err().to_string();
        assert!(error.contains("could not commit"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    use std::process::Command;

    use crate::harness::SpawnSpec;

    use super::prepare_spawn_dir;

    fn init_repo(path: &std::path::Path) -> String {
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(path)
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                path.to_string_lossy().as_ref(),
                "config",
                "user.email",
                "t@t",
            ])
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                path.to_string_lossy().as_ref(),
                "config",
                "user.name",
                "t",
            ])
            .status()
            .unwrap();
        std::fs::write(path.join("seed.txt"), "seed").unwrap();
        let out = Command::new("git")
            .args(["-C", path.to_string_lossy().as_ref(), "add", "-A"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let out = Command::new("git")
            .args([
                "-C",
                path.to_string_lossy().as_ref(),
                "commit",
                "-qm",
                "seed",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let out = Command::new("git")
            .args(["-C", path.to_string_lossy().as_ref(), "rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn spec(
        repo: &std::path::Path,
        base: &str,
        worktree: &std::path::Path,
        main_tree: bool,
    ) -> SpawnSpec {
        SpawnSpec {
            harness: "claude".to_owned(),
            branch: "lane-wt".to_owned(),
            base_sha: base.to_owned(),
            main_tree,
            setup: Vec::new(),
            prompt: "do the lane".to_owned(),
            resume_session: None,
            socket: None,
            worktree_dir: Some(worktree.to_path_buf()),
            repo: repo.to_path_buf(),
            env_stamp: None,
            model: None,
            on_exit: None,
            tmux: None,
            lane: "lane-test".to_owned(),
            mail_dir: std::env::temp_dir(),
            warm_start: false,
        }
    }

    #[test]
    fn worktree_spawn_creates_a_branch_at_the_base() {
        let base = std::env::temp_dir().join(format!("boop-wt-{}-repo", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sha = init_repo(&base);
        let wt = base.with_file_name(format!("boop-wt-{}-work", std::process::id()));
        let _ = std::fs::remove_dir_all(&wt);
        let dir = prepare_spawn_dir(&spec(&base, &sha, &wt, false)).unwrap();
        assert!(
            dir.join("seed.txt").exists(),
            "worktree must carry the seed commit"
        );
        let branch = String::from_utf8_lossy(
            &Command::new("git")
                .args([
                    "-C",
                    &wt.display().to_string(),
                    "rev-parse",
                    "--abbrev-ref",
                    "HEAD",
                ])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        assert_eq!(branch, "lane-wt");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&wt);
    }

    #[test]
    fn setup_steps_run_in_order_in_the_worktree() {
        let base = std::env::temp_dir().join(format!("boop-st-{}-repo", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sha = init_repo(&base);
        let wt = base.with_file_name(format!("boop-st-{}-work", std::process::id()));
        let _ = std::fs::remove_dir_all(&wt);
        let mut s = spec(&base, &sha, &wt, false);
        s.setup = vec![
            "echo first > marker.txt".to_owned(),
            "echo second >> marker.txt".to_owned(),
        ];
        let dir = prepare_spawn_dir(&s).unwrap();
        let marker = std::fs::read_to_string(dir.join("marker.txt")).unwrap();
        assert_eq!(
            marker, "first\nsecond\n",
            "setup steps run in order in the worktree"
        );
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&wt);
    }

    #[test]
    fn main_tree_spawn_refuses_a_non_fast_forward() {
        let base = std::env::temp_dir().join(format!("boop-ff-{}-repo", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sha0 = init_repo(&base);
        let path = base.to_string_lossy().to_string();
        // Move HEAD forward on the default branch: commit A on top of sha0.
        std::fs::write(base.join("a.txt"), "a").unwrap();
        Command::new("git")
            .args(["-C", &path, "add", "-A"])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &path, "commit", "-qm", "a"])
            .status()
            .unwrap();
        let main_branch = String::from_utf8_lossy(
            &Command::new("git")
                .args(["-C", &path, "rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        // Diverge on a side branch: commit B on top of sha0, then return HEAD.
        Command::new("git")
            .args(["-C", &path, "branch", "other", &sha0])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &path, "checkout", "-q", "other"])
            .status()
            .unwrap();
        std::fs::write(base.join("b.txt"), "b").unwrap();
        Command::new("git")
            .args(["-C", &path, "add", "-A"])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &path, "commit", "-qm", "b"])
            .status()
            .unwrap();
        let divergent = String::from_utf8_lossy(
            &Command::new("git")
                .args(["-C", &path, "rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        Command::new("git")
            .args(["-C", &path, "checkout", "-q", &main_branch])
            .status()
            .unwrap();
        // Pin a main-tree spawn to the diverged commit: it cannot fast-forward.
        let result = prepare_spawn_dir(&spec(&base, &divergent, &base.join("nope"), true));
        assert!(
            result.is_err(),
            "non-fast-forward main-tree spawn must be refused"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
