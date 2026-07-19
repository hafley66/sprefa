//! Failure-modes class 14 rail: `dl --check` in a LINKED WORKTREE with no warm
//! db must refuse the cold build (exit 0, loud skip note, no bytes written
//! under `roots/`). The incident: every agent worktree's pre-commit `--check`
//! cold-built a ~600MB `roots/<key>/db.sqlite` that orphaned the moment the
//! worktree was deleted (three 593MB orphans in one overnight fleet wave,
//! 2026-07-19).
//!
//! HERMETICITY: every test sets `XDG_STATE_HOME` to its own sandbox, so the
//! guard's roots.json probe and any (refused) db write stay off the real home.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

struct Sandbox {
    home: PathBuf,
    repo: PathBuf,
    worktree: PathBuf,
}

impl Sandbox {
    /// A real git repo with a committed `.dl/` program, plus a linked worktree
    /// of it (whose `.git` is a file — the shape the guard keys on).
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_wt_cold_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("xdg");
        let repo = base.join("repo");
        let worktree = base.join("wt");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(repo.join(".dl")).unwrap();
        fs::write(repo.join(".dl").join("p.dl"),
            "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\n").unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git").args(args).current_dir(&repo).output().unwrap();
            assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "init"]);
        git(&["worktree", "add", "-q", worktree.to_str().unwrap()]);
        assert!(worktree.join(".git").is_file(), "linked worktree .git must be a file");
        Sandbox { home, repo, worktree }
    }

    fn check_in(&self, dir: &PathBuf, extra_env: &[(&str, &str)]) -> (i32, String) {
        let mut cmd = Command::new(DL);
        cmd.arg("--check")
            .current_dir(dir)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .env("DL_NO_DAEMON", "1");
        for (k, v) in extra_env { cmd.env(k, v); }
        let out = cmd.output().unwrap();
        (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).into_owned())
    }

    /// Total db.sqlite bytes under the sandbox home's `roots/` tree.
    fn roots_db_bytes(&self) -> u64 {
        let roots = self.home.join("sprefa").join("roots");
        let mut total = 0u64;
        let mut stack = vec![roots];
        while let Some(d) = stack.pop() {
            for entry in fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.is_dir() { stack.push(p); }
                else if let Ok(m) = entry.metadata() { total += m.len(); }
            }
        }
        total
    }
}

#[test]
fn check_in_cold_unregistered_worktree_skips_instead_of_building() {
    let sb = Sandbox::new("skip");
    let (code, stderr) = sb.check_in(&sb.worktree.clone(), &[]);
    assert_eq!(code, 0, "green-by-skip, never a hook-blocking exit: {stderr}");
    assert!(stderr.contains("cold build refused"), "loud note expected, got: {stderr}");
    assert_eq!(sb.roots_db_bytes(), 0, "the refusal must not write a byte under roots/");
}

#[test]
fn escape_hatch_env_runs_the_real_check_and_builds() {
    let sb = Sandbox::new("hatch");
    let (code, stderr) = sb.check_in(&sb.worktree.clone(), &[("DL_ALLOW_WORKTREE_COLD", "1")]);
    assert_eq!(code, 0, "clean program checks green: {stderr}");
    assert!(!stderr.contains("cold build refused"), "guard must stand down: {stderr}");
    assert!(sb.roots_db_bytes() > 0, "the opted-in check builds the worktree db");
}

#[test]
fn main_checkout_is_untouched_by_the_guard() {
    let sb = Sandbox::new("main");
    let (code, stderr) = sb.check_in(&sb.repo.clone(), &[]);
    assert_eq!(code, 0, "clean program checks green: {stderr}");
    assert!(!stderr.contains("cold build refused"), "a .git DIR root never trips class 14: {stderr}");
    assert!(sb.roots_db_bytes() > 0, "the main checkout still builds its db");
}
