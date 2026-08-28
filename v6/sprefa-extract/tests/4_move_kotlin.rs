//! `extract move` over a Kotlin corpus: the explicit `import a.b.Decl`, the
//! moved file's own `package` declaration, and the two shapes that are counted
//! rather than rewritten (a wildcard import, a same-package bare use).
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST: every test here exits non-zero before `KotlinSource` joins
//! `rehomes()` (`lang/mod.rs:91`), on `extract move rehomes rust, prolog, ts:
//! src/com/lib/Util.kt` out of `0_move.rs:412`, so the `move_files` success
//! assert (`tests/4_move_kotlin.rs:112`) is the line that fails.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

/// A fixture tree copied off `tests/fixtures/kotlin_move/<tree>`, so a Kotlin
/// corpus states itself as files rather than as string constants.
fn fixture(tree: &str, label: &str) -> Fixture {
    let base = std::env::temp_dir().join(format!(
        "extract_move_kotlin_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).unwrap();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/kotlin_move/{tree}")),
        &root,
    );
    git(&root, &["init", "-q", "."]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.email=extract@move.test",
            "-c",
            "user.name=extract-move",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    Fixture {
        root: root.canonicalize().unwrap(),
        state,
    }
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

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("git stdout is UTF-8")
}

/// The batch door over rows named relative to the fixture root, so a test states
/// its moves the way the tree spells them.
fn move_files(fixture: &Fixture, rows: &[(&str, &str)], extra: &[&str]) -> String {
    let list = fixture.state.join("moves.tsv");
    let body: String = rows
        .iter()
        .map(|(old, new)| {
            format!(
                "{}\t{}\n",
                fixture.root.join(old).display(),
                fixture.root.join(new).display()
            )
        })
        .collect();
    std::fs::write(&list, body).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("move")
        .arg("--list")
        .arg(&list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .args(extra)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "extract move --list {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn kind_count(table: &str, kind: &str) -> usize {
    table
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter(|line| line.split_whitespace().next() == Some(kind))
        .count()
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).unwrap_or_else(|error| panic!("read {rel}: {error}"))
}

const OUT_OF_PACKAGE: [(&str, &str); 1] = [("src/com/lib/Util.kt", "src/com/core/Util.kt")];
const MISMATCHED: [(&str, &str); 1] = [("src/Odd.kt", "src/core/Odd.kt")];

/// An import names a DECL, so only the moved file's own decls respell: `Peer`
/// stays in `com.lib` and its importer keeps naming it there.
#[test]
fn an_explicit_importer_is_respelled_across_packages() {
    let fixture = fixture("basic", "explicit_importer");
    move_files(&fixture, &OUT_OF_PACKAGE, &["--commit"]);
    let main = read(&fixture.root, "src/com/app/Main.kt");

    assert!(main.contains("import com.core.Util\n"), "{main}");
    assert!(main.contains("import com.core.helper\n"), "{main}");
    assert!(main.contains("import com.lib.Peer\n"), "{main}");
    assert!(
        read(&fixture.root, "src/com/alias/Aliased.kt").contains("import com.core.Util as U\n"),
        "the alias survives the path rewrite:\n{}",
        read(&fixture.root, "src/com/alias/Aliased.kt")
    );
}

/// The `package` declaration is truth and the directory is advisory, so the
/// moved file's own declaration follows it to the new directory.
#[test]
fn the_moved_files_package_line_is_respelled() {
    let fixture = fixture("basic", "package_line");
    move_files(&fixture, &OUT_OF_PACKAGE, &["--commit"]);
    let moved = read(&fixture.root, "src/com/core/Util.kt");

    assert!(moved.starts_with("package com.core\n"), "{moved}");
    assert!(!fixture.root.join("src/com/lib/Util.kt").exists());
}

/// A wildcard may still cover the moved decls and the old package keeps its
/// other files, so there is no sound text rewrite: it is counted, out loud.
#[test]
fn a_wildcard_importer_is_warned_and_left_alone() {
    let fixture = fixture("basic", "wildcard");
    let before = read(&fixture.root, "src/com/wild/Wide.kt");
    let table = move_files(&fixture, &OUT_OF_PACKAGE, &["--commit"]);

    assert!(
        table.contains("warn src/com/lib/Util.kt: 1 wildcard import(s) of com.lib left alone"),
        "table:\n{table}"
    );
    assert!(
        table.contains("warn src/com/lib/Util.kt: 1 same-package bare use(s)"),
        "Peer.kt names Util with no import at all:\n{table}"
    );
    assert_eq!(read(&fixture.root, "src/com/wild/Wide.kt"), before);
}

/// The source root is derived from the old path plus the declared package, so a
/// layout that disagrees with the package leaves no root to re-aim against.
#[test]
fn a_layout_package_disagreement_is_a_named_error() {
    let fixture = fixture("mismatch", "disagreement");
    let table = move_files(&fixture, &MISMATCHED, &[]);

    assert!(
        table.contains(
            "error src/Odd.kt: its directory src does not match its declared package com.lib"
        ),
        "table:\n{table}"
    );
    assert_eq!(kind_count(&table, "replace"), 0, "table:\n{table}");
    assert!(read(&fixture.root, "src/com/app/Main.kt").contains("import com.lib.Odd"));
}

/// A dry run walks the same soopy path a real run does, against a temp mirror.
#[test]
fn a_dry_run_prints_every_respell_and_touches_nothing() {
    let fixture = fixture("basic", "dry_run");
    let table = move_files(&fixture, &OUT_OF_PACKAGE, &[]);

    assert_eq!(
        kind_count(&table, "replace"),
        3,
        "Main.kt, Aliased.kt and the moved file's own package line:\n{table}"
    );
    assert_eq!(kind_count(&table, "move"), 1, "table:\n{table}");
    assert!(fixture.root.join("src/com/lib/Util.kt").is_file());
    assert!(!fixture.root.join("src/com/core/Util.kt").exists());
    assert!(read(&fixture.root, "src/com/app/Main.kt").contains("import com.lib.Util"));
}
