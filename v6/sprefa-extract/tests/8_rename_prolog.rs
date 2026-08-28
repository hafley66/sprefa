//! `extract rename` on the Prolog arm: the clause head functor, the export-list
//! indicator, the `dynamic` declaration, the bare and module-qualified body
//! goals, the two arities `--at` tells apart, and the metacall stop.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST, against the arc-7 binary (`PrologSource` absent from `renames()`):
//!     prolog_rename_matches_the_hand_written_after ... exited exit status: 2:
//!         no rename arm for util.pl (extract rename renames ts, rust)
//!     variable_functor_is_a_dynamic_stop ... left: Some(2), right: Some(6)
//!     two_arities_need_at ... left: Some(2), right: Some(3)
//!     swipl_loads_the_after_tree ... passes before the arm exists: it judges the
//!         hand-written after/ tree, which is what makes the diff assertion mean
//!         something.

use std::path::{Path, PathBuf};
use std::process::Command;

const ANCHOR: &str = "util.pl";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "extract_rename_prolog_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    ))
}

fn fixture(case: &str, label: &str) -> Fixture {
    let base = scratch(&format!("{case}_{label}"));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    copy_tree(&tree(case, "before"), &root);
    Fixture {
        root: root.canonicalize().expect("canonicalize fixture root"),
        state,
    }
}

fn tree(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/prolog_rename/{case}/{side}"))
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
        }
    }
}

fn rename_verb(fixture: &Fixture, target: &str, new: &str, extra: &[&str]) -> String {
    let output = run_rename(&fixture.root, &fixture.state, target, new, extra);
    assert!(
        output.status.success(),
        "extract rename {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_rename(
    root: &Path,
    state: &Path,
    target: &str,
    new: &str,
    extra: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(root)
        .arg("--state")
        .arg(state)
        .args(extra)
        .output()
        .expect("extract binary runs")
}

/// `diff -rq left right`, as the arc-5 receipt spells it. Returns the entries.
fn diff_rq(left: &Path, right: &Path) -> Vec<String> {
    let output = Command::new("diff")
        .arg("-rq")
        .arg(left)
        .arg(right)
        .output()
        .expect("diff runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// The committed tree is the hand-written `after/` tree, byte for byte, which
/// pins what stays too: `main.pl`'s own `helper/3`, the `"helper ~w"` format
/// template, and `other.pl`'s `helper/2` in a module `main.pl` never imports.
/// @comment-ok: the after/ tree is the assertion, so the case list lives here
#[test]
fn prolog_rename_matches_the_hand_written_after() {
    let fixture = fixture("local", "commit");
    let stdout = rename_verb(&fixture, &format!("{ANCHOR}#helper"), "tool", &["--commit"]);
    for line in [
        format!("plan {ANCHOR} helper -> tool"),
        "  main.pl  2 uses".to_string(),
        "  util.pl  4 uses".to_string(),
    ] {
        assert!(stdout.contains(&line), "missing {line}:\n{stdout}");
    }
    let entries = diff_rq(&fixture.root, &tree("local", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `Goal =.. [helper, A, B], call(Goal)` builds the goal at runtime: exit 6 with
/// the goal's own offset, and the tree keeps its bytes.
#[test]
fn variable_functor_is_a_dynamic_stop() {
    let fixture = fixture("dynamic", "stop");
    let output = run_rename(
        &fixture.root,
        &fixture.state,
        &format!("{ANCHOR}#helper"),
        "tool",
        &["--commit"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(6), "Dynamic exits 6:\n{stderr}");
    let text = std::fs::read_to_string(tree("dynamic", "before").join("main.pl"))
        .expect("dynamic fixture text");
    let goal_offset = text
        .find("Goal =.. [helper, Input, Output]")
        .expect("the =.. goal is in the fixture");
    assert!(
        stderr.contains(&format!("main.pl byte {goal_offset}")),
        "the stop names the goal's own offset:\n{stderr}"
    );
    let entries = diff_rq(&fixture.root, &tree("dynamic", "before"));
    assert!(
        entries.is_empty(),
        "the stopped run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// `helper/2` and `helper/3` in one anchor are two symbols. Without `--at` the
/// run is `Ambiguous` (exit 3, tree untouched); with the byte of the `helper/2`
/// head, the `/2` clauses alone move and every `/3` seat stays.
#[test]
fn two_arities_need_at() {
    let blind = fixture("arities", "blind");
    let output = run_rename(
        &blind.root,
        &blind.state,
        &format!("{ANCHOR}#helper"),
        "tool",
        &["--commit"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(
        output.status.code(),
        Some(3),
        "Ambiguous exits 3:\n{stderr}"
    );
    let entries = diff_rq(&blind.root, &tree("arities", "before"));
    assert!(
        entries.is_empty(),
        "the ambiguous run edited the tree:\n{}",
        entries.join("\n")
    );

    let text = std::fs::read_to_string(tree("arities", "before").join(ANCHOR))
        .expect("arities fixture text");
    let head = text
        .find("helper(Input, Output) :-")
        .expect("the helper/2 head is in the fixture");
    let picked = fixture("arities", "at");
    rename_verb(
        &picked,
        &format!("{ANCHOR}#helper"),
        "tool",
        &["--commit", "--at", &head.to_string()],
    );
    let entries = diff_rq(&picked.root, &tree("arities", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// The oracle for the byte comparison above: swipl loads the hand-written tree
/// the rename is judged against, so `after/` is a program and not just bytes.
#[test]
fn swipl_loads_the_after_tree() {
    let after = tree("local", "after");
    let output = Command::new("swipl")
        .args(["-g", "halt", "-l", "main.pl"])
        .current_dir(&after)
        .output()
        .expect("swipl runs");
    assert!(
        output.status.success(),
        "swipl -g halt -l after/main.pl exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}
