//! `.dl` discovery walks the ancestry: a folder with no `.dl/` inherits the
//! nearest ancestor's rails (the vscode-ext "opened a subdir" case), and every
//! `.dl/` up to the git root merges into one program. A `.dl/` ABOVE the git
//! root is the boundary and does not leak in.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_chain_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    p
}

/// Discovery run (no positional program) rooted at `dir`; returns stdout.
/// `XDG_STATE_HOME` is sandboxed beside the fixture: discovery defaults the db
/// to the per-root `roots/<key>/db.sqlite` under the daemon home
/// (storage-endgame L2) — the resolved root here is the fixture's `.dl`-bearing
/// ANCESTOR, so without the override these runs would write into a
/// developer's real `~/.local/state/sprefa`.
fn run(dir: &PathBuf) -> String {
    let out = Command::new(DL)
        .current_dir(dir)
        .args(["--no-daemon"])
        .env("XDG_STATE_HOME", dir.join(".xdg-state"))
        .output()
        .expect("run dl discovery");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn write(path: &PathBuf, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}

#[test]
fn subdir_with_no_dl_inherits_the_ancestor_chain() {
    let ws = sandbox("inherit");
    fs::create_dir_all(ws.join(".git")).unwrap();
    write(
        &ws.join(".dl/rails.dl"),
        "rel top(x: text).\ntop(\"repo-root\").\n? top(x).\n",
    );
    let deep = ws.join("sub/nested/deep");
    fs::create_dir_all(&deep).unwrap();

    // Opened `sub/nested/deep` (no `.dl/`), yet the repo-root rail loads.
    let out = run(&deep);
    assert!(
        out.contains("repo-root"),
        "ancestor .dl loaded from a subdir: {out}"
    );
    let _ = fs::remove_dir_all(&ws);
}

#[test]
fn nested_dl_dirs_merge_up_to_the_git_root() {
    let ws = sandbox("merge");
    fs::create_dir_all(ws.join(".git")).unwrap();
    write(
        &ws.join(".dl/root.dl"),
        "rel a(x: text).\na(\"root-layer\").\n? a(x).\n",
    );
    write(
        &ws.join("sub/.dl/mid.dl"),
        "rel b(x: text).\nb(\"mid-layer\").\n? b(x).\n",
    );
    let deep = ws.join("sub/nested");
    fs::create_dir_all(&deep).unwrap();

    let out = run(&deep);
    assert!(out.contains("root-layer"), "root .dl in the chain: {out}");
    assert!(out.contains("mid-layer"), "mid .dl in the chain: {out}");
    let _ = fs::remove_dir_all(&ws);
}

#[test]
fn dl_above_the_git_root_is_the_boundary() {
    let outer = sandbox("boundary");
    // `.dl/` ABOVE the repo — must NOT leak into the repo's program.
    write(
        &outer.join(".dl/outer.dl"),
        "rel leaked(x: text).\nleaked(\"above-git\").\n? leaked(x).\n",
    );
    let repo = outer.join("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    write(
        &repo.join(".dl/repo.dl"),
        "rel inside(x: text).\ninside(\"in-repo\").\n? inside(x).\n",
    );
    let deep = repo.join("pkg/mod");
    fs::create_dir_all(&deep).unwrap();

    let out = run(&deep);
    assert!(out.contains("in-repo"), "repo .dl loads: {out}");
    assert!(
        !out.contains("above-git"),
        "the .dl above the git root is the boundary: {out}"
    );
    let _ = fs::remove_dir_all(&outer);
}
