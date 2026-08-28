//! `extract rename` on the TS arm, over `tests/fixtures/ts_rename/local`: one
//! symbol declared and used in one file, judged byte-exact against a hand
//! written `after/` tree.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST: with `TsSource` absent from `renames()`, `commit_renames_the_anchor_file`
//! measured `extract rename exited exit status: 2: no rename arm for src/app.ts`.

use std::path::{Path, PathBuf};
use std::process::Command;

const ANCHOR: &str = "src/app.ts";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_rename_ts_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(&tree("before"), &root);
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
}

fn tree(side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/ts_rename/local/{side}"))
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

fn rename_verb(fixture: &Fixture, extra: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(format!("{ANCHOR}#oldName"))
        .arg("newName")
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract rename {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// `diff -rq left right`, as the arc-1 receipt spells it. Returns the entries.
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

/// The committed tree is the hand-written `after/` tree, byte for byte. The
/// string literal, the comment and the shadowed inner binding are inside that
/// judgement: `after/` keeps all three spelling `oldName`.
#[test]
fn commit_renames_the_anchor_file() {
    let fixture = fixture("commit");
    let stdout = rename_verb(&fixture, &["--commit"]);
    assert!(
        stdout.contains(&format!("plan {ANCHOR} oldName -> newName")),
        "plan line missing:\n{stdout}"
    );
    assert!(
        stdout.contains("committed"),
        "commit line missing:\n{stdout}"
    );
    let entries = diff_rq(&fixture.root, &tree("after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// Without `--commit` the tree is byte-identical to `before/` and stdout carries
/// the plan lines.
#[test]
fn dry_run_touches_nothing() {
    let fixture = fixture("dry");
    let stdout = rename_verb(&fixture, &[]);
    for line in [
        &format!("root {}", fixture.root.display()),
        &format!("plan {ANCHOR} oldName -> newName"),
        &format!("  {ANCHOR}  4 uses"),
    ] {
        assert!(stdout.contains(line.as_str()), "missing {line}:\n{stdout}");
    }
    assert!(
        stdout.contains("dry run, tree untouched"),
        "dry-run stage line missing:\n{stdout}"
    );
    let entries = diff_rq(&fixture.root, &tree("before"));
    assert!(
        entries.is_empty(),
        "dry run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// A name no declaration in the anchor binds is `RenameStop::NotFound`, exit 2,
/// and the tree stays put.
#[test]
fn unknown_symbol_stops_and_writes_nothing() {
    let fixture = fixture("notfound");
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(format!("{ANCHOR}#absentName"))
        .arg("newName")
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .arg("--commit")
        .output()
        .expect("extract binary runs");
    assert_eq!(output.status.code(), Some(2), "a stop exits 2");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains(&format!("{ANCHOR} declares no absentName")),
        "NotFound message missing:\n{stderr}"
    );
    let entries = diff_rq(&fixture.root, &tree("before"));
    assert!(
        entries.is_empty(),
        "a stop edited the tree:\n{}",
        entries.join("\n")
    );
}
