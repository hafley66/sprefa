//! `extract rename` on the TS arm: arc 1's anchor-file rename over
//! `tests/fixtures/ts_rename/local`, judged byte-exact against a hand written
//! `after/` tree, and arc 2's named stops over `tests/fixtures/ts_rename/stops/`.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST (arc 2), against the arc-1 binary:
//!     ambiguous_stops_then_at_selects_the_class ... left: Some(0), right: Some(3)
//!         (the run renamed the class instead of stopping)
//!     commit_renames_the_anchor_file ... exited 2: error: unexpected argument '--at' found
//!     dry_run_touches_nothing ... exited 2: error: unexpected argument '--at' found
//!     dynamic_seats_stop_and_write_nothing ... read fixture dir: NotFound
//!         (the stop did not exist)
//!     inexact_is_unreachable_from_the_ts_arm ... ts_rename.rs constructs Inexact
//!     unknown_symbol_stops_and_writes_nothing ... left: Some(2), right: Some(4)

use std::path::{Path, PathBuf};
use std::process::Command;

const ANCHOR: &str = "src/app.ts";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn fixture(case: &str, label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_rename_ts_{case}_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(&tree(case, "before"), &root);
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
}

fn tree(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/ts_rename/{case}/{side}"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(target)
        .arg(new)
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

/// A failed rename run: status, stderr, and the untouched tree claim.
struct StoppedRun {
    code: Option<i32>,
    stderr: String,
}

fn stopped_rename_verb(fixture: &Fixture, target: &str, new: &str, extra: &[&str]) -> StoppedRun {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .arg("--commit")
        .output()
        .expect("extract binary runs");
    StoppedRun {
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn assert_untouched(fixture: &Fixture, case: &str) {
    let entries = diff_rq(&fixture.root, &tree(case, "before"));
    assert!(
        entries.is_empty(),
        "the run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// Byte offset of the identifier `"{prefix} {name}"` inside the fixture's
/// before tree, so a test never hardcodes a number the fixture text owns.
fn ident_offset(case: &str, prefix: &str, name: &str) -> usize {
    let text = std::fs::read_to_string(tree(case, "before").join(ANCHOR)).expect("fixture text");
    let needle = format!("{prefix} {name}");
    let start = text.find(&needle).expect("needle in fixture");
    start + prefix.len() + 1
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
    let fixture = fixture("local", "commit");
    let at = ident_offset("local", "function", "oldName").to_string();
    let stdout = rename_verb(
        &fixture,
        &format!("{ANCHOR}#oldName"),
        "newName",
        &["--at", &at, "--commit"],
    );
    assert!(
        stdout.contains(&format!("plan {ANCHOR} oldName -> newName")),
        "plan line missing:\n{stdout}"
    );
    assert!(
        stdout.contains("committed"),
        "commit line missing:\n{stdout}"
    );
    let entries = diff_rq(&fixture.root, &tree("local", "after"));
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
    let fixture = fixture("local", "dry");
    let at = ident_offset("local", "function", "oldName").to_string();
    let stdout = rename_verb(
        &fixture,
        &format!("{ANCHOR}#oldName"),
        "newName",
        &["--at", &at],
    );
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
    assert_untouched(&fixture, "local");
}

/// A name no declaration in the anchor binds is `RenameStop::NotFound`, exit 4,
/// and the tree stays put.
#[test]
fn unknown_symbol_stops_and_writes_nothing() {
    let fixture = fixture("local", "notfound");
    let stopped = stopped_rename_verb(&fixture, &format!("{ANCHOR}#absentName"), "newName", &[]);
    assert_eq!(
        stopped.code,
        Some(4),
        "NotFound exits 4:\n{}",
        stopped.stderr
    );
    assert!(
        stopped
            .stderr
            .contains(&format!("{ANCHOR} declares no absentName")),
        "NotFound message missing:\n{}",
        stopped.stderr
    );
    assert_untouched(&fixture, "local");
}

/// Two same-named declarations in one file stop the run with BOTH byte offsets
/// named; `--at <offset of the class>` then renames the class and its uses
/// only, leaving the function-local `Foo` spelling `Foo`.
#[test]
fn ambiguous_stops_then_at_selects_the_class() {
    let case = "stops/ambiguous";
    let stopped_tree = fixture(case, "stop");
    let stopped = stopped_rename_verb(&stopped_tree, &format!("{ANCHOR}#Foo"), "Bar", &[]);
    assert_eq!(
        stopped.code,
        Some(3),
        "Ambiguous exits 3:\n{}",
        stopped.stderr
    );
    let class_offset = ident_offset(case, "class", "Foo");
    let local_offset = ident_offset(case, "const", "Foo");
    let stderr = stopped.stderr.replace('\n', " ");
    assert!(
        stderr.contains(&format!("at bytes {local_offset}, {class_offset}"))
            || stderr.contains(&format!("at bytes {class_offset}, {local_offset}")),
        "Ambiguous message must name both offsets {local_offset} and {class_offset}:\n{}",
        stopped.stderr
    );
    assert_untouched(&stopped_tree, case);
    let copy = fixture(case, "select");
    let at = class_offset.to_string();
    rename_verb(
        &copy,
        &format!("{ANCHOR}#Foo"),
        "Bar",
        &["--at", &at, "--commit"],
    );
    let entries = diff_rq(&copy.root, &tree(case, "after"));
    assert!(
        entries.is_empty(),
        "--at commit differs from after/:\n{}",
        entries.join("\n")
    );
}

/// Every seat the TS arm can reach is pinned by `oxc_semantic` to the exact
/// identifier token, so `RenameStop::Inexact` is unreachable from this arm: the
/// source never constructs it, and a battery of read/write/type-ref seats
/// commits byte-exact against the hand-written after tree.
#[test]
fn inexact_is_unreachable_from_the_ts_arm() {
    let arm = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lang/ts_rename.rs");
    let source = std::fs::read_to_string(&arm).expect("ts_rename.rs readable");
    assert!(
        !source.contains("Inexact"),
        "ts_rename.rs constructs Inexact; the unreachable claim is void"
    );

    let case = "stops/inexact";
    let fixture = fixture(case, "commit");
    rename_verb(&fixture, &format!("{ANCHOR}#Foo"), "Bar", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree(case, "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `obj["Foo"]` and `import("./m").then(m => m.Foo)` reach the symbol only at
/// runtime; the run stops with the seat's file and offset, exit 6, and the
/// tree stays put.
#[test]
fn dynamic_seats_stop_and_write_nothing() {
    let case = "stops/dynamic";
    let fixture = fixture(case, "stop");
    let stopped = stopped_rename_verb(&fixture, &format!("{ANCHOR}#Foo"), "Bar", &[]);
    assert_eq!(
        stopped.code,
        Some(6),
        "Dynamic exits 6:\n{}",
        stopped.stderr
    );
    let seat_text =
        std::fs::read_to_string(tree(case, "before").join(ANCHOR)).expect("fixture text");
    let computed_offset = seat_text
        .find("[\"Foo\"]")
        .expect("computed seat in fixture")
        + 1;
    let stderr = stopped.stderr.replace('\n', " ");
    assert!(
        stderr.contains("computed member"),
        "Dynamic form missing:\n{}",
        stopped.stderr
    );
    assert!(
        stderr.contains(&format!("{ANCHOR} byte {computed_offset}")),
        "Dynamic seat offset {computed_offset} missing:\n{}",
        stopped.stderr
    );
    assert_untouched(&fixture, case);
}
