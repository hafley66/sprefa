//! Contract: `fs` is INPUT-DRIVEN, not a generator that walks a
//! lower-time `ctx.root`. Per input cursor it picks its walk root:
//!
//!   branch 1 — input cursor carries a repo+rev coord  → git blob-walk
//!              that revision, filtered by the `fs(glob…)` pattern.
//!   branch 2 — input cursor value is an existing directory path
//!              → filesystem-walk that directory.
//!   branch 3 — no meaningful input (empty/default bootstrap cursor)
//!              → walk the directory of the running `.sprf` file
//!              (`dirname(__file__)`), NOT `ctx.root`, NOT process cwd.
//!
//! The host plumbs `--root` to `fs` as an INPUT (the seed cursor's
//! value) so branch 2 fires; with no `--root` and no upstream the
//! bootstrap cursor stays empty and branch 3 walks the script's own
//! folder. `repo()/rev()` feed branch 1 by emitting repo+rev coord
//! cursors upstream of `fs`.
//!
//! Pre-fix these are RED (bare `fs` walked the lower-time root, ignoring
//! its input). Post-fix each branch materializes for the right reason.

use std::process::Command;

/// Run a script through the real `sprefa-run` binary. The script text
/// is written into `script_dir` (defaults to a scratch dir disjoint
/// from any corpus, so `**/*.c` cannot accidentally pick the script
/// itself up). Returns (stdout, `hits` row count). `root` maps to the
/// `--root` CLI flag; `None` omits it entirely.
fn run_in(
    script: &str,
    root: Option<&std::path::Path>,
    script_dir: &std::path::Path,
) -> (String, usize) {
    let sprf = script_dir.join("prog.sprf");
    std::fs::write(&sprf, script).unwrap();

    let bin = env!("CARGO_BIN_EXE_sprefa-run");
    let mut cmd = Command::new(bin);
    cmd.arg(&sprf);
    if let Some(r) = root {
        cmd.arg("--root").arg(r);
    }
    cmd.arg("--show-rows");
    let out = cmd.output().expect("sprefa-run executes");
    assert!(
        out.status.success(),
        "sprefa-run failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let n = stdout
        .lines()
        .find_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("hits:")?;
            rest.trim().split_whitespace().next()?.parse::<usize>().ok()
        })
        .unwrap_or_else(|| panic!("no `hits:` facts line in stdout:\n{stdout}"));
    (stdout, n)
}

/// Write a single C file containing one `printk` site into `dir`.
fn write_c(dir: &std::path::Path, name: &str) {
    std::fs::write(dir.join(name), "void f(void) { printk(\"hi\"); }\n").unwrap();
}

/// BRANCH 2 — the cursor flowing into `fs` carries a directory path as
/// its value (a top-level `\`<abs dir>\`` literal). `fs` must fs-walk
/// THAT directory, not a lower-time root. The corpus dir is disjoint
/// from the script dir and no `--root` is passed, so a row can only
/// appear if `fs` honored its input cursor's value.
#[test]
fn branch2_path_value_cursor_drives_fs_walk() {
    let corpus = tempfile::tempdir().unwrap();
    write_c(corpus.path(), "a.c");
    let scratch = tempfile::tempdir().unwrap();

    let script = format!(
        "`{}` > fs(glob`**/*.c`) > ast(:c)`printk($$$)` > fact(:hits, FS, LO, HI)\n",
        corpus.path().display()
    );
    let (stdout, n) = run_in(&script, None, scratch.path());
    assert!(
        n >= 1,
        "branch 2: a path-value input cursor must drive fs to walk that \
         directory; got {n} rows\n{stdout}"
    );
}

/// BRANCH 2 via `--root` — the host must plumb `--root` to `fs` as an
/// input cursor value (the seed), NOT as a lower-time `FsComponent`
/// field. Bare `fs(glob…)` head of pipe, `--root <corpus>`, script
/// elsewhere. A row proves `--root` reached `fs` as input.
#[test]
fn branch2_root_flag_seeds_fs_input() {
    let corpus = tempfile::tempdir().unwrap();
    write_c(corpus.path(), "b.c");
    let scratch = tempfile::tempdir().unwrap();

    let script = "fs(glob`**/*.c`) > ast(:c)`printk($$$)` > fact(:hits, FS, LO, HI)\n";
    let (stdout, n) = run_in(script, Some(corpus.path()), scratch.path());
    assert!(
        n >= 1,
        "branch 2: `--root` must reach `fs` as an input cursor value; \
         got {n} rows\n{stdout}"
    );
}

/// BRANCH 3 — no input (empty/default bootstrap cursor) and no
/// `--root`. `fs` must enumerate the directory of the running `.sprf`
/// file itself. The `.c` file lives NEXT TO the script; nothing else
/// points `fs` at it, so a row can only appear if `fs` fell back to
/// `dirname(__file__)` (the script's own folder), not `ctx.root` and
/// not process cwd.
#[test]
fn branch3_no_input_walks_script_dir() {
    let scratch = tempfile::tempdir().unwrap();
    // The C file sits in the SAME dir as prog.sprf.
    write_c(scratch.path(), "neighbor.c");

    let script = "fs(glob`**/*.c`) > ast(:c)`printk($$$)` > fact(:hits, FS, LO, HI)\n";
    // No --root: branch 3 must walk the script's own directory.
    let (stdout, n) = run_in(script, None, scratch.path());
    assert!(
        n >= 1,
        "branch 3: with no input and no --root, `fs` must walk the \
         script's own directory; got {n} rows\n{stdout}"
    );
}

/// BRANCH 3 negative — same bare `fs` script, but the `.c` file is NOT
/// next to the script (it is in an unrelated dir that nothing points
/// at). `fs` must NOT find it: branch 3 is strictly the script's own
/// folder, never process cwd or a stray corpus.
#[test]
fn branch3_does_not_walk_unrelated_dir() {
    let scratch = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    write_c(elsewhere.path(), "unreachable.c");

    let script = "fs(glob`**/*.c`) > ast(:c)`printk($$$)` > fact(:hits, FS, LO, HI)\n";
    let (stdout, n) = run_in(script, None, scratch.path());
    assert_eq!(
        n, 0,
        "branch 3: must walk ONLY the script's own dir; a `.c` in an \
         unrelated dir must not be picked up; got {n} rows\n{stdout}"
    );
}

/// BRANCH 1 — a repo+rev coord input must drive a git blob-walk of that
/// revision. Building a real git rev fixture + config wiring through
/// the CLI is heavy; branch 1's blob-walk path is exercised by
/// `tests/cross_rev_smoke.rs` / `tests/rev_smoke.rs` at the component
/// level and end-to-end by the `repo_rev_fs_read_example_runs_with_…`
/// smoke. This case is left documented + ignored per the contract's
/// explicit rev-fixture carve-out.
#[test]
#[ignore = "branch 1 blob-walk: needs git rev fixture"]
fn branch1_repo_rev_coord_blob_walks() {
    // repo() > rev(:HEAD) > fs(glob`**/Cargo.toml`) > read > re`…`
    // The coord-carrying cursor must make `fs` ls-tree that rev rather
    // than fs-walk anything. See examples/repo-rev-fs-read.sprf.
}
