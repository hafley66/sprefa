//! `extract rename` on the Kotlin arm: the declaration's own identifier, the
//! trailing segment of an `import`, a fully-qualified `a.Helper`, the alias
//! clause whose local binding stays, the wildcard stop, and the same-named
//! declaration in another package that reaches none of it.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST, with `KotlinSource` absent from `renames()`:
//!     kotlin_rename_matches_the_hand_written_after ... exited exit status: 2:
//!         no rename arm for src/main/kotlin/a/Util.kt
//!         (extract rename renames ts, rust)
//!     wildcard_importer_is_a_dynamic_stop ... left: Some(2), right: Some(6)
//!     shadow_in_other_package_needs_no_at ... left: Some(2), right: Some(0)

use std::path::{Path, PathBuf};
use std::process::Command;

const ANCHOR: &str = "src/main/kotlin/a/Util.kt";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "extract_rename_kotlin_{label}_{}_{}",
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
        .join(format!("tests/fixtures/kotlin_rename/{case}/{side}"))
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
/// pins what stays too: the `"Helper"` string, the `H` alias and its body use,
/// and the `package c` class of the same name with its own uses.
/// @comment-ok: the after/ tree is the assertion, so the case list lives here
#[test]
fn kotlin_rename_matches_the_hand_written_after() {
    let fixture = fixture("local", "commit");
    let stdout = rename_verb(&fixture, &format!("{ANCHOR}#Helper"), "Tool", &["--commit"]);
    for line in [
        format!("plan {ANCHOR} Helper -> Tool"),
        "  src/main/kotlin/a/Util.kt  3 uses".to_string(),
        "  src/main/kotlin/b/Alias.kt  3 uses".to_string(),
        "  src/main/kotlin/b/Main.kt  6 uses".to_string(),
    ] {
        assert!(stdout.contains(&line), "missing {line}:\n{stdout}");
    }
    assert!(
        !stdout.contains("src/main/kotlin/c/Own.kt"),
        "the package c file is not in the plan:\n{stdout}"
    );
    let entries = diff_rq(&fixture.root, &tree("local", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `import a.*` puts the symbol in a scope no clause names, and the file writes
/// the bare name: exit 6 at the import header, and the tree keeps its bytes.
#[test]
fn wildcard_importer_is_a_dynamic_stop() {
    let fixture = fixture("wildcard", "stop");
    let output = run_rename(
        &fixture.root,
        &fixture.state,
        &format!("{ANCHOR}#Helper"),
        "Tool",
        &["--commit"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(6), "Dynamic exits 6:\n{stderr}");
    let importer = "src/main/kotlin/b/Main.kt";
    let text = std::fs::read_to_string(tree("wildcard", "before").join(importer))
        .expect("wildcard fixture text");
    let import_offset = text.find("import a.*").expect("wildcard in fixture");
    assert!(
        stderr.contains(&format!("{importer} byte {import_offset}")),
        "the stop names the import header's own offset:\n{stderr}"
    );
    assert!(
        stderr.contains("wildcard import"),
        "the stop names the form:\n{stderr}"
    );
    let entries = diff_rq(&fixture.root, &tree("wildcard", "before"));
    assert!(
        entries.is_empty(),
        "the stopped run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// `package c` declares its own `Helper`. A declaration in another file is not a
/// second declaration in the anchor, so the run needs no `--at`: it plans at
/// exit 0 and touches nothing under `c/`.
#[test]
fn shadow_in_other_package_needs_no_at() {
    let fixture = fixture("local", "dry");
    let output = run_rename(
        &fixture.root,
        &fixture.state,
        &format!("{ANCHOR}#Helper"),
        "Tool",
        &[],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(0), "no stop fires:\n{stderr}");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        !stdout.contains("src/main/kotlin/c/Own.kt"),
        "the package c declaration is not a seat:\n{stdout}"
    );
    let entries = diff_rq(&fixture.root, &tree("local", "before"));
    assert!(
        entries.is_empty(),
        "the dry run edited the tree:\n{}",
        entries.join("\n")
    );
}
